use std::{
    fs,
    net::{SocketAddr, TcpListener as StdTcpListener},
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use rustls::{
    pki_types::{CertificateDer, ServerName},
    ClientConfig, RootCertStore,
};
use sdkwork_api_webserver_standalone_gateway::{
    run_data_plane_from_config_until, run_data_plane_until, DataPlaneError,
};
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::{JoinHandle, JoinSet},
    time::timeout,
};
use tokio_rustls::TlsConnector;

type DataPlaneTask = JoinHandle<Result<(), DataPlaneError>>;

#[derive(Clone, Copy)]
enum UpstreamBehavior {
    Echo(&'static str),
    Forbidden,
    InvalidUpgrade,
}

struct RawUpstream {
    address: SocketAddr,
    accepted: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

struct ConnectionGuard(Arc<AtomicUsize>);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn available_port() -> u16 {
    let listener = StdTcpListener::bind("127.0.0.1:0").expect("reserve available port");
    listener.local_addr().expect("read reserved port").port()
}

fn proxy_config(
    port: u16,
    upstream: SocketAddr,
    max_in_flight_requests: usize,
    max_connection_age_ms: u64,
) -> Value {
    json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "websocket-proxy-test",
        "limits": {
            "maxConcurrentRequests": 16,
            "requestTimeoutMs": 3_000,
            "drainTimeoutMs": 750,
            "maxConnections": 32,
            "maxConnectionAgeMs": max_connection_age_ms
        },
        "deployment": {
            "drainTimeoutMs": 750
        },
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "test-host",
            "maxConnections": 16
        }],
        "resources": [{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "websocket-upstream"
        }],
        "upstreams": [{
            "id": "websocket-upstream",
            "targets": [{"url": format!("http://{upstream}")}],
            "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]},
            "connectTimeoutMs": 1_000,
            "requestTimeoutMs": 2_000,
            "maxConnections": 1,
            "maxIdleConnections": 1,
            "idleConnectionTimeoutMs": 1_000,
            "maxInFlightRequests": max_in_flight_requests,
            "passiveHealth": {
                "failureThreshold": 2,
                "ejectionTimeMs": 1_000,
                "failureStatuses": [500, 502, 503, 504]
            }
        }],
        "virtualHosts": [{
            "id": "test-host",
            "listenerRefs": ["http"],
            "serverNames": ["test.localhost"],
            "routes": [{
                "id": "proxy-route",
                "match": {"pathType": "prefix", "path": "/"},
                "resourceRef": "proxy"
            }]
        }]
    })
}

fn write_config(directory: &Path, config: &Value) -> std::path::PathBuf {
    let path = directory.join("sdkwork.webserver.config.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(config).expect("serialize test config"),
    )
    .expect("write test config");
    path
}

fn write_self_signed_certificate(directory: &Path) -> Vec<u8> {
    let mut params =
        CertificateParams::new(vec!["localhost".to_owned()]).expect("certificate parameters");
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, "localhost");
    let key = KeyPair::generate().expect("generate certificate key");
    let certificate = params.self_signed(&key).expect("generate certificate");
    fs::write(directory.join("localhost.pem"), certificate.pem()).expect("write certificate");
    fs::write(directory.join("localhost.key"), key.serialize_pem()).expect("write private key");
    certificate.der().as_ref().to_vec()
}

fn enable_https(config: &mut Value) {
    config["listeners"][0]["id"] = json!("https");
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["listeners"][0]["tlsPolicyRef"] = json!("tls");
    config["virtualHosts"][0]["listenerRefs"] = json!(["https"]);
    config["virtualHosts"][0]["serverNames"] = json!(["localhost"]);
    config["certificates"] = json!([{
        "id": "cert",
        "serverNames": ["localhost"],
        "source": {
            "type": "protected-file",
            "certificateFile": "localhost.pem",
            "privateKeyFile": "localhost.key"
        }
    }]);
    config["tlsPolicies"] = json!([{
        "id": "tls",
        "certificateRef": "cert",
        "minimumVersion": "tls1.2",
        "maximumVersion": "tls1.3",
        "alpn": ["h2", "http/1.1"]
    }]);
}

fn spawn_data_plane(config_path: &Path) -> (oneshot::Sender<()>, DataPlaneTask) {
    let compiled = load_and_compile_webserver_config(config_path).expect("compile data plane");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    (shutdown_tx, task)
}

