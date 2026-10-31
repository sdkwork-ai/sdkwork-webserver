//! Public-API integration tests across the codec families.
//!
//! These tests drive the crate exactly as the later edge wiring wave will: build
//! the CGI variable set once, hand it to each protocol encoder, and parse the
//! synthetic upstream reply through the public parser types. They also exercise
//! adversarial truncation of every decoder to prove input never panics and
//! always fails closed.

use sdkwork_webserver_upstream_protocol::{
    cgi::{cgi_variables, UpstreamRequestFacts},
    fastcgi::{
        encode_request, FastCgiParams, FastCgiResponseEvent, FastCgiResponseParser,
        RequestIdAllocator,
    },
    grpc::{
        encode_message, grpc_request_headers, parse_grpc_trailers, GrpcMessageDecoder, GrpcTimeout,
    },
    limits::Limits,
    memcached::{
        encode_get, encode_set, parse_get_response, MemcachedGetResponse, MemcachedResponseEvent,
        MemcachedResponseParser,
    },
    scgi::{encode_request as encode_scgi_request, ScgiResponseEvent, ScgiResponseParser},
    uwsgi::{
        decode_vars, encode_request as encode_uwsgi_request, UwsgiPacketHeader, UwsgiResponseEvent,
        UwsgiResponseParser,
    },
    Error,
};

fn headers() -> Vec<(String, String)> {
    vec![
        ("Accept".to_owned(), "*/*".to_owned()),
        ("Cookie".to_owned(), "session=abc".to_owned()),
        ("X-Trace".to_owned(), "t-1".to_owned()),
    ]
}

fn facts<'a>(headers: &'a [(String, String)]) -> UpstreamRequestFacts<'a> {
    UpstreamRequestFacts {
        method: "POST",
        scheme: "https",
        uri: "/api/v1/items",
        query_string: Some("page=2"),
        server_protocol: "HTTP/1.1",
        server_name: "edge.example.com",
        server_port: 443,
        remote_addr: "198.51.100.9",
        document_root: "/srv/www",
        script_filename: "/srv/www/index.php",
        content_type: Some("application/json"),
        content_length: Some(2),
        headers,
    }
}

#[test]
fn cgi_variables_feed_every_protocol_encoder() {
    let headers = headers();
    let variables = cgi_variables(&facts(&headers), &Limits::default()).expect("cgi vars");
    let lookup = |name: &str| {
        variables
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(lookup("REQUEST_METHOD"), Some("POST"));
    assert_eq!(lookup("CONTENT_LENGTH"), Some("2"));
    assert_eq!(lookup("HTTP_X_TRACE"), Some("t-1"));

    // FastCGI
    let params = FastCgiParams::from_variables(&variables, &Limits::default()).expect("params");
    let fastcgi = encode_request(1, false, &params, b"{}", &Limits::default()).expect("fastcgi");
    assert_eq!(fastcgi[0], 1);
    assert_eq!(fastcgi[1], 1);

    // SCGI
    let scgi = encode_scgi_request(&variables, b"{}", &Limits::default()).expect("scgi");
    assert_eq!(scgi.last().copied(), Some(b'}'));
    assert!(scgi.contains(&b':'));

    // uWSGI
    let uwsgi = encode_uwsgi_request(&variables, b"", 0, 0, &Limits::default()).expect("uwsgi");
    let packet = UwsgiPacketHeader::decode(&uwsgi).expect("packet header");
    assert_eq!(
        usize::from(packet.data_size),
        uwsgi.len() - 4,
        "request body is not part of the packet"
    );
    assert_eq!(decode_vars(&uwsgi[4..]).expect("vars"), variables);
}

#[test]
fn fastcgi_response_round_trip_through_public_api() {
    let mut parser = FastCgiResponseParser::new(Limits::default());
    let mut events = parser.feed(b"").expect("empty feed");
    assert!(events.is_empty());
    // A STATUS/header block split across two records, then a body, then END.
    let record = |record_type: u8, content: &[u8]| {
        let mut out = Vec::new();
        let padding = (8 - content.len() % 8) % 8;
        out.push(1);
        out.push(record_type);
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&(content.len() as u16).to_be_bytes());
        out.push(padding as u8);
        out.push(0);
        out.extend_from_slice(content);
        out.resize(out.len() + padding, 0);
        out
    };
    let mut wire = record(6, b"Status: 200 OK\r\nContent-Type: text/plain\r\n\r\nhi");
    wire.extend_from_slice(&record(6, b"!"));
    wire.extend_from_slice(&record(6, b""));
    wire.extend_from_slice(&record(3, &[0, 0, 0, 0, 0, 0, 0, 0]));
    events.extend(parser.feed(&wire).expect("response"));
    parser.finish().expect("complete");
    let mut body = Vec::new();
    let mut status = None;
    for event in events {
        match event {
            FastCgiResponseEvent::Head(head) => status = Some(head.status),
            FastCgiResponseEvent::Body(mut chunk) => body.append(&mut chunk),
            FastCgiResponseEvent::Stderr(_) | FastCgiResponseEvent::End(_) => {}
        }
    }
    assert_eq!(status, Some(200));
    assert_eq!(body, b"hi!");
}

