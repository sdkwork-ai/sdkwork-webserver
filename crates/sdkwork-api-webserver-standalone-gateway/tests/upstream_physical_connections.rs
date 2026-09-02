use std::{
    convert::Infallible,
    fs,
    net::{SocketAddr, TcpListener as StdTcpListener},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use axum::{body::Body, http::Response};
use bytes::Bytes;
use http_body_util::channel::Channel;
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
    sync::{oneshot, Mutex, Notify},
    task::{JoinHandle, JoinSet},
    time::timeout,
};
use tokio_rustls::TlsAcceptor;

type DataPlaneTask = JoinHandle<Result<(), DataPlaneError>>;
type UpstreamTask = JoinHandle<()>;

struct ConnectionGuard(Arc<AtomicUsize>);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

struct CountedUpstream {
    address: SocketAddr,
    accepted: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    shutdown: oneshot::Sender<()>,
    task: UpstreamTask,
}

struct TestCertificateAuthority {
    certificate: Certificate,
    key: KeyPair,
    params: CertificateParams,
}

struct TestTlsIdentity {
    certificate: CertificateDer<'static>,
    private_key: PrivateKeyDer<'static>,
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

fn proxy_config(port: u16, targets: Vec<String>, idle_timeout_ms: u64) -> Value {
    json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "physical-connection-test",
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
            "targets": targets.into_iter().map(|url| json!({"url": url})).collect::<Vec<_>>(),
            "addressPolicy": {"allowedCidrs": ["127.0.0.0/8", "::1/128"]},
            "connectTimeoutMs": 2_000,
            "requestTimeoutMs": 5_000,
            "maxConnections": 1,
            "maxIdleConnections": 1,
            "idleConnectionTimeoutMs": idle_timeout_ms,
            "maxInFlightRequests": 16,
            "passiveHealth": {
                "failureThreshold": 1,
                "ejectionTimeMs": 5_000,
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

async fn stop_data_plane(shutdown: oneshot::Sender<()>, task: DataPlaneTask) {
    shutdown.send(()).expect("signal data-plane shutdown");
    timeout(Duration::from_secs(3), task)
        .await
        .expect("data plane stops within its drain budget")
        .expect("data-plane task joins")
        .expect("data plane stops cleanly");
}

async fn stop_upstream(upstream: CountedUpstream) {
    upstream
        .shutdown
        .send(())
        .expect("signal upstream shutdown");
    timeout(Duration::from_secs(3), upstream.task)
        .await
        .expect("upstream stops")
        .expect("upstream task joins");
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

async fn wait_for_at_least(counter: &AtomicUsize, expected: usize, context: &str) {
    timeout(Duration::from_secs(3), async {
        while counter.load(Ordering::Acquire) < expected {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "{context}: expected at least {expected}, observed {}",
            counter.load(Ordering::Acquire)
        )
    });
}

async fn wait_for_gateway(client: &reqwest::Client, url: &str) {
    timeout(Duration::from_secs(5), async {
        loop {
            if client
                .get(url)
                .header("host", "test.localhost")
                .send()
                .await
                .is_ok()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("gateway becomes reachable");
}

async fn read_request_head(stream: &mut TcpStream) -> Option<String> {
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
            return head.lines().next().map(str::to_owned);
        }
        if bytes.len() > 64 * 1024 {
            return None;
        }
    }
}

fn request_path(request_line: &str) -> &str {
    request_line.split_whitespace().nth(1).unwrap_or("/")
}

async fn spawn_controlled_http1_upstream(
) -> (CountedUpstream, oneshot::Sender<()>, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind controlled HTTP/1 upstream");
    let address = listener.local_addr().expect("controlled upstream address");
    let accepted = Arc::new(AtomicUsize::new(0));
    let active = Arc::new(AtomicUsize::new(0));
    let held = Arc::new(AtomicBool::new(false));
    let health_requests = Arc::new(AtomicUsize::new(0));
    let (release_tx, release_rx) = oneshot::channel();
    let release = Arc::new(Mutex::new(Some(release_rx)));
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let task_accepted = accepted.clone();
    let task_active = active.clone();
    let task_health_requests = health_requests.clone();
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
            task_active.fetch_add(1, Ordering::AcqRel);
            let active = task_active.clone();
            let held = held.clone();
            let release = release.clone();
            let health_requests = task_health_requests.clone();
            connections.spawn(async move {
                let _guard = ConnectionGuard(active);
                while let Some(line) = read_request_head(&mut stream).await {
                    let path = request_path(&line);
                    if path == "/health" {
                        health_requests.fetch_add(1, Ordering::AcqRel);
                        stream
                            .write_all(
                                b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: keep-alive\r\n\r\nok",
                            )
                            .await
                            .expect("write active-health response");
                        continue;
                    }
                    if path == "/held" && !held.swap(true, Ordering::AcqRel) {
                        stream
                            .write_all(
                                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n5\r\nstart\r\n",
                            )
                            .await
                            .expect("write held response prefix");
                        let receiver = release.lock().await.take();
                        if let Some(receiver) = receiver {
                            let _ = receiver.await;
                        }
                        stream
                            .write_all(b"3\r\nend\r\n0\r\n\r\n")
                            .await
                            .expect("complete held response");
                        continue;
                    }

                    let body = path.trim_start_matches('/');
                    let close = path == "/after-expiry";
                    let connection = if close { "close" } else { "keep-alive" };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: {connection}\r\n\r\n{body}",
                        body.len()
                    );
                    stream
                        .write_all(response.as_bytes())
                        .await
                        .expect("write controlled response");
                    if close {
                        break;
                    }
                }
            });
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    });
    (
        CountedUpstream {
            address,
            accepted,
            active,
            shutdown: shutdown_tx,
            task,
        },
        release_tx,
        health_requests,
    )
}

async fn spawn_labeled_http1_upstream(label: &'static str) -> CountedUpstream {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind labeled HTTP/1 upstream");
    let address = listener.local_addr().expect("labeled upstream address");
    let accepted = Arc::new(AtomicUsize::new(0));
    let active = Arc::new(AtomicUsize::new(0));
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let task_accepted = accepted.clone();
    let task_active = active.clone();
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
            task_active.fetch_add(1, Ordering::AcqRel);
            let active = task_active.clone();
            connections.spawn(async move {
                let _guard = ConnectionGuard(active);
                while read_request_head(&mut stream).await.is_some() {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{label}",
                        label.len()
                    );
                    if stream.write_all(response.as_bytes()).await.is_err() {
                        break;
                    }
                }
            });
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    });
    CountedUpstream {
        address,
        accepted,
        active,
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
        .push(DnType::CommonName, "physical connection test CA");
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

async fn spawn_h2_tls_upstream(identity: TestTlsIdentity) -> (CountedUpstream, Arc<Notify>) {
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
    let active = Arc::new(AtomicUsize::new(0));
    let held = Arc::new(AtomicBool::new(false));
    let release = Arc::new(Notify::new());
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let task_accepted = accepted.clone();
    let task_active = active.clone();
    let task_release = release.clone();
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
            task_active.fetch_add(1, Ordering::AcqRel);
            let acceptor = acceptor.clone();
            let active = task_active.clone();
            let held = held.clone();
            let release = task_release.clone();
            connections.spawn(async move {
                let _guard = ConnectionGuard(active);
                let Ok(stream) = acceptor.accept(stream).await else {
                    return;
                };
                let service = service_fn(move |_request| {
                    let held = held.clone();
                    let release = release.clone();
                    async move {
                        if !held.swap(true, Ordering::AcqRel) {
                            let (mut sender, body) = Channel::<Bytes>::new(2);
                            sender
                                .try_send(http_body::Frame::data(Bytes::from_static(b"start")))
                                .expect("queue H2 response prefix");
                            tokio::spawn(async move {
                                release.notified().await;
                                let _ = sender
                                    .try_send(http_body::Frame::data(Bytes::from_static(b"end")));
                            });
                            Ok::<_, Infallible>(Response::new(Body::new(body)))
                        } else {
                            Ok::<_, Infallible>(Response::new(Body::from("parallel")))
                        }
                    }
                });
                let _ = http2::Builder::new(TokioExecutor::new())
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    });
    (
        CountedUpstream {
            address,
            accepted,
            active,
            shutdown: shutdown_tx,
            task,
        },
        release,
    )
}

async fn spawn_streaming_h2_tls_upstream(
    identity: TestTlsIdentity,
    label: &'static str,
) -> (CountedUpstream, Arc<Notify>) {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mut tls = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&TLS13, &TLS12])
        .expect("build TLS version policy")
        .with_no_client_auth()
        .with_single_cert(vec![identity.certificate], identity.private_key)
        .expect("build streaming H2 server identity");
    tls.alpn_protocols = vec![b"h2".to_vec()];

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind streaming HTTPS/H2 upstream");
    let address = listener
        .local_addr()
        .expect("streaming HTTPS/H2 upstream address");
    let acceptor = TlsAcceptor::from(Arc::new(tls));
    let accepted = Arc::new(AtomicUsize::new(0));
    let active = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(Notify::new());
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let task_accepted = accepted.clone();
    let task_active = active.clone();
    let task_release = release.clone();
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
            task_active.fetch_add(1, Ordering::AcqRel);
            let acceptor = acceptor.clone();
            let active = task_active.clone();
            let release = task_release.clone();
            connections.spawn(async move {
                let _guard = ConnectionGuard(active);
                let Ok(stream) = acceptor.accept(stream).await else {
                    return;
                };
                let service = service_fn(move |_request| {
                    let release = release.clone();
                    async move {
                        let (mut sender, body) = Channel::<Bytes>::new(1);
                        tokio::spawn(async move {
                            release.notified().await;
                            let _ = sender.try_send(http_body::Frame::data(Bytes::from_static(
                                label.as_bytes(),
                            )));
                        });
                        let mut response = Response::new(Body::new(body));
                        response.headers_mut().insert(
                            "x-sdkwork-test-upstream",
                            label.parse().expect("valid upstream label Header"),
                        );
                        Ok::<_, Infallible>(response)
                    }
                });
                let _ = http2::Builder::new(TokioExecutor::new())
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    });
    (
        CountedUpstream {
            address,
            accepted,
            active,
            shutdown: shutdown_tx,
            task,
        },
        release,
    )
}

