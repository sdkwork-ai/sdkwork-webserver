//! End-to-end nginx `mail {}` Wave 9B plane: the auth_http exchange, the
//! backend relay with XCLIENT, frontend TLS/STARTTLS and the typed-plane
//! backend TLS extension.
//!
//! The expected wire strings were probed against nginx 1.29.6 (see
//! `tests/nginx/differential-mail` for the committed byte-level proof); these
//! tests cover the paths a byte differential cannot observe (auth_http request
//! contents, backend TLS trust, XCLIENT attributes, bounded retries).

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use sdkwork_api_webserver_standalone_gateway::{run_data_plane_until, DataPlaneError};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
    time::timeout,
};

const READ_TIMEOUT: Duration = Duration::from_secs(5);

fn available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve a port");
    listener.local_addr().expect("port").port()
}

fn write_config(directory: &Path, config: &Value) -> PathBuf {
    let path = directory.join("sdkwork.webserver.config.json");
    std::fs::write(&path, serde_json::to_vec_pretty(config).expect("serialize"))
        .expect("write config");
    path
}

fn mail_config(servers: Vec<Value>) -> Value {
    json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "mail-9b-test",
        "limits": {
            "maxConcurrentRequests": 32,
            "requestTimeoutMs": 5_000,
            "drainTimeoutMs": 1_000,
            "maxConnections": 64
        },
        "listeners": [],
        "resources": [],
        "virtualHosts": [],
        "mail": { "servers": servers }
    })
}

fn mail_server(id: &str, port: u16, protocol: &str, extra: Value) -> Value {
    let mut server = json!({
        "id": id,
        "bind": "127.0.0.1",
        "port": port,
        "protocol": protocol,
        "serverName": "mail.example.com",
        "timeoutMs": 10_000,
    });
    for (key, value) in extra.as_object().expect("extra object") {
        server[key] = value.clone();
    }
    server
}

fn spawn_data_plane(
    config_path: &Path,
) -> (oneshot::Sender<()>, JoinHandle<Result<(), DataPlaneError>>) {
    let compiled = sdkwork_webserver_core::load_and_compile_webserver_config(config_path)
        .expect("compile config");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    (shutdown_tx, task)
}

async fn connect(port: u16) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match TcpStream::connect(("127.0.0.1", port)).await {
            Ok(stream) => return stream,
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => panic!("cannot connect to {port}: {error}"),
        }
    }
}

async fn read_available(stream: &mut TcpStream) -> Vec<u8> {
    let mut buffer = [0_u8; 8192];
    match timeout(READ_TIMEOUT, stream.read(&mut buffer)).await {
        Ok(Ok(read)) => buffer[..read].to_vec(),
        Ok(Err(error)) => panic!("read failed: {error}"),
        Err(_) => panic!("timed out waiting for a mail response"),
    }
}

async fn read_until_closed(stream: &mut TcpStream) -> Vec<u8> {
    let mut collected = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        match timeout(Duration::from_secs(6), stream.read(&mut buffer)).await {
            Ok(Ok(0)) => return collected,
            Ok(Ok(read)) => collected.extend_from_slice(&buffer[..read]),
            Ok(Err(_)) => return collected,
            Err(_) => panic!("mail connection did not close"),
        }
    }
}

/// One recorded auth_http request.
#[derive(Clone, Debug)]
struct AuthRequest {
    head: String,
}

impl AuthRequest {
    fn value(&self, name: &str) -> Option<&str> {
        self.head.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }
}

