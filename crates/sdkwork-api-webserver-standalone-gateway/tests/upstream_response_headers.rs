use std::{
    convert::Infallible,
    fs,
    net::{SocketAddr, TcpListener as StdTcpListener},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use axum::{body::Body, http::Response};
use hyper::{server::conn::http2, service::service_fn};
use hyper_util::rt::{TokioExecutor, TokioIo};
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair, KeyUsagePurpose,
};
use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
    version::{TLS12, TLS13},
    ServerConfig,
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
use tokio_rustls::TlsAcceptor;

type DataPlaneTask = JoinHandle<Result<(), DataPlaneError>>;
type UpstreamTask = JoinHandle<()>;

struct TestCertificateAuthority {
    certificate: Certificate,
    key: KeyPair,
    params: CertificateParams,
}

struct TestTlsIdentity {
    certificate: CertificateDer<'static>,
    private_key: PrivateKeyDer<'static>,
}

struct TestUpstream {
    address: SocketAddr,
    accepted: Arc<AtomicUsize>,
    health_healthy: Arc<AtomicBool>,
    shutdown: oneshot::Sender<()>,
    task: UpstreamTask,
}

fn available_port() -> u16 {
    let listener = StdTcpListener::bind("127.0.0.1:0").expect("reserve an available port");
    listener.local_addr().expect("read reserved port").port()
}

fn write_config(directory: &Path, config: &Value) -> PathBuf {
    let path = directory.join("sdkwork.webserver.config.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(config).expect("serialize data-plane config"),
    )
    .expect("write data-plane config");
    path
}