fn spawn_watched_data_plane(config_path: &Path) -> (oneshot::Sender<()>, DataPlaneTask) {
    let config_path = config_path.to_path_buf();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_from_config_until(config_path, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    (shutdown_tx, task)
}

async fn stop_data_plane(shutdown: oneshot::Sender<()>, task: DataPlaneTask) {
    shutdown.send(()).expect("signal data-plane shutdown");
    timeout(Duration::from_secs(3), task)
        .await
        .expect("data plane stops within drain budget")
        .expect("data-plane task joins")
        .expect("data plane stops cleanly");
}

async fn spawn_upstream(behavior: UpstreamBehavior) -> RawUpstream {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind raw upstream");
    let address = listener.local_addr().expect("read upstream address");
    let accepted = Arc::new(AtomicUsize::new(0));
    let accepted_for_task = accepted.clone();
    let active = Arc::new(AtomicUsize::new(0));
    let active_for_task = active.clone();
    let (shutdown, mut shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { break };
                    accepted_for_task.fetch_add(1, Ordering::AcqRel);
                    active_for_task.fetch_add(1, Ordering::AcqRel);
                    let active = active_for_task.clone();
                    connections.spawn(async move {
                        let _guard = ConnectionGuard(active);
                        handle_upstream(stream, behavior).await;
                    });
                }
                Some(_) = connections.join_next(), if !connections.is_empty() => {}
            }
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    });
    RawUpstream {
        address,
        accepted,
        active,
        shutdown,
        task,
    }
}

async fn stop_upstream(upstream: RawUpstream) {
    upstream
        .shutdown
        .send(())
        .expect("signal upstream shutdown");
    timeout(Duration::from_secs(3), upstream.task)
        .await
        .expect("upstream stops")
        .expect("upstream task joins");
}

async fn handle_upstream(mut stream: TcpStream, behavior: UpstreamBehavior) {
    let Ok((_head, buffered)) = read_head(&mut stream).await else {
        return;
    };
    match behavior {
        UpstreamBehavior::Echo(banner) => {
            let response = format!(
                "HTTP/1.1 101 Switching Protocols\r\nConnection: upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Protocol: sdkwork-test\r\n\r\n{banner}"
            );
            if stream.write_all(response.as_bytes()).await.is_err() {
                return;
            }
            if !buffered.is_empty() && stream.write_all(&buffered).await.is_err() {
                return;
            }
            let mut buffer = [0_u8; 4096];
            loop {
                match stream.read(&mut buffer).await {
                    Ok(0) | Err(_) => {
                        let _ = stream.shutdown().await;
                        return;
                    }
                    Ok(read) if stream.write_all(&buffer[..read]).await.is_err() => return,
                    Ok(_) => {}
                }
            }
        }
        UpstreamBehavior::Forbidden => {
            let _ = stream
                .write_all(
                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 6\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\ndenied",
                )
                .await;
        }
        UpstreamBehavior::InvalidUpgrade => {
            let _ = stream
                .write_all(
                    b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nX-Secret: must-not-leak\r\n\r\nsecret-body",
                )
                .await;
        }
    }
}

async fn wait_for_gateway(port: u16) {
    timeout(Duration::from_secs(5), async {
        loop {
            if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("gateway becomes reachable");
}

async fn wait_for_count(counter: &AtomicUsize, expected: usize, context: &str) {
    timeout(Duration::from_secs(3), async {
        while counter.load(Ordering::Acquire) != expected {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "{context}: expected {expected}, observed {}",
            counter.load(Ordering::Acquire)
        )
    });
}

async fn read_head<S>(stream: &mut S) -> std::io::Result<(String, Vec<u8>)>
where
    S: tokio::io::AsyncRead + Unpin,
{
    let mut bytes = Vec::with_capacity(1024);
    let mut buffer = [0_u8; 1024];
    loop {
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
        }
        bytes.extend_from_slice(&buffer[..read]);
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let body_offset = position + 4;
            let tail = bytes.split_off(body_offset);
            let head = String::from_utf8(bytes)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
            return Ok((head, tail));
        }
        if bytes.len() > 64 * 1024 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "response Header exceeds test bound",
            ));
        }
    }
}

async fn connect_tls(
    port: u16,
    certificate_der: &[u8],
) -> tokio_rustls::client::TlsStream<TcpStream> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(certificate_der.to_vec()))
        .expect("trust generated certificate");
    let mut config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let tcp = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect TLS client");
    let tls = TlsConnector::from(Arc::new(config))
        .connect(
            ServerName::try_from("localhost".to_owned()).expect("valid DNS name"),
            tcp,
        )
        .await
        .expect("complete TLS handshake");
    assert_eq!(
        tls.get_ref().1.alpn_protocol(),
        Some(b"http/1.1".as_slice())
    );
    tls
}