/// Start a deterministic auth_http service.
///
/// `responder` receives the recorded request and returns the response body.
async fn start_auth_http(
    responder: impl Fn(&AuthRequest) -> String + Send + Sync + 'static,
) -> (u16, Arc<Mutex<Vec<AuthRequest>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind auth http");
    let port = listener.local_addr().expect("addr").port();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&requests);
    let responder = Arc::new(responder);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let requests = Arc::clone(&recorded);
            let responder = Arc::clone(&responder);
            tokio::spawn(async move {
                let mut buffer = Vec::new();
                let mut chunk = [0_u8; 1024];
                loop {
                    let Ok(read) = stream.read(&mut chunk).await else {
                        return;
                    };
                    if read == 0 {
                        return;
                    }
                    buffer.extend_from_slice(&chunk[..read]);
                    if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let head = String::from_utf8_lossy(&buffer)
                    .split("\r\n\r\n")
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                let request = AuthRequest { head };
                requests.lock().expect("lock").push(request.clone());
                let response = responder(&request);
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    (port, requests)
}

/// Start a scripted line backend; every received line is recorded.
async fn start_line_backend(
    greeting: &'static str,
    responder: impl Fn(&str) -> String + Send + Sync + 'static,
) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind backend");
    let port = listener.local_addr().expect("addr").port();
    let lines = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&lines);
    let responder = Arc::new(responder);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let lines = Arc::clone(&recorded);
            let responder = Arc::clone(&responder);
            tokio::spawn(async move {
                let _ = stream.write_all(greeting.as_bytes()).await;
                let mut pending = Vec::new();
                let mut chunk = [0_u8; 1024];
                loop {
                    let Ok(read) = stream.read(&mut chunk).await else {
                        return;
                    };
                    if read == 0 {
                        return;
                    }
                    pending.extend_from_slice(&chunk[..read]);
                    while let Some(position) =
                        pending.windows(2).position(|window| window == b"\r\n")
                    {
                        let line = String::from_utf8_lossy(&pending[..position]).into_owned();
                        pending.drain(..position + 2);
                        lines.lock().expect("lock").push(line.clone());
                        let reply = responder(&line);
                        if reply.is_empty() {
                            continue;
                        }
                        if stream.write_all(reply.as_bytes()).await.is_err() {
                            return;
                        }
                    }
                }
            });
        }
    });
    (port, lines)
}

fn auth_ok_response(backend_port: u16, login: &str) -> String {
    format!(
        "HTTP/1.0 200 OK\r\nAuth-Status: OK\r\nAuth-Server: 127.0.0.1\r\nAuth-Port: {backend_port}\r\nAuth-User: {login}\r\nAuth-Pass: backend-secret\r\n\r\n"
    )
}

#[tokio::test]
async fn pop3_auth_http_success_relays_backend_authentication_and_commands() {
    let (backend_port, backend_lines) = start_line_backend("+OK backend ready\r\n", |line| {
        let verb = line
            .split(' ')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        match verb.as_str() {
            "USER" => "+OK user ok\r\n".to_owned(),
            "PASS" => "+OK logged in\r\n".to_owned(),
            "QUIT" => "+OK bye\r\n".to_owned(),
            _ => format!("+OK echo {line}\r\n"),
        }
    })
    .await;
    let (auth_port, auth_requests) = {
        let backend_port = backend_port;
        start_auth_http(move |request| {
            assert_eq!(request.value("Auth-Protocol"), Some("pop3"));
            assert_eq!(request.value("Auth-User"), Some("alice"));
            assert_eq!(request.value("Auth-Pass"), Some("secret"));
            assert_eq!(request.value("Auth-Login-Attempt"), Some("1"));
            assert!(request.value("Client-IP").is_some());
            assert_eq!(request.value("Client-Host"), None);
            auth_ok_response(backend_port, "backend-alice")
        })
        .await
    };
    let port = available_port();
    let directory = tempfile::tempdir().expect("tempdir");
    let config = mail_config(vec![mail_server(
        "pop3",
        port,
        "pop3",
        json!({ "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth") } }),
    )]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let mut stream = connect(port).await;
    assert_eq!(read_available(&mut stream).await, b"+OK POP3 ready\r\n");
    stream.write_all(b"USER alice\r\n").await.expect("send");
    assert_eq!(read_available(&mut stream).await, b"+OK\r\n");
    stream.write_all(b"PASS secret\r\n").await.expect("send");
    // nginx forwards the backend's PASS reply as the client success response.
    assert_eq!(read_available(&mut stream).await, b"+OK logged in\r\n");
    stream.write_all(b"NOOP\r\n").await.expect("send");
    assert_eq!(read_available(&mut stream).await, b"+OK echo NOOP\r\n");
    stream.write_all(b"QUIT\r\n").await.expect("send");
    assert_eq!(read_available(&mut stream).await, b"+OK bye\r\n");

    let lines = backend_lines.lock().expect("lock").clone();
    assert_eq!(
        lines,
        vec![
            "USER backend-alice".to_owned(),
            "PASS backend-secret".to_owned(),
            "NOOP".to_owned(),
            "QUIT".to_owned()
        ]
    );
    assert_eq!(auth_requests.lock().expect("lock").len(), 1);
    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}

