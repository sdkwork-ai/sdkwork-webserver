//! Deterministic gRPC-over-HTTP/2 responder for the nginx differential slice.
//!
//! The differential runner starts one instance on a cleartext port (`grpc://`)
//! and, when a certificate/key pair is supplied, one instance on a TLS port
//! (`grpcs://`). Behaviour is keyed by `:path`, so both the real-nginx oracle and
//! the Rust data plane observe byte-identical upstream answers:
//!
//! | path | behaviour |
//! | --- | --- |
//! | `/grpc.test.Echo/Unary` | echo the received gRPC messages, trailers `grpc-status: 0` |
//! | `/grpc.test.Echo/Error` | trailers-only response with `grpc-status: 5` |
//! | `/grpc.test.Echo/StatusError` | one message, trailers `grpc-status: 3` |
//! | `/grpc.test.Echo/Stream` | three length-prefixed messages, trailers `grpc-status: 0` |
//! | `/grpc.test.Echo/Headers` | one message carrying the received headers as JSON |
//! | `/grpc.test.Echo/Hide` | response headers `X-Mock-Hide`/`X-Mock-Pass`/`X-Mock-Normal` |
//! | `/grpc.test.Echo/BadTrailers` | data without trailers |
//! | `/grpc.test.Echo/BadContentType` | `content-type: text/plain` instead of grpc |
//! | `/grpc.test.Echo/Slow` | five-second delay before the response headers |
//!
//! gRPC message framing is `1` flag byte + 4-byte big-endian length + payload.
//! The mock frames its own payloads; it does not parse the request payload, so
//! the echo is byte-identical to what the proxy forwarded.

use std::{env, io, net::SocketAddr, sync::Arc, time::Duration};

use bytes::{BufMut, Bytes, BytesMut};
use h2::server::{self, SendResponse};
use http::{HeaderMap, HeaderValue, Request, Response, StatusCode};
use rustls::{
    pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer},
    ServerConfig,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpListener,
};
use tokio_rustls::TlsAcceptor;

const MAXIMUM_REQUEST_BYTES: usize = 4 * 1024 * 1024;
const SLOW_DELAY: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    match run(arguments).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("sdkwork-webserver-grpc-mock: {message}");
            std::process::ExitCode::from(2)
        }
    }
}

async fn run(arguments: Vec<String>) -> Result<(), String> {
    let options = Options::parse(arguments)?;
    // Bind every listener before serving, so a port conflict is a startup
    // error with a nonzero exit code instead of a background panic.
    if let Some(port) = options.plain_port {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .await
            .map_err(|error| format!("cannot bind 127.0.0.1:{port}: {error}"))?;
        spawn_connections(listener, None);
    }
    if !options.tls_ports.is_empty() {
        let (certificate, key) = options
            .tls_material()
            .ok_or_else(|| "--tls-port requires --cert and --key".to_owned())?;
        let mut config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certificate, key)
            .map_err(|error| format!("invalid TLS material: {error}"))?;
        config.alpn_protocols = vec![b"h2".to_vec()];
        let acceptor = Arc::new(TlsAcceptor::from(Arc::new(config)));
        for port in options.tls_ports.clone() {
            let listener = TcpListener::bind(("127.0.0.1", port))
                .await
                .map_err(|error| format!("cannot bind 127.0.0.1:{port}: {error}"))?;
            spawn_connections(listener, Some(acceptor.clone()));
        }
    }
    // The differential runner owns the process lifetime.
    std::future::pending::<()>().await;
    Ok(())
}

#[derive(Clone)]
struct Options {
    plain_port: Option<u16>,
    /// One listener per TLS port; the differential fixture uses a second port
    /// for the untrusted-CA case so nginx's upstream connection reuse cannot
    /// carry a verified connection into it.
    tls_ports: Vec<u16>,
    certificate: Option<String>,
    key: Option<String>,
}