async fn send_handshake(port: u16, request: &[u8]) -> (TcpStream, String, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect to gateway");
    stream.write_all(request).await.expect("write handshake");
    let (head, tail) = timeout(Duration::from_secs(3), read_head(&mut stream))
        .await
        .expect("gateway responds")
        .expect("read gateway response");
    (stream, head, tail)
}

fn valid_handshake(path: &str) -> Vec<u8> {
    format!(
        "GET {path} HTTP/1.1\r\nHost: test.localhost\r\nConnection: keep-alive, Upgrade\r\nUpgrade: WebSocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: MDEyMzQ1Njc4OWFiY2RlZg==\r\nSec-WebSocket-Protocol: sdkwork-test\r\n\r\n"
    )
    .into_bytes()
}

async fn read_prefix(stream: &mut TcpStream, mut buffered: Vec<u8>, length: usize) -> Vec<u8> {
    while buffered.len() < length {
        let mut chunk = [0_u8; 64];
        let read = stream.read(&mut chunk).await.expect("read tunnel prefix");
        assert_ne!(read, 0);
        buffered.extend_from_slice(&chunk[..read]);
    }
    buffered
}

#[tokio::test]
async fn http1_websocket_tunnels_immediate_and_multi_buffer_bytes() {
    let upstream = spawn_upstream(UpstreamBehavior::Echo("upstream-ready")).await;
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let config = write_config(
        directory.path(),
        &proxy_config(port, upstream.address, 2, 5_000),
    );
    let (shutdown, task) = spawn_data_plane(&config);
    wait_for_gateway(port).await;

    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect to gateway");
    let mut request = valid_handshake("/socket");
    request.extend_from_slice(b"client-early");
    stream
        .write_all(&request)
        .await
        .expect("write handshake with immediate tunnel bytes");
    let (head, mut buffered) = timeout(Duration::from_secs(3), read_head(&mut stream))
        .await
        .expect("gateway responds")
        .expect("read gateway upgrade response");
    assert!(head.starts_with("HTTP/1.1 101"), "{head}");
    assert!(head.to_ascii_lowercase().contains("connection: upgrade"));
    assert!(head.to_ascii_lowercase().contains("upgrade: websocket"));
    assert!(head.contains("sec-websocket-protocol: sdkwork-test"));

    while buffered.len() < b"upstream-ready".len() {
        let mut chunk = [0_u8; 64];
        let read = stream.read(&mut chunk).await.expect("read immediate bytes");
        assert_ne!(read, 0);
        buffered.extend_from_slice(&chunk[..read]);
    }
    assert_eq!(&buffered[..b"upstream-ready".len()], b"upstream-ready");
    let mut carried = buffered.split_off(b"upstream-ready".len());
    while carried.len() < b"client-early".len() {
        let mut chunk = [0_u8; 64];
        let read = stream
            .read(&mut chunk)
            .await
            .expect("read echoed immediate client bytes");
        assert_ne!(read, 0);
        carried.extend_from_slice(&chunk[..read]);
    }
    assert_eq!(&carried[..b"client-early".len()], b"client-early");
    carried.drain(..b"client-early".len());

    let payload = (0..70_000)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    stream
        .write_all(&payload)
        .await
        .expect("write tunneled payload");
    let mut echoed = Vec::with_capacity(payload.len());
    echoed.append(&mut carried);
    let offset = echoed.len();
    echoed.resize(payload.len(), 0);
    stream
        .read_exact(&mut echoed[offset..])
        .await
        .expect("read echoed multi-buffer payload");
    assert_eq!(echoed, payload);

    stream.shutdown().await.expect("half-close client write");
    let mut trailing = [0_u8; 1];
    assert_eq!(
        timeout(Duration::from_secs(2), stream.read(&mut trailing))
            .await
            .expect("upstream half-close propagates")
            .expect("read propagated half-close"),
        0
    );

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream).await;
}