#[tokio::test]
async fn imap_auth_http_success_reuses_the_client_tag_for_the_backend_login() {
    let (backend_port, _backend_lines) = start_line_backend("* OK backend IMAP\r\n", |line| {
        if line.starts_with("a1 LOGIN") {
            "+ go\r\n".to_owned()
        } else if line.starts_with("alice {") {
            "+ next\r\n".to_owned()
        } else if line == "backend-secret" {
            "a1 OK logged in\r\n".to_owned()
        } else if line.contains("LOGOUT") {
            "* BYE\r\na4 OK completed\r\n".to_owned()
        } else {
            format!("{} OK echo\r\n", line.split(' ').next().unwrap_or("a"))
        }
    })
    .await;
    let (auth_port, _requests) =
        start_auth_http(move |_| auth_ok_response(backend_port, "alice")).await;
    let port = available_port();
    let directory = tempfile::tempdir().expect("tempdir");
    let config = mail_config(vec![mail_server(
        "imap",
        port,
        "imap",
        json!({ "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth") } }),
    )]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let mut stream = connect(port).await;
    assert_eq!(read_available(&mut stream).await, b"* OK IMAP4 ready\r\n");
    stream
        .write_all(b"a1 LOGIN alice secret\r\n")
        .await
        .expect("send");
    assert_eq!(read_available(&mut stream).await, b"a1 OK logged in\r\n");
    stream.write_all(b"a2 NOOP\r\n").await.expect("send");
    assert_eq!(read_available(&mut stream).await, b"a2 OK echo\r\n");
    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}

#[tokio::test]
async fn smtp_auth_http_success_sends_xclient_and_synthetic_auth_reply() {
    let (backend_port, backend_lines) = start_line_backend("220 backend ESMTP\r\n", |line| {
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("EHLO") || upper.starts_with("XCLIENT") {
            "250 ok\r\n".to_owned()
        } else if upper.starts_with("QUIT") {
            "221 bye\r\n".to_owned()
        } else {
            "250 ok\r\n".to_owned()
        }
    })
    .await;
    let (auth_port, auth_requests) = {
        let backend_port = backend_port;
        start_auth_http(move |request| {
            assert_eq!(request.value("Auth-Protocol"), Some("smtp"));
            // nginx sends the SMTP client host as `[UNAVAILABLE]` when no mail
            // resolver is configured.
            assert_eq!(request.value("Client-Host"), Some("[UNAVAILABLE]"));
            auth_ok_response(backend_port, "alice")
        })
        .await
    };
    let port = available_port();
    let directory = tempfile::tempdir().expect("tempdir");
    let config = mail_config(vec![mail_server(
        "smtp",
        port,
        "smtp",
        json!({ "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth") } }),
    )]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let mut stream = connect(port).await;
    assert_eq!(
        read_available(&mut stream).await,
        b"220 mail.example.com ESMTP ready\r\n"
    );
    stream
        .write_all(b"EHLO client.example\r\n")
        .await
        .expect("send");
    assert_eq!(
        read_available(&mut stream).await,
        b"250-mail.example.com\r\n250 AUTH PLAIN LOGIN\r\n"
    );
    let encoded = sdkwork_webserver_mail_protocol::auth::encode_plain(
        "alice",
        "secret",
        &sdkwork_webserver_mail_protocol::Limits::default(),
    )
    .expect("encode");
    stream
        .write_all(format!("AUTH PLAIN {encoded}\r\n").as_bytes())
        .await
        .expect("send");
    assert_eq!(read_available(&mut stream).await, b"235 2.0.0 OK\r\n");
    stream.write_all(b"QUIT\r\n").await.expect("send");
    assert_eq!(read_available(&mut stream).await, b"221 bye\r\n");

    let lines = backend_lines.lock().expect("lock").clone();
    assert!(lines.iter().any(|line| line == "EHLO mail.example.com"));
    assert!(lines
        .iter()
        .any(|line| line == "XCLIENT ADDR=127.0.0.1 LOGIN=alice NAME=[UNAVAILABLE]"));
    assert!(lines.iter().any(|line| line == "EHLO client.example"));
    assert!(lines.iter().any(|line| line == "QUIT"));
    assert_eq!(auth_requests.lock().expect("lock").len(), 1);
    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}