#[test]
fn scgi_and_uwsgi_responses_share_the_header_and_body_contract() {
    for (protocol, head) in [
        ("scgi", &b"Status: 201 Created\r\nX-A: 1\r\n\r\n"[..]),
        ("uwsgi", &b"HTTP/1.1 201 Created\r\nX-A: 1\r\n\r\n"[..]),
    ] {
        if protocol == "scgi" {
            let mut parser = ScgiResponseParser::new(Limits::default());
            let events = parser.feed(head).expect("scgi head");
            let status = events
                .iter()
                .find_map(|event| match event {
                    ScgiResponseEvent::Head(head) => Some(head.status),
                    ScgiResponseEvent::Body(_) => None,
                })
                .expect("head");
            assert_eq!(status, 201);
            parser.finish().expect("scgi complete");
        } else {
            let mut parser = UwsgiResponseParser::new(Limits::default());
            let events = parser.feed(head).expect("uwsgi head");
            let status = events
                .iter()
                .find_map(|event| match event {
                    UwsgiResponseEvent::Head(head) => Some(head.status),
                    UwsgiResponseEvent::Body(_) => None,
                })
                .expect("head");
            assert_eq!(status, 201);
            parser.finish().expect("uwsgi complete");
        }
    }
}

#[test]
fn memcached_get_and_set_round_trip() {
    assert_eq!(
        encode_get("user:42", &Limits::default()).expect("get"),
        b"get user:42\r\n".to_vec()
    );
    assert_eq!(
        encode_set("user:42", b"payload", 0, 30, &Limits::default()).expect("set"),
        b"set user:42 0 30 7\r\npayload\r\n".to_vec()
    );

    match parse_get_response(
        b"VALUE user:42 0 7\r\npayload\r\nEND\r\n",
        &Limits::default(),
    )
    .expect("hit")
    {
        MemcachedGetResponse::Hit(value) => {
            assert!(value.matches_key("user:42"));
            assert_eq!(value.data, b"payload");
        }
        MemcachedGetResponse::Miss => panic!("expected a hit"),
    }
    assert_eq!(
        parse_get_response(b"END\r\n", &Limits::default()).expect("miss"),
        MemcachedGetResponse::Miss
    );

    let mut parser = MemcachedResponseParser::new(Limits::default());
    let events = parser.feed(b"STORED\r\n").expect("stored");
    parser.finish().expect("complete");
    assert_eq!(events, vec![MemcachedResponseEvent::Stored]);
}

#[test]
fn grpc_call_plan_frames_and_trailers() {
    let headers = grpc_request_headers(
        "/sdkwork.items.v1.Items/Get",
        "api.example.com",
        "https",
        Some(GrpcTimeout::from_millis(1500).expect("timeout")),
        &[("x-tenant".to_owned(), "acme".to_owned())],
        &Limits::default(),
    )
    .expect("grpc headers");
    assert!(headers
        .iter()
        .any(|(name, value)| name == "content-type" && value == "application/grpc"));
    assert!(headers
        .iter()
        .any(|(name, value)| name == "te" && value == "trailers"));
    assert!(headers
        .iter()
        .any(|(name, value)| name == "grpc-timeout" && value == "1500m"));

    let request_frame = encode_message(b"request-proto", &Limits::default()).expect("frame");
    let response_frame = encode_message(b"response-proto", &Limits::default()).expect("frame");
    let mut decoder = GrpcMessageDecoder::new(Limits::default());
    let mut payloads = Vec::new();
    for byte in request_frame.iter().chain(response_frame.iter()) {
        for frame in decoder.feed(std::slice::from_ref(byte)).expect("feed") {
            payloads.extend(frame.payload);
        }
    }
    decoder.finish().expect("complete");
    assert_eq!(payloads, b"request-protoresponse-proto".to_vec());

    let trailer = parse_grpc_trailers(
        &[
            ("grpc-status".to_owned(), "0".to_owned()),
            ("grpc-message".to_owned(), "ok%20done".to_owned()),
        ],
        &Limits::default(),
    )
    .expect("trailers");
    assert!(trailer.is_ok());
    assert_eq!(trailer.message.as_deref(), Some("ok done"));
}