#[tokio::test]
async fn least_connections_holds_target_activity_through_websocket_tunnel_lifetime() {
    let primary = spawn_upstream(UpstreamBehavior::Echo("primary")).await;
    let secondary = spawn_upstream(UpstreamBehavior::Echo("secondary")).await;
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let mut config = proxy_config(port, primary.address, 4, 5_000);
    config["upstreams"][0]["loadBalancing"] = json!("least-connections");
    config["upstreams"][0]["targets"] = json!([
        {"url": format!("http://{}", primary.address)},
        {"url": format!("http://{}", secondary.address)}
    ]);
    config["upstreams"][0]["maxConnections"] = json!(4);
    config["upstreams"][0]["maxIdleConnections"] = json!(4);
    let config = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&config);
    wait_for_gateway(port).await;

    let (mut first, first_head, first_buffered) =
        send_handshake(port, &valid_handshake("/first")).await;
    assert!(first_head.starts_with("HTTP/1.1 101"), "{first_head}");
    let first_banner = read_prefix(&mut first, first_buffered, b"primary".len()).await;
    assert_eq!(&first_banner[..b"primary".len()], b"primary");

    let (mut second, second_head, second_buffered) =
        send_handshake(port, &valid_handshake("/second")).await;
    assert!(second_head.starts_with("HTTP/1.1 101"), "{second_head}");
    let second_banner = read_prefix(&mut second, second_buffered, b"secondary".len()).await;
    assert_eq!(&second_banner[..b"secondary".len()], b"secondary");

    first.shutdown().await.expect("close primary tunnel client");
    drop(first);
    wait_for_count(
        &primary.active,
        0,
        "closing the first tunnel releases primary activity",
    )
    .await;

    let (mut third, third_head, third_buffered) =
        send_handshake(port, &valid_handshake("/after-close")).await;
    assert!(third_head.starts_with("HTTP/1.1 101"), "{third_head}");
    let third_banner = read_prefix(&mut third, third_buffered, b"primary".len()).await;
    assert_eq!(&third_banner[..b"primary".len()], b"primary");

    second
        .shutdown()
        .await
        .expect("close secondary tunnel client");
    third
        .shutdown()
        .await
        .expect("close final primary tunnel client");
    stop_data_plane(shutdown, task).await;
    stop_upstream(primary).await;
    stop_upstream(secondary).await;
}

#[tokio::test]
async fn mixed_https_listener_upgrades_wss_over_verified_tls_and_http1_alpn() {
    let upstream = spawn_upstream(UpstreamBehavior::Echo("upstream-ready")).await;
    let directory = TempDir::new().expect("create config directory");
    let certificate = write_self_signed_certificate(directory.path());
    let port = available_port();
    let mut config = proxy_config(port, upstream.address, 1, 5_000);
    enable_https(&mut config);
    let config = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&config);
    wait_for_gateway(port).await;

    let mut stream = connect_tls(port, &certificate).await;
    let request = String::from_utf8(valid_handshake("/secure"))
        .expect("handshake is UTF-8")
        .replace("test.localhost", "localhost");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write WSS handshake");
    let (head, mut buffered) = timeout(Duration::from_secs(3), read_head(&mut stream))
        .await
        .expect("WSS handshake responds")
        .expect("read WSS response");
    assert!(head.starts_with("HTTP/1.1 101"), "{head}");
    while buffered.len() < b"upstream-ready".len() {
        let mut chunk = [0_u8; 64];
        let read = stream.read(&mut chunk).await.expect("read WSS ready bytes");
        assert_ne!(read, 0);
        buffered.extend_from_slice(&chunk[..read]);
    }
    assert_eq!(&buffered[..b"upstream-ready".len()], b"upstream-ready");

    stream
        .write_all(b"encrypted-tunnel")
        .await
        .expect("write WSS tunnel bytes");
    let mut echoed = [0_u8; 16];
    stream
        .read_exact(&mut echoed)
        .await
        .expect("read WSS tunnel bytes");
    assert_eq!(&echoed, b"encrypted-tunnel");

    drop(stream);
    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream).await;
}