#[tokio::test]
async fn auth_http_failure_replies_match_nginx_and_close() {
    let (auth_port, _requests) = start_auth_http(|request| {
        let protocol = request.value("Auth-Protocol").unwrap_or_default();
        if protocol == "smtp" {
            "HTTP/1.0 200 OK\r\nAuth-Status: invalid credentials\r\nAuth-Error-Code: 535 5.7.0\r\n\r\n".to_owned()
        } else {
            "HTTP/1.0 200 OK\r\nAuth-Status: invalid credentials\r\n\r\n".to_owned()
        }
    })
    .await;
    let pop3_port = available_port();
    let smtp_port = available_port();
    let directory = tempfile::tempdir().expect("tempdir");
    let auth = json!({ "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth") } });
    let config = mail_config(vec![
        mail_server("pop3", pop3_port, "pop3", auth.clone()),
        mail_server("smtp", smtp_port, "smtp", auth),
    ]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let mut stream = connect(pop3_port).await;
    assert_eq!(read_available(&mut stream).await, b"+OK POP3 ready\r\n");
    stream.write_all(b"USER alice\r\n").await.expect("send");
    assert_eq!(read_available(&mut stream).await, b"+OK\r\n");
    stream.write_all(b"PASS secret\r\n").await.expect("send");
    assert_eq!(
        read_until_closed(&mut stream).await,
        b"-ERR invalid credentials\r\n"
    );

    let mut stream = connect(smtp_port).await;
    assert_eq!(
        read_available(&mut stream).await,
        b"220 mail.example.com ESMTP ready\r\n"
    );
    stream
        .write_all(b"EHLO client.example\r\n")
        .await
        .expect("send");
    assert_eq!(
        read_available(&mut stream).await,
        b"250-mail.example.com\r\n250 AUTH PLAIN LOGIN\r\n"
    );
    let encoded = sdkwork_webserver_mail_protocol::auth::encode_plain(
        "alice",
        "secret",
        &sdkwork_webserver_mail_protocol::Limits::default(),
    )
    .expect("encode");
    stream
        .write_all(format!("AUTH PLAIN {encoded}\r\n").as_bytes())
        .await
        .expect("send");
    assert_eq!(
        read_until_closed(&mut stream).await,
        b"535 5.7.0 invalid credentials\r\n"
    );
    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}

#[tokio::test]
async fn auth_http_wait_is_retried_and_unreachable_service_closes() {
    let counter = Arc::new(Mutex::new(0u32));
    let responder_counter = Arc::clone(&counter);
    let (backend_port, _lines) = start_line_backend("+OK backend ready\r\n", |line| {
        if line.starts_with("USER") || line.starts_with("PASS") {
            "+OK\r\n".to_owned()
        } else {
            "+OK echo\r\n".to_owned()
        }
    })
    .await;
    let (auth_port, _requests) = {
        let backend_port = backend_port;
        start_auth_http(move |_| {
            let mut attempt = responder_counter.lock().expect("lock");
            *attempt += 1;
            if *attempt == 1 {
                "HTTP/1.0 200 OK\r\nAuth-Status: WAIT\r\nAuth-Wait: 0\r\n\r\n".to_owned()
            } else {
                auth_ok_response(backend_port, "alice")
            }
        })
        .await
    };
    let port = available_port();
    let directory = tempfile::tempdir().expect("tempdir");
    let config = mail_config(vec![mail_server(
        "pop3",
        port,
        "pop3",
        json!({ "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth"), "timeoutMs": 2000 } }),
    )]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let mut stream = connect(port).await;
    assert_eq!(read_available(&mut stream).await, b"+OK POP3 ready\r\n");
    stream.write_all(b"USER alice\r\n").await.expect("send");
    assert_eq!(read_available(&mut stream).await, b"+OK\r\n");
    stream.write_all(b"PASS secret\r\n").await.expect("send");
    assert_eq!(read_available(&mut stream).await, b"+OK\r\n");
    assert_eq!(*counter.lock().expect("lock"), 2, "WAIT retried once");
    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}

