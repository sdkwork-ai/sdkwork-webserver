//! Cross-protocol dialogue tests: full relay simulations, STARTTLS boundaries,
//! dot-stuffing/XCLIENT/PROXY composition, and adversarial no-panic sweeps.

use sdkwork_webserver_mail_protocol::{
    auth::{decode_plain, encode_plain},
    dot_stuffing::DotStuffingReader,
    frontend::{FrontendAction, FrontendSession, FrontendState, MailProtocol},
    imap::{ImapClient, ImapEvent},
    line::{encode_line, LineReader},
    pop3::{Pop3Client, Pop3State},
    proxy_protocol::ProxyProtocolV1,
    relay::{bootstrap_lines, BackendBootstrap, BackendRelay, RelayPhase},
    smtp::{SmtpClient, SmtpPipelineCommand, SmtpState},
    xclient::{XclientAttribute, XclientRequest},
    Error, Limits,
};

#[test]
fn pop3_capa_dialog_relays_a_dot_stuffed_capability_list() {
    let limits = Limits::default();
    let mut client = Pop3Client::new(&limits);
    client.greeting("+OK ready").expect("greeting");
    client.capa().expect("capa");
    client.on_line("+OK capabilities follow").expect("capa ok");
    let mut reader = DotStuffingReader::new("pop3", &limits);
    let lines = reader
        .feed(b"USER\r\nSASL PLAIN LOGIN\r\nTOP\r\n.\r\n")
        .expect("payload");
    reader.finish().expect("terminated");
    let _ = lines;
    assert_eq!(reader.lines(), ["USER", "SASL PLAIN LOGIN", "TOP"]);
    assert_eq!(client.state(), Pop3State::Authorization);
}

#[test]
fn smtp_backend_bootstrap_sends_ehlo_xclient_and_proxy_header() {
    let limits = Limits::default();
    let mut relay = BackendRelay::new(MailProtocol::Smtp, &limits);
    relay.on_greeting("220 backend ESMTP").expect("greeting");
    assert_eq!(relay.phase(), RelayPhase::Ready);

    let mut xclient = XclientRequest::new();
    xclient
        .push(
            XclientAttribute::Addr("203.0.113.7".parse().expect("ip")),
            &limits,
        )
        .expect("addr");
    xclient
        .push(XclientAttribute::Login("alice".to_owned()), &limits)
        .expect("login");
    xclient
        .push(
            XclientAttribute::Name("mail.example.com".to_owned()),
            &limits,
        )
        .expect("name");
    let bootstrap = BackendBootstrap {
        ehlo_name: Some("frontend.example.com".to_owned()),
        xclient: Some(xclient),
        capability_probe: false,
    };
    let lines = bootstrap_lines(MailProtocol::Smtp, &bootstrap, &limits).expect("bootstrap");
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], b"EHLO frontend.example.com\r\n");
    assert_eq!(
        lines[1],
        b"XCLIENT ADDR=203.0.113.7 LOGIN=alice NAME=mail.example.com\r\n"
    );

    let header = ProxyProtocolV1::tcp(
        "192.0.2.10:40000".parse().expect("src"),
        "192.0.2.20:25".parse().expect("dst"),
    )
    .expect("proxy header");
    let encoded = header.encode(&limits).expect("encode");
    assert_eq!(encoded, b"PROXY TCP4 192.0.2.10 192.0.2.20 40000 25\r\n");
    let relayed = relay
        .relay_line("MAIL FROM:<alice@example.com>")
        .expect("relay");
    assert_eq!(relayed, b"MAIL FROM:<alice@example.com>\r\n");
    relay.finish().expect("finish");
    assert!(relay.relay_line("QUIT").is_err());
}

#[test]
fn imap_starttls_then_login_and_literal_collection() {
    let limits = Limits::default();
    let mut client = ImapClient::new(&limits);
    assert!(matches!(
        client.on_line("* OK ready").expect("greeting"),
        ImapEvent::Greeting { .. }
    ));
    let tag = client.next_tag().expect("tag");
    client.starttls(&tag).expect("starttls");
    assert!(matches!(
        client
            .on_line(&format!("{tag} OK begin TLS"))
            .expect("reply"),
        ImapEvent::Tagged { .. }
    ));
    client.tls_established().expect("tls");
    let tag = client.next_tag().expect("tag");
    client.login(&tag, "alice", "hunter2").expect("login");
    assert!(matches!(
        client
            .on_line(&format!("{tag} OK logged in"))
            .expect("reply"),
        ImapEvent::Tagged { .. }
    ));
    let event = client.on_line("* 1 EXISTS").expect("untagged");
    assert!(matches!(event, ImapEvent::Untagged(_)));
}

#[test]
fn frontend_transitions_match_the_per_protocol_happy_path() {
    for protocol in [MailProtocol::Pop3, MailProtocol::Imap, MailProtocol::Smtp] {
        let mut session = FrontendSession::new(protocol, false);
        session
            .apply(FrontendAction::BeginAuth, None, &Limits::default())
            .expect("begin auth");
        session
            .apply(
                FrontendAction::CompleteAuth,
                Some("alice"),
                &Limits::default(),
            )
            .expect("complete");
        assert_eq!(session.authenticated_user(), Some("alice"));
        session
            .apply(FrontendAction::BeginRelay, None, &Limits::default())
            .expect("relay");
        assert_eq!(session.state(), FrontendState::Relaying);
        session
            .apply(FrontendAction::Close, None, &Limits::default())
            .expect("close");
    }
}