impl Options {
    fn parse(arguments: Vec<String>) -> Result<Self, String> {
        let mut options = Self {
            plain_port: None,
            tls_ports: Vec::new(),
            certificate: None,
            key: None,
        };
        let mut iterator = arguments.into_iter();
        while let Some(argument) = iterator.next() {
            let mut value = || {
                iterator
                    .next()
                    .ok_or_else(|| format!("{argument} requires a value"))
            };
            match argument.as_str() {
                "--plain-port" => {
                    options.plain_port = Some(
                        value()?
                            .parse()
                            .map_err(|_| "--plain-port must be a port number".to_owned())?,
                    )
                }
                "--tls-port" => options.tls_ports.push(
                    value()?
                        .parse()
                        .map_err(|_| "--tls-port must be a port number".to_owned())?,
                ),
                "--cert" => options.certificate = Some(value()?),
                "--key" => options.key = Some(value()?),
                other => return Err(format!("unknown argument `{other}`")),
            }
        }
        if options.plain_port.is_none() && options.tls_ports.is_empty() {
            return Err("at least one of --plain-port/--tls-port is required".to_owned());
        }
        Ok(options)
    }

    fn tls_material(&self) -> Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
        let certificate_pem = std::fs::read(self.certificate.as_deref()?).ok()?;
        let key_pem = std::fs::read(self.key.as_deref()?).ok()?;
        let certificates = CertificateDer::pem_slice_iter(&certificate_pem)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        let key = PrivateKeyDer::from_pem_slice(&key_pem).ok()?;
        Some((certificates, key))
    }
}

fn spawn_connections(listener: TcpListener, tls: Option<Arc<TlsAcceptor>>) {
    // Spawned owner: the accept loop lives for the process lifetime and is
    // stopped by the differential runner that owns this test-only binary.
    tokio::spawn(async move {
        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(accepted) => accepted,
                Err(error) => {
                    eprintln!("sdkwork-webserver-grpc-mock: accept failed: {error}");
                    continue;
                }
            };
            let tls = tls.clone();
            tokio::spawn(async move {
                match tls {
                    Some(acceptor) => match acceptor.accept(stream).await {
                        Ok(stream) => serve_connection(stream, peer).await,
                        Err(error) => {
                            eprintln!("sdkwork-webserver-grpc-mock: TLS handshake failed: {error}");
                        }
                    },
                    None => serve_connection(stream, peer).await,
                }
            });
        }
    });
}

async fn serve_connection<S>(stream: S, peer: SocketAddr)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let mut connection = match server::handshake(stream).await {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("sdkwork-webserver-grpc-mock: HTTP/2 handshake with {peer} failed: {error}");
            return;
        }
    };
    while let Some(accepted) = connection.accept().await {
        let (request, respond) = match accepted {
            Ok(accepted) => accepted,
            Err(error) => {
                eprintln!("sdkwork-webserver-grpc-mock: stream error from {peer}: {error}");
                break;
            }
        };
        tokio::spawn(async move {
            if let Err(error) = handle_request(request, respond).await {
                eprintln!("sdkwork-webserver-grpc-mock: request failed: {error}");
            }
        });
    }
}

fn internal_error(message: &str) -> io::Error {
    io::Error::other(message.to_owned())
}