#[tokio::test]
async fn saturated_target_does_not_consume_another_targets_capacity() {
    let (first, release_first, _first_health) = spawn_controlled_http1_upstream().await;
    let (second, release_second, _second_health) = spawn_controlled_http1_upstream().await;
    let directory = TempDir::new().expect("create test directory");
    let port = available_port();
    let mut config = proxy_config(
        port,
        vec![
            format!("http://127.0.0.1:{}", first.address.port()),
            format!("http://127.0.0.1:{}", second.address.port()),
        ],
        1_000,
    );
    config["upstreams"][0]["maxConnections"] = json!(3);
    config["upstreams"][0]["maxIdleConnections"] = json!(2);
    config["upstreams"][0]["targets"][0]["maxConnections"] = json!(1);
    config["upstreams"][0]["targets"][1]["maxConnections"] = json!(1);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::builder().no_proxy().build().expect("client");
    let base_url = format!("http://127.0.0.1:{port}");

    let held = client
        .get(format!("{base_url}/held"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("first target starts a held response");
    assert_eq!(held.status(), reqwest::StatusCode::OK);
    wait_for_count(&first.accepted, 1, "first target connection").await;

    let second_response = client
        .get(format!("{base_url}/second-target"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("second target retains independent capacity");
    assert_eq!(second_response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        second_response.text().await.expect("read second target"),
        "second-target"
    );
    wait_for_count(&second.accepted, 1, "second target connection").await;

    let saturated = client
        .get(format!("{base_url}/first-target-cap"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("round robin returns to saturated first target");
    assert_eq!(saturated.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(first.accepted.load(Ordering::Acquire), 1);
    assert_eq!(second.accepted.load(Ordering::Acquire), 1);

    release_first
        .send(())
        .expect("release first target response");
    assert_eq!(
        held.text().await.expect("complete first target response"),
        "startend"
    );
    drop(release_second);
    stop_data_plane(shutdown, task).await;
    wait_for_count(&first.active, 0, "shutdown drops first target permit").await;
    wait_for_count(&second.active, 0, "shutdown drops second target permit").await;
    stop_upstream(first).await;
    stop_upstream(second).await;
}

#[tokio::test]
async fn target_connection_limit_applies_before_aggregate_capacity_is_exhausted() {
    let (upstream, release, _health_requests) = spawn_controlled_http1_upstream().await;
    let directory = TempDir::new().expect("create test directory");
    let port = available_port();
    let mut config = proxy_config(
        port,
        vec![format!("http://127.0.0.1:{}", upstream.address.port())],
        1_000,
    );
    config["upstreams"][0]["maxConnections"] = json!(2);
    config["upstreams"][0]["maxIdleConnections"] = json!(2);
    config["upstreams"][0]["targets"][0]["maxConnections"] = json!(1);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::builder().no_proxy().build().expect("client");
    let base_url = format!("http://127.0.0.1:{port}");

    let held = client
        .get(format!("{base_url}/held"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("receive target-limited held response headers");
    assert_eq!(held.status(), reqwest::StatusCode::OK);
    wait_for_count(&upstream.accepted, 1, "one target physical connection").await;

    let started = Instant::now();
    let saturated = client
        .get(format!("{base_url}/target-cap"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("target saturation is a local response");
    assert_eq!(saturated.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(saturated.headers().get("retry-after").unwrap(), "1");
    assert_eq!(
        saturated.text().await.expect("read target saturation body"),
        "upstream connection capacity is saturated\n"
    );
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(upstream.accepted.load(Ordering::Acquire), 1);

    release.send(()).expect("release target-limited response");
    assert_eq!(
        held.text().await.expect("complete target-limited response"),
        "startend"
    );
    let recovered = client
        .get(format!("{base_url}/target-recovered"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("reuse target connection after response completion");
    assert_eq!(recovered.status(), reqwest::StatusCode::OK);
    assert_eq!(
        recovered.text().await.expect("read target recovery"),
        "target-recovered"
    );
    assert_eq!(upstream.accepted.load(Ordering::Acquire), 1);

    stop_data_plane(shutdown, task).await;
    wait_for_count(&upstream.active, 0, "shutdown drops target permit").await;
    stop_upstream(upstream).await;
}

#[tokio::test]
async fn bounds_http1_connections_through_streaming_idle_expiry_and_shutdown() {
    let (upstream, release, _health_requests) = spawn_controlled_http1_upstream().await;
    let directory = TempDir::new().expect("create test directory");
    let port = available_port();
    let config = proxy_config(
        port,
        vec![
            format!("http://127.0.0.1:{}", upstream.address.port()),
            format!("http://localhost:{}", upstream.address.port()),
        ],
        200,
    );
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::builder().no_proxy().build().expect("client");
    let base_url = format!("http://127.0.0.1:{port}");

    let held = client
        .get(format!("{base_url}/held"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("receive held response headers");
    assert_eq!(held.status(), reqwest::StatusCode::OK);
    wait_for_count(&upstream.accepted, 1, "first physical connection").await;

    let started = Instant::now();
    let saturated = client
        .get(format!("{base_url}/saturated"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("connection saturation is a local response");
    assert_eq!(saturated.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(saturated.headers().get("retry-after").unwrap(), "1");
    assert_eq!(
        saturated.text().await.expect("read saturation body"),
        "upstream connection capacity is saturated\n"
    );
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(upstream.accepted.load(Ordering::Acquire), 1);

    release.send(()).expect("release held upstream response");
    assert_eq!(
        held.text().await.expect("complete held response"),
        "startend"
    );

    let reused = client
        .get(format!("{base_url}/reused"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("reuse first upstream connection");
    assert_eq!(reused.text().await.expect("read reused response"), "reused");
    assert_eq!(upstream.accepted.load(Ordering::Acquire), 1);

    let idle_saturated = client
        .get(format!("{base_url}/idle-cap"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("idle connection retains physical capacity");
    assert_eq!(
        idle_saturated.status(),
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(upstream.accepted.load(Ordering::Acquire), 1);

    wait_for_count(
        &upstream.active,
        0,
        "idle expiry releases physical capacity",
    )
    .await;
    let after_expiry = client
        .get(format!("{base_url}/after-expiry"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("connect after idle expiry");
    assert_eq!(
        after_expiry
            .text()
            .await
            .expect("read post-expiry response"),
        "after-expiry"
    );
    wait_for_count(&upstream.active, 0, "Connection close releases permit").await;

    let recovered_target = client
        .get(format!("{base_url}/target-recovered"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("locally saturated target remains healthy");
    assert_eq!(
        recovered_target
            .text()
            .await
            .expect("read recovered target response"),
        "target-recovered"
    );
    assert_eq!(upstream.accepted.load(Ordering::Acquire), 3);

    stop_data_plane(shutdown, task).await;
    wait_for_count(&upstream.active, 0, "shutdown drops idle upstream pool").await;
    stop_upstream(upstream).await;
}

#[tokio::test]
async fn active_health_saturation_keeps_target_state_unchanged() {
    let (upstream, release, health_requests) = spawn_controlled_http1_upstream().await;
    let directory = TempDir::new().expect("create test directory");
    let port = available_port();
    let mut config = proxy_config(
        port,
        vec![format!("http://127.0.0.1:{}", upstream.address.port())],
        1_000,
    );
    config["upstreams"][0]["activeHealth"] = json!({
        "method": "GET",
        "uri": "/health",
        "intervalMs": 100,
        "timeoutMs": 100,
        "unhealthyThreshold": 1,
        "healthyThreshold": 5,
        "successStatusMin": 200,
        "successStatusMax": 299,
        "maxResponseBodyBytes": 16
    });
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::builder().no_proxy().build().expect("client");
    let base_url = format!("http://127.0.0.1:{port}");

    wait_for_at_least(&health_requests, 1, "initial active-health success").await;
    let held = client
        .get(format!("{base_url}/held"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("hold the only physical upstream connection");
    assert_eq!(held.status(), reqwest::StatusCode::OK);
    let checks_before_saturation = health_requests.load(Ordering::Acquire);
    tokio::time::sleep(Duration::from_millis(350)).await;
    assert_eq!(
        health_requests.load(Ordering::Acquire),
        checks_before_saturation,
        "saturated probes must not create hidden physical connections"
    );
    assert_eq!(upstream.accepted.load(Ordering::Acquire), 1);

    release.send(()).expect("release held business response");
    assert_eq!(
        held.text().await.expect("complete held response"),
        "startend"
    );
    let recovered = client
        .get(format!("{base_url}/after-health-saturation"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request target after local health-probe saturation");
    assert_eq!(recovered.status(), reqwest::StatusCode::OK);
    assert_eq!(
        recovered.text().await.expect("read recovered response"),
        "after-health-saturation"
    );

    stop_data_plane(shutdown, task).await;
    wait_for_count(&upstream.active, 0, "shutdown closes health-check pool").await;
    stop_upstream(upstream).await;
}

#[tokio::test]
async fn multiplexes_concurrent_https_h2_streams_on_one_physical_connection() {
    let directory = TempDir::new().expect("create test directory");
    let authority = write_test_ca(directory.path());
    let identity = signed_server_identity(&authority);
    let (upstream, release) = spawn_h2_tls_upstream(identity).await;
    let port = available_port();
    let mut config = proxy_config(
        port,
        vec![format!("https://localhost:{}", upstream.address.port())],
        1_000,
    );
    config["upstreams"][0]["tls"] = json!({
        "trustMode": "custom",
        "caCertificateFiles": ["upstream-ca.pem"],
        "minimumVersion": "tls1.2",
        "maximumVersion": "tls1.3"
    });
    config["upstreams"][0]["maxConnections"] = json!(2);
    config["upstreams"][0]["targets"][0]["maxConnections"] = json!(1);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::builder().no_proxy().build().expect("client");
    let base_url = format!("http://127.0.0.1:{port}");

    let held = client
        .get(format!("{base_url}/one"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("receive first H2 stream headers");
    assert_eq!(held.status(), reqwest::StatusCode::OK);
    wait_for_count(&upstream.accepted, 1, "one HTTPS/H2 physical connection").await;

    let parallel = client
        .get(format!("{base_url}/two"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("open a concurrent stream on the existing H2 connection");
    assert_eq!(parallel.status(), reqwest::StatusCode::OK);
    assert_eq!(
        parallel.text().await.expect("read parallel stream"),
        "parallel"
    );
    assert_eq!(upstream.accepted.load(Ordering::Acquire), 1);

    release.notify_waiters();
    assert_eq!(
        held.text().await.expect("complete held H2 stream"),
        "startend"
    );
    stop_data_plane(shutdown, task).await;
    wait_for_count(&upstream.active, 0, "shutdown closes H2 upstream pool").await;
    stop_upstream(upstream).await;
}

#[tokio::test]
async fn least_connections_counts_h2_streams_independently_from_physical_connections() {
    let directory = TempDir::new().expect("create test directory");
    let authority = write_test_ca(directory.path());
    let (primary, release_primary) =
        spawn_streaming_h2_tls_upstream(signed_server_identity(&authority), "primary").await;
    let (secondary, release_secondary) =
        spawn_streaming_h2_tls_upstream(signed_server_identity(&authority), "secondary").await;
    let port = available_port();
    let mut config = proxy_config(
        port,
        vec![
            format!("https://localhost:{}", primary.address.port()),
            format!("https://localhost:{}", secondary.address.port()),
        ],
        1_000,
    );
    config["upstreams"][0]["loadBalancing"] = json!("least-connections");
    config["upstreams"][0]["targets"][0]["weight"] = json!(2);
    config["upstreams"][0]["targets"][1]["weight"] = json!(1);
    config["upstreams"][0]["tls"] = json!({
        "trustMode": "custom",
        "caCertificateFiles": ["upstream-ca.pem"],
        "minimumVersion": "tls1.2",
        "maximumVersion": "tls1.3"
    });
    config["upstreams"][0]["maxConnections"] = json!(4);
    config["upstreams"][0]["maxIdleConnections"] = json!(4);
    config["upstreams"][0]["targets"][0]["maxConnections"] = json!(1);
    config["upstreams"][0]["targets"][1]["maxConnections"] = json!(1);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::builder().no_proxy().build().expect("client");
    let base_url = format!("http://127.0.0.1:{port}");

    let mut responses = Vec::new();
    for (index, expected) in ["primary", "secondary", "primary", "primary", "secondary"]
        .into_iter()
        .enumerate()
    {
        let response = client
            .get(format!("{base_url}/stream-{index}"))
            .header("host", "test.localhost")
            .send()
            .await
            .expect("receive concurrent upstream H2 Stream Headers");
        assert_eq!(
            response
                .headers()
                .get("x-sdkwork-test-upstream")
                .expect("upstream label Header"),
            expected
        );
        responses.push(response);
    }

    wait_for_count(&primary.accepted, 1, "one primary H2 physical connection").await;
    wait_for_count(
        &secondary.accepted,
        1,
        "one secondary H2 physical connection",
    )
    .await;
    assert_eq!(primary.active.load(Ordering::Acquire), 1);
    assert_eq!(secondary.active.load(Ordering::Acquire), 1);

    release_primary.notify_waiters();
    release_secondary.notify_waiters();
    for (response, expected) in
        responses
            .into_iter()
            .zip(["primary", "secondary", "primary", "primary", "secondary"])
    {
        assert_eq!(response.text().await.expect("complete H2 Stream"), expected);
    }

    stop_data_plane(shutdown, task).await;
    wait_for_count(&primary.active, 0, "shutdown closes primary H2 pool").await;
    wait_for_count(&secondary.active, 0, "shutdown closes secondary H2 pool").await;
    stop_upstream(primary).await;
    stop_upstream(secondary).await;
}

#[tokio::test]
async fn watch_replaces_connection_pool_and_closes_old_idle_generation() {
    let old = spawn_labeled_http1_upstream("old-generation").await;
    let new = spawn_labeled_http1_upstream("new-generation").await;
    let directory = TempDir::new().expect("create test directory");
    let port = available_port();
    let mut initial = proxy_config(
        port,
        vec![format!("http://127.0.0.1:{}", old.address.port())],
        10_000,
    );
    initial["deployment"] = json!({
        "drainTimeoutMs": 1_000,
        "reload": {"mode": "watch", "pollIntervalMs": 100}
    });
    initial["upstreams"][0]["maxConnections"] = json!(2);
    initial["upstreams"][0]["targets"][0]["maxConnections"] = json!(1);
    let path = write_config(directory.path(), &initial);
    let (shutdown, task) = spawn_watched_data_plane(&path);
    let client = reqwest::Client::builder().no_proxy().build().expect("client");
    let url = format!("http://127.0.0.1:{port}/generation");
    wait_for_gateway(&client, &url).await;

    let old_response = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request old generation");
    assert_eq!(
        old_response.text().await.expect("read old generation"),
        "old-generation"
    );
    wait_for_count(&old.active, 1, "old generation retains one idle connection").await;

    let mut replacement = proxy_config(
        port,
        vec![format!("http://127.0.0.1:{}", new.address.port())],
        10_000,
    );
    replacement["deployment"] = initial["deployment"].clone();
    replacement["upstreams"][0]["maxConnections"] = json!(2);
    replacement["upstreams"][0]["targets"][0]["maxConnections"] = json!(1);
    write_config(directory.path(), &replacement);
    timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(response) = client
                .get(&url)
                .header("host", "test.localhost")
                .send()
                .await
            {
                if response.text().await.ok().as_deref() == Some("new-generation") {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("Watch publishes the replacement generation");

    wait_for_count(&old.active, 0, "replacement drops old idle pool").await;
    assert_eq!(new.accepted.load(Ordering::Acquire), 1);
    assert_eq!(new.active.load(Ordering::Acquire), 1);

    stop_data_plane(shutdown, task).await;
    wait_for_count(&new.active, 0, "shutdown drops replacement idle pool").await;
    stop_upstream(old).await;
    stop_upstream(new).await;
}

#[tokio::test]
async fn watch_keeps_old_stream_alive_before_closing_retired_generation() {
    let (old, release, _health_requests) = spawn_controlled_http1_upstream().await;
    let new = spawn_labeled_http1_upstream("new-generation").await;
    let directory = TempDir::new().expect("create test directory");
    let port = available_port();
    let mut initial = proxy_config(
        port,
        vec![format!("http://127.0.0.1:{}", old.address.port())],
        10_000,
    );
    initial["deployment"] = json!({
        "drainTimeoutMs": 1_000,
        "reload": {"mode": "watch", "pollIntervalMs": 100}
    });
    initial["upstreams"][0]["maxConnections"] = json!(2);
    initial["upstreams"][0]["targets"][0]["maxConnections"] = json!(1);
    let path = write_config(directory.path(), &initial);
    let (shutdown, task) = spawn_watched_data_plane(&path);
    let client = reqwest::Client::builder().no_proxy().build().expect("client");
    let base_url = format!("http://127.0.0.1:{port}");
    wait_for_gateway(&client, &format!("{base_url}/ready")).await;

    let held = client
        .get(format!("{base_url}/held"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("receive old-generation streaming headers");
    assert_eq!(held.status(), reqwest::StatusCode::OK);

    let mut replacement = proxy_config(
        port,
        vec![format!("http://127.0.0.1:{}", new.address.port())],
        10_000,
    );
    replacement["deployment"] = initial["deployment"].clone();
    replacement["upstreams"][0]["maxConnections"] = json!(2);
    replacement["upstreams"][0]["targets"][0]["maxConnections"] = json!(1);
    write_config(directory.path(), &replacement);
    timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(response) = client
                .get(format!("{base_url}/generation"))
                .header("host", "test.localhost")
                .send()
                .await
            {
                if response.text().await.ok().as_deref() == Some("new-generation") {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("Watch publishes replacement while old Stream remains active");
    assert_eq!(
        old.active.load(Ordering::Acquire),
        1,
        "old generation remains alive until its streaming Body completes"
    );

    release.send(()).expect("complete old-generation Stream");
    assert_eq!(held.text().await.expect("read old Stream"), "startend");
    wait_for_count(
        &old.active,
        0,
        "completed Stream releases retired generation",
    )
    .await;

    stop_data_plane(shutdown, task).await;
    wait_for_count(&new.active, 0, "shutdown closes current generation").await;
    stop_upstream(old).await;
    stop_upstream(new).await;
}