#[test]
fn dot_stuffing_and_auth_helpers_round_trip_together() {
    let limits = Limits::default();
    let encoded = encode_plain("alice", "p@ss", &limits).expect("plain");
    let decoded = decode_plain(&encoded, &limits).expect("decode");
    assert_eq!(decoded.authentication_id, "alice");

    let body = sdkwork_webserver_mail_protocol::dot_stuffing::encode_multiline(
        [".first", "second", "."],
        &limits,
    )
    .expect("encode");
    assert_eq!(body, b"..first\r\nsecond\r\n..\r\n.\r\n");
    let decoded_lines =
        sdkwork_webserver_mail_protocol::dot_stuffing::decode_multiline("smtp", &body, &limits)
            .expect("decode");
    assert_eq!(decoded_lines, [".first", "second", "."]);
}

#[test]
fn smtp_pipeline_reaches_data_and_completes() {
    let limits = Limits::default();
    let mut client = SmtpClient::new(&limits);
    client.greeting("220 ready").expect("greeting");
    let batch = client
        .pipeline(&[
            SmtpPipelineCommand::MailFrom("alice@example.com".to_owned()),
            SmtpPipelineCommand::RcptTo("bob@example.com".to_owned()),
        ])
        .expect("pipeline");
    assert_eq!(
        batch,
        b"MAIL FROM:<alice@example.com>\r\nRCPT TO:<bob@example.com>\r\n"
    );
    client
        .on_line("250 ok")
        .expect("mail reply")
        .expect("reply");
    client
        .on_line("250 ok")
        .expect("rcpt reply")
        .expect("reply");
    client.data().expect("data");
    client.on_line("354 go").expect("354").expect("reply");
    let body = client.data_body([".dot", "text"]).expect("data body");
    assert_eq!(body, b"..dot\r\ntext\r\n.\r\n");
    client
        .on_line("250 queued")
        .expect("queued")
        .expect("reply");
    assert_eq!(client.state(), SmtpState::Ready);
}

#[test]
fn adversarial_bytes_never_panic_across_the_substrate() {
    let limits = Limits::default();
    let corpus: [&[u8]; 9] = [
        b"",
        b"\r\n",
        b".\r\n",
        b"\xff\xfe\xfd\r\n",
        &[0u8; 32],
        b"{99999999999999999999}\r\n",
        b"PROXY TCP4 ::::\r\n",
        b"XCLIENT ADDR=%%%\r\n",
        b"AUTH PLAIN !!!!",
    ];
    for input in corpus {
        let mut lines = LineReader::new("imap", &limits);
        let _ = lines.feed(input);
        let _ = lines.finish();
        let mut dots = DotStuffingReader::new("smtp", &limits);
        let _ = dots.feed(input);
        let _ = dots.finish();
        let _ = ProxyProtocolV1::parse(input, &limits);
        let _ = XclientRequest::parse(
            core::str::from_utf8(input).unwrap_or("XCLIENT ADDR=invalid"),
            &limits,
        );
        let mut imap = ImapClient::new(&limits);
        if let Ok(text) = core::str::from_utf8(input) {
            let _ = imap.on_line(text);
        }
        let mut pop3 = Pop3Client::new(&limits);
        if let Ok(text) = core::str::from_utf8(input) {
            let _ = pop3.on_line(text);
        }
        let mut smtp = SmtpClient::new(&limits);
        if let Ok(text) = core::str::from_utf8(input) {
            let _ = smtp.on_line(text);
        }
    }
}

#[test]
fn every_decoder_rejects_truncated_and_oversized_forms() {
    let limits = Limits {
        max_line_bytes: 32,
        max_literal_bytes: 4,
        ..Limits::default()
    };
    let mut reader = LineReader::new("smtp", &limits);
    assert!(matches!(
        reader.feed(&[b'x'; 40]),
        Err(Error::LineTooLong { .. })
    ));

    let mut imap = ImapClient::new(&limits);
    imap.on_line("* OK ready").expect("greeting");
    assert!(matches!(
        imap.on_line("* 1 FETCH (BODY {5}"),
        Err(Error::LiteralTooLarge { .. })
    ));

    assert!(encode_line(&"x".repeat(40), &limits).is_err());
    assert!(smtp_data_oversized().is_err());
}

fn smtp_data_oversized() -> sdkwork_webserver_mail_protocol::Result<()> {
    let limits = Limits {
        max_smtp_data_bytes: 4,
        ..Limits::default()
    };
    let mut client = SmtpClient::new(&limits);
    client.greeting("220 ready").expect("greeting");
    client.data().expect("data");
    client.on_line("354 go").expect("354").expect("reply");
    client.data_body(["too long"]).map(|_| ())
}
