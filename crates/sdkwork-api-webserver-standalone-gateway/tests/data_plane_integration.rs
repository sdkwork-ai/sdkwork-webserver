use std::{
    fs,
    net::TcpListener,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use axum::{
    body::Body,
    http::{Request, Response, StatusCode, Version},
    routing::any,
    Router,
};
use bytes::Bytes;
use futures_util::StreamExt;
use http_body_util::{channel::Channel, BodyExt};
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair, KeyUsagePurpose,
};
use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName},
    server::WebPkiClientVerifier,
    version::{TLS12, TLS13},
    ClientConfig, RootCertStore, ServerConfig,
};
use sdkwork_api_webserver_standalone_gateway::{
    run_data_plane_from_config_until, run_data_plane_until,
};
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    sync::{oneshot, Notify},
    task::JoinHandle,
    time::timeout,
};
use tokio_rustls::{TlsAcceptor, TlsConnector};

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

static NEXT_TEST_PORT: AtomicU16 = AtomicU16::new(32_000);

fn available_port() -> u16 {
    for _ in 0..28_000 {
        let candidate = NEXT_TEST_PORT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(if current >= 59_999 {
                    32_000
                } else {
                    current + 1
                })
            })
            .expect("test port allocator must update");
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", candidate)) {
            drop(listener);
            return candidate;
        }
    }
    panic!("could not find a free test port in the reserved range");
}

async fn spawn_frame_echo_upstream() -> (std::net::SocketAddr, oneshot::Sender<()>, UpstreamTask) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind upstream");
    let address = listener.local_addr().expect("upstream address");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let app = Router::new()
            .route(
                "/echo",
                any(|request: Request<Body>| async move {
                    let trailer = request.headers().get(axum::http::header::TRAILER).cloned();
                    let (_, body) = request.into_parts();
                    let mut response = Response::new(body);
                    if let Some(trailer) = trailer {
                        response
                            .headers_mut()
                            .insert(axum::http::header::TRAILER, trailer);
                    }
                    response
                }),
            )
            .route(
                "/inspect",
                any(|request: Request<Body>| async move {
                    let mut body = request.into_body();
                    let mut observed = String::new();
                    while let Some(frame) = body.frame().await {
                        let frame = frame.expect("read proxied request frame");
                        if let Ok(trailers) = frame.into_trailers() {
                            if let Some(value) = trailers.get("x-checksum") {
                                observed = value.to_str().expect("ASCII trailer").to_owned();
                            }
                        }
                    }
                    Response::new(Body::from(observed))
                }),
            )
            .route(
                "/query",
                any(|request: Request<Body>| async move {
                    Response::new(Body::from(
                        request.uri().query().unwrap_or_default().to_owned(),
                    ))
                }),
            )
            .route(
                "/expect",
                any(|request: Request<Body>| async move {
                    let observed = if request.headers().contains_key(axum::http::header::EXPECT) {
                        "present"
                    } else {
                        "absent"
                    };
                    let _ = request.into_body().collect().await;
                    Response::new(Body::from(observed))
                }),
            )
            .route(
                "/emit",
                any(|| async move {
                    let mut trailers = axum::http::HeaderMap::new();
                    trailers.insert("x-checksum", "emitted".parse().expect("trailer value"));
                    let (mut sender, body) = Channel::<Bytes>::new(2);
                    sender
                        .try_send(http_body::Frame::data(Bytes::from_static(b"four")))
                        .expect("queue response data");
                    sender
                        .try_send(http_body::Frame::trailers(trailers))
                        .expect("queue response trailers");
                    drop(sender);
                    let mut response = Response::new(Body::new(body));
                    response.headers_mut().insert(
                        axum::http::header::TRAILER,
                        "X-Checksum".parse().expect("trailer declaration"),
                    );
                    response
                }),
            )
            .route(
                "/emit-many",
                any(|| async move {
                    let mut trailers = axum::http::HeaderMap::new();
                    trailers.insert("x-one", "1".parse().expect("first trailer"));
                    trailers.insert("x-two", "2".parse().expect("second trailer"));
                    let (mut sender, body) = Channel::<Bytes>::new(2);
                    sender
                        .try_send(http_body::Frame::data(Bytes::from_static(b"four")))
                        .expect("queue response data");
                    sender
                        .try_send(http_body::Frame::trailers(trailers))
                        .expect("queue response trailers");
                    drop(sender);
                    let mut response = Response::new(Body::new(body));
                    response.headers_mut().insert(
                        axum::http::header::TRAILER,
                        "X-One, X-Two".parse().expect("trailer declaration"),
                    );
                    response
                }),
            )
            .fallback(any(|request: Request<Body>| async move {
                Response::new(Body::from(request.uri().path().to_owned()))
            }));
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("serve upstream");
    });
    (address, shutdown_tx, task)
}

async fn spawn_tls_upstream(
    server_config: ServerConfig,
) -> (std::net::SocketAddr, oneshot::Sender<()>, UpstreamTask) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind TLS upstream");
    let address = listener.local_addr().expect("TLS upstream address");
    let acceptor = TlsAcceptor::from(Arc::new(server_config));
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            let accepted = tokio::select! {
                _ = &mut shutdown_rx => break,
                accepted = listener.accept() => accepted,
            };
            let Ok((stream, _)) = accepted else {
                continue;
            };
            let acceptor = acceptor.clone();
            connections.spawn(async move {
                let Ok(mut stream) = acceptor.accept(stream).await else {
                    return;
                };
                let headers = read_header_block(&mut stream).await;
                let request_line = headers
                    .split(|byte| *byte == b'\n')
                    .next()
                    .unwrap_or_default();
                let body = if request_line.starts_with(b"GET /secure ") {
                    "secure-upstream"
                } else {
                    "tls-upstream"
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("write TLS upstream response");
                let _ = stream.shutdown().await;
            });
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    });
    (address, shutdown_tx, task)
}

async fn spawn_held_response_upstream() -> (
    std::net::SocketAddr,
    Arc<Notify>,
    oneshot::Sender<()>,
    UpstreamTask,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind held-response upstream");
    let address = listener.local_addr().expect("held-response address");
    let release = Arc::new(Notify::new());
    let hold_next = Arc::new(AtomicBool::new(true));
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task_release = release.clone();
    let task = tokio::spawn(async move {
        let app = Router::new().fallback(any(move || {
            let release = task_release.clone();
            let hold = hold_next.swap(false, Ordering::AcqRel);
            async move {
                if !hold {
                    return Response::new(Body::from("recovered"));
                }
                let (mut sender, body) = Channel::<Bytes>::new(2);
                sender
                    .try_send(http_body::Frame::data(Bytes::from_static(b"start")))
                    .expect("queue held response prefix");
                tokio::spawn(async move {
                    release.notified().await;
                    let _ = sender.try_send(http_body::Frame::data(Bytes::from_static(b"end")));
                });
                Response::new(Body::new(body))
            }
        }));
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("serve held-response upstream");
    });
    (address, release, shutdown_tx, task)
}

async fn spawn_toggle_health_upstream(
    initially_healthy: bool,
    healthy_body: &'static str,
) -> (
    std::net::SocketAddr,
    Arc<AtomicBool>,
    Arc<AtomicUsize>,
    oneshot::Sender<()>,
    UpstreamTask,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind health upstream");
    let address = listener.local_addr().expect("health upstream address");
    let healthy = Arc::new(AtomicBool::new(initially_healthy));
    let requests = Arc::new(AtomicUsize::new(0));
    let task_healthy = healthy.clone();
    let task_requests = requests.clone();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let app = Router::new().fallback(any(move || {
            let healthy = task_healthy.clone();
            let requests = task_requests.clone();
            async move {
                requests.fetch_add(1, Ordering::AcqRel);
                if healthy.load(Ordering::Acquire) {
                    Response::new(Body::from(healthy_body))
                } else {
                    let mut response = Response::new(Body::from("target-unhealthy"));
                    *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
                    response
                }
            }
        }));
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("serve health upstream");
    });
    (address, healthy, requests, shutdown_tx, task)
}

async fn spawn_body_sink_upstream() -> (std::net::SocketAddr, oneshot::Sender<()>, UpstreamTask) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind Body sink upstream");
    let address = listener.local_addr().expect("Body sink upstream address");
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        loop {
            let accepted = tokio::select! {
                _ = &mut shutdown_rx => break,
                accepted = listener.accept() => accepted,
            };
            let Ok((mut stream, _)) = accepted else {
                continue;
            };
            tokio::spawn(async move {
                let mut buffer = [0_u8; 1024];
                let mut observed = 0_usize;
                loop {
                    let read = timeout(Duration::from_secs(5), stream.read(&mut buffer)).await;
                    match read {
                        Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
                        Ok(Ok(length)) => {
                            observed = observed.saturating_add(length);
                            if observed > 1024 * 1024 {
                                break;
                            }
                        }
                    }
                }
            });
        }
    });
    (address, shutdown_tx, task)
}

async fn spawn_early_response_upstream(
    statuses: Vec<u16>,
) -> (
    std::net::SocketAddr,
    Arc<AtomicUsize>,
    oneshot::Sender<()>,
    UpstreamTask,
) {
    assert!(!statuses.is_empty(), "early-response statuses are required");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind early-response upstream");
    let address = listener.local_addr().expect("early-response address");
    let accepted = Arc::new(AtomicUsize::new(0));
    let accepted_for_task = accepted.clone();
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            let accepted_socket = tokio::select! {
                _ = &mut shutdown_rx => break,
                accepted_socket = listener.accept() => accepted_socket,
            };
            let Ok((mut stream, _)) = accepted_socket else {
                continue;
            };
            let index = accepted_for_task.fetch_add(1, Ordering::AcqRel);
            let status = statuses[index.min(statuses.len() - 1)];
            connections.spawn(async move {
                let headers = read_header_block(&mut stream).await;
                assert!(
                    headers.starts_with(b"POST "),
                    "early-response upstream receives a POST"
                );
                let body = format!("early-{status}");
                let response = format!(
                    "HTTP/1.1 {status} Early\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("write early upstream response");

                let mut discarded = [0_u8; 1024];
                let _ = timeout(Duration::from_secs(2), async {
                    loop {
                        match stream.read(&mut discarded).await {
                            Ok(0) | Err(_) => break,
                            Ok(_) => {}
                        }
                    }
                })
                .await;
            });
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    });
    (address, accepted, shutdown_tx, task)
}

async fn wait_for_accepted_connections(accepted: &AtomicUsize, expected: usize) {
    timeout(Duration::from_secs(2), async {
        while accepted.load(Ordering::Acquire) < expected {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("upstream accepts the expected connections");
}

async fn stop_upstream(shutdown: oneshot::Sender<()>, task: UpstreamTask) {
    shutdown.send(()).expect("stop upstream");
    timeout(Duration::from_secs(3), task)
        .await
        .expect("upstream drains")
        .expect("upstream task joins");
}

fn write_config(directory: &Path, config: &Value) -> std::path::PathBuf {
    let path = directory.join("sdkwork.webserver.config.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(config).expect("serialize config"),
    )
    .expect("write config");
    path
}

fn write_self_signed_certificate(directory: &Path, stem: &str, names: &[&str]) -> Vec<u8> {
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
    fs::write(directory.join(format!("{stem}.pem")), certificate.pem()).expect("write certificate");
    fs::write(directory.join(format!("{stem}.key")), key.serialize_pem())
        .expect("write private key");
    certificate.der().as_ref().to_vec()
}

fn write_test_ca(directory: &Path, stem: &str) -> TestCertificateAuthority {
    let mut params = CertificateParams::new(Vec::new()).expect("CA certificate parameters");
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, format!("{stem} test CA"));
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
    ];
    let key = KeyPair::generate().expect("generate CA key");
    let certificate = params.self_signed(&key).expect("self-sign test CA");
    fs::write(directory.join(format!("{stem}.pem")), certificate.pem())
        .expect("write test CA certificate");
    TestCertificateAuthority {
        certificate,
        key,
        params,
    }
}

fn write_signed_identity(
    directory: &Path,
    stem: &str,
    names: &[&str],
    authority: &TestCertificateAuthority,
    client: bool,
) -> TestTlsIdentity {
    let mut params = CertificateParams::new(
        names
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    )
    .expect("signed certificate parameters");
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, names[0]);
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![if client {
        ExtendedKeyUsagePurpose::ClientAuth
    } else {
        ExtendedKeyUsagePurpose::ServerAuth
    }];
    let key = KeyPair::generate().expect("generate signed identity key");
    let certificate = params
        .signed_by(&key, &Issuer::new(authority.params.clone(), &authority.key))
        .expect("sign test identity");
    fs::write(directory.join(format!("{stem}.pem")), certificate.pem())
        .expect("write signed certificate");
    fs::write(directory.join(format!("{stem}.key")), key.serialize_pem())
        .expect("write signed private key");
    TestTlsIdentity {
        certificate: CertificateDer::from(certificate.der().to_vec()),
        private_key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der())),
    }
}

fn tls_upstream_server_config(
    identity: TestTlsIdentity,
    client_ca: Option<&TestCertificateAuthority>,
    tls12_only: bool,
) -> ServerConfig {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let versions = if tls12_only {
        vec![&TLS12]
    } else {
        vec![&TLS13, &TLS12]
    };
    let builder = ServerConfig::builder_with_protocol_versions(&versions);
    let builder = if let Some(client_ca) = client_ca {
        let mut roots = RootCertStore::empty();
        roots
            .add(CertificateDer::from(client_ca.certificate.der().to_vec()))
            .expect("add test client CA");
        let verifier = WebPkiClientVerifier::builder(Arc::new(roots))
            .build()
            .expect("build client certificate verifier");
        builder.with_client_cert_verifier(verifier)
    } else {
        builder.with_no_client_auth()
    };
    let mut config = builder
        .with_single_cert(vec![identity.certificate], identity.private_key)
        .expect("build TLS upstream config");
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    config
}

fn upstream_tls_proxy_config(
    port: u16,
    upstream_address: std::net::SocketAddr,
    authority: &str,
    tls: Option<Value>,
) -> Value {
    let mut config = base_config(
        port,
        json!([{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "tls-upstream"
        }]),
        json!([{
            "id": "tls-upstream",
            "targets": [{"url": format!("https://{authority}:{}", upstream_address.port())}],
            "connectTimeoutMs": 1_000,
            "requestTimeoutMs": 2_000,
            "idleConnectionTimeoutMs": 100
        }]),
        json!([{
            "id": "proxy-route",
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": "proxy"
        }]),
    );
    if let Some(tls) = tls {
        config["upstreams"][0]["tls"] = tls;
    }
    config
}

fn base_config(port: u16, resources: Value, mut upstreams: Value, routes: Value) -> Value {
    if let Some(items) = upstreams.as_array_mut() {
        for upstream in items {
            upstream
                .as_object_mut()
                .expect("test upstream is an object")
                .entry("addressPolicy")
                .or_insert_with(|| json!({"allowedCidrs": ["127.0.0.0/8", "::1/128"]}));
        }
    }
    json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-test-web",
        "limits": {
            "maxRequestBodyBytes": 1048576,
            "requestTimeoutMs": 5000,
            "drainTimeoutMs": 1000,
            "maxConnections": 128
        },
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "test-host",
            "maxConnections": 64
        }],
        "resources": resources,
        "upstreams": upstreams,
        "virtualHosts": [{
            "id": "test-host",
            "listenerRefs": ["http"],
            "serverNames": ["test.localhost"],
            "routes": routes
        }]
    })
}

fn single_https_config(port: u16, server_name: &str, certificate_stem: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-certificate-validation-test",
        "listeners": [{
            "id": "https",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1"],
            "tlsPolicyRef": "tls",
            "defaultVirtualHostRef": "https-host"
        }],
        "certificates": [{
            "id": "cert",
            "serverNames": [server_name],
            "source": {
                "type": "protected-file",
                "certificateFile": format!("{certificate_stem}.pem"),
                "privateKeyFile": format!("{certificate_stem}.key")
            }
        }],
        "tlsPolicies": [{
            "id": "tls",
            "certificateRef": "cert",
            "minimumVersion": "tls1.2",
            "maximumVersion": "tls1.3",
            "alpn": ["http/1.1"]
        }],
        "resources": [{
            "id": "response",
            "type": "respond",
            "status": 200,
            "body": "unreachable"
        }],
        "virtualHosts": [{
            "id": "https-host",
            "listenerRefs": ["https"],
            "serverNames": [server_name],
            "routes": [{
                "id": "response-route",
                "match": {"pathType": "prefix", "path": "/"},
                "resourceRef": "response"
            }]
        }]
    })
}

fn held_https_proxy_config(port: u16, upstream_address: std::net::SocketAddr) -> Value {
    let mut config = single_https_config(port, "localhost", "localhost");
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["tlsPolicies"][0]["alpn"] = json!(["h2", "http/1.1"]);
    config["limits"] = json!({
        "maxConcurrentRequests": 1,
        "requestTimeoutMs": 5_000,
        "drainTimeoutMs": 1_000
    });
    config["resources"] = json!([
        {
            "id": "held-proxy",
            "type": "proxy",
            "upstreamRef": "held-upstream",
            "stripPrefix": false
        },
        {
            "id": "ready-response",
            "type": "respond",
            "status": 200,
            "body": "ready"
        }
    ]);
    config["upstreams"] = json!([{
        "id": "held-upstream",
        "targets": [{"url": format!("http://{upstream_address}")}],
        "addressPolicy": {"allowedCidrs": ["127.0.0.0/8", "::1/128"]},
        "connectTimeoutMs": 1_000,
        "requestTimeoutMs": 5_000,
        "maxIdleConnections": 2
    }]);
    config["virtualHosts"][0]["routes"] = json!([
        {
            "id": "ready-route",
            "match": {"pathType": "exact", "path": "/ready"},
            "resourceRef": "ready-response"
        },
        {
            "id": "held-route",
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": "held-proxy"
        }
    ]);
    config
}

fn held_http_proxy_config(port: u16, upstream_address: std::net::SocketAddr) -> Value {
    let mut config = base_config(
        port,
        json!([
            {
                "id": "held-proxy",
                "type": "proxy",
                "upstreamRef": "held-upstream",
                "stripPrefix": false
            },
            {
                "id": "ready-response",
                "type": "respond",
                "status": 200,
                "body": "ready"
            }
        ]),
        json!([{
            "id": "held-upstream",
            "targets": [{"url": format!("http://{upstream_address}")}],
            "connectTimeoutMs": 1_000,
            "requestTimeoutMs": 5_000,
            "maxIdleConnections": 2
        }]),
        json!([
            {
                "id": "ready-route",
                "match": {"pathType": "exact", "path": "/ready"},
                "resourceRef": "ready-response"
            },
            {
                "id": "held-route",
                "match": {"pathType": "prefix", "path": "/"},
                "resourceRef": "held-proxy"
            }
        ]),
    );
    config["limits"]["maxConcurrentRequests"] = json!(1);
    config
}

fn early_response_http_proxy_config(port: u16, upstream_address: std::net::SocketAddr) -> Value {
    let mut config = held_http_proxy_config(port, upstream_address);
    config["limits"]["requestBodyStartTimeoutMs"] = json!(500);
    config["limits"]["requestBodyIdleTimeoutMs"] = json!(500);
    config["limits"]["responseBodyIdleTimeoutMs"] = json!(1_000);
    config
}

fn early_response_https_proxy_config(port: u16, upstream_address: std::net::SocketAddr) -> Value {
    let mut config = held_https_proxy_config(port, upstream_address);
    config["limits"]["requestBodyStartTimeoutMs"] = json!(500);
    config["limits"]["requestBodyIdleTimeoutMs"] = json!(500);
    config["limits"]["responseBodyIdleTimeoutMs"] = json!(1_000);
    config
}

fn spawn_data_plane(
    config_path: &Path,
) -> (
    oneshot::Sender<()>,
    JoinHandle<Result<(), sdkwork_api_webserver_standalone_gateway::DataPlaneError>>,
) {
    let compiled =
        load_and_compile_webserver_config(config_path).expect("compile data-plane config");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    (shutdown_tx, task)
}

