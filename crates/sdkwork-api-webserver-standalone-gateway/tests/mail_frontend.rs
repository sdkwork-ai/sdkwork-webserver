//! End-to-end nginx `mail {}` frontend plane (Wave 9A): plaintext POP3/IMAP/SMTP
//! greeting and authorization dialogs, bounded client framing, idle timeout,
//! smtp greeting delay, and inbound PROXY protocol v1.
//!
//! Wave 9A has no `auth_http`: each authorization attempt is answered with
//! nginx's auth-service-unavailable reply and the connection closes (see
//! `data_plane::mail_frontend`). The expected wire strings below were probed
//! against nginx 1.29.6 with an unreachable `auth_http`.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use sdkwork_api_webserver_standalone_gateway::{run_data_plane_until, DataPlaneError};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
    task::JoinHandle,
    time::timeout,
};

const READ_TIMEOUT: Duration = Duration::from_secs(5);

fn available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve an available port");
    listener.local_addr().expect("read reserved port").port()
}

fn write_config(directory: &Path, config: &Value) -> PathBuf {
    let path = directory.join("sdkwork.webserver.config.json");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(config).expect("serialize data-plane config"),
    )
    .expect("write data-plane config");
    path
}

fn mail_config(servers: Vec<Value>) -> Value {
    json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "mail-frontend-test",
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

fn mail_server(id: &str, port: u16, protocol: &str) -> Value {
    json!({
        "id": id,
        "bind": "127.0.0.1",
        "port": port,
        "protocol": protocol,
        "serverName": format!("{protocol}.example.com"),
        "timeoutMs": 5_000
    })
}

fn spawn_data_plane(
    config_path: &Path,
) -> (oneshot::Sender<()>, JoinHandle<Result<(), DataPlaneError>>) {
    let compiled = sdkwork_webserver_core::load_and_compile_webserver_config(config_path)
        .expect("compile data-plane config");
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
            Err(error) => panic!("cannot connect to mail listener on {port}: {error}"),
        }
    }
}