#[tokio::test]
async fn watch_replaces_generation_without_terminating_existing_tunnel() {
    let old = spawn_upstream(UpstreamBehavior::Echo("old-ready")).await;
    let new = spawn_upstream(UpstreamBehavior::Echo("new-ready")).await;
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let mut initial = proxy_config(port, old.address, 8, 10_000);
    initial["deployment"] = json!({
        "drainTimeoutMs": 750,
        "reload": {"mode": "watch", "pollIntervalMs": 100}
    });
    let path = write_config(directory.path(), &initial);
    let (shutdown, task) = spawn_watched_data_plane(&path);
    wait_for_gateway(port).await;

    let (mut old_tunnel, old_head, old_buffered) =
        send_handshake(port, &valid_handshake("/old")).await;
    assert!(old_head.starts_with("HTTP/1.1 101"), "{old_head}");
    let old_buffered = read_prefix(&mut old_tunnel, old_buffered, b"old-ready".len()).await;
    assert_eq!(&old_buffered[..b"old-ready".len()], b"old-ready");
    wait_for_count(&old.active, 1, "old tunnel owns its physical connection").await;

    let mut replacement = proxy_config(port, new.address, 8, 10_000);
    replacement["deployment"] = initial["deployment"].clone();
    write_config(directory.path(), &replacement);
    timeout(Duration::from_secs(5), async {
        loop {
            let (mut candidate, head, buffered) =
                send_handshake(port, &valid_handshake("/new")).await;
            if head.starts_with("HTTP/1.1 101") {
                let buffered = read_prefix(&mut candidate, buffered, b"new-ready".len()).await;
                if buffered.starts_with(b"new-ready") {
                    drop(candidate);
                    return;
                }
            }
            drop(candidate);
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("Watch publishes the new WebSocket generation");

    wait_for_count(&old.active, 1, "reload retains the old active tunnel").await;
    old_tunnel
        .write_all(b"old-still-alive")
        .await
        .expect("write through retired generation");
    let mut echoed = [0_u8; 15];
    old_tunnel
        .read_exact(&mut echoed)
        .await
        .expect("read through retired generation");
    assert_eq!(&echoed, b"old-still-alive");

    drop(old_tunnel);
    wait_for_count(&old.active, 0, "old generation releases after tunnel close").await;
    stop_data_plane(shutdown, task).await;
    stop_upstream(old).await;
    stop_upstream(new).await;
}

#[tokio::test]
async fn malformed_and_unsupported_upgrades_fail_before_upstream_connect() {
    let upstream = spawn_upstream(UpstreamBehavior::Echo("upstream-ready")).await;
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let config = write_config(
        directory.path(),
        &proxy_config(port, upstream.address, 2, 5_000),
    );
    let (shutdown, task) = spawn_data_plane(&config);
    wait_for_gateway(port).await;

    let cases = [
        (
            b"GET / HTTP/1.1\r\nHost: test.localhost\r\nUpgrade: websocket\r\n\r\n".as_slice(),
            "400",
        ),
        (
            b"POST / HTTP/1.1\r\nHost: test.localhost\r\nConnection: upgrade\r\nUpgrade: websocket\r\n\r\n".as_slice(),
            "400",
        ),
        (
            b"GET / HTTP/1.1\r\nHost: test.localhost\r\nConnection: upgrade\r\nUpgrade: websocket\r\nContent-Length: 0\r\n\r\n".as_slice(),
            "400",
        ),
        (
            b"GET / HTTP/1.1\r\nHost: test.localhost\r\nConnection: upgrade\r\nUpgrade: h2c\r\n\r\n".as_slice(),
            "501",
        ),
    ];
    for (request, status) in cases {
        let (_stream, head, _) = send_handshake(port, request).await;
        assert!(head.starts_with(&format!("HTTP/1.1 {status}")), "{head}");
    }
    assert_eq!(upstream.accepted.load(Ordering::Acquire), 0);

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream).await;
}

#[tokio::test]
async fn upstream_handshake_rejection_remains_a_normal_proxy_response() {
    let upstream = spawn_upstream(UpstreamBehavior::Forbidden).await;
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let config = write_config(
        directory.path(),
        &proxy_config(port, upstream.address, 1, 5_000),
    );
    let (shutdown, task) = spawn_data_plane(&config);
    wait_for_gateway(port).await;

    let (mut stream, head, mut body) = send_handshake(port, &valid_handshake("/")).await;
    assert!(head.starts_with("HTTP/1.1 403"), "{head}");
    while body.len() < 6 {
        let mut chunk = [0_u8; 6];
        let read = stream.read(&mut chunk).await.expect("read rejection body");
        assert_ne!(read, 0);
        body.extend_from_slice(&chunk[..read]);
    }
    assert_eq!(&body[..6], b"denied");

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream).await;
}

#[tokio::test]
async fn invalid_upstream_101_is_replaced_without_metadata_disclosure() {
    let upstream = spawn_upstream(UpstreamBehavior::InvalidUpgrade).await;
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let config = write_config(
        directory.path(),
        &proxy_config(port, upstream.address, 1, 5_000),
    );
    let (shutdown, task) = spawn_data_plane(&config);
    wait_for_gateway(port).await;

    let (_stream, head, body) = send_handshake(port, &valid_handshake("/")).await;
    assert!(head.starts_with("HTTP/1.1 502"), "{head}");
    assert!(!head.to_ascii_lowercase().contains("x-secret"));
    assert!(!body.windows(6).any(|bytes| bytes == b"secret"));

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream).await;
}