fn spawn_watched_data_plane(
    config_path: &Path,
) -> (
    oneshot::Sender<()>,
    JoinHandle<Result<(), sdkwork_api_webserver_standalone_gateway::DataPlaneError>>,
) {
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

async fn wait_for_http(client: &reqwest::Client, url: &str, host: &str) -> reqwest::Response {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match client.get(url).header("host", host).send().await {
            Ok(response) => return response,
            Err(error) if tokio::time::Instant::now() < deadline => {
                let _ = error;
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => panic!("data plane did not become ready: {error}"),
        }
    }
}

async fn wait_for_body(client: &reqwest::Client, url: &str, host: &str, expected: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(response) = client.get(url).header("host", host).send().await {
            if response.status() == reqwest::StatusCode::OK {
                let body = response.text().await.expect("read watched response");
                if body == expected {
                    return;
                }
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "watched data plane did not publish body {expected:?}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_for_status(
    client: &reqwest::Client,
    url: &str,
    host: &str,
    expected: reqwest::StatusCode,
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(response) = client.get(url).header("host", host).send().await {
            if response.status() == expected {
                return;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "watched data plane did not publish status {expected}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn watched_response_config(port: u16, body: &str) -> Value {
    let mut config = base_config(
        port,
        json!([{
            "id": "response",
            "type": "respond",
            "status": 200,
            "body": body
        }]),
        json!([]),
        json!([{
            "id": "response-route",
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": "response"
        }]),
    );
    config["deployment"] = json!({
        "drainTimeoutMs": 1000,
        "reload": {"mode": "watch", "pollIntervalMs": 100}
    });
    config
}

async fn stop_data_plane(
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<Result<(), sdkwork_api_webserver_standalone_gateway::DataPlaneError>>,
) {
    shutdown.send(()).expect("send shutdown");
    timeout(Duration::from_secs(3), task)
        .await
        .expect("data plane drains before deadline")
        .expect("data-plane task joins")
        .expect("data plane stops cleanly");
}

async fn raw_http_status(port: u16, request: &[u8]) -> u16 {
    let response = raw_http_response(port, request).await;
    raw_status_code(&response).expect("status line")
}

async fn raw_http_response(port: u16, request: &[u8]) -> Vec<u8> {
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect raw HTTP client");
    stream.write_all(request).await.expect("write raw request");
    let mut response = Vec::new();
    timeout(Duration::from_secs(2), stream.read_to_end(&mut response))
        .await
        .expect("raw response completes")
        .expect("read raw response");
    response
}

async fn read_header_block<R>(stream: &mut R) -> Vec<u8>
where
    R: AsyncRead + Unpin,
{
    let mut response = Vec::with_capacity(256);
    timeout(Duration::from_secs(3), async {
        loop {
            response.push(stream.read_u8().await.expect("read response header byte"));
            assert!(response.len() <= 8192, "response header block is bounded");
            if response.ends_with(b"\r\n\r\n") {
                break;
            }
        }
    })
    .await
    .expect("response header block arrives");
    response
}

async fn read_http1_fixed_response<R>(stream: &mut R) -> (Vec<u8>, Vec<u8>)
where
    R: AsyncRead + Unpin,
{
    let headers = read_header_block(stream).await;
    let header_text = std::str::from_utf8(&headers).expect("HTTP/1 response headers are UTF-8");
    let content_length = header_text
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length").then(|| {
                value
                    .trim()
                    .parse::<usize>()
                    .expect("numeric Content-Length")
            })
        })
        .unwrap_or(0);
    assert!(
        content_length <= 1024 * 1024,
        "test response body is bounded"
    );
    let mut body = vec![0_u8; content_length];
    timeout(Duration::from_secs(2), stream.read_exact(&mut body))
        .await
        .expect("fixed response Body arrives")
        .expect("read fixed response Body");
    (headers, body)
}

async fn assert_http1_connection_ends<R>(stream: &mut R)
where
    R: AsyncRead + Unpin,
{
    let mut byte = [0_u8; 1];
    let result = timeout(Duration::from_secs(2), stream.read(&mut byte))
        .await
        .expect("HTTP/1 connection ends after the response");
    match result {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::UnexpectedEof
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::BrokenPipe
            ) => {}
        Ok(length) => panic!("received {length} unexpected bytes after the complete response"),
        Err(error) => panic!("unexpected HTTP/1 close error: {error}"),
    }
}

async fn read_until_contains<R>(stream: &mut R, needle: &[u8]) -> Vec<u8>
where
    R: AsyncRead + Unpin,
{
    let mut observed = Vec::with_capacity(256);
    timeout(Duration::from_secs(2), async {
        let mut buffer = [0_u8; 256];
        while !observed
            .windows(needle.len())
            .any(|window| window == needle)
        {
            let length = stream
                .read(&mut buffer)
                .await
                .expect("read streamed response");
            assert!(
                length > 0,
                "connection closed before expected response bytes"
            );
            observed.extend_from_slice(&buffer[..length]);
            assert!(
                observed.len() <= 1024 * 1024,
                "streamed response is bounded"
            );
        }
    })
    .await
    .expect("expected streamed response bytes arrive");
    observed
}

fn raw_status_code(response: &[u8]) -> Option<u16> {
    let status_line = std::str::from_utf8(response)
        .expect("HTTP response is UTF-8 in status line")
        .lines()
        .next()?;
    status_line.split_whitespace().nth(1)?.parse().ok()
}

async fn assert_raw_request_rejected(port: u16, request: &[u8]) {
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect rejected raw HTTP client");
    stream
        .write_all(request)
        .await
        .expect("write rejected raw request");
    let mut response = Vec::new();
    let read = timeout(Duration::from_secs(2), stream.read_to_end(&mut response))
        .await
        .expect("rejected raw response completes");
    if let Err(error) = read {
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::ConnectionReset,
            "a rejected request may close or reset, but no other read error is valid"
        );
    }
    assert!(
        raw_status_code(&response).is_none_or(|status| !(200..300).contains(&status)),
        "ambiguous request must close or return a non-success status"
    );
}

async fn raw_tls_http1_response(port: u16, certificate_der: &[u8], request: &[u8]) -> Vec<u8> {
    let mut tls = connect_tls(port, certificate_der, b"http/1.1").await;
    tls.write_all(request).await.expect("write raw TLS request");
    let mut response = Vec::new();
    let read_result = timeout(Duration::from_secs(2), tls.read_to_end(&mut response))
        .await
        .expect("raw TLS response completes");
    if let Err(error) = read_result {
        assert!(
            matches!(
                error.kind(),
                std::io::ErrorKind::UnexpectedEof
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::BrokenPipe
            ),
            "unexpected raw TLS read error: {error}"
        );
    }
    response
}

async fn connect_tls(
    port: u16,
    certificate_der: &[u8],
    alpn: &'static [u8],
) -> tokio_rustls::client::TlsStream<tokio::net::TcpStream> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(certificate_der.to_vec()))
        .expect("trust generated certificate");
    let mut tls_config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    tls_config.alpn_protocols = vec![alpn.to_vec()];
    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect raw TLS client");
    let tls = TlsConnector::from(Arc::new(tls_config))
        .connect(
            ServerName::try_from("localhost".to_owned()).expect("valid DNS name"),
            tcp,
        )
        .await
        .expect("complete TLS handshake");
    assert_eq!(tls.get_ref().1.alpn_protocol(), Some(alpn));
    tls
}

async fn read_h2_frame<R>(stream: &mut R) -> (u8, u8, u32, Vec<u8>)
where
    R: AsyncRead + Unpin,
{
    let mut header = [0_u8; 9];
    timeout(Duration::from_secs(2), stream.read_exact(&mut header))
        .await
        .expect("HTTP/2 frame header arrives")
        .expect("read HTTP/2 frame header");
    let length = ((header[0] as usize) << 16) | ((header[1] as usize) << 8) | header[2] as usize;
    assert!(length <= 1024 * 1024, "test HTTP/2 frame is bounded");
    let mut payload = vec![0_u8; length];
    timeout(Duration::from_secs(2), stream.read_exact(&mut payload))
        .await
        .expect("HTTP/2 frame payload arrives")
        .expect("read HTTP/2 frame payload");
    let stream_id = u32::from_be_bytes([header[5] & 0x7f, header[6], header[7], header[8]]);
    (header[3], header[4], stream_id, payload)
}