fn proxy_config(
    port: u16,
    target: String,
    max_response_header_bytes: usize,
    max_response_headers: usize,
    passive_failure_threshold: usize,
) -> Value {
    json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "upstream-response-header-test",
        "limits": {
            "maxConcurrentRequests": 32,
            "requestTimeoutMs": 5_000,
            "drainTimeoutMs": 1_000,
            "maxConnections": 64
        },
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "test-host",
            "maxConnections": 32
        }],
        "resources": [{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "bounded-upstream"
        }],
        "upstreams": [{
            "id": "bounded-upstream",
            "targets": [{"url": target}],
            "addressPolicy": {"allowedCidrs": ["127.0.0.0/8", "::1/128"]},
            "connectTimeoutMs": 2_000,
            "requestTimeoutMs": 5_000,
            "maxConnections": 4,
            "maxIdleConnections": 4,
            "maxResponseHeaderBytes": max_response_header_bytes,
            "maxResponseHeaders": max_response_headers,
            "idleConnectionTimeoutMs": 1_000,
            "maxInFlightRequests": 16,
            "passiveHealth": {
                "failureThreshold": passive_failure_threshold,
                "ejectionTimeMs": 200,
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

async fn wait_for_gateway(port: u16) {
    timeout(Duration::from_secs(5), async {
        loop {
            if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("gateway becomes reachable");
}

async fn stop_data_plane(shutdown: oneshot::Sender<()>, task: DataPlaneTask) {
    shutdown.send(()).expect("signal data-plane shutdown");
    timeout(Duration::from_secs(3), task)
        .await
        .expect("data plane stops within its drain budget")
        .expect("data-plane task joins")
        .expect("data plane stops cleanly");
}

async fn stop_upstream(upstream: TestUpstream) {
    upstream
        .shutdown
        .send(())
        .expect("signal upstream shutdown");
    timeout(Duration::from_secs(3), upstream.task)
        .await
        .expect("upstream stops")
        .expect("upstream task joins");
}

async fn read_request_path(stream: &mut TcpStream) -> Option<String> {
    let mut bytes = Vec::with_capacity(1024);
    let mut buffer = [0_u8; 1024];
    loop {
        let read = stream.read(&mut buffer).await.ok()?;
        if read == 0 {
            return None;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8(bytes).ok()?;
            return head
                .lines()
                .next()?
                .split_whitespace()
                .nth(1)
                .map(str::to_owned);
        }
        if bytes.len() > 64 * 1024 {
            return None;
        }
    }
}

fn http1_response(path: &str, health_healthy: bool) -> String {
    let effective_path = if path == "/health" && !health_healthy {
        "/count"
    } else {
        path
    };
    match effective_path {
        "/count" => concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Length: 11\r\n",
            "Connection: close\r\n",
            "X-Origin-Secret: must-not-forward\r\n",
            "X-Count-1: one\r\n",
            "X-Count-2: two\r\n",
            "X-Count-3: three\r\n",
            "X-Count-4: four\r\n",
            "\r\n",
            "secret-body"
        )
        .to_owned(),
        "/bytes" => format!(
            "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\nX-Origin-Secret: {}\r\n\r\nsecret-body",
            "a".repeat(8_200)
        ),
        "/wide" => format!(
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\nX-Wide: {}\r\n\r\nok",
            "a".repeat(9_000)
        ),
        _ => concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Length: 2\r\n",
            "Connection: close\r\n",
            "X-Origin: normal\r\n",
            "\r\n",
            "ok"
        )
        .to_owned(),
    }
}

async fn spawn_http1_upstream() -> TestUpstream {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind HTTP/1 upstream");
    let address = listener.local_addr().expect("HTTP/1 upstream address");
    let accepted = Arc::new(AtomicUsize::new(0));
    let health_healthy = Arc::new(AtomicBool::new(false));
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let task_accepted = accepted.clone();
    let task_health_healthy = health_healthy.clone();
    let task = tokio::spawn(async move {
        let mut connections = JoinSet::new();
        loop {
            let accepted_socket = tokio::select! {
                _ = &mut shutdown_rx => break,
                accepted_socket = listener.accept() => accepted_socket,
            };
            let Ok((mut stream, _)) = accepted_socket else {
                continue;
            };
            task_accepted.fetch_add(1, Ordering::AcqRel);
            let health_healthy = task_health_healthy.load(Ordering::Acquire);
            connections.spawn(async move {
                let Some(path) = read_request_path(&mut stream).await else {
                    return;
                };
                let response = http1_response(&path, health_healthy);
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    });
    TestUpstream {
        address,
        accepted,
        health_healthy,
        shutdown: shutdown_tx,
        task,
    }
}

fn write_test_ca(directory: &Path) -> TestCertificateAuthority {
    let mut params = CertificateParams::new(Vec::new()).expect("CA certificate parameters");
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, "response Header test CA");
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
    ];
    let key = KeyPair::generate().expect("generate CA key");
    let certificate = params.self_signed(&key).expect("self-sign test CA");
    fs::write(directory.join("upstream-ca.pem"), certificate.pem()).expect("write upstream CA");
    TestCertificateAuthority {
        certificate,
        key,
        params,
    }
}

fn signed_server_identity(authority: &TestCertificateAuthority) -> TestTlsIdentity {
    let mut params =
        CertificateParams::new(vec!["localhost".to_owned()]).expect("server certificate params");
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, "localhost");
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let key = KeyPair::generate().expect("generate server key");
    let certificate = params
        .signed_by(&key, &Issuer::new(authority.params.clone(), &authority.key))
        .expect("sign server identity");
    TestTlsIdentity {
        certificate: CertificateDer::from(certificate.der().to_vec()),
        private_key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der())),
    }
}

async fn spawn_h2_tls_upstream(identity: TestTlsIdentity) -> TestUpstream {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mut tls = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&TLS13, &TLS12])
        .expect("build TLS version policy")
        .with_no_client_auth()
        .with_single_cert(vec![identity.certificate], identity.private_key)
        .expect("build H2 server identity");
    tls.alpn_protocols = vec![b"h2".to_vec()];

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind HTTPS/H2 upstream");
    let address = listener.local_addr().expect("HTTPS/H2 upstream address");
    let acceptor = TlsAcceptor::from(Arc::new(tls));
    let accepted = Arc::new(AtomicUsize::new(0));
    let health_healthy = Arc::new(AtomicBool::new(true));
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let task_accepted = accepted.clone();
    let task = tokio::spawn(async move {
        let mut connections = JoinSet::new();
        loop {
            let accepted_socket = tokio::select! {
                _ = &mut shutdown_rx => break,
                accepted_socket = listener.accept() => accepted_socket,
            };
            let Ok((stream, _)) = accepted_socket else {
                continue;
            };
            task_accepted.fetch_add(1, Ordering::AcqRel);
            let acceptor = acceptor.clone();
            connections.spawn(async move {
                let Ok(stream) = acceptor.accept(stream).await else {
                    return;
                };
                let service = service_fn(|request| async move {
                    let mut response = Response::new(Body::from("ok"));
                    match request.uri().path() {
                        "/count" => {
                            response.headers_mut().insert(
                                "x-origin-secret",
                                "must-not-forward".parse().expect("secret Header"),
                            );
                            for name in [
                                "x-count-1",
                                "x-count-2",
                                "x-count-3",
                                "x-count-4",
                                "x-count-5",
                            ] {
                                response
                                    .headers_mut()
                                    .insert(name, "value".parse().expect("count Header"));
                            }
                        }
                        "/oversize" => {
                            response.headers_mut().insert(
                                "x-origin-secret",
                                vec![b'a'; 8_200]
                                    .try_into()
                                    .expect("oversized but syntactically valid Header"),
                            );
                        }
                        _ => {
                            response
                                .headers_mut()
                                .insert("x-origin", "normal".parse().expect("normal Header"));
                        }
                    }
                    Ok::<_, Infallible>(response)
                });
                let _ = http2::Builder::new(TokioExecutor::new())
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    });
    TestUpstream {
        address,
        accepted,
        health_healthy,
        shutdown: shutdown_tx,
        task,
    }
}

async fn gateway_get(client: &reqwest::Client, port: u16, path: &str) -> reqwest::Response {
    client
        .get(format!("http://127.0.0.1:{port}{path}"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("gateway returns a bounded response")
}

#[tokio::test]
async fn http1_limits_headers_without_forwarding_and_passively_recovers() {
    let upstream = spawn_http1_upstream().await;
    let directory = TempDir::new().expect("create test directory");
    let port = available_port();
    let config = proxy_config(
        port,
        format!("http://127.0.0.1:{}", upstream.address.port()),
        8_192,
        4,
        1,
    );
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    wait_for_gateway(port).await;
    let client = reqwest::Client::new();

    let normal = gateway_get(&client, port, "/ok").await;
    assert_eq!(normal.status(), reqwest::StatusCode::OK);
    assert_eq!(normal.text().await.expect("read normal body"), "ok");

    let count = gateway_get(&client, port, "/count").await;
    assert_eq!(count.status(), reqwest::StatusCode::BAD_GATEWAY);
    assert!(count.headers().get("x-origin-secret").is_none());
    assert_eq!(
        count.text().await.expect("read local error"),
        "upstream failed\n"
    );

    let ejected = gateway_get(&client, port, "/ok").await;
    assert_eq!(ejected.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(
        gateway_get(&client, port, "/ok").await.status(),
        reqwest::StatusCode::OK
    );

    let bytes = gateway_get(&client, port, "/bytes").await;
    assert_eq!(bytes.status(), reqwest::StatusCode::BAD_GATEWAY);
    assert!(bytes.headers().get("x-origin-secret").is_none());
    assert_eq!(
        bytes.text().await.expect("read local error"),
        "upstream failed\n"
    );
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(
        gateway_get(&client, port, "/ok").await.status(),
        reqwest::StatusCode::OK
    );

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream).await;
}

#[tokio::test]
async fn https_h2_rejects_field_count_and_header_list_then_recovers() {
    let directory = TempDir::new().expect("create test directory");
    let authority = write_test_ca(directory.path());
    let identity = signed_server_identity(&authority);
    let upstream = spawn_h2_tls_upstream(identity).await;
    let port = available_port();
    let mut config = proxy_config(
        port,
        format!("https://localhost:{}", upstream.address.port()),
        8_192,
        4,
        100,
    );
    config["upstreams"][0]["tls"] = json!({
        "trustMode": "custom",
        "caCertificateFiles": ["upstream-ca.pem"],
        "minimumVersion": "tls1.2",
        "maximumVersion": "tls1.3"
    });
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    wait_for_gateway(port).await;
    let client = reqwest::Client::new();

    let count = gateway_get(&client, port, "/count").await;
    assert_eq!(count.status(), reqwest::StatusCode::BAD_GATEWAY);
    assert!(count.headers().get("x-origin-secret").is_none());
    assert_eq!(
        count.text().await.expect("read count failure"),
        "upstream failed\n"
    );
    assert_eq!(
        gateway_get(&client, port, "/ok").await.status(),
        reqwest::StatusCode::OK
    );

    let oversized = gateway_get(&client, port, "/oversize").await;
    assert_eq!(oversized.status(), reqwest::StatusCode::BAD_GATEWAY);
    assert!(oversized.headers().get("x-origin-secret").is_none());
    assert_eq!(
        oversized.text().await.expect("read Header List failure"),
        "upstream failed\n"
    );
    assert_eq!(
        gateway_get(&client, port, "/ok").await.status(),
        reqwest::StatusCode::OK
    );
    assert!(upstream.accepted.load(Ordering::Acquire) >= 1);

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream).await;
}

#[tokio::test]
async fn oversized_active_health_response_marks_target_unhealthy_and_recovers() {
    let upstream = spawn_http1_upstream().await;
    let directory = TempDir::new().expect("create test directory");
    let port = available_port();
    let mut config = proxy_config(
        port,
        format!("http://127.0.0.1:{}", upstream.address.port()),
        8_192,
        4,
        100,
    );
    config["upstreams"][0]["activeHealth"] = json!({
        "method": "GET",
        "uri": "/health",
        "intervalMs": 100,
        "timeoutMs": 100,
        "unhealthyThreshold": 1,
        "healthyThreshold": 1,
        "successStatusMin": 200,
        "successStatusMax": 299,
        "maxResponseBodyBytes": 16
    });
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    wait_for_gateway(port).await;
    let client = reqwest::Client::new();

    timeout(Duration::from_secs(5), async {
        loop {
            if gateway_get(&client, port, "/business").await.status()
                == reqwest::StatusCode::SERVICE_UNAVAILABLE
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("oversized health response makes target unavailable");

    upstream.health_healthy.store(true, Ordering::Release);
    timeout(Duration::from_secs(5), async {
        loop {
            if gateway_get(&client, port, "/business").await.status() == reqwest::StatusCode::OK {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("bounded healthy observation restores target");

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream).await;
}

#[tokio::test]
async fn watch_publishes_new_header_budgets_and_retains_valid_generation() {
    let upstream = spawn_http1_upstream().await;
    let directory = TempDir::new().expect("create test directory");
    let port = available_port();
    let target = format!("http://127.0.0.1:{}", upstream.address.port());
    let mut wide = proxy_config(port, target.clone(), 16_384, 100, 100);
    wide["deployment"] = json!({
        "drainTimeoutMs": 1_000,
        "reload": {"mode": "watch", "pollIntervalMs": 100}
    });
    let path = write_config(directory.path(), &wide);
    let (shutdown, task) = spawn_watched_data_plane(&path);
    wait_for_gateway(port).await;
    let client = reqwest::Client::new();
    assert_eq!(
        gateway_get(&client, port, "/wide").await.status(),
        reqwest::StatusCode::OK
    );

    let mut narrow = proxy_config(port, target.clone(), 8_192, 100, 100);
    narrow["deployment"] = wide["deployment"].clone();
    write_config(directory.path(), &narrow);
    timeout(Duration::from_secs(5), async {
        loop {
            if gateway_get(&client, port, "/wide").await.status()
                == reqwest::StatusCode::BAD_GATEWAY
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("Watch publishes client with narrow response Header budget");

    write_config(directory.path(), &wide);
    timeout(Duration::from_secs(5), async {
        loop {
            if gateway_get(&client, port, "/wide").await.status() == reqwest::StatusCode::OK {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("Watch publishes client with restored response Header budget");

    let mut invalid = proxy_config(port, target, 8_191, 100, 100);
    invalid["deployment"] = wide["deployment"].clone();
    write_config(directory.path(), &invalid);
    for _ in 0..4 {
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            gateway_get(&client, port, "/wide").await.status(),
            reqwest::StatusCode::OK,
            "invalid candidate must not replace the active generation"
        );
    }

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream).await;
}