#[tokio::test]
async fn tunnel_holds_upstream_admission_until_it_closes() {
    let upstream = spawn_upstream(UpstreamBehavior::Echo("upstream-ready")).await;
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let config = write_config(
        directory.path(),
        &proxy_config(port, upstream.address, 1, 5_000),
    );
    let (shutdown, task) = spawn_data_plane(&config);
    wait_for_gateway(port).await;

    let (first, first_head, _) = send_handshake(port, &valid_handshake("/first")).await;
    assert!(first_head.starts_with("HTTP/1.1 101"), "{first_head}");
    let (_second, second_head, _) = send_handshake(port, &valid_handshake("/second")).await;
    assert!(second_head.starts_with("HTTP/1.1 503"), "{second_head}");
    drop(first);

    timeout(Duration::from_secs(2), async {
        loop {
            let (stream, head, _) = send_handshake(port, &valid_handshake("/third")).await;
            if head.starts_with("HTTP/1.1 101") {
                drop(stream);
                return;
            }
            assert!(head.starts_with("HTTP/1.1 503"), "{head}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("admission recovers after tunnel closes");

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream).await;
}

#[tokio::test]
async fn tunnel_holds_upstream_physical_connection_capacity_until_it_closes() {
    let upstream = spawn_upstream(UpstreamBehavior::Echo("upstream-ready")).await;
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let config = write_config(
        directory.path(),
        &proxy_config(port, upstream.address, 2, 5_000),
    );
    let (shutdown, task) = spawn_data_plane(&config);
    wait_for_gateway(port).await;

    let (first, first_head, _) = send_handshake(port, &valid_handshake("/first")).await;
    assert!(first_head.starts_with("HTTP/1.1 101"), "{first_head}");
    wait_for_count(&upstream.accepted, 1, "first tunnel owns one socket").await;
    let (_second, second_head, _) = send_handshake(port, &valid_handshake("/second")).await;
    assert!(second_head.starts_with("HTTP/1.1 503"), "{second_head}");
    assert_eq!(upstream.accepted.load(Ordering::Acquire), 1);
    drop(first);

    timeout(Duration::from_secs(2), async {
        loop {
            let (stream, head, _) = send_handshake(port, &valid_handshake("/third")).await;
            if head.starts_with("HTTP/1.1 101") {
                drop(stream);
                return;
            }
            assert!(head.starts_with("HTTP/1.1 503"), "{head}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("physical connection capacity recovers after tunnel closes");
    assert_eq!(upstream.accepted.load(Ordering::Acquire), 2);

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream).await;
}

#[tokio::test]
async fn shutdown_and_hard_lifetime_close_active_tunnels() {
    let upstream = spawn_upstream(UpstreamBehavior::Echo("upstream-ready")).await;
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let config = write_config(
        directory.path(),
        &proxy_config(port, upstream.address, 1, 250),
    );
    let (shutdown, task) = spawn_data_plane(&config);
    wait_for_gateway(port).await;

    let started = Instant::now();
    let (mut stream, head, _) = send_handshake(port, &valid_handshake("/lifetime")).await;
    assert!(head.starts_with("HTTP/1.1 101"), "{head}");
    let mut byte = [0_u8; 1];
    assert_eq!(
        timeout(Duration::from_secs(2), stream.read(&mut byte))
            .await
            .expect("hard lifetime closes tunnel")
            .expect("read hard-lifetime close"),
        0
    );
    assert!(started.elapsed() < Duration::from_secs(2));

    let (mut stream, head, _) = send_handshake(port, &valid_handshake("/shutdown")).await;
    assert!(head.starts_with("HTTP/1.1 101"), "{head}");
    let shutdown_started = Instant::now();
    shutdown.send(()).expect("signal data-plane shutdown");
    let read = timeout(Duration::from_secs(2), stream.read(&mut byte));
    let joined = timeout(Duration::from_secs(2), task);
    let (read, joined) = tokio::join!(read, joined);
    assert_eq!(
        read.expect("shutdown closes tunnel")
            .expect("read shutdown close"),
        0
    );
    joined
        .expect("data plane returns inside drain budget")
        .expect("data-plane task joins")
        .expect("data plane stops cleanly");
    assert!(shutdown_started.elapsed() < Duration::from_secs(2));

    stop_upstream(upstream).await;
}