fn encode_h2_frame(frame_type: u8, flags: u8, stream_id: u32, payload: &[u8]) -> Vec<u8> {
    assert!(payload.len() <= 0x00ff_ffff, "HTTP/2 test payload fits");
    let length = payload.len();
    let mut frame = Vec::with_capacity(9 + length);
    frame.extend_from_slice(&[
        ((length >> 16) & 0xff) as u8,
        ((length >> 8) & 0xff) as u8,
        (length & 0xff) as u8,
        frame_type,
        flags,
    ]);
    frame.extend_from_slice(&(stream_id & 0x7fff_ffff).to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

async fn connect_h2(
    port: u16,
    certificate_der: &[u8],
) -> (
    h2::client::SendRequest<Bytes>,
    JoinHandle<Result<(), h2::Error>>,
) {
    let tls = connect_tls(port, certificate_der, b"h2").await;
    let (send, connection) = h2::client::handshake(tls)
        .await
        .expect("complete HTTP/2 connection preface");
    (send, tokio::spawn(connection))
}

#[tokio::test]
async fn serves_fixed_and_static_routes_and_drains() {
    let directory = TempDir::new().expect("create temp directory");
    let public = directory.path().join("public");
    fs::create_dir(&public).expect("create public directory");
    fs::write(public.join("index.html"), "<h1>real static response</h1>")
        .expect("write static index");
    fs::write(public.join("asset.txt"), "0123456789").expect("write static asset");
    fs::create_dir(public.join("docs")).expect("create static subdirectory");
    fs::write(public.join("docs/index.html"), "<h1>docs index</h1>")
        .expect("write nested static index");
    let port = available_port();
    let config = base_config(
        port,
        json!([
            {
                "id": "health-response",
                "type": "respond",
                "status": 200,
                "body": "ok\n"
            },
            {
                "id": "static-files",
                "type": "static",
                "root": "public",
                "indexFiles": ["index.html"],
                "spaFallback": "index.html",
                "followSymlinks": false
            },
            {
                "id": "moved-response",
                "type": "redirect",
                "status": 308,
                "location": "/healthz"
            }
        ]),
        json!([]),
        json!([
            {
                "id": "health",
                "match": {"pathType": "exact", "path": "/healthz"},
                "resourceRef": "health-response"
            },
            {
                "id": "static",
                "match": {"pathType": "prefix", "path": "/"},
                "resourceRef": "static-files"
            },
            {
                "id": "moved",
                "match": {"pathType": "exact", "path": "/moved"},
                "resourceRef": "moved-response"
            }
        ]),
    );
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::new();

    let health = wait_for_http(
        &client,
        &format!("http://127.0.0.1:{port}/healthz"),
        "test.localhost",
    )
    .await;
    assert_eq!(health.status(), reqwest::StatusCode::OK);
    assert_eq!(health.text().await.expect("read health body"), "ok\n");

    let default_host = client
        .get(format!("http://127.0.0.1:{port}/healthz"))
        .header("host", "unmatched.localhost")
        .send()
        .await
        .expect("request default virtual host");
    assert_eq!(default_host.status(), reqwest::StatusCode::OK);

    let no_redirect_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("build no-redirect client");
    let moved = no_redirect_client
        .get(format!("http://127.0.0.1:{port}/moved"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request redirect route");
    assert_eq!(moved.status(), reqwest::StatusCode::PERMANENT_REDIRECT);
    assert_eq!(moved.headers()[reqwest::header::LOCATION], "/healthz");

    let index = client
        .get(format!("http://127.0.0.1:{port}/"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request static index");
    assert_eq!(index.status(), reqwest::StatusCode::OK);
    assert!(index
        .text()
        .await
        .expect("read static body")
        .contains("real static response"));

    let nested_redirect = no_redirect_client
        .get(format!("http://127.0.0.1:{port}/docs?view=compact"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request static directory redirect");
    assert_eq!(
        nested_redirect.status(),
        reqwest::StatusCode::TEMPORARY_REDIRECT
    );
    assert_eq!(
        nested_redirect.headers()[reqwest::header::LOCATION],
        "/docs/?view=compact"
    );

    let nested_index = client
        .get(format!("http://127.0.0.1:{port}/docs/"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request nested static index");
    assert_eq!(nested_index.status(), reqwest::StatusCode::OK);
    assert!(nested_index.text().await.unwrap().contains("docs index"));

    let fallback = client
        .get(format!("http://127.0.0.1:{port}/application/route"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request SPA fallback");
    assert_eq!(fallback.status(), reqwest::StatusCode::OK);
    assert!(fallback
        .text()
        .await
        .unwrap()
        .contains("real static response"));

    let partial = client
        .get(format!("http://127.0.0.1:{port}/asset.txt"))
        .header("host", "test.localhost")
        .header(reqwest::header::RANGE, "bytes=2-5")
        .send()
        .await
        .expect("request static byte range");
    assert_eq!(partial.status(), reqwest::StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        partial.headers()[reqwest::header::CONTENT_RANGE],
        "bytes 2-5/10"
    );
    assert_eq!(partial.text().await.unwrap(), "2345");

    let asset = client
        .get(format!("http://127.0.0.1:{port}/asset.txt"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request static asset metadata");
    let last_modified = asset.headers()[reqwest::header::LAST_MODIFIED].clone();
    assert_eq!(asset.text().await.unwrap(), "0123456789");
    let not_modified = client
        .get(format!("http://127.0.0.1:{port}/asset.txt"))
        .header("host", "test.localhost")
        .header(reqwest::header::IF_MODIFIED_SINCE, last_modified)
        .send()
        .await
        .expect("request conditional static asset");
    assert_eq!(not_modified.status(), reqwest::StatusCode::NOT_MODIFIED);

    let head = client
        .head(format!("http://127.0.0.1:{port}/asset.txt"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request static HEAD");
    assert_eq!(head.status(), reqwest::StatusCode::OK);
    assert_eq!(head.headers()[reqwest::header::CONTENT_LENGTH], "10");
    assert!(head.bytes().await.unwrap().is_empty());

    assert_eq!(
        raw_http_status(
            port,
            b"GET /%2e%2e/secret HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n",
        )
        .await,
        400
    );
    assert_eq!(
        raw_http_status(
            port,
            b"GET http://evil.example/ HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n",
        )
        .await,
        400
    );

    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn streams_proxy_body_and_enforces_body_limit() {
    let (upstream_address, upstream_shutdown, upstream_task) = spawn_frame_echo_upstream().await;
    let direct_emitted = raw_http_response(
        upstream_address.port(),
        b"POST /emit HTTP/1.1\r\nHost: upstream.localhost\r\nContent-Length: 0\r\nTE: trailers\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(
        direct_emitted
            .windows(b"x-checksum: emitted".len())
            .any(|window| window.eq_ignore_ascii_case(b"x-checksum: emitted")),
        "upstream fixture emits a real HTTP/1 trailer: {}",
        String::from_utf8_lossy(&direct_emitted)
    );

    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let mut config = base_config(
        port,
        json!([{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "echo-upstream",
            "stripPrefix": false
        }]),
        json!([{
            "id": "echo-upstream",
            "targets": [{"url": format!("http://{upstream_address}")}],
            "connectTimeoutMs": 1000,
            "requestTimeoutMs": 5000,
            "maxIdleConnections": 8
        }]),
        json!([{
            "id": "proxy-route",
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": "proxy"
        }]),
    );
    config["limits"]["maxRequestBodyBytes"] = json!(4);
    config["limits"]["maxTrailerBytes"] = json!(64);
    config["limits"]["maxTrailers"] = json!(1);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::new();

    let ready = wait_for_http(
        &client,
        &format!("http://127.0.0.1:{port}/echo"),
        "test.localhost",
    )
    .await;
    assert_eq!(ready.status(), reqwest::StatusCode::OK);

    let echoed = client
        .post(format!("http://127.0.0.1:{port}/echo"))
        .header("host", "test.localhost")
        .body("four")
        .send()
        .await
        .expect("proxy bounded body");
    assert_eq!(echoed.status(), reqwest::StatusCode::OK);
    assert_eq!(echoed.text().await.expect("read echo"), "four");

    let mut expecting = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect Expect proxy client");
    expecting
        .write_all(
            b"POST /expect HTTP/1.1\r\nHost: test.localhost\r\nContent-Length: 4\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n",
        )
        .await
        .expect("write proxy Expect headers");
    assert_eq!(
        read_header_block(&mut expecting).await,
        b"HTTP/1.1 100 Continue\r\n\r\n"
    );
    expecting
        .write_all(b"four")
        .await
        .expect("write proxy Expect body");
    let mut expectation_result = Vec::new();
    timeout(
        Duration::from_secs(2),
        expecting.read_to_end(&mut expectation_result),
    )
    .await
    .expect("proxy Expect response completes")
    .expect("read proxy Expect response");
    assert_eq!(raw_status_code(&expectation_result), Some(200));
    assert!(
        expectation_result.ends_with(b"absent"),
        "gateway terminates Expect instead of forwarding it upstream: {}",
        String::from_utf8_lossy(&expectation_result)
    );

    let chunked = raw_http_response(
        port,
        b"POST /echo HTTP/1.1\r\nHost: test.localhost\r\nTransfer-Encoding: chunked\r\nTE: trailers\r\nTrailer: X-Checksum\r\nConnection: close\r\n\r\n4;source=test\r\nfour\r\n0\r\nX-Checksum: verified\r\n\r\n",
    )
    .await;
    assert_eq!(raw_status_code(&chunked), Some(200));
    assert!(
        chunked.windows(4).any(|window| window == b"four"),
        "chunked body reaches the streaming proxy"
    );

    let inspected = raw_http_response(
        port,
        b"POST /inspect HTTP/1.1\r\nHost: test.localhost\r\nTransfer-Encoding: chunked\r\nTrailer: X-Checksum\r\nConnection: close\r\n\r\n4\r\nfour\r\n0\r\nX-Checksum: verified\r\n\r\n",
    )
    .await;
    assert!(
        inspected
            .windows(b"verified".len())
            .any(|window| window == b"verified"),
        "request trailer reaches upstream: {}",
        String::from_utf8_lossy(&inspected)
    );
    assert_eq!(
        raw_http_status(
            port,
            b"POST /inspect HTTP/1.1\r\nHost: test.localhost\r\nTransfer-Encoding: chunked\r\nTrailer: Content-Length\r\nConnection: close\r\n\r\n0\r\n\r\n",
        )
        .await,
        400
    );
    let emitted = raw_http_response(
        port,
        b"POST /emit HTTP/1.1\r\nHost: test.localhost\r\nContent-Length: 0\r\nTE: trailers\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(
        emitted
            .windows(b"x-checksum: emitted".len())
            .any(|window| window.eq_ignore_ascii_case(b"x-checksum: emitted")),
        "response trailer reaches downstream: {}",
        String::from_utf8_lossy(&emitted)
    );
    assert!(
        chunked
            .windows(b"x-checksum: verified".len())
            .any(|window| window.eq_ignore_ascii_case(b"x-checksum: verified")),
        "request and response trailer frames survive the HTTP/1 proxy hop: {}",
        String::from_utf8_lossy(&chunked)
    );

    assert_raw_request_rejected(
        port,
        b"POST /echo HTTP/1.1\r\nHost: test.localhost\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nlarge\r\n0\r\n\r\n",
    )
    .await;
    assert_raw_request_rejected(
        port,
        b"POST /echo HTTP/1.1\r\nHost: test.localhost\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n1\r\nx\r\n0\r\nContent-Length: 1\r\n\r\n",
    )
    .await;

    let rejected = client
        .post(format!("http://127.0.0.1:{port}/echo"))
        .header("host", "test.localhost")
        .body("exceeds")
        .send()
        .await
        .expect("request oversized body");
    assert_eq!(rejected.status(), reqwest::StatusCode::PAYLOAD_TOO_LARGE);

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn system_dns_denies_loopback_by_default_and_allows_explicit_local_policy() {
    let (upstream_address, upstream_shutdown, upstream_task) = spawn_frame_echo_upstream().await;
    let directory = TempDir::new().expect("create temp directory");
    let denied_port = available_port();
    let mut denied = base_config(
        denied_port,
        json!([{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "dns-upstream"
        }]),
        json!([{
            "id": "dns-upstream",
            "targets": [{"url": format!("http://localhost:{}", upstream_address.port())}],
            "idleConnectionTimeoutMs": 100
        }]),
        json!([{
            "id": "proxy-route",
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": "proxy"
        }]),
    );
    denied["upstreams"][0]
        .as_object_mut()
        .expect("test upstream object")
        .remove("addressPolicy");
    let denied_path = write_config(directory.path(), &denied);
    let (denied_shutdown, denied_task) = spawn_data_plane(&denied_path);
    let client = reqwest::Client::new();
    let denied_response = wait_for_http(
        &client,
        &format!("http://127.0.0.1:{denied_port}/blocked"),
        "test.localhost",
    )
    .await;
    assert_eq!(denied_response.status(), reqwest::StatusCode::BAD_GATEWAY);
    stop_data_plane(denied_shutdown, denied_task).await;

    let allowed_port = available_port();
    let allowed = base_config(
        allowed_port,
        json!([{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "dns-upstream"
        }]),
        json!([{
            "id": "dns-upstream",
            "resolverRef": "system-dns",
            "targets": [{"url": format!("http://localhost:{}", upstream_address.port())}],
            "idleConnectionTimeoutMs": 100
        }]),
        json!([{
            "id": "proxy-route",
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": "proxy"
        }]),
    );
    let mut allowed = allowed;
    allowed["resolvers"] = json!([{
        "id": "system-dns",
        "timeoutMs": 1_000,
        "maximumAnswers": 16,
        "maxConcurrentQueries": 4
    }]);
    let allowed_path = write_config(directory.path(), &allowed);
    let (allowed_shutdown, allowed_task) = spawn_data_plane(&allowed_path);
    let allowed_response = wait_for_http(
        &client,
        &format!("http://127.0.0.1:{allowed_port}/allowed"),
        "test.localhost",
    )
    .await;
    assert_eq!(allowed_response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        allowed_response
            .text()
            .await
            .expect("read allowed DNS proxy response"),
        "/allowed"
    );
    stop_data_plane(allowed_shutdown, allowed_task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn upstream_admission_holds_capacity_through_streaming_response_lifetime() {
    let (upstream_address, release, upstream_shutdown, upstream_task) =
        spawn_held_response_upstream().await;
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let mut config = held_http_proxy_config(port, upstream_address);
    config["limits"]["maxConcurrentRequests"] = json!(4);
    config["upstreams"][0]["maxInFlightRequests"] = json!(1);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}");

    let held = wait_for_http(&client, &format!("{url}/held"), "test.localhost").await;
    assert_eq!(held.status(), reqwest::StatusCode::OK);

    let saturated = client
        .get(format!("{url}/excess"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("upstream admission returns a response without queueing");
    assert_eq!(saturated.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(saturated.headers().get("retry-after").unwrap(), "1");
    assert_eq!(
        saturated.text().await.expect("read saturation response"),
        "upstream is saturated\n"
    );

    drop(held);
    release.notify_waiters();
    wait_for_body(
        &client,
        &format!("{url}/recovered"),
        "test.localhost",
        "recovered",
    )
    .await;
    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn passive_health_ejects_failure_and_recovers_with_one_later_probe() {
    let (failing_address, failing_healthy, failing_requests, failing_shutdown, failing_task) =
        spawn_toggle_health_upstream(false, "recovered-target").await;
    let (healthy_address, _, healthy_requests, healthy_shutdown, healthy_task) =
        spawn_toggle_health_upstream(true, "healthy-target").await;
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let config = base_config(
        port,
        json!([{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "health-upstream"
        }]),
        json!([{
            "id": "health-upstream",
            "targets": [
                {"url": format!("http://{failing_address}")},
                {"url": format!("http://{healthy_address}")}
            ],
            "maxInFlightRequests": 8,
            "passiveHealth": {
                "failureThreshold": 1,
                "ejectionTimeMs": 500,
                "failureStatuses": [503]
            }
        }]),
        json!([{
            "id": "proxy-route",
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": "proxy"
        }]),
    );
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/health");

    let failed = wait_for_http(&client, &url, "test.localhost").await;
    assert_eq!(failed.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        failed.text().await.expect("read target failure"),
        "target-unhealthy"
    );
    assert_eq!(failing_requests.load(Ordering::Acquire), 1);

    for _ in 0..4 {
        let response = client
            .get(&url)
            .header("host", "test.localhost")
            .send()
            .await
            .expect("healthy alternative response");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert_eq!(
            response.text().await.expect("read healthy response"),
            "healthy-target"
        );
    }
    assert_eq!(failing_requests.load(Ordering::Acquire), 1);
    assert_eq!(healthy_requests.load(Ordering::Acquire), 4);

    failing_healthy.store(true, Ordering::Release);
    tokio::time::sleep(Duration::from_millis(550)).await;
    let mut observed_recovery = false;
    for _ in 0..2 {
        let response = client
            .get(&url)
            .header("host", "test.localhost")
            .send()
            .await
            .expect("half-open recovery response");
        if response.text().await.expect("read recovery response") == "recovered-target" {
            observed_recovery = true;
        }
    }
    assert!(
        observed_recovery,
        "expired target receives one recovery probe"
    );
    assert_eq!(failing_requests.load(Ordering::Acquire), 2);

    stop_data_plane(shutdown, task).await;
    stop_upstream(failing_shutdown, failing_task).await;
    stop_upstream(healthy_shutdown, healthy_task).await;
}

#[tokio::test]
async fn all_ejected_targets_fail_locally_and_reload_starts_fresh_health_state() {
    let (upstream_address, _, upstream_requests, upstream_shutdown, upstream_task) =
        spawn_toggle_health_upstream(false, "unused").await;
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let mut config = base_config(
        port,
        json!([{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "health-upstream"
        }]),
        json!([{
            "id": "health-upstream",
            "targets": [{"url": format!("http://{upstream_address}")}],
            "passiveHealth": {
                "failureThreshold": 1,
                "ejectionTimeMs": 5_000,
                "failureStatuses": [503]
            }
        }]),
        json!([{
            "id": "proxy-route",
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": "proxy"
        }]),
    );
    config["deployment"] = json!({
        "drainTimeoutMs": 1_000,
        "reload": {"mode": "watch", "pollIntervalMs": 100}
    });
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_watched_data_plane(&path);
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/health");

    let first = wait_for_http(&client, &url, "test.localhost").await;
    assert_eq!(first.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        first.text().await.expect("read upstream 503"),
        "target-unhealthy"
    );
    assert_eq!(upstream_requests.load(Ordering::Acquire), 1);

    let unavailable = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("all-ejected response");
    assert_eq!(
        unavailable.status(),
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(unavailable.headers().get("retry-after").unwrap(), "1");
    assert_eq!(
        unavailable.text().await.expect("read all-ejected response"),
        "all upstream targets are unavailable\n"
    );
    assert_eq!(
        upstream_requests.load(Ordering::Acquire),
        1,
        "local rejection performs no hidden retry"
    );

    config["upstreams"][0]["passiveHealth"]["failureThreshold"] = json!(2);
    write_config(directory.path(), &config);
    timeout(Duration::from_secs(5), async {
        while upstream_requests.load(Ordering::Acquire) < 2 {
            let _ = client
                .get(&url)
                .header("host", "test.localhost")
                .send()
                .await;
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("valid reload publishes fresh health state");

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn private_ca_https_upstream_requires_custom_trust_and_correct_hostname() {
    let directory = TempDir::new().expect("create temp directory");
    let authority = write_test_ca(directory.path(), "upstream-ca");
    let server_identity = write_signed_identity(
        directory.path(),
        "upstream-server",
        &["localhost"],
        &authority,
        false,
    );
    let server_config = tls_upstream_server_config(server_identity, None, false);
    let (upstream_address, upstream_shutdown, upstream_task) =
        spawn_tls_upstream(server_config).await;
    let client = reqwest::Client::new();

    let system_port = available_port();
    let system = upstream_tls_proxy_config(system_port, upstream_address, "localhost", None);
    let system_path = write_config(directory.path(), &system);
    let (system_shutdown, system_task) = spawn_data_plane(&system_path);
    let system_response = wait_for_http(
        &client,
        &format!("http://127.0.0.1:{system_port}/secure"),
        "test.localhost",
    )
    .await;
    assert_eq!(system_response.status(), reqwest::StatusCode::BAD_GATEWAY);
    stop_data_plane(system_shutdown, system_task).await;

    let trusted_port = available_port();
    let trusted = upstream_tls_proxy_config(
        trusted_port,
        upstream_address,
        "localhost",
        Some(json!({
            "trustMode": "custom",
            "caCertificateFiles": ["upstream-ca.pem"],
            "minimumVersion": "tls1.2",
            "maximumVersion": "tls1.3"
        })),
    );
    let trusted_path = write_config(directory.path(), &trusted);
    let (trusted_shutdown, trusted_task) = spawn_data_plane(&trusted_path);
    let trusted_response = wait_for_http(
        &client,
        &format!("http://127.0.0.1:{trusted_port}/secure"),
        "test.localhost",
    )
    .await;
    assert_eq!(trusted_response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        trusted_response
            .text()
            .await
            .expect("read trusted response"),
        "secure-upstream"
    );
    stop_data_plane(trusted_shutdown, trusted_task).await;

    let wrong_name_port = available_port();
    let wrong_name = upstream_tls_proxy_config(
        wrong_name_port,
        upstream_address,
        "127.0.0.1",
        Some(json!({
            "trustMode": "custom",
            "caCertificateFiles": ["upstream-ca.pem"]
        })),
    );
    let wrong_name_path = write_config(directory.path(), &wrong_name);
    let (wrong_name_shutdown, wrong_name_task) = spawn_data_plane(&wrong_name_path);
    let wrong_name_response = wait_for_http(
        &client,
        &format!("http://127.0.0.1:{wrong_name_port}/secure"),
        "test.localhost",
    )
    .await;
    assert_eq!(
        wrong_name_response.status(),
        reqwest::StatusCode::BAD_GATEWAY
    );
    stop_data_plane(wrong_name_shutdown, wrong_name_task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn mutual_tls_upstream_requires_the_configured_client_identity() {
    let directory = TempDir::new().expect("create temp directory");
    let authority = write_test_ca(directory.path(), "mtls-ca");
    let server_identity = write_signed_identity(
        directory.path(),
        "mtls-server",
        &["localhost"],
        &authority,
        false,
    );
    write_signed_identity(
        directory.path(),
        "mtls-client",
        &["sdkwork-client"],
        &authority,
        true,
    );
    let server_config = tls_upstream_server_config(server_identity, Some(&authority), false);
    let (upstream_address, upstream_shutdown, upstream_task) =
        spawn_tls_upstream(server_config).await;
    let client = reqwest::Client::new();

    let anonymous_port = available_port();
    let anonymous = upstream_tls_proxy_config(
        anonymous_port,
        upstream_address,
        "localhost",
        Some(json!({
            "trustMode": "custom",
            "caCertificateFiles": ["mtls-ca.pem"]
        })),
    );
    let anonymous_path = write_config(directory.path(), &anonymous);
    let (anonymous_shutdown, anonymous_task) = spawn_data_plane(&anonymous_path);
    let anonymous_response = wait_for_http(
        &client,
        &format!("http://127.0.0.1:{anonymous_port}/secure"),
        "test.localhost",
    )
    .await;
    assert_eq!(
        anonymous_response.status(),
        reqwest::StatusCode::BAD_GATEWAY
    );
    stop_data_plane(anonymous_shutdown, anonymous_task).await;

    let authenticated_port = available_port();
    let authenticated = upstream_tls_proxy_config(
        authenticated_port,
        upstream_address,
        "localhost",
        Some(json!({
            "trustMode": "custom",
            "caCertificateFiles": ["mtls-ca.pem"],
            "clientCertificateFile": "mtls-client.pem",
            "clientPrivateKeyFile": "mtls-client.key"
        })),
    );
    let authenticated_path = write_config(directory.path(), &authenticated);
    let (authenticated_shutdown, authenticated_task) = spawn_data_plane(&authenticated_path);
    let authenticated_response = wait_for_http(
        &client,
        &format!("http://127.0.0.1:{authenticated_port}/secure"),
        "test.localhost",
    )
    .await;
    assert_eq!(authenticated_response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        authenticated_response
            .text()
            .await
            .expect("read mutual TLS response"),
        "secure-upstream"
    );
    stop_data_plane(authenticated_shutdown, authenticated_task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn upstream_tls_version_policy_rejects_incompatible_peers() {
    let directory = TempDir::new().expect("create temp directory");
    let authority = write_test_ca(directory.path(), "tls12-ca");
    let server_identity = write_signed_identity(
        directory.path(),
        "tls12-server",
        &["localhost"],
        &authority,
        false,
    );
    let server_config = tls_upstream_server_config(server_identity, None, true);
    let (upstream_address, upstream_shutdown, upstream_task) =
        spawn_tls_upstream(server_config).await;
    let client = reqwest::Client::new();

    let incompatible_port = available_port();
    let incompatible = upstream_tls_proxy_config(
        incompatible_port,
        upstream_address,
        "localhost",
        Some(json!({
            "trustMode": "custom",
            "caCertificateFiles": ["tls12-ca.pem"],
            "minimumVersion": "tls1.3",
            "maximumVersion": "tls1.3"
        })),
    );
    let incompatible_path = write_config(directory.path(), &incompatible);
    let (incompatible_shutdown, incompatible_task) = spawn_data_plane(&incompatible_path);
    let incompatible_response = wait_for_http(
        &client,
        &format!("http://127.0.0.1:{incompatible_port}/secure"),
        "test.localhost",
    )
    .await;
    assert_eq!(
        incompatible_response.status(),
        reqwest::StatusCode::BAD_GATEWAY
    );
    stop_data_plane(incompatible_shutdown, incompatible_task).await;

    let compatible_port = available_port();
    let compatible = upstream_tls_proxy_config(
        compatible_port,
        upstream_address,
        "localhost",
        Some(json!({
            "trustMode": "custom",
            "caCertificateFiles": ["tls12-ca.pem"],
            "minimumVersion": "tls1.2",
            "maximumVersion": "tls1.2"
        })),
    );
    let compatible_path = write_config(directory.path(), &compatible);
    let (compatible_shutdown, compatible_task) = spawn_data_plane(&compatible_path);
    let compatible_response = wait_for_http(
        &client,
        &format!("http://127.0.0.1:{compatible_port}/secure"),
        "test.localhost",
    )
    .await;
    assert_eq!(compatible_response.status(), reqwest::StatusCode::OK);
    stop_data_plane(compatible_shutdown, compatible_task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn invalid_upstream_tls_material_fails_before_listener_activation() {
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    fs::write(directory.path().join("invalid-ca.pem"), "not a certificate")
        .expect("write malformed CA");
    let malformed = upstream_tls_proxy_config(
        port,
        "127.0.0.1:443".parse().expect("test upstream address"),
        "localhost",
        Some(json!({
            "trustMode": "custom",
            "caCertificateFiles": ["invalid-ca.pem"]
        })),
    );
    let malformed_path = write_config(directory.path(), &malformed);
    let malformed_app = load_and_compile_webserver_config(&malformed_path)
        .expect("malformed PEM remains a runtime material check");
    let malformed_error = run_data_plane_until(malformed_app, std::future::pending::<()>())
        .await
        .expect_err("malformed CA must fail before listener activation");
    assert!(
        matches!(
            &malformed_error,
            sdkwork_api_webserver_standalone_gateway::DataPlaneError::InvalidUpstreamCaBundle { .. }
        ),
        "{malformed_error:?}"
    );

    fs::write(directory.path().join("empty-ca.pem"), b"").expect("write empty CA");
    let empty = upstream_tls_proxy_config(
        port,
        "127.0.0.1:443".parse().expect("test upstream address"),
        "localhost",
        Some(json!({
            "trustMode": "custom",
            "caCertificateFiles": ["empty-ca.pem"]
        })),
    );
    let empty_path = write_config(directory.path(), &empty);
    let empty_app = load_and_compile_webserver_config(&empty_path).expect("compile empty CA path");
    let empty_error = run_data_plane_until(empty_app, std::future::pending::<()>())
        .await
        .expect_err("empty CA must fail before listener activation");
    assert!(matches!(
        empty_error,
        sdkwork_api_webserver_standalone_gateway::DataPlaneError::EmptyUpstreamCaBundle { .. }
    ));

    fs::write(
        directory.path().join("oversized-ca.pem"),
        vec![b'x'; 1024 * 1024 + 1],
    )
    .expect("write oversized CA");
    let oversized = upstream_tls_proxy_config(
        port,
        "127.0.0.1:443".parse().expect("test upstream address"),
        "localhost",
        Some(json!({
            "trustMode": "custom",
            "caCertificateFiles": ["oversized-ca.pem"]
        })),
    );
    let oversized_path = write_config(directory.path(), &oversized);
    let oversized_app =
        load_and_compile_webserver_config(&oversized_path).expect("compile oversized CA path");
    let oversized_error = run_data_plane_until(oversized_app, std::future::pending::<()>())
        .await
        .expect_err("oversized CA must fail before listener activation");
    assert!(matches!(
        oversized_error,
        sdkwork_api_webserver_standalone_gateway::DataPlaneError::TlsMaterialTooLarge { .. }
    ));

    let many_roots_authority = write_test_ca(directory.path(), "many-roots-ca");
    fs::write(
        directory.path().join("many-roots.pem"),
        many_roots_authority.certificate.pem().repeat(65),
    )
    .expect("write excessive root bundle");
    let many_roots = upstream_tls_proxy_config(
        port,
        "127.0.0.1:443".parse().expect("test upstream address"),
        "localhost",
        Some(json!({
            "trustMode": "custom",
            "caCertificateFiles": ["many-roots.pem"]
        })),
    );
    let many_roots_path = write_config(directory.path(), &many_roots);
    let many_roots_app =
        load_and_compile_webserver_config(&many_roots_path).expect("compile bounded root path");
    let many_roots_error = run_data_plane_until(many_roots_app, std::future::pending::<()>())
        .await
        .expect_err("more than 64 custom roots must fail before listener activation");
    assert!(matches!(
        many_roots_error,
        sdkwork_api_webserver_standalone_gateway::DataPlaneError::TooManyUpstreamRootCertificates { .. }
    ));

    let authority = write_test_ca(directory.path(), "identity-ca");
    write_signed_identity(
        directory.path(),
        "identity-client",
        &["sdkwork-client"],
        &authority,
        true,
    );
    let wrong_key = KeyPair::generate().expect("generate mismatched client key");
    fs::write(
        directory.path().join("identity-client.key"),
        wrong_key.serialize_pem(),
    )
    .expect("replace client key with mismatch");
    let mismatched = upstream_tls_proxy_config(
        port,
        "127.0.0.1:443".parse().expect("test upstream address"),
        "localhost",
        Some(json!({
            "trustMode": "custom",
            "caCertificateFiles": ["identity-ca.pem"],
            "clientCertificateFile": "identity-client.pem",
            "clientPrivateKeyFile": "identity-client.key"
        })),
    );
    let mismatched_path = write_config(directory.path(), &mismatched);
    let mismatched_app = load_and_compile_webserver_config(&mismatched_path)
        .expect("compile mismatched identity paths");
    let mismatched_error = run_data_plane_until(mismatched_app, std::future::pending::<()>())
        .await
        .expect_err("mismatched client identity must fail before listener activation");
    assert!(matches!(
        mismatched_error,
        sdkwork_api_webserver_standalone_gateway::DataPlaneError::UpstreamClient { .. }
    ));
}

#[tokio::test]
async fn upstream_tls_watch_reload_retains_failure_and_replaces_security_pool() {
    let directory = TempDir::new().expect("create temp directory");
    let authority = write_test_ca(directory.path(), "reload-ca");
    let server_identity = write_signed_identity(
        directory.path(),
        "reload-server",
        &["localhost"],
        &authority,
        false,
    );
    let server_config = tls_upstream_server_config(server_identity, None, false);
    let (upstream_address, upstream_shutdown, upstream_task) =
        spawn_tls_upstream(server_config).await;
    let port = available_port();
    let mut trusted = upstream_tls_proxy_config(
        port,
        upstream_address,
        "localhost",
        Some(json!({
            "trustMode": "custom",
            "caCertificateFiles": ["reload-ca.pem"]
        })),
    );
    trusted["deployment"] = json!({
        "drainTimeoutMs": 1_000,
        "reload": {"mode": "watch", "pollIntervalMs": 100}
    });
    let path = write_config(directory.path(), &trusted);
    let (shutdown, task) = spawn_watched_data_plane(&path);
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/secure");
    wait_for_body(&client, &url, "test.localhost", "secure-upstream").await;

    fs::write(directory.path().join("broken-ca.pem"), "not a certificate")
        .expect("write invalid reload CA");
    let mut invalid = trusted.clone();
    invalid["upstreams"][0]["tls"]["caCertificateFiles"] = json!(["broken-ca.pem"]);
    write_config(directory.path(), &invalid);
    tokio::time::sleep(Duration::from_millis(350)).await;
    let retained = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request retained TLS generation");
    assert_eq!(retained.status(), reqwest::StatusCode::OK);

    let mut system = trusted.clone();
    system["upstreams"][0]["tls"] = json!({"trustMode": "system"});
    write_config(directory.path(), &system);
    wait_for_status(
        &client,
        &url,
        "test.localhost",
        reqwest::StatusCode::BAD_GATEWAY,
    )
    .await;

    write_config(directory.path(), &trusted);
    wait_for_body(&client, &url, "test.localhost", "secure-upstream").await;
    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn serves_https_with_rustls() {
    let directory = TempDir::new().expect("create temp directory");
    let mut params = CertificateParams::new(vec!["localhost".to_owned()]).expect("cert params");
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, "localhost");
    let key = KeyPair::generate().expect("generate key");
    let certificate = params.self_signed(&key).expect("generate certificate");
    fs::write(directory.path().join("cert.pem"), certificate.pem()).expect("write cert");
    fs::write(directory.path().join("key.pem"), key.serialize_pem()).expect("write key");

    let port = available_port();
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-https-test",
        "limits": {
            "requestTimeoutMs": 5000,
            "drainTimeoutMs": 1000,
            "maxConnections": 64
        },
        "listeners": [{
            "id": "https",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1", "http2"],
            "tlsPolicyRef": "tls",
            "defaultVirtualHostRef": "https-host"
        }],
        "certificates": [{
            "id": "cert",
            "serverNames": ["localhost"],
            "source": {
                "type": "protected-file",
                "certificateFile": "cert.pem",
                "privateKeyFile": "key.pem"
            }
        }],
        "tlsPolicies": [{
            "id": "tls",
            "certificateRef": "cert",
            "minimumVersion": "tls1.2",
            "maximumVersion": "tls1.3",
            "alpn": ["h2", "http/1.1"]
        }],
        "resources": [{
            "id": "secure-response",
            "type": "respond",
            "status": 200,
            "body": "secure\n"
        }],
        "virtualHosts": [{
            "id": "https-host",
            "listenerRefs": ["https"],
            "serverNames": ["localhost"],
            "routes": [{
                "id": "secure",
                "match": {"pathType": "prefix", "path": "/"},
                "resourceRef": "secure-response"
            }]
        }]
    });
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build TLS test client");
    let response = wait_for_http(&client, &format!("https://localhost:{port}/"), "localhost").await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(response.version(), Version::HTTP_2);
    assert_eq!(response.text().await.expect("read TLS body"), "secure\n");

    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn guards_chunked_framing_after_tls_decryption() {
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let mut config = single_https_config(port, "localhost", "localhost");
    config["limits"]["maxRequestLineBytes"] = json!(64);
    config["limits"]["maxRequestMethodBytes"] = json!(8);
    config["limits"]["maxRequestTargetBytes"] = json!(16);
    config["limits"]["maxHeaderNameBytes"] = json!(32);
    config["limits"]["maxHeaderValueBytes"] = json!(32);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let readiness_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build HTTPS readiness client");
    wait_for_http(
        &readiness_client,
        &format!("https://localhost:{port}/"),
        "localhost",
    )
    .await;

    let valid = raw_tls_http1_response(
        port,
        &certificate_der,
        b"POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\nTrailer: X-Checksum\r\nConnection: close\r\n\r\n4\r\ntest\r\n0\r\nX-Checksum: ok\r\n\r\n",
    )
    .await;
    assert_eq!(raw_status_code(&valid), Some(200));

    for ambiguous in [
        b"POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\nContent-Length: 4\r\nConnection: close\r\n\r\n0\r\n\r\n".as_slice(),
        b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 4\r\nContent-Length: 4\r\nConnection: close\r\n\r\ntest".as_slice(),
    ] {
        let response = raw_tls_http1_response(port, &certificate_der, ambiguous).await;
        assert!(
            raw_status_code(&response).is_none_or(|status| !(200..300).contains(&status)),
            "ambiguous TLS request must close or return a non-success status"
        );
    }
    let oversized_target = raw_tls_http1_response(
        port,
        &certificate_der,
        b"GET /1234567890123456 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(
        raw_status_code(&oversized_target).is_none_or(|status| !(200..300).contains(&status)),
        "oversized TLS request target must not execute a successful route"
    );

    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn preserves_expect_continue_semantics_over_tls() {
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let config = single_https_config(port, "localhost", "localhost");
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let readiness_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build HTTPS readiness client");
    wait_for_http(
        &readiness_client,
        &format!("https://localhost:{port}/"),
        "localhost",
    )
    .await;

    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(certificate_der))
        .expect("trust generated certificate");
    let mut tls_config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    tls_config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect raw TLS client");
    let mut tls = TlsConnector::from(Arc::new(tls_config))
        .connect(
            ServerName::try_from("localhost".to_owned()).expect("valid DNS name"),
            tcp,
        )
        .await
        .expect("complete raw HTTP/1 TLS handshake");
    assert_eq!(
        tls.get_ref().1.alpn_protocol(),
        Some(b"http/1.1".as_slice())
    );
    tls.write_all(
        b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 4\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n",
    )
    .await
    .expect("write TLS Expect headers");
    let informational = read_header_block(&mut tls).await;
    assert_eq!(informational, b"HTTP/1.1 100 Continue\r\n\r\n");
    tls.write_all(b"four")
        .await
        .expect("write TLS request body");
    let mut response = Vec::new();
    timeout(Duration::from_secs(2), tls.read_to_end(&mut response))
        .await
        .expect("TLS final response completes")
        .expect("read TLS final response");
    assert_eq!(raw_status_code(&response), Some(200));

    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn selects_exact_and_wildcard_sni_certificates_and_fails_closed() {
    let directory = TempDir::new().expect("create temp directory");
    let alpha_der = write_self_signed_certificate(directory.path(), "alpha", &["alpha.localhost"]);
    let beta_der = write_self_signed_certificate(directory.path(), "beta", &["beta.localhost"]);
    let wildcard_der =
        write_self_signed_certificate(directory.path(), "wildcard", &["*.example.test"]);
    let api_der = write_self_signed_certificate(directory.path(), "api", &["api.example.test"]);
    let port = available_port();
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-sni-test",
        "limits": {
            "requestTimeoutMs": 5000,
            "drainTimeoutMs": 1000,
            "maxConnections": 64
        },
        "listeners": [{
            "id": "https",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1"],
            "tlsPolicyRef": "tls",
            "defaultVirtualHostRef": "https-host"
        }],
        "certificates": [
            {
                "id": "alpha-cert",
                "serverNames": ["alpha.localhost"],
                "source": {
                    "type": "protected-file",
                    "certificateFile": "alpha.pem",
                    "privateKeyFile": "alpha.key"
                }
            },
            {
                "id": "beta-cert",
                "serverNames": ["beta.localhost"],
                "source": {
                    "type": "protected-file",
                    "certificateFile": "beta.pem",
                    "privateKeyFile": "beta.key"
                }
            },
            {
                "id": "wildcard-cert",
                "serverNames": ["*.example.test"],
                "source": {
                    "type": "protected-file",
                    "certificateFile": "wildcard.pem",
                    "privateKeyFile": "wildcard.key"
                }
            },
            {
                "id": "api-cert",
                "serverNames": ["api.example.test"],
                "source": {
                    "type": "protected-file",
                    "certificateFile": "api.pem",
                    "privateKeyFile": "api.key"
                }
            }
        ],
        "tlsPolicies": [{
            "id": "tls",
            "certificateRefs": ["alpha-cert", "beta-cert", "wildcard-cert", "api-cert"],
            "minimumVersion": "tls1.2",
            "maximumVersion": "tls1.3",
            "alpn": ["http/1.1"]
        }],
        "resources": [{
            "id": "secure-response",
            "type": "respond",
            "status": 200,
            "body": "secure\n"
        }],
        "virtualHosts": [{
            "id": "https-host",
            "listenerRefs": ["https"],
            "serverNames": ["alpha.localhost", "beta.localhost", "*.example.test"],
            "routes": [{
                "id": "secure",
                "match": {"pathType": "prefix", "path": "/"},
                "resourceRef": "secure-response"
            }]
        }]
    });
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let address = ([127, 0, 0, 1], port).into();
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .tls_info(true)
        .resolve("alpha.localhost", address)
        .resolve("beta.localhost", address)
        .resolve("www.example.test", address)
        .resolve("api.example.test", address)
        .resolve("unknown.localhost", address)
        .build()
        .expect("build SNI test client");

    for (server_name, expected_der) in [
        ("alpha.localhost", alpha_der.as_slice()),
        ("beta.localhost", beta_der.as_slice()),
        ("www.example.test", wildcard_der.as_slice()),
        ("api.example.test", api_der.as_slice()),
    ] {
        let response = wait_for_http(
            &client,
            &format!("https://{server_name}:{port}/"),
            server_name,
        )
        .await;
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let tls_info = response
            .extensions()
            .get::<reqwest::tls::TlsInfo>()
            .expect("TLS response metadata");
        assert_eq!(
            tls_info.peer_certificate().expect("peer certificate"),
            expected_der,
            "unexpected certificate for SNI {server_name}"
        );
    }

    client
        .get(format!("https://unknown.localhost:{port}/"))
        .send()
        .await
        .expect_err("unknown SNI must fail the TLS handshake");
    client
        .get(format!("https://127.0.0.1:{port}/"))
        .send()
        .await
        .expect_err("a connection without DNS SNI must fail the TLS handshake");

    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn rejects_certificate_whose_san_does_not_cover_declared_name() {
    let directory = TempDir::new().expect("create temp directory");
    write_self_signed_certificate(directory.path(), "wrong", &["wrong.localhost"]);
    let port = available_port();
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-invalid-certificate-test",
        "listeners": [{
            "id": "https",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1"],
            "tlsPolicyRef": "tls",
            "defaultVirtualHostRef": "https-host"
        }],
        "certificates": [{
            "id": "wrong-cert",
            "serverNames": ["expected.localhost"],
            "source": {
                "type": "protected-file",
                "certificateFile": "wrong.pem",
                "privateKeyFile": "wrong.key"
            }
        }],
        "tlsPolicies": [{
            "id": "tls",
            "certificateRef": "wrong-cert",
            "minimumVersion": "tls1.2",
            "maximumVersion": "tls1.3",
            "alpn": ["http/1.1"]
        }],
        "resources": [{
            "id": "response",
            "type": "respond",
            "status": 200,
            "body": "unreachable"
        }],
        "virtualHosts": [{
            "id": "https-host",
            "listenerRefs": ["https"],
            "serverNames": ["expected.localhost"],
            "routes": [{
                "id": "response-route",
                "match": {"pathType": "prefix", "path": "/"},
                "resourceRef": "response"
            }]
        }]
    });
    let path = write_config(directory.path(), &config);
    let compiled = load_and_compile_webserver_config(path).expect("compile TLS references");
    let error = run_data_plane_until(compiled, std::future::pending())
        .await
        .expect_err("mismatched SAN must fail before serving");
    assert!(matches!(
        error,
        sdkwork_api_webserver_standalone_gateway::DataPlaneError::TlsFiles { .. }
    ));
}

#[tokio::test]
async fn rejects_certificate_and_private_key_mismatch_before_listener_start() {
    let directory = TempDir::new().expect("create temp directory");
    write_self_signed_certificate(directory.path(), "expected", &["expected.localhost"]);
    write_self_signed_certificate(directory.path(), "other", &["other.localhost"]);
    fs::copy(
        directory.path().join("other.key"),
        directory.path().join("expected.key"),
    )
    .expect("replace private key with mismatched key");
    let port = available_port();
    let config = single_https_config(port, "expected.localhost", "expected");
    let path = write_config(directory.path(), &config);
    let compiled = load_and_compile_webserver_config(path).expect("compile TLS references");
    let error = run_data_plane_until(compiled, std::future::pending())
        .await
        .expect_err("mismatched private key must fail before serving");
    assert!(matches!(
        error,
        sdkwork_api_webserver_standalone_gateway::DataPlaneError::TlsFiles { .. }
    ));
}

#[tokio::test]
async fn advertises_http2_stream_budget_and_rejects_oversized_header_lists() {
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let mut config = single_https_config(port, "localhost", "localhost");
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["tlsPolicies"][0]["alpn"] = json!(["h2", "http/1.1"]);
    config["limits"]["http2MaxConcurrentStreams"] = json!(3);
    config["limits"]["http2MaxHeaderListBytes"] = json!(1_024);
    config["limits"]["http2MaxSendBufferBytes"] = json!(4_096);
    config["limits"]["http2MaxFrameBytes"] = json!(32_768);
    config["limits"]["maxRequestBodyBytes"] = json!(4);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let readiness_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build readiness client");
    wait_for_http(
        &readiness_client,
        &format!("https://localhost:{port}/"),
        "localhost",
    )
    .await;

    let mut raw_h2 = connect_tls(port, &certificate_der, b"h2").await;
    raw_h2
        .write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n\0\0\0\x04\0\0\0\0\0")
        .await
        .expect("write raw HTTP/2 preface and SETTINGS");
    let mut advertised_streams = None;
    let mut advertised_frame_bytes = None;
    let mut advertised_header_bytes = None;
    for _ in 0..4 {
        let (frame_type, flags, stream_id, payload) = read_h2_frame(&mut raw_h2).await;
        if frame_type != 0x4 || flags & 0x1 != 0 {
            continue;
        }
        assert_eq!(stream_id, 0);
        assert_eq!(payload.len() % 6, 0);
        for setting in payload.chunks_exact(6) {
            let id = u16::from_be_bytes([setting[0], setting[1]]);
            let value = u32::from_be_bytes([setting[2], setting[3], setting[4], setting[5]]);
            match id {
                0x3 => advertised_streams = Some(value),
                0x5 => advertised_frame_bytes = Some(value),
                0x6 => advertised_header_bytes = Some(value),
                _ => {}
            }
        }
        break;
    }
    assert_eq!(advertised_streams, Some(3));
    assert_eq!(advertised_frame_bytes, Some(32_768));
    assert_eq!(advertised_header_bytes, Some(1_024));
    drop(raw_h2);

    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(certificate_der))
        .expect("trust generated certificate");
    let mut tls_config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    tls_config.alpn_protocols = vec![b"h2".to_vec()];
    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect HTTP/2 TLS client");
    let tls = TlsConnector::from(Arc::new(tls_config))
        .connect(
            ServerName::try_from("localhost".to_owned()).expect("valid DNS name"),
            tcp,
        )
        .await
        .expect("complete HTTP/2 TLS handshake");
    assert_eq!(tls.get_ref().1.alpn_protocol(), Some(b"h2".as_slice()));
    let (mut send, connection) = h2::client::handshake(tls)
        .await
        .expect("complete HTTP/2 connection preface");
    let connection_task = tokio::spawn(connection);

    timeout(Duration::from_secs(2), async {
        loop {
            if send.current_max_send_streams() == 3 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("receive server SETTINGS_MAX_CONCURRENT_STREAMS");

    send = send.ready().await.expect("HTTP/2 sender ready");
    let normal = Request::builder()
        .uri("https://localhost/")
        .body(())
        .expect("build normal HTTP/2 request");
    let (normal_response, _) = send
        .send_request(normal, true)
        .expect("send normal HTTP/2 request");
    assert_eq!(
        normal_response
            .await
            .expect("receive normal HTTP/2 response")
            .status(),
        reqwest::StatusCode::OK
    );

    send = send
        .ready()
        .await
        .expect("HTTP/2 sender ready for body limit");
    let body_request = Request::builder()
        .method("POST")
        .uri("https://localhost/")
        .body(())
        .expect("build HTTP/2 body request");
    let (body_response, mut body_stream) = send
        .send_request(body_request, false)
        .expect("send HTTP/2 body request headers");
    body_stream
        .send_data(Bytes::from_static(b"large"), true)
        .expect("send oversized HTTP/2 body without Content-Length");
    assert_eq!(
        body_response
            .await
            .expect("receive HTTP/2 body-limit response")
            .status(),
        reqwest::StatusCode::PAYLOAD_TOO_LARGE
    );

    send = send
        .ready()
        .await
        .expect("HTTP/2 sender ready after response");
    let oversized = Request::builder()
        .uri("https://localhost/")
        .header("x-oversized", "x".repeat(4_096))
        .body(())
        .expect("build oversized HTTP/2 request");
    let rejected = match send.send_request(oversized, true) {
        Err(_) => true,
        Ok((response, _)) => match response.await {
            Err(_) => true,
            Ok(response) => response.status() != reqwest::StatusCode::OK,
        },
    };
    assert!(rejected, "oversized HTTP/2 header list must be rejected");

    connection_task.abort();
    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn sheds_excess_http2_stream_until_streaming_response_body_completes() {
    let (upstream_address, release, upstream_shutdown, upstream_task) =
        spawn_held_response_upstream().await;
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let config = held_https_proxy_config(port, upstream_address);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let readiness_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build admission readiness client");
    wait_for_http(
        &readiness_client,
        &format!("https://localhost:{port}/ready"),
        "localhost",
    )
    .await;

    let (mut send, connection_task) = connect_h2(port, &certificate_der).await;
    send = send.ready().await.expect("HTTP/2 sender ready");
    let held_request = Request::builder()
        .uri("https://localhost/held")
        .body(())
        .expect("build held request");
    let (held_response, _) = send
        .send_request(held_request, true)
        .expect("send held request");
    let held_response = held_response.await.expect("receive held response headers");
    assert_eq!(held_response.status(), reqwest::StatusCode::OK);
    let mut held_body = held_response.into_body();

    send = send.ready().await.expect("HTTP/2 sender remains ready");
    let excess_request = Request::builder()
        .uri("https://localhost/excess")
        .body(())
        .expect("build excess request");
    let (excess_response, _) = send
        .send_request(excess_request, true)
        .expect("send excess request");
    let excess_response = excess_response
        .await
        .expect("receive bounded overload response");
    assert_eq!(
        excess_response.status(),
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        excess_response
            .headers()
            .get(axum::http::header::RETRY_AFTER),
        Some(&"1".parse().expect("Retry-After value"))
    );
    assert!(!excess_response
        .headers()
        .contains_key(axum::http::header::CONNECTION));

    release.notify_one();
    let mut held_bytes = Vec::new();
    while let Some(data) = held_body.data().await {
        held_bytes.extend_from_slice(&data.expect("read held response data"));
    }
    assert_eq!(held_bytes, b"startend");

    send = send.ready().await.expect("HTTP/2 sender recovers capacity");
    let recovered_request = Request::builder()
        .uri("https://localhost/recovered")
        .body(())
        .expect("build recovered request");
    let (recovered_response, _) = send
        .send_request(recovered_request, true)
        .expect("send recovered request");
    let recovered_response = recovered_response
        .await
        .expect("receive recovered response");
    assert_eq!(recovered_response.status(), reqwest::StatusCode::OK);

    drop(send);
    connection_task.abort();
    let _ = connection_task.await;
    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn releases_http2_admission_after_client_stream_reset() {
    let (upstream_address, release, upstream_shutdown, upstream_task) =
        spawn_held_response_upstream().await;
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let config = held_https_proxy_config(port, upstream_address);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let readiness_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build cancellation readiness client");
    wait_for_http(
        &readiness_client,
        &format!("https://localhost:{port}/ready"),
        "localhost",
    )
    .await;

    let (mut send, connection_task) = connect_h2(port, &certificate_der).await;
    send = send.ready().await.expect("HTTP/2 sender ready");
    let held_request = Request::builder()
        .uri("https://localhost/held")
        .body(())
        .expect("build cancellable request");
    let (held_response, mut held_request_stream) = send
        .send_request(held_request, true)
        .expect("send cancellable request");
    let held_response = held_response
        .await
        .expect("receive cancellable response headers");
    let _held_body = held_response.into_body();

    send = send.ready().await.expect("HTTP/2 sender remains ready");
    let excess_request = Request::builder()
        .uri("https://localhost/excess")
        .body(())
        .expect("build pre-cancel excess request");
    let (excess_response, _) = send
        .send_request(excess_request, true)
        .expect("send pre-cancel excess request");
    assert_eq!(
        excess_response
            .await
            .expect("receive pre-cancel overload response")
            .status(),
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );

    let cancellation_started = tokio::time::Instant::now();
    held_request_stream.send_reset(h2::Reason::CANCEL);
    timeout(Duration::from_secs(2), async {
        loop {
            send = send
                .ready()
                .await
                .expect("HTTP/2 connection survives reset");
            let recovered_request = Request::builder()
                .uri("https://localhost/recovered")
                .body(())
                .expect("build post-cancel request");
            let (response, _) = send
                .send_request(recovered_request, true)
                .expect("send post-cancel request");
            if response
                .await
                .expect("receive post-cancel response")
                .status()
                == reqwest::StatusCode::OK
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("stream reset releases request admission");
    assert!(cancellation_started.elapsed() < Duration::from_secs(2));

    release.notify_one();
    connection_task.abort();
    let _ = connection_task.await;
    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn closes_overloaded_http1_connection_and_recovers_after_stream_completion() {
    let (upstream_address, release, upstream_shutdown, upstream_task) =
        spawn_held_response_upstream().await;
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let config = held_http_proxy_config(port, upstream_address);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let first_client = reqwest::Client::new();
    let second_client = reqwest::Client::new();
    let base_url = format!("http://127.0.0.1:{port}");
    wait_for_http(
        &first_client,
        &format!("{base_url}/ready"),
        "test.localhost",
    )
    .await;

    let mut held_stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect cancellable HTTP/1 client");
    held_stream
        .write_all(b"GET /held HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("write cancellable HTTP/1 request");
    let held_headers = read_header_block(&mut held_stream).await;
    assert_eq!(raw_status_code(&held_headers), Some(200));

    let excess_response = second_client
        .get(format!("{base_url}/excess"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("receive HTTP/1 overload response");
    assert_eq!(
        excess_response.status(),
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        excess_response
            .headers()
            .get(axum::http::header::RETRY_AFTER),
        Some(&"1".parse().expect("Retry-After value"))
    );
    assert_eq!(
        excess_response
            .headers()
            .get(axum::http::header::CONNECTION),
        Some(&"close".parse().expect("Connection value"))
    );

    release.notify_one();
    let mut held_body = Vec::new();
    timeout(
        Duration::from_secs(2),
        held_stream.read_to_end(&mut held_body),
    )
    .await
    .expect("held HTTP/1 response completes")
    .expect("read held HTTP/1 response Body");
    assert!(held_body.windows(5).any(|window| window == b"start"));
    assert!(held_body.windows(3).any(|window| window == b"end"));
    wait_for_body(
        &second_client,
        &format!("{base_url}/recovered"),
        "test.localhost",
        "recovered",
    )
    .await;
    let recovered = second_client
        .get(format!("{base_url}/recovered"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request recovers after response Body cancellation");
    assert_eq!(recovered.status(), reqwest::StatusCode::OK);
    assert_eq!(
        recovered.text().await.expect("read recovered Body"),
        "recovered"
    );

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn times_out_idle_http1_response_body_and_releases_admission() {
    let (upstream_address, release, upstream_shutdown, upstream_task) =
        spawn_held_response_upstream().await;
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let mut config = held_http_proxy_config(port, upstream_address);
    config["limits"]["responseBodyIdleTimeoutMs"] = json!(200);
    config["limits"]["connectionWriteTimeoutMs"] = json!(5_000);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::new();
    let base_url = format!("http://127.0.0.1:{port}");
    wait_for_http(&client, &format!("{base_url}/ready"), "test.localhost").await;

    let mut held_stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect idle-response HTTP/1 client");
    held_stream
        .write_all(b"GET /held HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("write idle-response request");
    let held_headers = read_header_block(&mut held_stream).await;
    assert_eq!(raw_status_code(&held_headers), Some(200));
    let timeout_started = tokio::time::Instant::now();
    let mut partial_body = Vec::new();
    timeout(
        Duration::from_secs(2),
        held_stream.read_to_end(&mut partial_body),
    )
    .await
    .expect("idle HTTP/1 response closes before upstream timeout")
    .expect("read partial idle HTTP/1 response");
    assert!(timeout_started.elapsed() < Duration::from_secs(2));
    assert!(partial_body.windows(5).any(|window| window == b"start"));
    assert!(!partial_body.windows(3).any(|window| window == b"end"));

    wait_for_body(
        &client,
        &format!("{base_url}/recovered"),
        "test.localhost",
        "recovered",
    )
    .await;

    release.notify_one();
    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn times_out_idle_http2_response_stream_and_keeps_connection_healthy() {
    let (upstream_address, release, upstream_shutdown, upstream_task) =
        spawn_held_response_upstream().await;
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let mut config = held_https_proxy_config(port, upstream_address);
    config["limits"]["responseBodyIdleTimeoutMs"] = json!(200);
    config["limits"]["connectionWriteTimeoutMs"] = json!(5_000);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let readiness_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build idle H2 readiness client");
    wait_for_http(
        &readiness_client,
        &format!("https://localhost:{port}/ready"),
        "localhost",
    )
    .await;

    let (mut send, connection_task) = connect_h2(port, &certificate_der).await;
    send = send.ready().await.expect("HTTP/2 sender ready");
    let request = Request::builder()
        .uri("https://localhost/held")
        .body(())
        .expect("build idle H2 request");
    let (response, _) = send
        .send_request(request, true)
        .expect("send idle H2 request");
    let response = response.await.expect("receive idle H2 response headers");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let mut body = response.into_body();
    assert_eq!(
        body.data()
            .await
            .expect("receive H2 response prefix")
            .expect("valid H2 response prefix"),
        Bytes::from_static(b"start")
    );
    let timeout_started = tokio::time::Instant::now();
    let stalled = timeout(Duration::from_secs(2), body.data())
        .await
        .expect("idle H2 Stream terminates before upstream timeout");
    assert!(
        stalled.is_none() || stalled.is_some_and(|result| result.is_err()),
        "idle H2 response must end or reset without another Data Frame"
    );
    assert!(timeout_started.elapsed() < Duration::from_secs(2));

    send = send
        .ready()
        .await
        .expect("H2 connection survives idle Stream");
    let recovered_request = Request::builder()
        .uri("https://localhost/recovered")
        .body(())
        .expect("build recovered H2 request");
    let (recovered_response, _) = send
        .send_request(recovered_request, true)
        .expect("send recovered H2 request");
    assert_eq!(
        recovered_response
            .await
            .expect("receive recovered H2 response")
            .status(),
        reqwest::StatusCode::OK
    );

    release.notify_one();
    connection_task.abort();
    let _ = connection_task.await;
    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn closes_slow_reading_http1_client_at_connection_write_deadline() {
    const LARGE_FILE_BYTES: u64 = 32 * 1024 * 1024;

    let directory = TempDir::new().expect("create temp directory");
    let public = directory.path().join("public");
    fs::create_dir(&public).expect("create static directory");
    let large_file = fs::File::create(public.join("large.bin")).expect("create large static file");
    large_file
        .set_len(LARGE_FILE_BYTES)
        .expect("size sparse static file");
    let port = available_port();
    let mut config = base_config(
        port,
        json!([
            {
                "id": "ready-response",
                "type": "respond",
                "status": 200,
                "body": "ready"
            },
            {
                "id": "large-static",
                "type": "static",
                "root": "public",
                "indexFiles": ["index.html"],
                "followSymlinks": false
            }
        ]),
        json!([]),
        json!([
            {
                "id": "ready-route",
                "match": {"pathType": "exact", "path": "/ready"},
                "resourceRef": "ready-response"
            },
            {
                "id": "large-route",
                "match": {"pathType": "prefix", "path": "/"},
                "resourceRef": "large-static"
            }
        ]),
    );
    config["limits"]["maxConcurrentRequests"] = json!(1);
    config["limits"]["responseBodyIdleTimeoutMs"] = json!(5_000);
    config["limits"]["connectionWriteTimeoutMs"] = json!(200);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::new();
    let base_url = format!("http://127.0.0.1:{port}");
    wait_for_http(&client, &format!("{base_url}/ready"), "test.localhost").await;

    let mut slow_client = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect slow-reading client");
    slow_client
        .write_all(b"GET /large.bin HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("request large static file");
    let headers = read_header_block(&mut slow_client).await;
    assert_eq!(raw_status_code(&headers), Some(200));
    tokio::time::sleep(Duration::from_millis(750)).await;

    let transferred = timeout(Duration::from_secs(3), async {
        let mut total = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = slow_client
                .read(&mut buffer)
                .await
                .expect("read buffered slow-client response");
            if read == 0 {
                return total;
            }
            total = total.saturating_add(read as u64);
            assert!(total <= LARGE_FILE_BYTES, "slow-client read stays bounded");
        }
    })
    .await
    .expect("connection write timeout closes slow client");
    assert!(
        transferred < LARGE_FILE_BYTES,
        "write timeout must close before the complete large response is delivered"
    );

    wait_for_body(
        &client,
        &format!("{base_url}/ready"),
        "test.localhost",
        "ready",
    )
    .await;
    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn closes_http2_reset_churn_and_recovers_on_a_new_connection() {
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let mut config = single_https_config(port, "localhost", "localhost");
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["tlsPolicies"][0]["alpn"] = json!(["h2", "http/1.1"]);
    config["limits"]["http2AbuseWindowMs"] = json!(5_000);
    config["limits"]["http2MaxFramesPerWindow"] = json!(1_000);
    config["limits"]["http2MaxNewStreamsPerWindow"] = json!(100);
    config["limits"]["http2MaxResetFramesPerWindow"] = json!(2);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let readiness_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build HTTP/2 readiness client");
    wait_for_http(
        &readiness_client,
        &format!("https://localhost:{port}/"),
        "localhost",
    )
    .await;

    let (mut send, connection_task) = connect_h2(port, &certificate_der).await;
    for _ in 0..3 {
        send = match send.ready().await {
            Ok(send) => send,
            Err(_) => break,
        };
        let request = Request::builder()
            .uri("https://localhost/")
            .body(())
            .expect("build reset-churn request");
        let Ok((_response, mut body)) = send.send_request(request, false) else {
            break;
        };
        body.send_reset(h2::Reason::CANCEL);
    }
    let connection_result = timeout(Duration::from_secs(2), connection_task)
        .await
        .expect("reset-churn connection closes")
        .expect("reset-churn connection task joins");
    assert!(
        connection_result.is_err(),
        "reset churn must close only the offending HTTP/2 connection"
    );

    let fresh_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build fresh HTTP/2 client");
    let healthy = wait_for_http(
        &fresh_client,
        &format!("https://localhost:{port}/"),
        "localhost",
    )
    .await;
    assert_eq!(healthy.status(), reqwest::StatusCode::OK);
    assert_eq!(healthy.version(), Version::HTTP_2);

    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn closes_http2_new_stream_churn_and_recovers_on_a_new_connection() {
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let mut config = single_https_config(port, "localhost", "localhost");
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["tlsPolicies"][0]["alpn"] = json!(["h2", "http/1.1"]);
    config["limits"]["http2AbuseWindowMs"] = json!(60_000);
    config["limits"]["http2MaxFramesPerWindow"] = json!(100);
    config["limits"]["http2MaxNewStreamsPerWindow"] = json!(2);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let readiness_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build HTTP/2 readiness client");
    wait_for_http(
        &readiness_client,
        &format!("https://localhost:{port}/"),
        "localhost",
    )
    .await;

    let (mut send, connection_task) = connect_h2(port, &certificate_der).await;
    for _ in 0..3 {
        send = match send.ready().await {
            Ok(send) => send,
            Err(_) => break,
        };
        let request = Request::builder()
            .uri("https://localhost/")
            .body(())
            .expect("build stream-churn request");
        match send.send_request(request, true) {
            Ok((response, _)) => {
                let _ = response.await;
            }
            Err(_) => break,
        }
    }
    let connection_result = timeout(Duration::from_secs(2), connection_task)
        .await
        .expect("new-stream-churn connection closes")
        .expect("new-stream-churn connection task joins");
    assert!(
        connection_result.is_err(),
        "new Stream churn must close only the offending HTTP/2 connection"
    );

    let fresh_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build fresh HTTP/2 client");
    let healthy = wait_for_http(
        &fresh_client,
        &format!("https://localhost:{port}/"),
        "localhost",
    )
    .await;
    assert_eq!(healthy.status(), reqwest::StatusCode::OK);
    assert_eq!(healthy.version(), Version::HTTP_2);

    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn closes_http2_frame_flood_and_recovers_on_a_new_connection() {
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let mut config = single_https_config(port, "localhost", "localhost");
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["tlsPolicies"][0]["alpn"] = json!(["h2", "http/1.1"]);
    config["limits"]["http2AbuseWindowMs"] = json!(60_000);
    config["limits"]["http2MaxFramesPerWindow"] = json!(100);
    config["limits"]["http2MaxNewStreamsPerWindow"] = json!(100);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let readiness_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build HTTP/2 readiness client");
    wait_for_http(
        &readiness_client,
        &format!("https://localhost:{port}/"),
        "localhost",
    )
    .await;

    let mut raw_h2 = connect_tls(port, &certificate_der, b"h2").await;
    let mut flood = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n".to_vec();
    flood.extend(encode_h2_frame(0x4, 0, 0, &[]));
    for sequence in 0_u64..100 {
        flood.extend(encode_h2_frame(0x6, 0, 0, &sequence.to_be_bytes()));
    }
    raw_h2
        .write_all(&flood)
        .await
        .expect("write bounded HTTP/2 PING flood");
    let mut response = Vec::new();
    let _closed_result = timeout(Duration::from_secs(2), raw_h2.read_to_end(&mut response))
        .await
        .expect("frame-flood connection closes at the configured limit");

    let fresh_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build fresh HTTP/2 client");
    let healthy = wait_for_http(
        &fresh_client,
        &format!("https://localhost:{port}/"),
        "localhost",
    )
    .await;
    assert_eq!(healthy.status(), reqwest::StatusCode::OK);
    assert_eq!(healthy.version(), Version::HTTP_2);

    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn sends_enhance_your_calm_after_excessive_http2_local_error_resets() {
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let mut config = single_https_config(port, "localhost", "localhost");
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["tlsPolicies"][0]["alpn"] = json!(["h2", "http/1.1"]);
    config["limits"]["http2MaxLocalErrorResetStreams"] = json!(2);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let readiness_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build HTTP/2 readiness client");
    wait_for_http(
        &readiness_client,
        &format!("https://localhost:{port}/"),
        "localhost",
    )
    .await;

    let (mut send, connection_task) = connect_h2(port, &certificate_der).await;
    for index in 0..4 {
        send = match send.ready().await {
            Ok(send) => send,
            Err(_) => break,
        };
        let request = Request::builder()
            .uri("https://localhost/")
            .header("content-length", "1")
            .header("x-local-error-sequence", index.to_string())
            .body(())
            .expect("build local-error-reset request");
        match send.send_request(request, true) {
            Ok((response, _)) => {
                let _ = response.await;
            }
            Err(_) => break,
        }
    }
    let connection_error = timeout(Duration::from_secs(2), connection_task)
        .await
        .expect("local-error-reset connection receives GOAWAY")
        .expect("local-error-reset connection task joins")
        .expect_err("excessive local error resets must close with GOAWAY");
    assert!(connection_error.is_go_away());
    assert_eq!(
        connection_error.reason(),
        Some(h2::Reason::ENHANCE_YOUR_CALM)
    );

    let fresh_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build fresh HTTP/2 client");
    let healthy = wait_for_http(
        &fresh_client,
        &format!("https://localhost:{port}/"),
        "localhost",
    )
    .await;
    assert_eq!(healthy.status(), reqwest::StatusCode::OK);
    assert_eq!(healthy.version(), Version::HTTP_2);

    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn closes_oversized_http2_encoded_header_blocks_and_recovers() {
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let mut config = single_https_config(port, "localhost", "localhost");
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["tlsPolicies"][0]["alpn"] = json!(["h2", "http/1.1"]);
    config["limits"]["http2MaxHeaderListBytes"] = json!(65_536);
    config["limits"]["http2MaxEncodedHeaderBlockBytes"] = json!(1_024);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let readiness_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build HTTP/2 readiness client");
    wait_for_http(
        &readiness_client,
        &format!("https://localhost:{port}/"),
        "localhost",
    )
    .await;

    let (mut send, connection_task) = connect_h2(port, &certificate_der).await;
    send = send.ready().await.expect("HTTP/2 sender ready");
    let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let oversized = (0..8_192)
        .map(|index| alphabet[(index * 37 + index / 17) % alphabet.len()] as char)
        .collect::<String>();
    let request = Request::builder()
        .uri("https://localhost/")
        .header("x-encoded-block", oversized)
        .body(())
        .expect("build encoded-header abuse request");
    let _ = send.send_request(request, true);
    let connection_result = timeout(Duration::from_secs(2), connection_task)
        .await
        .expect("encoded-header connection closes")
        .expect("encoded-header connection task joins");
    assert!(
        connection_result.is_err(),
        "oversized encoded Header Block must close the offending connection"
    );

    let fresh_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build fresh HTTP/2 client");
    let healthy = wait_for_http(
        &fresh_client,
        &format!("https://localhost:{port}/"),
        "localhost",
    )
    .await;
    assert_eq!(healthy.status(), reqwest::StatusCode::OK);
    assert_eq!(healthy.version(), Version::HTTP_2);

    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn drains_inflight_http2_stream_after_goaway_and_rejects_new_streams() {
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let mut config = single_https_config(port, "localhost", "localhost");
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["tlsPolicies"][0]["alpn"] = json!(["h2", "http/1.1"]);
    config["limits"]["drainTimeoutMs"] = json!(2_000);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let readiness_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build HTTP/2 readiness client");
    wait_for_http(
        &readiness_client,
        &format!("https://localhost:{port}/"),
        "localhost",
    )
    .await;

    let (mut send, connection_task) = connect_h2(port, &certificate_der).await;
    send = send.ready().await.expect("HTTP/2 sender ready");
    let request = Request::builder()
        .method("POST")
        .uri("https://localhost/")
        .body(())
        .expect("build in-flight HTTP/2 request");
    let (response, mut body) = send
        .send_request(request, false)
        .expect("start in-flight HTTP/2 request");
    body.send_data(Bytes::from_static(b"fo"), false)
        .expect("send partial in-flight HTTP/2 body");
    tokio::time::sleep(Duration::from_millis(50)).await;

    shutdown.send(()).expect("begin graceful data-plane drain");
    tokio::time::sleep(Duration::from_millis(150)).await;
    let new_stream = timeout(Duration::from_secs(1), send.ready())
        .await
        .expect("GOAWAY reaches the HTTP/2 client");
    assert!(
        new_stream.is_err(),
        "HTTP/2 GOAWAY must prevent new Streams after the grace handshake"
    );

    body.send_data(Bytes::from_static(b"ur"), true)
        .expect("complete the in-flight HTTP/2 request");
    let response = timeout(Duration::from_secs(1), response)
        .await
        .expect("in-flight response completes before drain deadline")
        .expect("receive in-flight HTTP/2 response");
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    timeout(Duration::from_secs(3), connection_task)
        .await
        .expect("HTTP/2 client connection closes after GOAWAY")
        .expect("HTTP/2 connection task joins")
        .expect("HTTP/2 connection drains cleanly");
    timeout(Duration::from_secs(3), task)
        .await
        .expect("data plane exits within finite drain deadline")
        .expect("data-plane task joins")
        .expect("data plane stops cleanly after HTTP/2 drain");
}

#[tokio::test]
async fn maximum_connection_age_closes_http1_keep_alive_and_allows_a_fresh_connection() {
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let mut config = base_config(
        port,
        json!([{
            "id": "response",
            "type": "respond",
            "status": 200,
            "body": "aged"
        }]),
        json!([]),
        json!([{
            "id": "response-route",
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": "response"
        }]),
    );
    config["limits"]["maxConnectionAgeMs"] = json!(300);
    config["limits"]["http1KeepAliveIdleTimeoutMs"] = json!(5_000);
    config["limits"]["drainTimeoutMs"] = json!(500);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect HTTP/1 maximum-age client");
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: test.localhost\r\n\r\n")
        .await
        .expect("write first maximum-age request");
    let (_, body) = read_http1_fixed_response(&mut stream).await;
    assert_eq!(body, b"aged");

    tokio::time::sleep(Duration::from_millis(450)).await;
    assert_http1_connection_ends(&mut stream).await;

    let response = raw_http_response(
        port,
        b"GET / HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert_eq!(raw_status_code(&response), Some(200));
    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn maximum_connection_age_sends_h2_goaway_and_drains_an_inflight_stream() {
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let mut config = single_https_config(port, "localhost", "localhost");
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["tlsPolicies"][0]["alpn"] = json!(["h2", "http/1.1"]);
    config["limits"]["maxConnectionAgeMs"] = json!(300);
    config["limits"]["drainTimeoutMs"] = json!(1_500);
    config["limits"]["requestTimeoutMs"] = json!(5_000);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let (mut send, connection_task) = connect_h2(port, &certificate_der).await;
    send = send.ready().await.expect("maximum-age H2 sender ready");
    let request = Request::builder()
        .method("POST")
        .uri("https://localhost/")
        .body(())
        .expect("build maximum-age in-flight request");
    let (response, mut body) = send
        .send_request(request, false)
        .expect("start maximum-age in-flight Stream");
    body.send_data(Bytes::from_static(b"fo"), false)
        .expect("send partial maximum-age request Body");

    tokio::time::sleep(Duration::from_millis(450)).await;
    let new_stream = timeout(Duration::from_secs(1), send.ready())
        .await
        .expect("maximum-age GOAWAY reaches client");
    assert!(new_stream.is_err(), "GOAWAY must reject a new Stream");

    body.send_data(Bytes::from_static(b"ur"), true)
        .expect("complete in-flight request before age drain deadline");
    let response = timeout(Duration::from_secs(1), response)
        .await
        .expect("aged in-flight response completes")
        .expect("receive aged in-flight response");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    timeout(Duration::from_secs(2), connection_task)
        .await
        .expect("aged H2 connection closes")
        .expect("aged H2 connection task joins")
        .expect("aged H2 connection drains cleanly");

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build fresh client after connection retirement");
    let healthy = wait_for_http(&client, &format!("https://localhost:{port}/"), "localhost").await;
    assert_eq!(healthy.status(), reqwest::StatusCode::OK);
    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn maximum_connection_age_cancels_a_stream_after_the_finite_drain_deadline() {
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let mut config = single_https_config(port, "localhost", "localhost");
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["tlsPolicies"][0]["alpn"] = json!(["h2", "http/1.1"]);
    config["limits"]["maxConnections"] = json!(2);
    config["limits"]["maxConnectionAgeMs"] = json!(500);
    config["limits"]["drainTimeoutMs"] = json!(200);
    config["limits"]["requestTimeoutMs"] = json!(5_000);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let readiness_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build finite-drain readiness client");
    let ready = wait_for_http(
        &readiness_client,
        &format!("https://localhost:{port}/"),
        "localhost",
    )
    .await;
    assert_eq!(ready.status(), reqwest::StatusCode::OK);
    drop(ready);
    drop(readiness_client);
    tokio::time::sleep(Duration::from_millis(25)).await;

    let (mut send, connection_task) = connect_h2(port, &certificate_der).await;
    send = send.ready().await.expect("finite-drain H2 sender ready");
    let request = Request::builder()
        .method("POST")
        .uri("https://localhost/")
        .body(())
        .expect("build held request past age drain deadline");
    let (_response, mut body) = send
        .send_request(request, false)
        .expect("start held maximum-age Stream");
    body.send_data(Bytes::from_static(b"held"), false)
        .expect("hold request Body open");

    let _connection_result = timeout(Duration::from_secs(2), connection_task)
        .await
        .expect("connection closes by age plus drain deadline")
        .expect("finite-drain connection task joins");
    assert!(
        send.ready().await.is_err(),
        "forced drain must prevent further H2 Streams"
    );

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build recovery client after forced age drain");
    let healthy = wait_for_http(&client, &format!("https://localhost:{port}/"), "localhost").await;
    assert_eq!(healthy.status(), reqwest::StatusCode::OK);
    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn forwards_and_bounds_http2_request_and_response_trailers() {
    let (upstream_address, upstream_shutdown, upstream_task) = spawn_frame_echo_upstream().await;
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let mut config = base_config(
        port,
        json!([{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "echo-upstream",
            "stripPrefix": false
        }]),
        json!([{
            "id": "echo-upstream",
            "targets": [{"url": format!("http://{upstream_address}")}],
            "connectTimeoutMs": 1000,
            "requestTimeoutMs": 5000,
            "maxIdleConnections": 8
        }]),
        json!([{
            "id": "proxy-route",
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": "proxy"
        }]),
    );
    config["limits"]["maxRequestBodyBytes"] = json!(4);
    config["limits"]["maxTrailerBytes"] = json!(64);
    config["limits"]["maxTrailers"] = json!(1);
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["listeners"][0]["tlsPolicyRef"] = json!("tls");
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
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let readiness_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build readiness client");
    wait_for_http(
        &readiness_client,
        &format!("https://localhost:{port}/echo"),
        "localhost",
    )
    .await;

    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(certificate_der))
        .expect("trust generated certificate");
    let mut tls_config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    tls_config.alpn_protocols = vec![b"h2".to_vec()];
    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect HTTP/2 TLS client");
    let tls = TlsConnector::from(Arc::new(tls_config))
        .connect(
            ServerName::try_from("localhost".to_owned()).expect("valid DNS name"),
            tcp,
        )
        .await
        .expect("complete HTTP/2 TLS handshake");
    let (mut send, connection) = h2::client::handshake(tls)
        .await
        .expect("complete HTTP/2 connection preface");
    let connection_task = tokio::spawn(connection);

    send = send.ready().await.expect("HTTP/2 sender ready");
    let request = Request::builder()
        .method("POST")
        .uri("https://localhost/echo")
        .header("te", "trailers")
        .header("trailer", "x-checksum")
        .body(())
        .expect("build HTTP/2 trailer request");
    let (response, mut request_body) = send
        .send_request(request, false)
        .expect("send HTTP/2 request headers");
    request_body
        .send_data(Bytes::from_static(b"four"), false)
        .expect("send HTTP/2 request data");
    let mut request_trailers = axum::http::HeaderMap::new();
    request_trailers.insert("x-checksum", "verified".parse().expect("trailer value"));
    request_body
        .send_trailers(request_trailers)
        .expect("send HTTP/2 request trailers");
    let response = response.await.expect("receive HTTP/2 response");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let mut response_body = response.into_body();
    let data = response_body
        .data()
        .await
        .expect("receive response data")
        .expect("valid response data");
    assert_eq!(data, Bytes::from_static(b"four"));
    response_body
        .flow_control()
        .release_capacity(data.len())
        .expect("release response flow-control capacity");
    let response_trailers = response_body
        .trailers()
        .await
        .expect("receive response trailers")
        .expect("response includes trailers");
    assert_eq!(response_trailers["x-checksum"], "verified");

    send = send
        .ready()
        .await
        .expect("HTTP/2 sender ready after trailers");
    let rejected_request = Request::builder()
        .method("POST")
        .uri("https://localhost/inspect")
        .header("te", "trailers")
        .header("trailer", "x-one, x-two")
        .body(())
        .expect("build over-budget HTTP/2 trailer request");
    let (rejected_response, mut rejected_body) = send
        .send_request(rejected_request, false)
        .expect("send over-budget HTTP/2 request headers");
    let mut too_many = axum::http::HeaderMap::new();
    too_many.insert("x-one", "1".parse().expect("first trailer"));
    too_many.insert("x-two", "2".parse().expect("second trailer"));
    rejected_body
        .send_trailers(too_many)
        .expect("send over-budget HTTP/2 trailers");
    assert_eq!(
        rejected_response
            .await
            .expect("receive invalid trailer response")
            .status(),
        reqwest::StatusCode::BAD_REQUEST
    );

    send = send
        .ready()
        .await
        .expect("HTTP/2 sender ready for response limit");
    let response_limit_request = Request::builder()
        .uri("https://localhost/emit-many")
        .header("te", "trailers")
        .body(())
        .expect("build response-trailer-limit request");
    let (response_limit_response, _) = send
        .send_request(response_limit_request, true)
        .expect("send response-trailer-limit request");
    let response_limit_response = response_limit_response
        .await
        .expect("receive response-trailer-limit headers");
    assert_eq!(
        response_limit_response.status(),
        reqwest::StatusCode::BAD_GATEWAY,
        "an over-budget upstream Trailer declaration fails before response commitment"
    );

    connection_task.abort();
    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn normalizes_route_static_and_rewritten_proxy_paths_once() {
    let (upstream_address, upstream_shutdown, upstream_task) = spawn_frame_echo_upstream().await;
    let directory = TempDir::new().expect("create temp directory");
    let public = directory.path().join("public");
    fs::create_dir(&public).expect("create public root");
    fs::write(public.join("asset.txt"), "canonical-static").expect("write canonical static asset");
    let port = available_port();
    let config = base_config(
        port,
        json!([
            {
                "id": "static",
                "type": "static",
                "root": "public",
                "indexFiles": ["index.html"],
                "followSymlinks": false,
                "stripPrefix": true
            },
            {
                "id": "proxy",
                "type": "proxy",
                "upstreamRef": "origin",
                "stripPrefix": true
            },
            {"id": "b", "type": "respond", "status": 200, "body": "route-b"},
            {"id": "ab", "type": "respond", "status": 200, "body": "route-a-b"},
            {"id": "reserved", "type": "respond", "status": 200, "body": "route-reserved"}
        ]),
        json!([{
            "id": "origin",
            "targets": [{"url": format!("http://{upstream_address}")}],
            "connectTimeoutMs": 1000,
            "requestTimeoutMs": 5000,
            "maxIdleConnections": 2
        }]),
        json!([
            {"id": "files", "match": {"pathType": "prefix", "path": "/files"}, "resourceRef": "static"},
            {"id": "rewrite", "match": {"pathType": "prefix", "path": "/rewrite"}, "resourceRef": "proxy"},
            {"id": "b-route", "match": {"pathType": "exact", "path": "/b"}, "resourceRef": "b"},
            {"id": "ab-route", "match": {"pathType": "exact", "path": "/a/b"}, "resourceRef": "ab"},
            {"id": "reserved-route", "match": {"pathType": "exact", "path": "/reserved/a?b#c%d"}, "resourceRef": "reserved"}
        ]),
    );
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    for (path, expected) in [
        ("/a/../b", "route-b"),
        ("/a/%2e%2e/b", "route-b"),
        ("//a///b", "route-a-b"),
        ("/a%2fb", "route-a-b"),
    ] {
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n");
        let response = raw_http_response(port, request.as_bytes()).await;
        assert_eq!(raw_status_code(&response), Some(200));
        assert!(response
            .windows(expected.len())
            .any(|window| window == expected.as_bytes()));
    }
    let reserved_request = b"GET /reserved/a%3Fb%23c%25d HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n";
    let reserved_response = raw_http_response(port, reserved_request).await;
    assert_eq!(raw_status_code(&reserved_response), Some(200));
    assert!(reserved_response
        .windows(b"route-reserved".len())
        .any(|window| window == b"route-reserved"));
    assert_eq!(
        raw_http_status(
            port,
            b"GET /../../b HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n"
        )
        .await,
        400
    );
    assert_eq!(
        raw_http_status(
            port,
            b"GET /a/%5cb HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n"
        )
        .await,
        400
    );

    let static_response = raw_http_response(
        port,
        b"GET /files/nested/../asset.txt HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert_eq!(raw_status_code(&static_response), Some(200));
    assert!(static_response
        .windows(b"canonical-static".len())
        .any(|window| window == b"canonical-static"));

    let proxy_response = raw_http_response(
        port,
        b"GET /rewrite/nested/../query?x=1 HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert_eq!(raw_status_code(&proxy_response), Some(200));
    assert!(proxy_response
        .windows(b"x=1".len())
        .any(|window| window == b"x=1"));

    let reserved_proxy_response = raw_http_response(
        port,
        b"GET /rewrite/a%3Fb%23c%25d/%E4%B8%AD?x=1 HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert_eq!(raw_status_code(&reserved_proxy_response), Some(200));
    assert!(reserved_proxy_response
        .windows(b"/a%3Fb%23c%25d/%E4%B8%AD".len())
        .any(|window| window == b"/a%3Fb%23c%25d/%E4%B8%AD"));

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn enforces_uri_query_budgets_and_preserves_valid_proxy_query() {
    let (upstream_address, upstream_shutdown, upstream_task) = spawn_frame_echo_upstream().await;
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let mut config = base_config(
        port,
        json!([
            {
                "id": "response",
                "type": "respond",
                "status": 200,
                "body": "ok"
            },
            {
                "id": "proxy",
                "type": "proxy",
                "upstreamRef": "origin",
                "stripPrefix": true
            }
        ]),
        json!([{
            "id": "origin",
            "targets": [{"url": format!("http://{upstream_address}")}],
            "connectTimeoutMs": 1000,
            "requestTimeoutMs": 5000,
            "maxIdleConnections": 2
        }]),
        json!([
            {
                "id": "proxy-route",
                "match": {"pathType": "prefix", "path": "/proxy"},
                "resourceRef": "proxy"
            },
            {
                "id": "response-route",
                "match": {"pathType": "prefix", "path": "/"},
                "resourceRef": "response"
            }
        ]),
    );
    config["limits"]["maxUriPathBytes"] = json!(64);
    config["limits"]["maxDecodedPathBytes"] = json!(64);
    config["limits"]["maxPathSegments"] = json!(4);
    config["limits"]["maxQueryStringBytes"] = json!(32);
    config["limits"]["maxQueryParameters"] = json!(2);
    config["limits"]["maxQueryComponentBytes"] = json!(4);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    assert_eq!(
        raw_http_status(
            port,
            b"GET /a/b/c/d/e HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n"
        )
        .await,
        414
    );
    assert_eq!(
        raw_http_status(
            port,
            b"GET /?a=1&b=2&c=3 HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n"
        )
        .await,
        414
    );
    assert_eq!(
        raw_http_status(
            port,
            b"GET /?abcde=1 HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n"
        )
        .await,
        414
    );
    assert_eq!(
        raw_http_status(
            port,
            b"GET /bad%00path HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n"
        )
        .await,
        400
    );

    let client = reqwest::Client::new();
    let proxied = client
        .get(format!("http://127.0.0.1:{port}/proxy/query?a=1&b=2"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("proxy valid bounded Query");
    assert_eq!(proxied.status(), reqwest::StatusCode::OK);
    assert_eq!(proxied.text().await.expect("read proxied Query"), "a=1&b=2");

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn uri_query_rejection_isolates_http2_stream_and_recovers() {
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let mut config = single_https_config(port, "localhost", "localhost");
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["tlsPolicies"][0]["alpn"] = json!(["h2", "http/1.1"]);
    config["resources"]
        .as_array_mut()
        .expect("HTTPS resources")
        .push(json!({"id": "canonical-b", "type": "respond", "status": 200, "body": "route-b"}));
    config["virtualHosts"][0]["routes"]
        .as_array_mut()
        .expect("HTTPS routes")
        .insert(
            0,
            json!({"id": "canonical-b-route", "match": {"pathType": "exact", "path": "/b"}, "resourceRef": "canonical-b"}),
        );
    config["limits"] = json!({
        "maxUriPathBytes": 64,
        "maxDecodedPathBytes": 64,
        "maxPathSegments": 4,
        "maxQueryStringBytes": 16,
        "maxQueryParameters": 1,
        "maxQueryComponentBytes": 4
    });
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let (mut send, connection_task) = connect_h2(port, &certificate_der).await;

    send = send
        .ready()
        .await
        .expect("HTTP/2 sender ready for canonical path");
    let request = Request::builder()
        .uri("https://localhost/a/%2e%2e/b")
        .body(())
        .expect("build canonical HTTP/2 request");
    let (response, _) = send
        .send_request(request, true)
        .expect("send canonical HTTP/2 request");
    let response = response.await.expect("receive canonical HTTP/2 response");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let mut body = response.into_body();
    let data = body
        .data()
        .await
        .expect("canonical response Data")
        .expect("valid canonical response Data");
    assert_eq!(data, Bytes::from_static(b"route-b"));
    body.flow_control()
        .release_capacity(data.len())
        .expect("release canonical response capacity");

    for (uri, expected) in [
        (
            "https://localhost/a/b/c/d/e",
            reqwest::StatusCode::URI_TOO_LONG,
        ),
        (
            "https://localhost/?a=1&b=2",
            reqwest::StatusCode::URI_TOO_LONG,
        ),
        (
            "https://localhost/bad%00path",
            reqwest::StatusCode::BAD_REQUEST,
        ),
    ] {
        send = send.ready().await.expect("HTTP/2 sender ready");
        let request = Request::builder()
            .uri(uri)
            .body(())
            .expect("build bounded URI request");
        let (response, _) = send
            .send_request(request, true)
            .expect("send bounded URI request");
        assert_eq!(
            response.await.expect("receive URI rejection").status(),
            expected
        );
    }

    send = send.ready().await.expect("HTTP/2 sender recovers");
    let request = Request::builder()
        .uri("https://localhost/ok?q=1")
        .body(())
        .expect("build healthy URI request");
    let (response, _) = send
        .send_request(request, true)
        .expect("send healthy URI request");
    assert_eq!(
        response.await.expect("receive healthy response").status(),
        reqwest::StatusCode::OK
    );

    connection_task.abort();
    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn atomically_reloads_handler_uri_budgets() {
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let path = write_config(
        directory.path(),
        &watched_response_config(port, "wide-uri-budget"),
    );
    let (shutdown, task) = spawn_watched_data_plane(&path);
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/");
    wait_for_body(&client, &url, "test.localhost", "wide-uri-budget").await;

    let mut candidate = watched_response_config(port, "narrow-uri-budget");
    candidate["limits"]["maxUriPathBytes"] = json!(4);
    candidate["limits"]["maxDecodedPathBytes"] = json!(4);
    write_config(directory.path(), &candidate);
    wait_for_body(&client, &url, "test.localhost", "narrow-uri-budget").await;
    let rejected = client
        .get(format!("http://127.0.0.1:{port}/12345"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request after URI budget reload");
    assert_eq!(rejected.status(), reqwest::StatusCode::URI_TOO_LONG);

    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn closes_idle_http1_keep_alive_without_interrupting_active_upload() {
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let mut config = base_config(
        port,
        json!([{
            "id": "response",
            "type": "respond",
            "status": 200,
            "body": "complete"
        }]),
        json!([]),
        json!([{
            "id": "response-route",
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": "response"
        }]),
    );
    config["limits"]["maxConnections"] = json!(1);
    config["listeners"][0]["maxConnections"] = json!(1);
    config["limits"]["http1KeepAliveIdleTimeoutMs"] = json!(200);
    config["limits"]["requestBodyStartTimeoutMs"] = json!(1_000);
    config["limits"]["requestBodyIdleTimeoutMs"] = json!(1_000);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::new();
    let readiness = wait_for_http(
        &client,
        &format!("http://127.0.0.1:{port}/"),
        "test.localhost",
    )
    .await;
    drop(readiness);
    drop(client);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut idle = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect idle HTTP/1 client");
    idle.write_all(
        b"GET / HTTP/1.1\r\nHost: test.localhost\r\n\r\nGET / HTTP/1.1\r\nHost: test.localhost\r\n\r\n",
    )
        .await
        .expect("write pipelined Keep-Alive requests");
    let (_, body) = read_http1_fixed_response(&mut idle).await;
    assert_eq!(body, b"complete");
    let (_, body) = read_http1_fixed_response(&mut idle).await;
    assert_eq!(body, b"complete");
    let mut byte = [0_u8; 1];
    match timeout(Duration::from_secs(2), idle.read(&mut byte)).await {
        Ok(Ok(0)) => {}
        Ok(Err(error))
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::UnexpectedEof
            ) => {}
        result => panic!("idle HTTP/1 connection did not close at its deadline: {result:?}"),
    }

    let mut uploading = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect slow upload client");
    uploading
        .write_all(
            b"POST / HTTP/1.1\r\nHost: test.localhost\r\nContent-Length: 4\r\nConnection: close\r\n\r\na",
        )
        .await
        .expect("write upload prefix");
    tokio::time::sleep(Duration::from_millis(350)).await;
    uploading
        .write_all(b"bcd")
        .await
        .expect("finish active upload after Keep-Alive budget");
    let mut response = Vec::new();
    timeout(Duration::from_secs(2), uploading.read_to_end(&mut response))
        .await
        .expect("active upload receives response")
        .expect("read active upload response");
    assert_eq!(raw_status_code(&response), Some(200));

    let healthy = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("new connection succeeds after idle reaping");
    assert_eq!(healthy.status(), reqwest::StatusCode::OK);
    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn http1_keep_alive_timeout_does_not_interrupt_streaming_response() {
    let (upstream_address, release, upstream_shutdown, upstream_task) =
        spawn_held_response_upstream().await;
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let mut config = held_http_proxy_config(port, upstream_address);
    config["limits"]["http1KeepAliveIdleTimeoutMs"] = json!(200);
    config["limits"]["responseBodyIdleTimeoutMs"] = json!(1_000);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect streaming HTTP/1 client");
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: test.localhost\r\n\r\n")
        .await
        .expect("write streaming request");
    let first = read_until_contains(&mut stream, b"start").await;
    assert!(first.starts_with(b"HTTP/1.1 200"));
    tokio::time::sleep(Duration::from_millis(350)).await;
    release.notify_waiters();
    read_until_contains(&mut stream, b"0\r\n\r\n").await;

    stream
        .write_all(b"GET /ready HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("reuse connection after long response");
    let mut response = Vec::new();
    timeout(Duration::from_secs(2), stream.read_to_end(&mut response))
        .await
        .expect("reused connection completes")
        .expect("read reused connection response");
    assert_eq!(raw_status_code(&response), Some(200));
    assert!(response
        .windows(b"ready".len())
        .any(|part| part == b"ready"));

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn tls_http1_keep_alive_idles_out_but_http2_remains_protocol_scoped() {
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let mut config = single_https_config(port, "localhost", "localhost");
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["tlsPolicies"][0]["alpn"] = json!(["h2", "http/1.1"]);
    config["limits"] = json!({
        "http1KeepAliveIdleTimeoutMs": 200,
        "requestTimeoutMs": 5_000,
        "drainTimeoutMs": 1_000
    });
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let mut http1 = connect_tls(port, &certificate_der, b"http/1.1").await;
    http1
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .expect("write TLS HTTP/1 request");
    let (_, body) = read_http1_fixed_response(&mut http1).await;
    assert_eq!(body, b"unreachable");
    let mut byte = [0_u8; 1];
    let closed = timeout(Duration::from_secs(2), http1.read(&mut byte))
        .await
        .expect("TLS HTTP/1 idle deadline completes");
    assert!(
        matches!(closed, Ok(0) | Err(_)),
        "TLS HTTP/1 Keep-Alive connection must close"
    );

    let (mut send, connection_task) = connect_h2(port, &certificate_der).await;
    send = send.ready().await.expect("HTTP/2 sender ready");
    let request = Request::builder()
        .uri("https://localhost/")
        .body(())
        .expect("build first HTTP/2 request");
    let (response, _) = send
        .send_request(request, true)
        .expect("send first HTTP/2 request");
    let response = response.await.expect("receive first HTTP/2 response");
    let mut body = response.into_body();
    while let Some(data) = body.data().await {
        let data = data.expect("read first HTTP/2 Body");
        body.flow_control()
            .release_capacity(data.len())
            .expect("release first HTTP/2 Body capacity");
    }
    tokio::time::sleep(Duration::from_millis(350)).await;
    send = send
        .ready()
        .await
        .expect("HTTP/2 remains ready beyond HTTP/1 idle deadline");
    let request = Request::builder()
        .uri("https://localhost/")
        .body(())
        .expect("build second HTTP/2 request");
    let (response, _) = send
        .send_request(request, true)
        .expect("send second HTTP/2 request");
    assert_eq!(
        response
            .await
            .expect("receive second HTTP/2 response")
            .status(),
        reqwest::StatusCode::OK
    );

    connection_task.abort();
    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn bounds_tls_http1_pipeline_depth_and_recovers_without_affecting_h2() {
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let mut config = single_https_config(port, "localhost", "localhost");
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["tlsPolicies"][0]["alpn"] = json!(["h2", "http/1.1"]);
    config["limits"] = json!({
        "maxConnections": 1,
        "http1MaxPipelineDepth": 2,
        "requestTimeoutMs": 5_000,
        "drainTimeoutMs": 1_000
    });
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let mut excessive = Vec::with_capacity(6 * 1024);
    for index in 0..64 {
        let connection = if index == 63 { "close" } else { "keep-alive" };
        excessive.extend_from_slice(
            format!("GET / HTTP/1.1\r\nHost: localhost\r\nConnection: {connection}\r\n\r\n")
                .as_bytes(),
        );
    }
    let rejected = raw_tls_http1_response(port, &certificate_der, &excessive).await;
    let responses = rejected
        .windows(b"HTTP/1.1 200".len())
        .filter(|window| *window == b"HTTP/1.1 200")
        .count();
    assert!(
        responses < 64,
        "TLS Pipeline depth must be enforced after decryption"
    );

    let recovered = raw_tls_http1_response(
        port,
        &certificate_der,
        b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert_eq!(raw_status_code(&recovered), Some(200));

    let (mut send, connection_task) = connect_h2(port, &certificate_der).await;
    send = send.ready().await.expect("HTTP/2 sender ready");
    let request = Request::builder()
        .uri("https://localhost/")
        .body(())
        .expect("build HTTP/2 request");
    let (response, _) = send
        .send_request(request, true)
        .expect("send HTTP/2 request");
    assert_eq!(
        response.await.expect("receive HTTP/2 response").status(),
        200
    );

    connection_task.abort();
    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn enforces_http2_ping_ack_timeout_and_keeps_protocol_scope() {
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let mut config = single_https_config(port, "localhost", "localhost");
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["tlsPolicies"][0]["alpn"] = json!(["h2", "http/1.1"]);
    config["limits"] = json!({
        "maxConnections": 1,
        "http1KeepAliveIdleTimeoutMs": 5_000,
        "http2KeepAliveIntervalMs": 1_000,
        "http2KeepAliveTimeoutMs": 300,
        "requestTimeoutMs": 5_000,
        "drainTimeoutMs": 1_000
    });
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let mut http1 = connect_tls(port, &certificate_der, b"http/1.1").await;
    http1
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .expect("write first HTTP/1 request");
    let (_, body) = read_http1_fixed_response(&mut http1).await;
    assert_eq!(body, b"unreachable");
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    http1
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("reuse HTTP/1 beyond H2 Keep-Alive interval");
    let mut second_http1 = Vec::new();
    http1
        .read_to_end(&mut second_http1)
        .await
        .expect("read second HTTP/1 response");
    assert_eq!(raw_status_code(&second_http1), Some(200));

    let mut raw_h2 = connect_tls(port, &certificate_der, b"h2").await;
    raw_h2
        .write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n\0\0\0\x04\0\0\0\0\0")
        .await
        .expect("write HTTP/2 preface and SETTINGS");
    let ping_payload = loop {
        let (frame_type, flags, stream_id, payload) = read_h2_frame(&mut raw_h2).await;
        if frame_type == 0x4 && flags & 0x1 == 0 {
            raw_h2
                .write_all(&encode_h2_frame(0x4, 0x1, 0, &[]))
                .await
                .expect("acknowledge server SETTINGS");
        }
        if frame_type == 0x6 && flags & 0x1 == 0 {
            assert_eq!(stream_id, 0);
            assert_eq!(payload.len(), 8);
            break payload;
        }
    };
    assert_eq!(ping_payload.len(), 8);

    let goaway = loop {
        let (frame_type, _, stream_id, payload) = read_h2_frame(&mut raw_h2).await;
        if frame_type == 0x7 {
            assert_eq!(stream_id, 0);
            break payload;
        }
    };
    assert!(goaway.len() >= 8);
    assert_eq!(&goaway[4..8], &[0, 0, 0, 0], "GOAWAY reason is NO_ERROR");
    let mut after_goaway = Vec::new();
    let close = timeout(
        Duration::from_secs(2),
        raw_h2.read_to_end(&mut after_goaway),
    )
    .await
    .expect("H2 connection closes after Keep-Alive GOAWAY");
    if let Err(error) = close {
        assert!(
            matches!(
                error.kind(),
                std::io::ErrorKind::UnexpectedEof
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::BrokenPipe
            ),
            "unexpected post-GOAWAY read error: {error}"
        );
    }
    drop(raw_h2);

    let (mut send, connection_task) = connect_h2(port, &certificate_der).await;
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    send = send
        .ready()
        .await
        .expect("PING ACK keeps recovered H2 connection ready");
    let request = Request::builder()
        .uri("https://localhost/")
        .body(())
        .expect("build recovered H2 request");
    let (response, _) = send
        .send_request(request, true)
        .expect("send recovered H2 request");
    assert_eq!(
        response
            .await
            .expect("receive recovered H2 response")
            .status(),
        200
    );

    connection_task.abort();
    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn request_body_progress_timeouts_close_http1_and_allow_fresh_connections() {
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let mut config = base_config(
        port,
        json!([{
            "id": "response",
            "type": "respond",
            "status": 200,
            "body": "complete"
        }]),
        json!([]),
        json!([{
            "id": "response-route",
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": "response"
        }]),
    );
    config["limits"]["maxConcurrentRequests"] = json!(1);
    config["limits"]["requestBodyStartTimeoutMs"] = json!(250);
    config["limits"]["requestBodyIdleTimeoutMs"] = json!(250);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::new();
    wait_for_http(
        &client,
        &format!("http://127.0.0.1:{port}/"),
        "test.localhost",
    )
    .await;

    let mut missing = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect missing-Body client");
    missing
        .write_all(b"POST / HTTP/1.1\r\nHost: test.localhost\r\nContent-Length: 4\r\n\r\n")
        .await
        .expect("write missing-Body headers");
    let mut response = Vec::new();
    timeout(Duration::from_secs(2), missing.read_to_end(&mut response))
        .await
        .expect("request Body start timeout closes HTTP/1")
        .expect("read request Body start timeout response");
    assert_eq!(raw_status_code(&response), Some(408));
    assert!(
        String::from_utf8_lossy(&response)
            .to_ascii_lowercase()
            .contains("connection: close"),
        "HTTP/1 request Body timeout forces connection close"
    );

    let mut stalled = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect stalled-Body client");
    stalled
        .write_all(b"POST / HTTP/1.1\r\nHost: test.localhost\r\nContent-Length: 4\r\n\r\na")
        .await
        .expect("write first request Body byte");
    tokio::time::sleep(Duration::from_millis(350)).await;
    let mut response = Vec::new();
    timeout(Duration::from_secs(2), stalled.read_to_end(&mut response))
        .await
        .expect("request Body idle timeout closes HTTP/1")
        .expect("read request Body idle timeout response");
    assert_eq!(raw_status_code(&response), Some(408));

    let mut progressing = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect progressing-Body client");
    progressing
        .write_all(b"POST / HTTP/1.1\r\nHost: test.localhost\r\nContent-Length: 4\r\nConnection: close\r\n\r\na")
        .await
        .expect("write progressing request headers and first byte");
    for byte in *b"bcd" {
        tokio::time::sleep(Duration::from_millis(100)).await;
        progressing
            .write_all(&[byte])
            .await
            .expect("write progressing request Body byte");
    }
    let mut response = Vec::new();
    timeout(
        Duration::from_secs(2),
        progressing.read_to_end(&mut response),
    )
    .await
    .expect("progressing request completes")
    .expect("read progressing request response");
    assert_eq!(raw_status_code(&response), Some(200));

    let healthy = client
        .get(format!("http://127.0.0.1:{port}/"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("fresh connection succeeds after request Body timeouts");
    assert_eq!(healthy.status(), reqwest::StatusCode::OK);
    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn proxy_request_body_timeout_maps_to_408_instead_of_upstream_failure() {
    let (upstream_address, upstream_shutdown, upstream_task) = spawn_body_sink_upstream().await;
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let mut config = base_config(
        port,
        json!([{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "origin",
            "stripPrefix": false
        }]),
        json!([{
            "id": "origin",
            "targets": [{"url": format!("http://{upstream_address}")}],
            "connectTimeoutMs": 1000,
            "requestTimeoutMs": 5000,
            "maxIdleConnections": 2
        }]),
        json!([{
            "id": "proxy-route",
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": "proxy"
        }]),
    );
    config["limits"]["requestBodyStartTimeoutMs"] = json!(200);
    config["limits"]["requestBodyIdleTimeoutMs"] = json!(200);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect proxy timeout client");
    stream
        .write_all(b"POST / HTTP/1.1\r\nHost: test.localhost\r\nContent-Length: 4\r\n\r\n")
        .await
        .expect("write incomplete proxy request");
    let mut response = Vec::new();
    timeout(Duration::from_secs(2), stream.read_to_end(&mut response))
        .await
        .expect("proxy request Body timeout closes HTTP/1")
        .expect("read proxy timeout response");
    assert_eq!(raw_status_code(&response), Some(408));
    assert!(!response.starts_with(b"HTTP/1.1 400"));
    assert!(!response.starts_with(b"HTTP/1.1 502"));
    assert!(!response.starts_with(b"HTTP/1.1 504"));

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn forwards_http1_early_responses_closes_uploads_and_avoids_upstream_pool_reuse() {
    let statuses = vec![302, 401, 413, 417, 500];
    let (upstream_address, accepted, upstream_shutdown, upstream_task) =
        spawn_early_response_upstream(statuses.clone()).await;
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let config = early_response_http_proxy_config(port, upstream_address);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::new();
    wait_for_http(
        &client,
        &format!("http://127.0.0.1:{port}/ready"),
        "test.localhost",
    )
    .await;

    for (index, status) in statuses.into_iter().enumerate() {
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect early-response HTTP/1 client");
        let request = format!(
            "POST /early/{status} HTTP/1.1\r\nHost: test.localhost\r\nContent-Length: 100000\r\n\r\nseed"
        );
        stream
            .write_all(request.as_bytes())
            .await
            .expect("start incomplete HTTP/1 upload");

        let (headers, body) = read_http1_fixed_response(&mut stream).await;
        assert_eq!(raw_status_code(&headers), Some(status));
        assert!(
            std::str::from_utf8(&headers)
                .expect("HTTP/1 response headers are UTF-8")
                .lines()
                .any(|line| line.eq_ignore_ascii_case("connection: close")),
            "early HTTP/1 response declares connection close"
        );
        assert_eq!(body, format!("early-{status}").as_bytes());
        assert_http1_connection_ends(&mut stream).await;
        wait_for_accepted_connections(&accepted, index + 1).await;
    }

    tokio::time::sleep(Duration::from_millis(700)).await;
    let recovered = client
        .get(format!("http://127.0.0.1:{port}/ready"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request after early-response upload cancellation");
    assert_eq!(recovered.status(), reqwest::StatusCode::OK);
    assert_eq!(recovered.text().await.expect("read recovery Body"), "ready");
    assert_eq!(accepted.load(Ordering::Acquire), 5);

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn closes_tls_http1_after_an_upstream_early_response() {
    let (upstream_address, accepted, upstream_shutdown, upstream_task) =
        spawn_early_response_upstream(vec![401]).await;
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let config = early_response_https_proxy_config(port, upstream_address);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let readiness_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build early-response TLS readiness client");
    wait_for_http(
        &readiness_client,
        &format!("https://localhost:{port}/ready"),
        "localhost",
    )
    .await;

    let mut tls = connect_tls(port, &certificate_der, b"http/1.1").await;
    tls.write_all(b"POST /early HTTP/1.1\r\nHost: localhost\r\nContent-Length: 100000\r\n\r\nseed")
        .await
        .expect("start incomplete TLS HTTP/1 upload");
    let (headers, body) = read_http1_fixed_response(&mut tls).await;
    assert_eq!(raw_status_code(&headers), Some(401));
    assert!(std::str::from_utf8(&headers)
        .expect("TLS HTTP/1 headers are UTF-8")
        .lines()
        .any(|line| line.eq_ignore_ascii_case("connection: close")));
    assert_eq!(body, b"early-401");
    assert_http1_connection_ends(&mut tls).await;
    wait_for_accepted_connections(&accepted, 1).await;

    tokio::time::sleep(Duration::from_millis(700)).await;
    let recovered = readiness_client
        .get(format!("https://localhost:{port}/ready"))
        .header("host", "localhost")
        .send()
        .await
        .expect("request after TLS HTTP/1 early response");
    assert_eq!(recovered.status(), reqwest::StatusCode::OK);

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn cancels_an_early_response_http2_upload_and_reuses_the_connection() {
    let (upstream_address, accepted, upstream_shutdown, upstream_task) =
        spawn_early_response_upstream(vec![401]).await;
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let config = early_response_https_proxy_config(port, upstream_address);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let readiness_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build early-response H2 readiness client");
    wait_for_http(
        &readiness_client,
        &format!("https://localhost:{port}/ready"),
        "localhost",
    )
    .await;

    let (mut send, connection_task) = connect_h2(port, &certificate_der).await;
    send = send.ready().await.expect("HTTP/2 sender ready");
    let request = Request::builder()
        .method("POST")
        .uri("https://localhost/early")
        .header("content-length", "100000")
        .body(())
        .expect("build early-response H2 request");
    let (response, mut request_body) = send
        .send_request(request, false)
        .expect("send early-response H2 headers");
    request_body
        .send_data(Bytes::from_static(b"seed"), false)
        .expect("start incomplete H2 upload");

    let response = timeout(Duration::from_secs(2), response)
        .await
        .expect("upstream early response arrives before request timeout")
        .expect("receive early H2 response");
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    assert!(!response.headers().contains_key("connection"));
    let response_body = response.into_body();

    let reset = timeout(
        Duration::from_secs(2),
        futures_util::future::poll_fn(|context| request_body.poll_reset(context)),
    )
    .await
    .expect("early H2 response resets the incomplete upload")
    .expect("receive H2 reset reason");
    assert_eq!(reset, h2::Reason::NO_ERROR);
    drop(response_body);
    wait_for_accepted_connections(&accepted, 1).await;

    tokio::time::sleep(Duration::from_millis(700)).await;
    send = send
        .ready()
        .await
        .expect("H2 connection remains usable after early response");
    let recovered_request = Request::builder()
        .uri("https://localhost/ready")
        .body(())
        .expect("build recovered H2 request");
    let (recovered, _) = send
        .send_request(recovered_request, true)
        .expect("send recovered H2 request");
    assert_eq!(
        recovered
            .await
            .expect("receive recovered H2 response")
            .status(),
        reqwest::StatusCode::OK
    );

    connection_task.abort();
    let _ = connection_task.await;
    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn preserves_an_early_response_body_for_a_hyper_http2_client() {
    let (upstream_address, accepted, upstream_shutdown, upstream_task) =
        spawn_early_response_upstream(vec![401]).await;
    let directory = TempDir::new().expect("create temp directory");
    write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let config = early_response_https_proxy_config(port, upstream_address);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .http2_prior_knowledge()
        .build()
        .expect("build Hyper HTTP/2 early-response client");

    let upload = futures_util::stream::once(async {
        Ok::<Bytes, std::io::Error>(Bytes::from_static(b"seed"))
    })
    .chain(futures_util::stream::pending());
    let response = timeout(
        Duration::from_secs(2),
        client
            .post(format!("https://localhost:{port}/early"))
            .body(reqwest::Body::wrap_stream(upload))
            .send(),
    )
    .await
    .expect("Hyper H2 client receives early response headers")
    .expect("send Hyper H2 slow upload");
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.bytes().await.expect("read Hyper H2 response Body"),
        Bytes::from_static(b"early-401")
    );
    wait_for_accepted_connections(&accepted, 1).await;

    stop_data_plane(shutdown, task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn request_body_timeout_isolates_http2_stream_and_preserves_connection() {
    let directory = TempDir::new().expect("create temp directory");
    let certificate_der =
        write_self_signed_certificate(directory.path(), "localhost", &["localhost"]);
    let port = available_port();
    let mut config = single_https_config(port, "localhost", "localhost");
    config["listeners"][0]["protocols"] = json!(["http1", "http2"]);
    config["tlsPolicies"][0]["alpn"] = json!(["h2", "http/1.1"]);
    config["limits"] = json!({
        "maxConcurrentRequests": 1,
        "requestBodyStartTimeoutMs": 200,
        "requestBodyIdleTimeoutMs": 200,
        "requestTimeoutMs": 5000,
        "drainTimeoutMs": 1000,
        "maxConnections": 128
    });
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let (mut send, connection_task) = connect_h2(port, &certificate_der).await;

    send = send.ready().await.expect("HTTP/2 sender ready");
    let request = Request::builder()
        .method("POST")
        .uri("https://localhost/")
        .body(())
        .expect("build stalled HTTP/2 request");
    let (response, _request_body) = send
        .send_request(request, false)
        .expect("send stalled HTTP/2 request headers");
    let response = timeout(Duration::from_secs(2), response)
        .await
        .expect("HTTP/2 request Body timeout returns response")
        .expect("receive HTTP/2 timeout response");
    assert_eq!(response.status(), reqwest::StatusCode::REQUEST_TIMEOUT);
    assert!(!response.headers().contains_key("connection"));
    let mut timeout_body = response.into_body();
    while let Some(data) = timeout_body.data().await {
        match data {
            Ok(data) => timeout_body
                .flow_control()
                .release_capacity(data.len())
                .expect("release HTTP/2 timeout Body capacity"),
            Err(_) => break,
        }
    }

    send = send
        .ready()
        .await
        .expect("HTTP/2 connection remains ready after Stream timeout");
    let healthy = Request::builder()
        .uri("https://localhost/")
        .body(())
        .expect("build healthy HTTP/2 request");
    let (healthy, _) = send
        .send_request(healthy, true)
        .expect("send healthy HTTP/2 request");
    assert_eq!(
        healthy
            .await
            .expect("receive healthy HTTP/2 response")
            .status(),
        reqwest::StatusCode::OK
    );

    connection_task.abort();
    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn rejects_oversized_tls_material_before_listener_start() {
    let directory = TempDir::new().expect("create temp directory");
    fs::write(
        directory.path().join("oversized-cert.pem"),
        vec![b'x'; 1024 * 1024 + 1],
    )
    .expect("write oversized certificate");
    fs::write(directory.path().join("key.pem"), b"not-a-private-key")
        .expect("write placeholder key");
    let port = available_port();
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-oversized-tls-test",
        "listeners": [{
            "id": "https",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1"],
            "tlsPolicyRef": "tls",
            "defaultVirtualHostRef": "https-host"
        }],
        "certificates": [{
            "id": "cert",
            "serverNames": ["localhost"],
            "source": {
                "type": "protected-file",
                "certificateFile": "oversized-cert.pem",
                "privateKeyFile": "key.pem"
            }
        }],
        "tlsPolicies": [{
            "id": "tls",
            "certificateRef": "cert",
            "minimumVersion": "tls1.2",
            "maximumVersion": "tls1.3",
            "alpn": ["http/1.1"]
        }],
        "resources": [{
            "id": "response",
            "type": "respond",
            "status": 200,
            "body": "unreachable"
        }],
        "virtualHosts": [{
            "id": "https-host",
            "listenerRefs": ["https"],
            "serverNames": ["localhost"],
            "routes": [{
                "id": "response-route",
                "match": {"pathType": "prefix", "path": "/"},
                "resourceRef": "response"
            }]
        }]
    });
    let path = write_config(directory.path(), &config);
    let compiled = load_and_compile_webserver_config(path).expect("compile TLS references");
    let error = run_data_plane_until(compiled, std::future::pending())
        .await
        .expect_err("oversized TLS material must fail before serving");
    assert!(matches!(
        error,
        sdkwork_api_webserver_standalone_gateway::DataPlaneError::TlsMaterialTooLarge { .. }
    ));
}

#[tokio::test]
async fn rejects_connections_over_limit_without_blocking_shutdown() {
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let mut config = base_config(
        port,
        json!([{
            "id": "response",
            "type": "respond",
            "status": 200,
            "body": "ok"
        }]),
        json!([]),
        json!([{
            "id": "response-route",
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": "response"
        }]),
    );
    config["limits"]["maxConnections"] = json!(1);
    config["listeners"][0]["maxConnections"] = json!(1);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let mut first = loop {
        match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
            Ok(stream) => break stream,
            Err(_) => tokio::time::sleep(Duration::from_millis(25)).await,
        }
    };
    first
        .write_all(b"GET / HTTP/1.1\r\nHost: test.localhost\r\n")
        .await
        .expect("hold first connection open");
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut second = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("TCP backlog accepts the overload connection");
    second
        .write_all(b"GET / HTTP/1.1\r\nHost: test.localhost\r\n\r\n")
        .await
        .expect("write overload request");
    let mut buffer = [0u8; 1];
    let result = timeout(Duration::from_secs(1), second.read(&mut buffer))
        .await
        .expect("overload connection is closed promptly");
    assert!(matches!(result, Ok(0) | Err(_)));

    stop_data_plane(shutdown, task).await;
    let _ = first.shutdown().await;
}

#[tokio::test]
async fn enforces_http1_header_count_bytes_timeout_and_strict_framing() {
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let mut config = base_config(
        port,
        json!([{
            "id": "response",
            "type": "respond",
            "status": 200,
            "body": "ok"
        }]),
        json!([]),
        json!([{
            "id": "response-route",
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": "response"
        }]),
    );
    config["limits"]["maxRequestHeaderBytes"] = json!(8_192);
    config["limits"]["maxRequestLineBytes"] = json!(64);
    config["limits"]["maxRequestMethodBytes"] = json!(8);
    config["limits"]["maxRequestTargetBytes"] = json!(32);
    config["limits"]["maxHeaderNameBytes"] = json!(32);
    config["limits"]["maxHeaderValueBytes"] = json!(32);
    config["limits"]["maxRequestHeaders"] = json!(8);
    config["limits"]["requestHeaderTimeoutMs"] = json!(200);
    config["limits"]["maxRequestBodyBytes"] = json!(4);
    config["limits"]["maxChunkLineBytes"] = json!(16);
    config["limits"]["maxTrailerBytes"] = json!(32);
    config["limits"]["maxTrailers"] = json!(1);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    let client = reqwest::Client::new();
    wait_for_body(
        &client,
        &format!("http://127.0.0.1:{port}/"),
        "test.localhost",
        "ok",
    )
    .await;

    let mut too_many_headers =
        String::from("GET / HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n");
    for index in 0..8 {
        too_many_headers.push_str(&format!("X-Test-{index}: value\r\n"));
    }
    too_many_headers.push_str("\r\n");
    assert_raw_request_rejected(port, too_many_headers.as_bytes()).await;

    let oversized_value = "x".repeat(9_000);
    let oversized_request = format!(
        "GET / HTTP/1.1\r\nHost: test.localhost\r\nX-Large: {oversized_value}\r\nConnection: close\r\n\r\n"
    );
    assert_raw_request_rejected(port, oversized_request.as_bytes()).await;

    assert_raw_request_rejected(
        port,
        b"LONGMETHOD / HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert_raw_request_rejected(
        port,
        b"GET /12345678901234567890123456789012 HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert_raw_request_rejected(
        port,
        b"GET / HTTP/1.1\r\nX-Header-Name-That-Is-Over-Thirty-Two: x\r\nConnection: close\r\n\r\n",
    )
    .await;
    let oversized_field_value = "x".repeat(33);
    let oversized_field_request = format!(
        "GET / HTTP/1.1\r\nHost: test.localhost\r\nX-Large: {oversized_field_value}\r\nConnection: close\r\n\r\n"
    );
    assert_raw_request_rejected(port, oversized_field_request.as_bytes()).await;

    assert_raw_request_rejected(
        port,
        b"POST / HTTP/1.1\r\nHost: test.localhost\r\nContent-Length: 4\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
    )
    .await;
    assert_raw_request_rejected(
        port,
        b"POST / HTTP/1.1\r\nHost: test.localhost\r\nContent-Length: 4\r\nContent-Length: 4\r\nConnection: close\r\n\r\ntest",
    )
    .await;
    assert_raw_request_rejected(
        port,
        b"POST / HTTP/1.1\r\nHost: test.localhost\r\nTransfer-Encoding: chunked\r\nContent-Length: 4\r\nConnection: close\r\n\r\n0\r\n\r\n",
    )
    .await;
    assert_eq!(
        raw_http_status(
            port,
            b"POST / HTTP/1.1\r\nHost: test.localhost\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n0\r\n\r\n",
        )
        .await,
        200
    );
    assert_eq!(
        raw_http_status(
            port,
            b"GET / HTTP/1.1\r\nHost: test.localhost\r\nTE: gzip\r\nConnection: close\r\n\r\n",
        )
        .await,
        400
    );
    assert_eq!(
        raw_http_status(
            port,
            b"POST / HTTP/1.1\r\nHost: test.localhost\r\nTransfer-Encoding: chunked\r\nTrailer: X-Checksum\r\nConnection: close\r\n\r\n4\r\ntest\r\n0\r\nX-Checksum: ok\r\n\r\n",
        )
        .await,
        200
    );
    assert_eq!(
        raw_http_status(
            port,
            b"POST / HTTP/1.1\r\nHost: test.localhost\r\nContent-Length: 5\r\nConnection: close\r\n\r\nlarge",
        )
        .await,
        413
    );
    assert_raw_request_rejected(
        port,
        b"POST / HTTP/1.1\r\nHost: test.localhost\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nlarge\r\n0\r\n\r\n",
    )
    .await;
    assert_raw_request_rejected(
        port,
        b"POST / HTTP/1.1\r\nHost: test.localhost\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n1\r\nx\r\n0\r\nX-One: 1\r\nX-Two: 2\r\n\r\n",
    )
    .await;
    assert_raw_request_rejected(
        port,
        b"POST / HTTP/1.1\r\nHost: test.localhost\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n1;extension-is-too-long\r\nx\r\n0\r\n\r\n",
    )
    .await;
    assert_raw_request_rejected(
        port,
        b"GET / HTTP/1.1\r\nHost: test.localhost\r\nX-Folded: first\r\n second\r\nConnection: close\r\n\r\n",
    )
    .await;

    let pipelined = raw_http_response(
        port,
        b"GET / HTTP/1.1\r\nHost: test.localhost\r\n\r\nPOST / HTTP/1.1\r\nHost: test.localhost\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n4\r\ntest\r\n0\r\n\r\n",
    )
    .await;
    assert_eq!(
        pipelined
            .windows(b"HTTP/1.1 200".len())
            .filter(|window| *window == b"HTTP/1.1 200")
            .count(),
        2,
        "wire guard resets framing state for a pipelined request"
    );

    let mut slow = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect slow HTTP client");
    slow.write_all(b"GET / HTTP/1.1\r\nHost: test.localhost\r\nX-Slow:")
        .await
        .expect("write incomplete headers");
    tokio::time::sleep(Duration::from_millis(350)).await;
    let mut response = Vec::new();
    timeout(Duration::from_secs(1), slow.read_to_end(&mut response))
        .await
        .expect("slow header connection closes after deadline")
        .expect("read slow header close");
    assert!(
        response.is_empty() || !response.starts_with(b"HTTP/1.1 200"),
        "slow header request must never reach the application"
    );

    let healthy = client
        .get(format!("http://127.0.0.1:{port}/"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("server remains healthy after rejected protocol inputs");
    assert_eq!(healthy.status(), reqwest::StatusCode::OK);
    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn atomically_reloads_valid_config_and_retains_generation_on_failure() {
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let path = write_config(
        directory.path(),
        &watched_response_config(port, "generation-one"),
    );
    let (shutdown, task) = spawn_watched_data_plane(&path);
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/");
    wait_for_body(&client, &url, "test.localhost", "generation-one").await;

    write_config(
        directory.path(),
        &watched_response_config(port, "generation-two"),
    );
    wait_for_body(&client, &url, "test.localhost", "generation-two").await;

    fs::write(&path, b"{").expect("write invalid candidate");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let retained = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request retained generation");
    assert_eq!(
        retained.text().await.expect("read retained response"),
        "generation-two"
    );

    let restart_port = available_port();
    fs::write(
        &path,
        serde_json::to_vec_pretty(&watched_response_config(
            restart_port,
            "restart-only-candidate",
        ))
        .expect("serialize restart-only candidate"),
    )
    .expect("write restart-only candidate");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let retained = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request generation after topology rejection");
    assert_eq!(
        retained.text().await.expect("read retained response"),
        "generation-two"
    );

    let mut protocol_limit_candidate = watched_response_config(port, "protocol-limit-candidate");
    protocol_limit_candidate["limits"]["requestHeaderTimeoutMs"] = json!(500);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&protocol_limit_candidate)
            .expect("serialize protocol-limit candidate"),
    )
    .expect("write protocol-limit candidate");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let retained = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request generation after protocol-limit topology rejection");
    assert_eq!(
        retained.text().await.expect("read retained response"),
        "generation-two"
    );

    let mut request_field_candidate = watched_response_config(port, "request-field-candidate");
    request_field_candidate["limits"]["maxRequestTargetBytes"] = json!(4_096);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&request_field_candidate)
            .expect("serialize request-field candidate"),
    )
    .expect("write request-field candidate");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let retained = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request generation after request-field topology rejection");
    assert_eq!(
        retained.text().await.expect("read retained response"),
        "generation-two"
    );

    let mut http2_abuse_candidate = watched_response_config(port, "http2-abuse-candidate");
    http2_abuse_candidate["limits"]["http2MaxFramesPerWindow"] = json!(9_999);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&http2_abuse_candidate)
            .expect("serialize HTTP/2 abuse-limit candidate"),
    )
    .expect("write HTTP/2 abuse-limit candidate");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let retained = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request generation after HTTP/2 abuse-limit topology rejection");
    assert_eq!(
        retained.text().await.expect("read retained response"),
        "generation-two"
    );

    let mut request_admission_candidate =
        watched_response_config(port, "request-admission-candidate");
    request_admission_candidate["limits"]["maxConcurrentRequests"] = json!(2_048);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&request_admission_candidate)
            .expect("serialize request-admission candidate"),
    )
    .expect("write request-admission candidate");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let retained = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request generation after request-admission topology rejection");
    assert_eq!(
        retained.text().await.expect("read retained response"),
        "generation-two"
    );

    let mut keep_alive_candidate = watched_response_config(port, "keep-alive-candidate");
    keep_alive_candidate["limits"]["http1KeepAliveIdleTimeoutMs"] = json!(60_000);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&keep_alive_candidate)
            .expect("serialize HTTP/1 Keep-Alive candidate"),
    )
    .expect("write HTTP/1 Keep-Alive candidate");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let retained = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request generation after Keep-Alive topology rejection");
    assert_eq!(
        retained.text().await.expect("read retained response"),
        "generation-two"
    );

    let mut h2_keep_alive_interval_candidate =
        watched_response_config(port, "h2-keep-alive-interval-candidate");
    h2_keep_alive_interval_candidate["limits"]["http2KeepAliveIntervalMs"] = json!(50_000);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&h2_keep_alive_interval_candidate)
            .expect("serialize H2 Keep-Alive interval candidate"),
    )
    .expect("write H2 Keep-Alive interval candidate");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let retained = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request generation after H2 Keep-Alive interval rejection");
    assert_eq!(
        retained.text().await.expect("read retained response"),
        "generation-two"
    );

    let mut maximum_age_candidate = watched_response_config(port, "maximum-age-candidate");
    maximum_age_candidate["limits"]["maxConnectionAgeMs"] = json!(3_000_000);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&maximum_age_candidate)
            .expect("serialize connection maximum-age candidate"),
    )
    .expect("write connection maximum-age candidate");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let retained = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request generation after maximum-age topology rejection");
    assert_eq!(
        retained.text().await.expect("read retained response"),
        "generation-two"
    );

    let mut h2_keep_alive_timeout_candidate =
        watched_response_config(port, "h2-keep-alive-timeout-candidate");
    h2_keep_alive_timeout_candidate["limits"]["http2KeepAliveTimeoutMs"] = json!(15_000);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&h2_keep_alive_timeout_candidate)
            .expect("serialize H2 Keep-Alive timeout candidate"),
    )
    .expect("write H2 Keep-Alive timeout candidate");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let retained = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request generation after H2 Keep-Alive timeout rejection");
    assert_eq!(
        retained.text().await.expect("read retained response"),
        "generation-two"
    );

    let mut pipeline_candidate = watched_response_config(port, "pipeline-candidate");
    pipeline_candidate["limits"]["http1MaxPipelineDepth"] = json!(8);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&pipeline_candidate)
            .expect("serialize HTTP/1 Pipeline candidate"),
    )
    .expect("write HTTP/1 Pipeline candidate");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let retained = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request generation after Pipeline topology rejection");
    assert_eq!(
        retained.text().await.expect("read retained response"),
        "generation-two"
    );

    let mut request_body_start_candidate =
        watched_response_config(port, "request-body-start-candidate");
    request_body_start_candidate["limits"]["requestBodyStartTimeoutMs"] = json!(25_000);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&request_body_start_candidate)
            .expect("serialize request-Body-start candidate"),
    )
    .expect("write request-Body-start candidate");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let retained = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request generation after request-Body-start topology rejection");
    assert_eq!(
        retained.text().await.expect("read retained response"),
        "generation-two"
    );

    let mut request_body_idle_candidate =
        watched_response_config(port, "request-body-idle-candidate");
    request_body_idle_candidate["limits"]["requestBodyIdleTimeoutMs"] = json!(25_000);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&request_body_idle_candidate)
            .expect("serialize request-Body-idle candidate"),
    )
    .expect("write request-Body-idle candidate");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let retained = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request generation after request-Body-idle topology rejection");
    assert_eq!(
        retained.text().await.expect("read retained response"),
        "generation-two"
    );

    let mut response_idle_candidate = watched_response_config(port, "response-idle-candidate");
    response_idle_candidate["limits"]["responseBodyIdleTimeoutMs"] = json!(25_000);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&response_idle_candidate)
            .expect("serialize response-idle candidate"),
    )
    .expect("write response-idle candidate");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let retained = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request generation after response-idle topology rejection");
    assert_eq!(
        retained.text().await.expect("read retained response"),
        "generation-two"
    );

    let mut write_timeout_candidate = watched_response_config(port, "write-timeout-candidate");
    write_timeout_candidate["limits"]["connectionWriteTimeoutMs"] = json!(25_000);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&write_timeout_candidate)
            .expect("serialize write-timeout candidate"),
    )
    .expect("write write-timeout candidate");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let retained = client
        .get(&url)
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request generation after write-timeout topology rejection");
    assert_eq!(
        retained.text().await.expect("read retained response"),
        "generation-two"
    );

    let mut body_limit_generation = watched_response_config(port, "body-limit-generation");
    body_limit_generation["limits"]["maxRequestBodyBytes"] = json!(1);
    write_config(directory.path(), &body_limit_generation);
    wait_for_body(&client, &url, "test.localhost", "body-limit-generation").await;
    let body_rejected = client
        .post(&url)
        .header("host", "test.localhost")
        .body("xx")
        .send()
        .await
        .expect("request reloaded body limit");
    assert_eq!(
        body_rejected.status(),
        reqwest::StatusCode::PAYLOAD_TOO_LARGE
    );

    write_config(
        directory.path(),
        &watched_response_config(port, "generation-three"),
    );
    wait_for_body(&client, &url, "test.localhost", "generation-three").await;
    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn concurrent_requests_observe_only_complete_reload_generations() {
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let path = write_config(
        directory.path(),
        &watched_response_config(port, "generation-a"),
    );
    let (shutdown, task) = spawn_watched_data_plane(&path);
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/");
    wait_for_body(&client, &url, "test.localhost", "generation-a").await;

    let writer_path = path.clone();
    let writer = tokio::spawn(async move {
        for index in 0..12 {
            let body = if index % 2 == 0 {
                "generation-b"
            } else {
                "generation-a"
            };
            let bytes = serde_json::to_vec_pretty(&watched_response_config(port, body))
                .expect("serialize reload generation");
            tokio::fs::write(&writer_path, bytes)
                .await
                .expect("write reload generation");
            tokio::time::sleep(Duration::from_millis(120)).await;
        }
    });

    let mut readers = Vec::new();
    for _ in 0..8 {
        let client = client.clone();
        let url = url.clone();
        readers.push(tokio::spawn(async move {
            for _ in 0..60 {
                let response = client
                    .get(&url)
                    .header("host", "test.localhost")
                    .send()
                    .await
                    .expect("request during reload churn");
                assert_eq!(response.status(), reqwest::StatusCode::OK);
                let body = response.text().await.expect("read reload response");
                assert!(matches!(body.as_str(), "generation-a" | "generation-b"));
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }));
    }

    writer.await.expect("reload writer joins");
    for reader in readers {
        reader.await.expect("reload reader joins");
    }
    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn acme_http01_challenge_is_served_with_narrow_precedence() {
    let directory = TempDir::new().expect("create temp directory");
    let challenge_dir = directory
        .path()
        .join("acme-webroot")
        .join(".well-known")
        .join("acme-challenge");
    fs::create_dir_all(&challenge_dir).expect("create challenge directory");
    fs::write(challenge_dir.join("valid-token"), "valid-token.thumbprint")
        .expect("write challenge token");

    let port = available_port();
    let mut config = base_config(
        port,
        json!([{
            "id": "catch-all",
            "type": "respond",
            "status": 200,
            "body": "ordinary"
        }]),
        json!([]),
        json!([{
            "id": "catch-all-route",
            "match": {"pathType": "prefix", "path": "/"},
            "resourceRef": "catch-all"
        }]),
    );
    config["listeners"][0]["acmeHttp01"] = json!({"webroot": "acme-webroot"});
    let path = write_config(directory.path(), &config);
    let (shutdown_tx, task) = spawn_data_plane(&path);
    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    wait_for_http(&client, &base, "test.localhost").await;

    let response = client
        .get(format!("{base}/.well-known/acme-challenge/valid-token"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("challenge request");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(
        response.text().await.expect("challenge body"),
        "valid-token.thumbprint"
    );

    let head = client
        .head(format!("{base}/.well-known/acme-challenge/valid-token"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("challenge HEAD");
    assert_eq!(head.status(), reqwest::StatusCode::OK);
    assert_eq!(head.text().await.expect("HEAD body").len(), 0);

    let missing = client
        .get(format!("{base}/.well-known/acme-challenge/unknown-token"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("missing challenge request");
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);

    let invalid = client
        .get(format!("{base}/.well-known/acme-challenge/dot.token"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("invalid token request");
    assert_eq!(invalid.status(), reqwest::StatusCode::NOT_FOUND);

    let post = client
        .post(format!("{base}/.well-known/acme-challenge/valid-token"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("challenge POST");
    assert_eq!(post.status(), reqwest::StatusCode::METHOD_NOT_ALLOWED);

    let ordinary = client
        .get(format!("{base}/index.html"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("ordinary request");
    assert_eq!(ordinary.status(), reqwest::StatusCode::OK);
    assert_eq!(ordinary.text().await.expect("ordinary body"), "ordinary");

    stop_data_plane(shutdown_tx, task).await;
}