async fn read_available(stream: &mut TcpStream) -> Vec<u8> {
    let mut buffer = [0_u8; 8192];
    match timeout(READ_TIMEOUT, stream.read(&mut buffer)).await {
        Ok(Ok(read)) => buffer[..read].to_vec(),
        Ok(Err(error)) => panic!("mail read failed: {error}"),
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

async fn send(stream: &mut TcpStream, bytes: &[u8]) {
    timeout(READ_TIMEOUT, stream.write_all(bytes))
        .await
        .expect("mail write completes")
        .expect("mail write succeeds");
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("utf-8 mail response")
}

/// Wait until `expected` appears in the response, returning everything read.
async fn read_until(stream: &mut TcpStream, expected: &str) -> String {
    let mut collected = String::new();
    while !collected.contains(expected) {
        let chunk = read_available(stream).await;
        assert!(!chunk.is_empty(), "connection closed before {expected:?}");
        collected.push_str(&text(&chunk));
    }
    collected
}

#[tokio::test]
async fn pop3_greeting_capa_user_pass_and_auth_unavailable_close() {
    let directory = tempfile::tempdir().expect("temp dir");
    let port = available_port();
    let config_path = write_config(
        directory.path(),
        &mail_config(vec![mail_server("pop3", port, "pop3")]),
    );
    let (shutdown_tx, task) = spawn_data_plane(&config_path);
    let mut stream = connect(port).await;

    let greeting = read_until(&mut stream, "\r\n").await;
    eprintln!("DBG greeting={greeting:?}");
    assert_eq!(greeting, "+OK POP3 ready\r\n");

    send(&mut stream, b"CAPA\r\n").await;
    eprintln!("DBG sent CAPA");
    let capa = read_until(&mut stream, ".\r\n").await;
    assert_eq!(
        capa,
        "+OK Capability list follows\r\nTOP\r\nUSER\r\nUIDL\r\nSASL PLAIN LOGIN\r\n.\r\n"
    );

    send(&mut stream, b"NOOP\r\n").await;
    assert_eq!(read_until(&mut stream, "\r\n").await, "+OK\r\n");

    send(&mut stream, b"USER alice\r\n").await;
    assert_eq!(read_until(&mut stream, "\r\n").await, "+OK\r\n");

    send(&mut stream, b"PASS s3cret\r\n").await;
    let auth = read_until(&mut stream, "\r\n").await;
    assert_eq!(auth, "-ERR internal server error\r\n");
    assert!(read_until_closed(&mut stream).await.is_empty());

    let _ = shutdown_tx.send(());
    let _ = task.await;
}

#[tokio::test]
async fn imap_capability_login_and_logout_dialogs() {
    let directory = tempfile::tempdir().expect("temp dir");
    let port = available_port();
    let config_path = write_config(
        directory.path(),
        &mail_config(vec![mail_server("imap", port, "imap")]),
    );
    let (shutdown_tx, task) = spawn_data_plane(&config_path);

    let mut stream = connect(port).await;
    assert_eq!(
        read_until(&mut stream, "\r\n").await,
        "* OK IMAP4 ready\r\n"
    );
    send(&mut stream, b"a1 CAPABILITY\r\n").await;
    let capability = read_until(&mut stream, "a1 OK completed\r\n").await;
    assert!(capability.starts_with("* CAPABILITY IMAP4 IMAP4rev1 UIDPLUS AUTH=PLAIN\r\n"));
    send(&mut stream, b"a2 NOOP\r\n").await;
    assert_eq!(read_until(&mut stream, "\r\n").await, "a2 OK completed\r\n");
    send(&mut stream, b"a3 LOGIN alice s3cret\r\n").await;
    assert_eq!(
        read_until(&mut stream, "\r\n").await,
        "* BAD internal server error\r\n"
    );
    assert!(read_until_closed(&mut stream).await.is_empty());

    // LOGOUT is the clean pre-auth close.
    let mut stream = connect(port).await;
    let _ = read_until(&mut stream, "\r\n").await;
    send(&mut stream, b"a1 LOGOUT\r\n").await;
    let logout = read_until(&mut stream, "a1 OK completed\r\n").await;
    assert_eq!(logout, "* BYE\r\na1 OK completed\r\n");
    assert!(read_until_closed(&mut stream).await.is_empty());

    let _ = shutdown_tx.send(());
    let _ = task.await;
}

#[tokio::test]
async fn smtp_ehlo_auth_and_quit_dialogs() {
    let directory = tempfile::tempdir().expect("temp dir");
    let port = available_port();
    let config_path = write_config(
        directory.path(),
        &mail_config(vec![mail_server("smtp", port, "smtp")]),
    );
    let (shutdown_tx, task) = spawn_data_plane(&config_path);

    let mut stream = connect(port).await;
    assert_eq!(
        read_until(&mut stream, "\r\n").await,
        "220 smtp.example.com ESMTP ready\r\n"
    );
    send(&mut stream, b"EHLO client.example\r\n").await;
    let ehlo = read_until(&mut stream, "250 AUTH PLAIN LOGIN\r\n").await;
    assert_eq!(ehlo, "250-smtp.example.com\r\n250 AUTH PLAIN LOGIN\r\n");
    send(&mut stream, b"MAIL FROM:<a@b>\r\n").await;
    assert_eq!(
        read_until(&mut stream, "\r\n").await,
        "530 5.7.1 Authentication required\r\n"
    );
    send(&mut stream, b"AUTH PLAIN AGEAYg==\r\n").await;
    assert_eq!(
        read_until(&mut stream, "\r\n").await,
        "451 4.3.2 Internal server error\r\n"
    );
    assert!(read_until_closed(&mut stream).await.is_empty());

    let mut stream = connect(port).await;
    let _ = read_until(&mut stream, "\r\n").await;
    send(&mut stream, b"QUIT\r\n").await;
    assert_eq!(read_until(&mut stream, "\r\n").await, "221 2.0.0 Bye\r\n");
    assert!(read_until_closed(&mut stream).await.is_empty());

    let _ = shutdown_tx.send(());
    let _ = task.await;
}

#[tokio::test]
async fn smtp_greeting_delay_is_honored() {
    let directory = tempfile::tempdir().expect("temp dir");
    let port = available_port();
    let mut server = mail_server("smtp-delay", port, "smtp");
    server["smtpGreetingDelayMs"] = json!(250);
    let config_path = write_config(directory.path(), &mail_config(vec![server]));
    let (shutdown_tx, task) = spawn_data_plane(&config_path);

    let started = Instant::now();
    let mut stream = connect(port).await;
    let greeting = read_until(&mut stream, "\r\n").await;
    let elapsed = started.elapsed();
    assert_eq!(greeting, "220 smtp.example.com ESMTP ready\r\n");
    assert!(
        elapsed >= Duration::from_millis(200),
        "greeting arrived after {elapsed:?}, expected the 250 ms delay"
    );
    assert!(elapsed < Duration::from_secs(3));

    let _ = shutdown_tx.send(());
    let _ = task.await;
}

#[tokio::test]
async fn all_three_protocols_listen_together_and_advertise_configured_mechanisms() {
    let directory = tempfile::tempdir().expect("temp dir");
    let pop3_port = available_port();
    let imap_port = available_port();
    let smtp_port = available_port();
    let mut pop3 = mail_server("pop3", pop3_port, "pop3");
    pop3["pop3Auth"] = json!(["plain"]);
    let mut imap = mail_server("imap", imap_port, "imap");
    imap["imapAuth"] = json!(["plain", "login"]);
    let smtp = mail_server("smtp", smtp_port, "smtp");
    let config_path = write_config(directory.path(), &mail_config(vec![pop3, imap, smtp]));
    let (shutdown_tx, task) = spawn_data_plane(&config_path);

    let mut pop3_stream = connect(pop3_port).await;
    let _ = read_until(&mut pop3_stream, "\r\n").await;
    send(&mut pop3_stream, b"CAPA\r\n").await;
    let capa = read_until(&mut pop3_stream, ".\r\n").await;
    assert!(
        capa.contains("SASL PLAIN\r\n"),
        "configured pop3Auth must pin the advertised SASL list: {capa:?}"
    );
    assert!(!capa.contains("SASL PLAIN LOGIN"));

    let mut imap_stream = connect(imap_port).await;
    let _ = read_until(&mut imap_stream, "\r\n").await;
    send(&mut imap_stream, b"a1 CAPABILITY\r\n").await;
    let capability = read_until(&mut imap_stream, "a1 OK completed\r\n").await;
    assert!(capability.contains("AUTH=PLAIN AUTH=LOGIN"));

    let mut smtp_stream = connect(smtp_port).await;
    let _ = read_until(&mut smtp_stream, "\r\n").await;

    let _ = shutdown_tx.send(());
    let _ = task.await;
}

#[tokio::test]
async fn oversized_client_line_is_bounded_and_closes() {
    let directory = tempfile::tempdir().expect("temp dir");
    let port = available_port();
    let mut server = mail_server("smtp-small", port, "smtp");
    server["smtpClientBuffer"] = json!(512);
    let config_path = write_config(directory.path(), &mail_config(vec![server]));
    let (shutdown_tx, task) = spawn_data_plane(&config_path);

    let mut stream = connect(port).await;
    let _ = read_until(&mut stream, "\r\n").await;
    let oversized = vec![b'A'; 1024];
    send(&mut stream, &oversized).await;
    // The bounded line reader fails closed before the line terminates.
    assert!(read_until_closed(&mut stream).await.is_empty());

    let _ = shutdown_tx.send(());
    let _ = task.await;
}

#[tokio::test]
async fn malformed_client_bytes_close_without_a_reply() {
    let directory = tempfile::tempdir().expect("temp dir");
    let port = available_port();
    let config_path = write_config(
        directory.path(),
        &mail_config(vec![mail_server("imap", port, "imap")]),
    );
    let (shutdown_tx, task) = spawn_data_plane(&config_path);

    let mut stream = connect(port).await;
    let _ = read_until(&mut stream, "\r\n").await;
    send(&mut stream, b"\xff\xfe\r\n").await;
    assert!(read_until_closed(&mut stream).await.is_empty());

    let _ = shutdown_tx.send(());
    let _ = task.await;
}

#[tokio::test]
async fn idle_session_times_out() {
    let directory = tempfile::tempdir().expect("temp dir");
    let port = available_port();
    let mut server = mail_server("imap-idle", port, "imap");
    server["timeoutMs"] = json!(150);
    let config_path = write_config(directory.path(), &mail_config(vec![server]));
    let (shutdown_tx, task) = spawn_data_plane(&config_path);

    let mut stream = connect(port).await;
    let _ = read_until(&mut stream, "\r\n").await;
    let started = Instant::now();
    let mut buffer = [0_u8; 64];
    let closed = timeout(Duration::from_secs(3), stream.read(&mut buffer))
        .await
        .expect("idle session must close")
        .expect("idle read succeeds");
    assert_eq!(closed, 0, "idle session must close the connection");
    assert!(started.elapsed() >= Duration::from_millis(100));

    let _ = shutdown_tx.send(());
    let _ = task.await;
}

#[tokio::test]
async fn inbound_proxy_protocol_v1_is_parsed_and_required() {
    let directory = tempfile::tempdir().expect("temp dir");
    let port = available_port();
    let mut server = mail_server("pop3-proxy", port, "pop3");
    server["proxyProtocol"] = json!(true);
    server["timeoutMs"] = json!(500);
    let config_path = write_config(directory.path(), &mail_config(vec![server]));
    let (shutdown_tx, task) = spawn_data_plane(&config_path);

    // A client that does not send the PROXY header is dropped before greeting.
    let mut silent = connect(port).await;
    assert!(read_until_closed(&mut silent).await.is_empty());

    // A valid header is accepted; the greeting and dialog follow.
    let mut stream = connect(port).await;
    send(
        &mut stream,
        b"PROXY TCP4 203.0.113.7 198.51.100.9 12345 110\r\n",
    )
    .await;
    assert_eq!(read_until(&mut stream, "\r\n").await, "+OK POP3 ready\r\n");
    send(&mut stream, b"NOOP\r\n").await;
    assert_eq!(read_until(&mut stream, "\r\n").await, "+OK\r\n");

    // A malformed header is dropped before greeting.
    let mut malformed = connect(port).await;
    send(&mut malformed, b"PROXY TCP9 1.2.3.4 5.6.7.8 1 2\r\n").await;
    assert!(read_until_closed(&mut malformed).await.is_empty());

    let _ = shutdown_tx.send(());
    let _ = task.await;
}

#[tokio::test]
async fn so_keepalive_listener_serves_the_greeting() {
    let directory = tempfile::tempdir().expect("temp dir");
    let port = available_port();
    let mut server = mail_server("smtp-keepalive", port, "smtp");
    server["soKeepalive"] = json!(true);
    let config_path = write_config(directory.path(), &mail_config(vec![server]));
    let (shutdown_tx, task) = spawn_data_plane(&config_path);

    let mut stream = connect(port).await;
    assert_eq!(
        read_until(&mut stream, "\r\n").await,
        "220 smtp.example.com ESMTP ready\r\n"
    );

    let _ = shutdown_tx.send(());
    let _ = task.await;
}