async fn handle_request(
    request: Request<h2::RecvStream>,
    mut respond: SendResponse<Bytes>,
) -> Result<(), io::Error> {
    let method = request.method().clone();
    let request_path = request
        .uri()
        .path_and_query()
        .map(|value| value.as_str().to_owned())
        .unwrap_or_default();
    let path = request.uri().path().to_owned();
    let authority = request
        .uri()
        .authority()
        .map(|authority| authority.to_string())
        .unwrap_or_default();
    let scheme = request.uri().scheme_str().unwrap_or_default().to_owned();
    let headers = request.headers().clone();
    let mut body = request.into_body();
    let mut received = BytesMut::new();
    while let Some(chunk) = body.data().await {
        let chunk = chunk.map_err(|error| internal_error(&format!("read DATA: {error}")))?;
        if received.len() + chunk.len() > MAXIMUM_REQUEST_BYTES {
            return Err(internal_error("request body exceeded the mock limit"));
        }
        received.extend_from_slice(&chunk);
    }

    // Dispatch on the final path segment so a `grpc_pass` URI part or a
    // pass-through location prefix both reach the same behaviour.
    let behaviour = path.rsplit('/').next().unwrap_or_default();
    match behaviour {
        // The TLS and intercept probe locations forward their own path so nginx
        // offers the same mock behaviour under a transport/policy variant.
        "Unary" | "TlsOk" | "TlsUntrusted" => {
            response_with_messages(&mut respond, &[received.freeze()], 0, None)
        }
        "Error" => trailers_only(&mut respond, 5, Some("not%20found")),
        "StatusError" | "Intercept" => response_with_messages(
            &mut respond,
            &[frame(b"failed")],
            3,
            Some("invalid%20argument"),
        ),
        "Stream" => response_with_messages(
            &mut respond,
            &[frame(b"one"), frame(b"two"), frame(b"three")],
            0,
            None,
        ),
        "Headers" => {
            let payload = serde_json::to_vec(&json_object(vec![
                (
                    "method".to_owned(),
                    serde_json::Value::String(method.as_str().to_owned()),
                ),
                (
                    "path".to_owned(),
                    serde_json::Value::String(request_path.clone()),
                ),
                ("authority".to_owned(), serde_json::Value::String(authority)),
                ("scheme".to_owned(), serde_json::Value::String(scheme)),
                (
                    "headers".to_owned(),
                    serde_json::Value::Array(
                        headers_as_json(&headers)
                            .into_iter()
                            .map(|(name, value)| {
                                serde_json::Value::Array(vec![
                                    serde_json::Value::String(name),
                                    serde_json::Value::String(value),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]))
            .map_err(|error| internal_error(&format!("encode headers: {error}")))?;
            response_with_messages(&mut respond, &[frame(&payload)], 0, None)
        }
        "HideTrailer" => {
            let mut response = grpc_headers();
            response
                .headers_mut()
                .insert("x-mock-hide", HeaderValue::from_static("hide-me"));
            let mut send = respond
                .send_response(response, false)
                .map_err(|error| internal_error(&format!("send HEADERS: {error}")))?;
            send.send_data(frame(b"hide-trailer"), false)
                .map_err(|error| internal_error(&format!("send DATA: {error}")))?;
            let mut trailer_map = trailers(0, None);
            trailer_map.insert(
                "x-mock-trailer-hide",
                HeaderValue::from_static("hide-trailer"),
            );
            trailer_map.insert(
                "x-mock-trailer-pass",
                HeaderValue::from_static("pass-trailer"),
            );
            send.send_trailers(trailer_map)
                .map_err(|error| internal_error(&format!("send trailers: {error}")))?;
            Ok(())
        }
        "Hide" => {
            let mut response = Response::new(());
            *response.status_mut() = StatusCode::OK;
            response
                .headers_mut()
                .insert("content-type", HeaderValue::from_static("application/grpc"));
            response
                .headers_mut()
                .insert("x-mock-hide", HeaderValue::from_static("hide-me"));
            response
                .headers_mut()
                .insert("x-mock-pass", HeaderValue::from_static("pass-me"));
            response
                .headers_mut()
                .insert("x-mock-normal", HeaderValue::from_static("normal"));
            let mut send = respond
                .send_response(response, false)
                .map_err(|error| internal_error(&format!("send HEADERS: {error}")))?;
            send.send_data(frame(b"hide"), false)
                .map_err(|error| internal_error(&format!("send DATA: {error}")))?;
            send.send_trailers(trailers(0, None))
                .map_err(|error| internal_error(&format!("send trailers: {error}")))?;
            Ok(())
        }
        "BadTrailers" => {
            let mut response = Response::new(());
            *response.status_mut() = StatusCode::OK;
            response
                .headers_mut()
                .insert("content-type", HeaderValue::from_static("application/grpc"));
            let mut send = respond
                .send_response(response, false)
                .map_err(|error| internal_error(&format!("send HEADERS: {error}")))?;
            send.send_data(frame(b"x"), true)
                .map_err(|error| internal_error(&format!("send DATA: {error}")))?;
            Ok(())
        }
        "BadContentType" => {
            let mut response = Response::new(());
            *response.status_mut() = StatusCode::OK;
            response
                .headers_mut()
                .insert("content-type", HeaderValue::from_static("text/plain"));
            let mut send = respond
                .send_response(response, false)
                .map_err(|error| internal_error(&format!("send HEADERS: {error}")))?;
            send.send_data(frame(b"bad"), false)
                .map_err(|error| internal_error(&format!("send DATA: {error}")))?;
            send.send_trailers(trailers(0, None))
                .map_err(|error| internal_error(&format!("send trailers: {error}")))?;
            Ok(())
        }
        "HttpStatus" | "InterceptHttp" => {
            let mut response = Response::new(());
            *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
            response
                .headers_mut()
                .insert("content-type", HeaderValue::from_static("application/grpc"));
            let mut send = respond
                .send_response(response, false)
                .map_err(|error| internal_error(&format!("send HEADERS: {error}")))?;
            send.send_data(frame(b"http-status"), true)
                .map_err(|error| internal_error(&format!("send DATA: {error}")))?;
            Ok(())
        }
        "BadStatusTrailer" => {
            let mut send = respond
                .send_response(grpc_headers(), false)
                .map_err(|error| internal_error(&format!("send HEADERS: {error}")))?;
            send.send_data(frame(b"bad-status"), false)
                .map_err(|error| internal_error(&format!("send DATA: {error}")))?;
            let mut trailers = HeaderMap::new();
            trailers.insert("grpc-status", HeaderValue::from_static("abc"));
            send.send_trailers(trailers)
                .map_err(|error| internal_error(&format!("send trailers: {error}")))?;
            Ok(())
        }
        "Slow" => {
            tokio::time::sleep(SLOW_DELAY).await;
            trailers_only(&mut respond, 0, None)
        }
        other => {
            eprintln!("sdkwork-webserver-grpc-mock: unknown path `{other}`");
            trailers_only(&mut respond, 12, Some("unimplemented"))
        }
    }
}

fn frame(payload: &[u8]) -> Bytes {
    let mut framed = BytesMut::with_capacity(5 + payload.len());
    framed.put_u8(0);
    framed.put_u32(payload.len() as u32);
    framed.extend_from_slice(payload);
    framed.freeze()
}

fn trailers(status: u32, message: Option<&str>) -> HeaderMap {
    let mut trailers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(&status.to_string()) {
        trailers.insert("grpc-status", value);
    }
    if let Some(message) = message {
        if let Ok(value) = HeaderValue::from_str(message) {
            trailers.insert("grpc-message", value);
        }
    }
    trailers
}

fn grpc_headers() -> Response<()> {
    let mut response = Response::new(());
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert("content-type", HeaderValue::from_static("application/grpc"));
    response
}

fn response_with_messages(
    respond: &mut SendResponse<Bytes>,
    chunks: &[Bytes],
    status: u32,
    message: Option<&str>,
) -> Result<(), io::Error> {
    let mut send = respond
        .send_response(grpc_headers(), false)
        .map_err(|error| internal_error(&format!("send HEADERS: {error}")))?;
    for chunk in chunks {
        send.send_data(chunk.clone(), false)
            .map_err(|error| internal_error(&format!("send DATA: {error}")))?;
    }
    send.send_trailers(trailers(status, message))
        .map_err(|error| internal_error(&format!("send trailers: {error}")))?;
    Ok(())
}

fn trailers_only(
    respond: &mut SendResponse<Bytes>,
    status: u32,
    message: Option<&str>,
) -> Result<(), io::Error> {
    let mut response = grpc_headers();
    for (name, value) in trailers(status, message) {
        if let Some(name) = name {
            response.headers_mut().insert(name, value);
        }
    }
    respond
        .send_response(response, true)
        .map(|_| ())
        .map_err(|error| internal_error(&format!("send trailers-only HEADERS: {error}")))
}

fn json_object(entries: Vec<(String, serde_json::Value)>) -> serde_json::Value {
    serde_json::Value::Object(entries.into_iter().collect())
}

fn headers_as_json(headers: &HeaderMap) -> Vec<(String, String)> {
    let mut fields = headers
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_owned(),
                value.to_str().unwrap_or_default().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    fields.sort();
    fields
}