#[tokio::test]
async fn unreachable_auth_http_closes_with_internal_server_error() {
    let auth_port = available_port();
    let port = available_port();
    let directory = tempfile::tempdir().expect("tempdir");
    let config = mail_config(vec![mail_server(
        "pop3",
        port,
        "pop3",
        json!({ "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth"), "timeoutMs": 1500 } }),
    )]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let mut stream = connect(port).await;
    assert_eq!(read_available(&mut stream).await, b"+OK POP3 ready\r\n");
    stream.write_all(b"USER alice\r\n").await.expect("send");
    assert_eq!(read_available(&mut stream).await, b"+OK\r\n");
    stream.write_all(b"PASS secret\r\n").await.expect("send");
    assert_eq!(
        read_until_closed(&mut stream).await,
        b"-ERR internal server error\r\n"
    );
    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}

// ---------------------------------------------------------------------------
// TLS: frontend implicit/STARTTLS and the typed-plane backend TLS extension
// ---------------------------------------------------------------------------

fn install_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        // Another test thread may install the provider between the check and
        // the call; an AlreadyInstalled error is therefore not a failure.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }
    assert!(rustls::crypto::CryptoProvider::get_default().is_some());
}

fn generate_certificate() -> (PathBuf, PathBuf) {
    install_crypto_provider();
    let directory = tempfile::tempdir().expect("cert tempdir");
    // Keep the directory alive by leaking the path: tests are short-lived and
    // the files must outlive the config compile + listener preparation.
    let root = directory.keep();
    let params = rcgen::CertificateParams::new(vec!["mail.example.com".to_owned()])
        .expect("certificate parameters");
    let key = rcgen::KeyPair::generate().expect("key");
    let certificate = params.self_signed(&key).expect("self-signed certificate");
    let certificate_path = root.join("mail.pem");
    let key_path = root.join("mail.key");
    std::fs::write(&certificate_path, certificate.pem()).expect("write cert");
    std::fs::write(&key_path, key.serialize_pem()).expect("write key");
    (certificate_path, key_path)
}

async fn start_tls_line_backend(
    certificate_path: &Path,
    key_path: &Path,
    greeting: &'static str,
    responder: impl Fn(&str) -> String + Send + Sync + 'static,
) -> u16 {
    use rustls::pki_types::pem::PemObject;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind tls backend");
    let port = listener.local_addr().expect("addr").port();
    let certificate = std::fs::read(certificate_path).expect("read cert");
    let key = std::fs::read(key_path).expect("read key");
    let chain = rustls::pki_types::CertificateDer::pem_slice_iter(&certificate)
        .collect::<Result<Vec<_>, _>>()
        .expect("cert chain");
    let key = rustls::pki_types::PrivateKeyDer::from_pem_slice(&key).expect("private key");
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .expect("server config");
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let responder = Arc::new(responder);
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let acceptor = acceptor.clone();
            let responder = Arc::clone(&responder);
            tokio::spawn(async move {
                let Ok(mut stream) = acceptor.accept(stream).await else {
                    return;
                };
                let _ = stream.write_all(greeting.as_bytes()).await;
                let mut pending = Vec::new();
                let mut chunk = [0_u8; 1024];
                loop {
                    let Ok(read) = stream.read(&mut chunk).await else {
                        return;
                    };
                    if read == 0 {
                        return;
                    }
                    pending.extend_from_slice(&chunk[..read]);
                    while let Some(position) =
                        pending.windows(2).position(|window| window == b"\r\n")
                    {
                        let line = String::from_utf8_lossy(&pending[..position]).into_owned();
                        pending.drain(..position + 2);
                        let reply = responder(&line);
                        if !reply.is_empty() && stream.write_all(reply.as_bytes()).await.is_err() {
                            return;
                        }
                    }
                }
            });
        }
    });
    port
}

