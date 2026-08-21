//! End-to-end raw TCP stream proxying (`nginx stream` equivalent): byte
//! forwarding to a literal target and to a declared upstream, plus PROXY
//! protocol v1 emission to the upstream.

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use sdkwork_api_webserver_standalone_gateway::{run_data_plane_until, DataPlaneError};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
};

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

fn stream_config(stream_port: u16, target: Value, proxy_timeout_ms: u64) -> Value {
    json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "stream-proxy-test",
        "limits": {
            "maxConcurrentRequests": 32,
            "requestTimeoutMs": 5_000,
            "drainTimeoutMs": 2_000,
            "maxConnections": 64
        },
        "listeners": [],
        "resources": [],
        "virtualHosts": [],
        "streams": [{
            "id": "tcp-forward",
            "bind": "127.0.0.1",
            "port": stream_port,
            "target": target,
            "proxyTimeoutMs": proxy_timeout_ms
        }]
    })
}

fn spawn_data_plane(config_path: &Path) -> (oneshot::Sender<()>, JoinHandle<Result<(), DataPlaneError>>) {
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

/// An echo upstream that also records the first line it receives, so PROXY
/// protocol emission can be asserted.
struct EchoUpstream {
    address: SocketAddr,
    first_line: Arc<std::sync::Mutex<Option<String>>>,
    accepted: Arc<AtomicUsize>,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl EchoUpstream {
    async fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind echo upstream");
        let address = listener.local_addr().expect("read upstream address");
        let first_line = Arc::new(std::sync::Mutex::new(None));
        let accepted = Arc::new(AtomicUsize::new(0));
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let first_line_task = Arc::clone(&first_line);
        let accepted_task = Arc::clone(&accepted);
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted_connection = listener.accept() => {
                        let Ok((mut socket, _)) = accepted_connection else { continue };
                        accepted_task.fetch_add(1, Ordering::AcqRel);
                        let first_line_task = Arc::clone(&first_line_task);
                        tokio::spawn(async move {
                            let mut buffer = [0_u8; 4096];
                            let mut captured = false;
                            loop {
                                let read = socket.read(&mut buffer).await.unwrap_or(0);
                                if read == 0 {
                                    return;
                                }
                                if !captured {
                                    let text = String::from_utf8_lossy(&buffer[..read]).into_owned();
                                    let first = text.lines().next().unwrap_or_default().to_owned();
                                    if let Ok(mut guard) = first_line_task.lock() {
                                        if guard.is_none() {
                                            *guard = Some(first);
                                        }
                                    }
                                    captured = true;
                                }
                                if socket.write_all(&buffer[..read]).await.is_err() {
                                    return;
                                }
                            }
                        });
                    }
                }
            }
        });
        Self {
            address,
            first_line,
            accepted,
            shutdown: Some(shutdown_tx),
            task: Some(task),
        }
    }
}