/// Every parser/decoder must reject each prefix of a valid frame without
/// panicking. This is the crate's adversarial truncation contract.
#[test]
fn truncated_prefixes_fail_closed_for_every_decoder() {
    let limits = Limits::default();

    let fastcgi = {
        let mut parser = FastCgiResponseParser::new(limits);
        parser
            .feed(&fastcgi_stdout(b"Status: 200 OK\r\n\r\nbody"))
            .expect("prefix response");
        parser
    };
    assert!(fastcgi.finish().is_err());

    let fastcgi_wire = fastcgi_stdout(b"Status: 200 OK\r\n\r\n");
    for end in 0..fastcgi_wire.len() {
        let mut parser = FastCgiResponseParser::new(limits);
        let events = parser.feed(&fastcgi_wire[..end]);
        assert!(events.is_ok(), "fastcgi prefix {end} must not panic");
        assert!(
            parser.finish().is_err(),
            "fastcgi prefix {end} must not claim completeness"
        );
    }

    let memcached_wire = b"VALUE k 0 3\r\nabc\r\nEND\r\n";
    for end in 0..memcached_wire.len() {
        let mut parser = MemcachedResponseParser::new(limits);
        let events = parser.feed(&memcached_wire[..end]);
        assert!(events.is_ok(), "memcached prefix {end} must not panic");
        assert!(
            parser.finish().is_err(),
            "memcached prefix {end} must not claim completeness"
        );
    }

    let grpc_wire = encode_message(b"payload", &limits).expect("frame");
    // An empty gRPC body is a complete (zero-message) stream; every non-empty
    // prefix of one frame is incomplete.
    for end in 1..grpc_wire.len() {
        let mut decoder = GrpcMessageDecoder::new(limits);
        assert!(decoder.feed(&grpc_wire[..end]).is_ok());
        assert!(decoder.finish().is_err(), "grpc prefix {end}");
    }

    let scgi_wire = b"Status: 200 OK\r\n\r\n";
    for end in 0..scgi_wire.len() {
        let mut parser = ScgiResponseParser::new(limits);
        assert!(parser.feed(&scgi_wire[..end]).is_ok());
        assert!(parser.finish().is_err(), "scgi prefix {end}");
    }

    let uwsgi_wire = b"HTTP/1.1 200 OK\r\n\r\n";
    for end in 0..uwsgi_wire.len() {
        let mut parser = UwsgiResponseParser::new(limits);
        assert!(parser.feed(&uwsgi_wire[..end]).is_ok());
        assert!(parser.finish().is_err(), "uwsgi prefix {end}");
    }
}

/// Malformed bytes must produce typed errors, never a panic.
#[test]
fn adversarial_bytes_produce_typed_errors() {
    let limits = Limits::default();
    // A decoder may buffer adversarial bytes, but it must never report them as
    // a complete response (`feed` succeeds only if `finish` then fails).
    let garbage: Vec<u8> = (0u8..=255).cycle().take(4096).collect();

    let mut parser = FastCgiResponseParser::new(limits);
    if parser.feed(&garbage).is_ok() {
        assert!(parser.finish().is_err());
    }

    let mut parser = ScgiResponseParser::new(limits);
    if parser.feed(&garbage).is_ok() {
        assert!(parser.finish().is_err());
    }

    let mut parser = UwsgiResponseParser::new(limits);
    if parser.feed(&garbage).is_ok() {
        assert!(parser.finish().is_err());
    }

    let mut parser = MemcachedResponseParser::new(limits);
    if parser.feed(&garbage).is_ok() {
        assert!(parser.finish().is_err());
    }

    // Structurally invalid inputs are rejected at `feed` time.
    let mut parser = FastCgiResponseParser::new(limits);
    assert!(parser.feed(&[2, 6, 0, 1, 0, 0, 0, 0]).is_err());
    let mut parser = MemcachedResponseParser::new(limits);
    assert!(parser.feed(b"BOGUS\r\n").is_err());
    let mut parser = ScgiResponseParser::new(limits);
    assert!(parser.feed(&vec![b'A'; 40_000]).is_err());
    let mut parser = UwsgiResponseParser::new(limits);
    assert!(parser.feed(&vec![b'A'; 40_000]).is_err());
    let mut decoder = GrpcMessageDecoder::new(limits);
    let error = decoder.feed(&[0xff; 5]).expect_err("garbage grpc");
    assert!(matches!(
        error,
        Error::Protocol { .. } | Error::LimitExceeded { .. }
    ));

    // The FastCGI request-id allocator is bounded too.
    let mut allocator = RequestIdAllocator::new();
    for _ in 0..u16::MAX {
        allocator.allocate().expect("id available");
    }
    assert!(matches!(
        allocator.allocate(),
        Err(Error::RequestIdExhausted)
    ));
}

fn fastcgi_stdout(content: &[u8]) -> Vec<u8> {
    let padding = (8 - content.len() % 8) % 8;
    let mut wire = Vec::new();
    wire.push(1);
    wire.push(6);
    wire.extend_from_slice(&1u16.to_be_bytes());
    wire.extend_from_slice(&(content.len() as u16).to_be_bytes());
    wire.push(padding as u8);
    wire.push(0);
    wire.extend_from_slice(content);
    wire.resize(wire.len() + padding, 0);
    wire
}