async fn start_tls_auth_http(backend_port: u16) -> u16 {
    let (port, _requests) = start_auth_http(move |_| auth_ok_response(backend_port, "alice")).await;
    port
}

async fn frontend_tls_connect(port: u16) -> tokio_rustls::client::TlsStream<TcpStream> {
    // The fixture certificate is self-signed; the test asserts the proxy's TLS
    // termination, not certificate validation, so verification is disabled.
    let mut config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyCertificate))
        .with_no_client_auth();
    config.alpn_protocols.clear();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let stream = connect(port).await;
    let server_name =
        rustls::pki_types::ServerName::try_from("mail.example.com").expect("server name");
    connector
        .connect(server_name, stream)
        .await
        .expect("TLS connect")
}

#[derive(Debug)]
struct AcceptAnyCertificate;

impl rustls::client::danger::ServerCertVerifier for AcceptAnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _certificate: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _certificate: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

#[tokio::test]
async fn frontend_implicit_tls_terminates_and_shares_auth_http_ssl_facts() {
    let (certificate, key) = generate_certificate();
    let (backend_port, _lines) = start_line_backend("220 backend ESMTP\r\n", |line| {
        if line.to_ascii_uppercase().starts_with("QUIT") {
            "221 bye\r\n".to_owned()
        } else {
            "250 ok\r\n".to_owned()
        }
    })
    .await;
    let auth_port = start_tls_auth_http(backend_port).await;
    let port = available_port();
    let directory = tempfile::tempdir().expect("tempdir");
    let config = mail_config(vec![mail_server(
        "smtp-tls",
        port,
        "smtp",
        json!({
            "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth") },
            "tls": {
                "certificateFile": certificate.to_string_lossy(),
                "privateKeyFile": key.to_string_lossy(),
                "implicit": true,
            }
        }),
    )]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let mut stream = frontend_tls_connect(port).await;
    let mut buffer = [0_u8; 4096];
    let read = timeout(READ_TIMEOUT, stream.read(&mut buffer))
        .await
        .expect("read greeting")
        .expect("greeting");
    assert_eq!(&buffer[..read], b"220 mail.example.com ESMTP ready\r\n");
    stream
        .write_all(b"EHLO client.example\r\n")
        .await
        .expect("ehlo");
    let read = timeout(READ_TIMEOUT, stream.read(&mut buffer))
        .await
        .expect("read ehlo")
        .expect("ehlo reply");
    let reply = String::from_utf8_lossy(&buffer[..read]).into_owned();
    assert!(reply.contains("250-mail.example.com"), "{reply}");
    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}

#[tokio::test]
async fn backend_tls_extension_verifies_the_typed_trust_anchor() {
    let (certificate, key) = generate_certificate();
    let backend_port =
        start_tls_line_backend(&certificate, &key, "+OK backend ready\r\n", |line| {
            let verb = line
                .split(' ')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            match verb.as_str() {
                "USER" => "+OK user\r\n".to_owned(),
                "PASS" => "+OK logged in\r\n".to_owned(),
                _ => "+OK echo\r\n".to_owned(),
            }
        })
        .await;
    let auth_port = start_tls_auth_http(backend_port).await;
    let port = available_port();
    let directory = tempfile::tempdir().expect("tempdir");
    let config = mail_config(vec![mail_server(
        "pop3-backend-tls",
        port,
        "pop3",
        json!({
            "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth") },
            "proxy": {
                "tls": {
                    "verify": true,
                    "trustedCertificateFile": certificate.to_string_lossy(),
                    "serverName": "mail.example.com"
                }
            }
        }),
    )]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let mut stream = connect(port).await;
    assert_eq!(read_available(&mut stream).await, b"+OK POP3 ready\r\n");
    stream.write_all(b"USER alice\r\n").await.expect("send");
    assert_eq!(read_available(&mut stream).await, b"+OK\r\n");
    stream.write_all(b"PASS secret\r\n").await.expect("send");
    assert_eq!(read_available(&mut stream).await, b"+OK logged in\r\n");
    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}