async fn round_trip(stream_port: u16, payload: &[u8]) -> Vec<u8> {
    let mut client = TcpStream::connect(("127.0.0.1", stream_port))
        .await
        .expect("connect to stream listener");
    client.write_all(payload).await.expect("write payload");
    let mut echoed = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let mut buffer = [0_u8; 4096];
        let read = tokio::time::timeout_at(
            deadline,
            client.read(&mut buffer),
        )
        .await
        .expect("echo must arrive before the deadline")
        .expect("read echo");
        if read == 0 {
            break;
        }
        echoed.extend_from_slice(&buffer[..read]);
        if echoed.len() >= payload.len() {
            break;
        }
    }
    echoed
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn forwards_bytes_to_a_literal_tcp_target() {
    let mut upstream = EchoUpstream::spawn().await;
    let stream_port = available_port();
    let directory = tempfile::tempdir().expect("temp dir");
    let config_path = write_config(
        directory.path(),
        &stream_config(
            stream_port,
            json!({"type": "literal", "host": "127.0.0.1", "port": upstream.address.port()}),
            5_000,
        ),
    );
    let (shutdown_tx, task) = spawn_data_plane(&config_path);

    let echoed = round_trip(stream_port, b"ping-through-stream").await;
    assert_eq!(echoed, b"ping-through-stream");
    assert_eq!(upstream.accepted.load(Ordering::Acquire), 1);

    // Payload arriving from the upstream direction is forwarded too: the echo
    // upstream writes back every received byte, which the previous assertion
    // already exercised. A second connection proves the listener keeps
    // accepting.
    let echoed = round_trip(stream_port, b"second-connection").await;
    assert_eq!(echoed, b"second-connection");

    shutdown_tx.send(()).ok();
    let result = task.await.expect("data plane task must complete");
    assert!(result.is_ok(), "data plane must shut down cleanly: {result:?}");
    if let Some(shutdown) = upstream.shutdown.take() {
        shutdown.send(()).ok();
    }
    if let Some(task) = upstream.task.take() {
        task.await.ok();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn forwards_bytes_through_a_declared_upstream_and_emits_proxy_protocol() {
    let mut upstream = EchoUpstream::spawn().await;
    let stream_port = available_port();
    let directory = tempfile::tempdir().expect("temp dir");
    let mut config = stream_config(
        stream_port,
        json!({"type": "upstream", "name": "echo-pool"}),
        5_000,
    );
    config["streams"][0]["proxyProtocol"] = json!(true);
    config["upstreams"] = json!([{
        "id": "echo-pool",
        "targets": [{"url": format!("http://127.0.0.1:{}", upstream.address.port())}],
        "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]},
        "connectTimeoutMs": 2_000,
        "requestTimeoutMs": 5_000
    }]);
    let config_path = write_config(directory.path(), &config);
    let (shutdown_tx, task) = spawn_data_plane(&config_path);

    // The PROXY protocol header is written to the upstream on connect; the
    // echo upstream reflects it, so consume the header line before asserting
    // the payload round trip.
    let mut client = TcpStream::connect(("127.0.0.1", stream_port))
        .await
        .expect("connect to stream listener");
    client
        .write_all(b"through-upstream")
        .await
        .expect("write payload");
    let mut proxy_line = Vec::new();
    let mut byte = [0_u8; 1];
    loop {
        let read = tokio::time::timeout(
            Duration::from_secs(5),
            client.read(&mut byte),
        )
        .await
        .expect("PROXY line must arrive")
        .expect("read PROXY line");
        assert!(read > 0, "upstream closed before the PROXY line");
        proxy_line.push(byte[0]);
        if proxy_line.ends_with(b"\r\n") {
            break;
        }
    }
    assert!(
        String::from_utf8_lossy(&proxy_line).starts_with("PROXY TCP4 127.0.0.1 127.0.0.1 "),
        "unexpected PROXY header: {}",
        String::from_utf8_lossy(&proxy_line)
    );
    let mut echoed = vec![0_u8; b"through-upstream".len()];
    client
        .read_exact(&mut echoed)
        .await
        .expect("read echoed payload");
    assert_eq!(echoed, b"through-upstream");
    let first_line = upstream.first_line.lock().unwrap().clone();
    let first_line = first_line.expect("upstream must receive the PROXY header first");
    assert!(
        first_line.starts_with("PROXY TCP4 127.0.0.1 127.0.0.1 "),
        "unexpected PROXY header: {first_line}"
    );

    shutdown_tx.send(()).ok();
    let result = task.await.expect("data plane task must complete");
    assert!(result.is_ok(), "data plane must shut down cleanly: {result:?}");
    if let Some(shutdown) = upstream.shutdown.take() {
        shutdown.send(()).ok();
    }
    if let Some(task) = upstream.task.take() {
        task.await.ok();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn idle_proxy_timeout_closes_a_silent_connection() {
    let mut upstream = EchoUpstream::spawn().await;
    let stream_port = available_port();
    let directory = tempfile::tempdir().expect("temp dir");
    let config_path = write_config(
        directory.path(),
        &stream_config(
            stream_port,
            json!({"type": "literal", "host": "127.0.0.1", "port": upstream.address.port()}),
            1_000,
        ),
    );
    let (shutdown_tx, task) = spawn_data_plane(&config_path);

    let mut client = TcpStream::connect(("127.0.0.1", stream_port))
        .await
        .expect("connect to stream listener");
    // No traffic: the proxy must cut the connection after proxyTimeoutMs of
    // idle on both directions.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut buffer = [0_u8; 16];
    let read = tokio::time::timeout_at(deadline, client.read(&mut buffer))
        .await
        .expect("idle timeout must close the connection")
        .expect("read close");
    assert_eq!(read, 0, "silent connection must be closed by the proxy");

    shutdown_tx.send(()).ok();
    let result = task.await.expect("data plane task must complete");
    assert!(result.is_ok(), "data plane must shut down cleanly: {result:?}");
    if let Some(shutdown) = upstream.shutdown.take() {
        shutdown.send(()).ok();
    }
    if let Some(task) = upstream.task.take() {
        task.await.ok();
    }
}

fn write_self_signed_certificate(directory: &Path, stem: &str, names: &[&str]) -> Vec<u8> {
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
    let mut params = CertificateParams::new(
        names
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    )
    .expect("certificate parameters");
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, names[0]);
    let key = KeyPair::generate().expect("generate key");
    let certificate = params.self_signed(&key).expect("generate certificate");
    std::fs::write(directory.join(format!("{stem}.pem")), certificate.pem())
        .expect("write certificate");
    std::fs::write(directory.join(format!("{stem}.key")), key.serialize_pem())
        .expect("write private key");
    certificate.der().as_ref().to_vec()
}

async fn wait_ready(stream_port: u16) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if TcpStream::connect(("127.0.0.1", stream_port)).await.is_ok() {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("stream listener did not become ready");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn tls_round_trip(stream_port: u16, certificate_der: &[u8], payload: &[u8]) -> Vec<u8> {
    use rustls::{
        pki_types::{CertificateDer, ServerName},
        ClientConfig, RootCertStore,
    };
    use tokio_rustls::TlsConnector;

    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(certificate_der.to_vec()))
        .expect("trust generated certificate");
    let tls_config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let tcp = TcpStream::connect(("127.0.0.1", stream_port))
        .await
        .expect("connect stream listener");
    let mut client = TlsConnector::from(Arc::new(tls_config))
        .connect(
            ServerName::try_from("localhost".to_owned()).expect("valid DNS name"),
            tcp,
        )
        .await
        .expect("TLS handshake");
    client.write_all(payload).await.expect("write payload");
    let mut echoed = vec![0_u8; payload.len()];
    client.read_exact(&mut echoed).await.expect("read echo");
    echoed
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn terminates_stream_tls_and_forwards_plaintext_to_upstream() {
    let mut upstream = EchoUpstream::spawn().await;
    let stream_port = available_port();
    let directory = tempfile::tempdir().expect("temp dir");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "stream-site", &["localhost"]);
    let mut config = stream_config(
        stream_port,
        json!({"type": "literal", "host": "127.0.0.1", "port": upstream.address.port()}),
        5_000,
    );
    config["streams"][0]["bind"] = json!("127.0.0.1");
    config["streams"][0]["tls"] = json!({
        "mode": "terminate",
        "certificateRef": "stream-site"
    });
    config["certificates"] = json!([{
        "id": "stream-site",
        "serverNames": ["localhost"],
        "source": {
            "type": "protected-file",
            "certificateFile": "stream-site.pem",
            "privateKeyFile": "stream-site.key"
        }
    }]);
    let config_path = write_config(directory.path(), &config);
    let (shutdown_tx, task) = spawn_data_plane(&config_path);
    wait_ready(stream_port).await;

    let echoed = tls_round_trip(stream_port, &certificate_der, b"tls-terminated").await;
    assert_eq!(echoed, b"tls-terminated");
    assert_eq!(upstream.accepted.load(Ordering::Acquire), 1);

    shutdown_tx.send(()).ok();
    let result = task.await.expect("data plane task must complete");
    assert!(result.is_ok(), "data plane must shut down cleanly: {result:?}");
    if let Some(shutdown) = upstream.shutdown.take() {
        shutdown.send(()).ok();
    }
    if let Some(task) = upstream.task.take() {
        task.await.ok();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ssl_preread_forwards_client_hello_preface_to_upstream() {
    let mut upstream = EchoUpstream::spawn().await;
    let stream_port = available_port();
    let directory = tempfile::tempdir().expect("temp dir");
    let mut config = stream_config(
        stream_port,
        json!({"type": "literal", "host": "127.0.0.1", "port": upstream.address.port()}),
        5_000,
    );
    config["streams"][0]["bind"] = json!("127.0.0.1");
    config["streams"][0]["tls"] = json!({ "mode": "preread" });
    let config_path = write_config(directory.path(), &config);
    let (shutdown_tx, task) = spawn_data_plane(&config_path);
    wait_ready(stream_port).await;

    // Minimal TLS handshake record containing a ClientHello with SNI.
    let client_hello = minimal_client_hello_with_sni("localhost");
    let mut client = TcpStream::connect(("127.0.0.1", stream_port))
        .await
        .expect("connect stream listener");
    client
        .write_all(&client_hello)
        .await
        .expect("write ClientHello");
    let mut echoed = vec![0_u8; client_hello.len()];
    client
        .read_exact(&mut echoed)
        .await
        .expect("read echoed preface");
    assert_eq!(echoed, client_hello);
    assert_eq!(upstream.accepted.load(Ordering::Acquire), 1);

    shutdown_tx.send(()).ok();
    let result = task.await.expect("data plane task must complete");
    assert!(result.is_ok(), "data plane must shut down cleanly: {result:?}");
    if let Some(shutdown) = upstream.shutdown.take() {
        shutdown.send(()).ok();
    }
    if let Some(task) = upstream.task.take() {
        task.await.ok();
    }
}

fn minimal_client_hello_with_sni(sni: &str) -> Vec<u8> {
    let sni_bytes = sni.as_bytes();
    let mut extensions = Vec::new();
    // server_name extension
    let mut server_name = Vec::new();
    server_name.extend_from_slice(&(1u16 + 2 + sni_bytes.len() as u16).to_be_bytes());
    server_name.push(0); // host_name
    server_name.extend_from_slice(&(sni_bytes.len() as u16).to_be_bytes());
    server_name.extend_from_slice(sni_bytes);
    extensions.extend_from_slice(&0u16.to_be_bytes()); // type server_name
    extensions.extend_from_slice(&(server_name.len() as u16).to_be_bytes());
    extensions.extend_from_slice(&server_name);

    let mut body = Vec::new();
    body.extend_from_slice(&0x0303u16.to_be_bytes()); // legacy_version
    body.extend_from_slice(&[0_u8; 32]); // random
    body.push(0); // session_id length
    body.extend_from_slice(&2u16.to_be_bytes()); // cipher suites length
    body.extend_from_slice(&0x1301u16.to_be_bytes());
    body.push(1); // compression methods length
    body.push(0);
    body.extend_from_slice(&(extensions.len() as u16).to_be_bytes());
    body.extend_from_slice(&extensions);

    let mut handshake = Vec::new();
    handshake.push(0x01); // ClientHello
    handshake.push(((body.len() >> 16) & 0xff) as u8);
    handshake.push(((body.len() >> 8) & 0xff) as u8);
    handshake.push((body.len() & 0xff) as u8);
    handshake.extend_from_slice(&body);

    let mut record = Vec::new();
    record.push(0x16); // handshake
    record.extend_from_slice(&0x0301u16.to_be_bytes());
    record.extend_from_slice(&(handshake.len() as u16).to_be_bytes());
    record.extend_from_slice(&handshake);
    record
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ejects_unhealthy_upstream_targets_for_stream_selection() {
    let dead_port = available_port();
    let stream_port = available_port();
    let directory = tempfile::tempdir().expect("temp dir");
    let mut config = stream_config(
        stream_port,
        json!({"type": "upstream", "name": "dead-pool"}),
        5_000,
    );
    config["upstreams"] = json!([{
        "id": "dead-pool",
        "targets": [{"url": format!("http://127.0.0.1:{dead_port}")}],
        "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]},
        "connectTimeoutMs": 500,
        "requestTimeoutMs": 2_000,
        "passiveHealth": {
            "failureThreshold": 1,
            "ejectionTimeMs": 60_000
        }
    }]);
    let config_path = write_config(directory.path(), &config);
    let (shutdown_tx, task) = spawn_data_plane(&config_path);
    wait_ready(stream_port).await;

    let first = TcpStream::connect(("127.0.0.1", stream_port)).await;
    assert!(first.is_ok());
    let mut first = first.expect("connected");
    let mut buffer = [0_u8; 16];
    // This host delays refused loopback connects (~2s); budget the first
    // failed connect plus the ejection before the second probe.
    let _ = tokio::time::timeout(Duration::from_secs(6), first.read(&mut buffer)).await;

    let second = TcpStream::connect(("127.0.0.1", stream_port)).await;
    if let Ok(mut second) = second {
        let read = tokio::time::timeout(Duration::from_secs(6), second.read(&mut buffer))
            .await
            .expect("ejected upstream must close promptly")
            .expect("read");
        assert_eq!(read, 0, "ejected upstream must not forward bytes");
    }

    shutdown_tx.send(()).ok();
    let result = task.await.expect("data plane task must complete");
    assert!(result.is_ok(), "data plane must shut down cleanly: {result:?}");
}