#[tokio::test]
async fn backend_tls_extension_rejects_an_untrusted_anchor() {
    let (certificate, key) = generate_certificate();
    let backend_port = start_tls_line_backend(&certificate, &key, "+OK backend ready\r\n", |_| {
        "+OK\r\n".to_owned()
    })
    .await;
    let auth_port = start_tls_auth_http(backend_port).await;
    let port = available_port();
    let directory = tempfile::tempdir().expect("tempdir");
    let config = mail_config(vec![mail_server(
        "pop3-backend-tls-untrusted",
        port,
        "pop3",
        json!({
            "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth") },
            "proxy": { "tls": { "verify": true, "serverName": "mail.example.com" } }
        }),
    )]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let mut stream = connect(port).await;
    assert_eq!(read_available(&mut stream).await, b"+OK POP3 ready\r\n");
    stream.write_all(b"USER alice\r\n").await.expect("send");
    assert_eq!(read_available(&mut stream).await, b"+OK\r\n");
    stream.write_all(b"PASS secret\r\n").await.expect("send");
    assert_eq!(
        read_until_closed(&mut stream).await,
        b"-ERR internal server error\r\n"
    );
    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}

#[tokio::test]
async fn frontend_starttls_upgrade_then_auth_http() {
    let (certificate, key) = generate_certificate();
    let (backend_port, _lines) = start_line_backend("220 backend ESMTP\r\n", |line| {
        if line.to_ascii_uppercase().starts_with("QUIT") {
            "221 bye\r\n".to_owned()
        } else {
            "250 ok\r\n".to_owned()
        }
    })
    .await;
    let auth_port = start_tls_auth_http(backend_port).await;
    let port = available_port();
    let directory = tempfile::tempdir().expect("tempdir");
    let config = mail_config(vec![mail_server(
        "smtp-starttls",
        port,
        "smtp",
        json!({
            "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth") },
            "tls": {
                "certificateFile": certificate.to_string_lossy(),
                "privateKeyFile": key.to_string_lossy(),
                "starttls": "on",
            }
        }),
    )]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let mut stream = connect(port).await;
    assert_eq!(
        read_available(&mut stream).await,
        b"220 mail.example.com ESMTP ready\r\n"
    );
    stream
        .write_all(b"EHLO client.example\r\n")
        .await
        .expect("ehlo");
    let reply = read_available(&mut stream).await;
    let reply = String::from_utf8_lossy(&reply).into_owned();
    assert!(reply.contains("250 STARTTLS"), "{reply}");
    stream.write_all(b"STARTTLS\r\n").await.expect("starttls");
    assert_eq!(
        read_available(&mut stream).await,
        b"220 2.0.0 Start TLS\r\n"
    );
    let mut config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyCertificate))
        .with_no_client_auth();
    config.alpn_protocols.clear();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let server_name =
        rustls::pki_types::ServerName::try_from("mail.example.com").expect("server name");
    let mut stream = connector
        .connect(server_name, stream)
        .await
        .expect("TLS upgrade");
    // nginx does not resend the greeting after STARTTLS.
    let encoded = sdkwork_webserver_mail_protocol::auth::encode_plain(
        "alice",
        "secret",
        &sdkwork_webserver_mail_protocol::Limits::default(),
    )
    .expect("encode");
    stream
        .write_all(b"EHLO client.example\r\n")
        .await
        .expect("ehlo");
    let mut buffer = [0_u8; 4096];
    let read = timeout(READ_TIMEOUT, stream.read(&mut buffer))
        .await
        .expect("read")
        .expect("reply");
    assert!(String::from_utf8_lossy(&buffer[..read]).contains("250 AUTH"));
    stream
        .write_all(format!("AUTH PLAIN {encoded}\r\n").as_bytes())
        .await
        .expect("auth");
    let read = timeout(READ_TIMEOUT, stream.read(&mut buffer))
        .await
        .expect("read")
        .expect("auth reply");
    assert_eq!(&buffer[..read], b"235 2.0.0 OK\r\n");
    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}
