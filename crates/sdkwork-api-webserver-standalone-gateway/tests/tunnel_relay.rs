//! Data-plane tunnel relay integration test (PRD §92, §134).
//!
//! A JSON app config with `[tunnel]` enabled runs the data plane plus the
//! QUIC tunnel gateway; a real agent registers a route; a raw HTTP request
//! addressed to the tunnel domain over the data-plane listener is relayed
//! to the agent's local target.

use std::time::Duration;

use sdkwork_api_webserver_standalone_gateway::{run_data_plane_until, DataPlaneError};
use sdkwork_webserver_core::load_and_compile_webserver_config_json;
use sdkwork_webserver_tunnel::agent::{AgentEvent, AgentRuntime, AgentRuntimeOptions};
use sdkwork_webserver_tunnel::metrics::TunnelMetrics;
use sdkwork_webserver_tunnel_core::{
    Device, DeviceId, DevicePlatform, RoutePolicy, TunnelConfig, TunnelProtocolKind,
    TunnelRouteTemplate,
};
use sdkwork_webserver_tunnel_transport::tls;
use sdkwork_webserver_tunnel_transport::RemoteEndpoint;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

type DataPlaneTask = tokio::task::JoinHandle<Result<(), DataPlaneError>>;

fn available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve port");
    listener.local_addr().expect("local addr").port()
}

/// Serializes the tests in this file.
///
/// Each one configures the tunnel gateway's TLS material through process-wide
/// environment variables (`SDKWORK_TUNNEL_TEST_TLS_CERT` / `..._KEY`), and
/// cargo runs the tests of one binary in parallel. Without this guard one
/// test's certificate lands under another test's gateway, and the agent then
/// rejects the handshake as `invalid peer certificate: BadSignature` — a
/// failure that looks like a TLS defect and is really a test-isolation defect.
/// Per-test token variables (one name per test) already avoid the same race
/// for credentials; the certificate pair has no per-test name, so it is
/// serialized here instead.
async fn tls_env_serial() -> tokio::sync::MutexGuard<'static, ()> {
    static GUARD: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    GUARD
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

#[tokio::test]
async fn data_plane_relays_tunnel_domain_requests() {
    let _serial = tls_env_serial().await;
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("debug"))
        .with_test_writer()
        .try_init();
    std::env::set_var("SDKWORK_TUNNEL_GATEWAY_TOKEN", "dp-t0ken");

    // Local HTTP target the agent will expose.
    let target = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("target bind");
    let target_port = target.local_addr().expect("target addr").port();
    let target_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = target.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buffer = [0_u8; 2048];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                let _ = &buffer[..read];
                let body = b"HELLO-FROM-LOCAL";
                let response = format!("HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.write_all(body).await;
            });
        }
    });

    // Gateway TLS material files, so the agent can trust the CA path.
    let material = tls::generate_self_signed(&["127.0.0.1".to_owned(), "localhost".to_owned()])
        .expect("material");
    let dir = tempfile::tempdir().expect("tempdir");
    let cert_path = dir.path().join("gateway-cert.pem");
    let key_path = dir.path().join("gateway-key.pem");
    std::fs::write(&cert_path, &material.cert_pem).expect("cert");
    std::fs::write(&key_path, &material.key_pem).expect("key");
    std::env::set_var(
        "SDKWORK_TUNNEL_TEST_TLS_CERT",
        cert_path.display().to_string(),
    );
    std::env::set_var(
        "SDKWORK_TUNNEL_TEST_TLS_KEY",
        key_path.display().to_string(),
    );

    let tunnel_port = available_port();
    let data_plane_port = available_port();

    // App config: one ordinary virtual host plus the tunnel section.
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "tunnel-relay-test",
        "limits": {
            "maxConcurrentRequests": 16,
            "requestTimeoutMs": 5_000,
            "drainTimeoutMs": 500,
            "maxConnections": 32
        },
        "listeners": [
            {
                "id": "main",
                "bind": "127.0.0.1",
                "port": data_plane_port,
                "protocols": ["http1"],
                "defaultVirtualHostRef": "main-host"
            }
        ],
        "resources": [
            { "id": "proxy", "type": "proxy", "upstreamRef": "local" }
        ],
        "virtualHosts": [
            {
                "id": "main-host",
                "listenerRefs": ["main"],
                "serverNames": ["main.test"],
                "routes": [
                    { "id": "root", "match": {"pathType": "prefix", "path": "/"}, "resourceRef": "proxy" }
                ]
            }
        ],
        "upstreams": [
            {
                "id": "local",
                "targets": [ { "url": format!("http://127.0.0.1:{target_port}") } ],
                "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]}
            }
        ],
        "tunnel": {
            "enabled": true,
            "gateway": {
                "listen": format!("127.0.0.1:{tunnel_port}"),
                "agentTokenEnv": ["SDKWORK_TUNNEL_GATEWAY_TOKEN"],
                "tlsCertPemEnv": "SDKWORK_TUNNEL_TEST_TLS_CERT",
                "tlsKeyPemEnv": "SDKWORK_TUNNEL_TEST_TLS_KEY"
            }
        }
    });
    let app: sdkwork_webserver_core::WebServerAppConfig =
        serde_json::from_value(config).expect("config parses");
    let compiled = load_and_compile_webserver_config_json(&app).expect("compiles");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let data_plane: DataPlaneTask = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });

    // Give the data plane and tunnel listeners a moment to bind.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Real agent against the in-process tunnel gateway.
    let mut config = TunnelConfig::disabled();
    config.routes = Some(vec![TunnelRouteTemplate {
        name: "web".to_owned(),
        protocol: TunnelProtocolKind::Http,
        domain: Some("demo.tunnel.test".to_owned()),
        port: None,
        target: format!("127.0.0.1:{target_port}"),
        policy: Some(RoutePolicy {
            allow_public: true,
            ..RoutePolicy::private()
        }),
        proxy_protocol: None,
        locations: Vec::new(),
        ..TunnelRouteTemplate::default()
    }]);
    let device = Device::new(
        DeviceId::parse("dev_dataplane").expect("valid id"),
        "data-plane-test",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", tunnel_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            ca_pem_path: Some(cert_path.display().to_string()),
            ..tls::ClientTlsOptions::default()
        },
        device,
        token: "dp-t0ken".to_owned(),
        routes: config.routes_or_empty().to_vec(),
        network: TunnelConfig::disabled().network_or_default(),
        timeouts: config.timeout_or_default(),
        metrics: std::sync::Arc::new(TunnelMetrics::new()),
    });

    // Wait for route registration.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let ready = loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .expect("agent never became ready");
        match tokio::time::timeout(remaining, agent.next_event()).await {
            Ok(Some(AgentEvent::Ready { routes, rejected })) => {
                assert!(rejected.is_empty(), "{rejected:?}");
                break routes;
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("agent stopped before ready"),
            Err(_) => panic!("timed out waiting for agent ready"),
        }
    };
    assert_eq!(ready.len(), 1);

    // Visitor request through the DATA PLANE listener, addressed to the
    // tunnel domain.
    let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", data_plane_port))
        .await
        .expect("visitor connect");
    visitor
        .write_all(b"GET /preview HTTP/1.1\r\nHost: demo.tunnel.test\r\nConnection: close\r\n\r\n")
        .await
        .expect("request written");
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), visitor.read_to_end(&mut response))
        .await
        .expect("response within 5s")
        .expect("response read");
    let response_text = String::from_utf8_lossy(&response).to_string();
    assert!(response_text.contains("200 OK"), "got: {response_text}");
    assert!(
        response_text.contains("HELLO-FROM-LOCAL"),
        "got: {response_text}"
    );

    agent.shutdown().await;
    let _ = shutdown_tx.send(());
    let _ = data_plane.await;
    target_task.abort();
}

#[tokio::test]
async fn a_route_rewrites_the_host_and_sets_headers_in_both_directions() {
    let _serial = tls_env_serial().await;
    // FRP `hostHeaderRewrite` + `requestHeaders.set` / `responseHeaders.set`:
    // the gateway is the hop that parses HTTP, so the rewriting has to happen
    // there and the local service must observe exactly the configured message.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    std::env::set_var("SDKWORK_TUNNEL_GATEWAY_TOKEN", "rewrite-t0ken");

    // Local HTTP target that records the head it received and answers with its
    // own header, so both directions are observable.
    let target = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("target bind");
    let target_port = target.local_addr().expect("target addr").port();
    let recorded: std::sync::Arc<tokio::sync::Mutex<String>> =
        std::sync::Arc::new(tokio::sync::Mutex::new(String::new()));
    let recorder = recorded.clone();
    let target_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = target.accept().await else {
                return;
            };
            let recorder = recorder.clone();
            tokio::spawn(async move {
                let mut buffer = [0_u8; 4096];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                *recorder.lock().await = String::from_utf8_lossy(&buffer[..read]).to_string();
                let body = b"REWRITTEN";
                let response = format!(
                    "HTTP/1.0 200 OK\r\nContent-Length: {}\r\nX-Origin-Server: local-1\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.write_all(body).await;
            });
        }
    });

    let material = tls::generate_self_signed(&["127.0.0.1".to_owned(), "localhost".to_owned()])
        .expect("material");
    let dir = tempfile::tempdir().expect("tempdir");
    let cert_path = dir.path().join("gateway-cert.pem");
    let key_path = dir.path().join("gateway-key.pem");
    std::fs::write(&cert_path, &material.cert_pem).expect("cert");
    std::fs::write(&key_path, &material.key_pem).expect("key");
    std::env::set_var(
        "SDKWORK_TUNNEL_TEST_TLS_CERT",
        cert_path.display().to_string(),
    );
    std::env::set_var(
        "SDKWORK_TUNNEL_TEST_TLS_KEY",
        key_path.display().to_string(),
    );

    let tunnel_port = available_port();
    let data_plane_port = available_port();
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "tunnel-rewrite-test",
        "limits": {
            "maxConcurrentRequests": 16,
            "requestTimeoutMs": 5_000,
            "drainTimeoutMs": 500,
            "maxConnections": 32
        },
        "listeners": [
            {
                "id": "main",
                "bind": "127.0.0.1",
                "port": data_plane_port,
                "protocols": ["http1"],
                "defaultVirtualHostRef": "main-host"
            }
        ],
        "resources": [ { "id": "proxy", "type": "proxy", "upstreamRef": "local" } ],
        "virtualHosts": [
            {
                "id": "main-host",
                "listenerRefs": ["main"],
                "serverNames": ["main.test"],
                "routes": [
                    { "id": "root", "match": {"pathType": "prefix", "path": "/"}, "resourceRef": "proxy" }
                ]
            }
        ],
        "upstreams": [
            {
                "id": "local",
                "targets": [ { "url": format!("http://127.0.0.1:{target_port}") } ],
                "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]}
            }
        ],
        "tunnel": {
            "enabled": true,
            "gateway": {
                "listen": format!("127.0.0.1:{tunnel_port}"),
                "agentTokenEnv": ["SDKWORK_TUNNEL_GATEWAY_TOKEN"],
                "tlsCertPemEnv": "SDKWORK_TUNNEL_TEST_TLS_CERT",
                "tlsKeyPemEnv": "SDKWORK_TUNNEL_TEST_TLS_KEY"
            }
        }
    });
    let app: sdkwork_webserver_core::WebServerAppConfig =
        serde_json::from_value(config).expect("config parses");
    let compiled = load_and_compile_webserver_config_json(&app).expect("compiles");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let data_plane: DataPlaneTask = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut set = std::collections::BTreeMap::new();
    set.insert("X-From".to_owned(), "edge".to_owned());
    let mut response_set = std::collections::BTreeMap::new();
    response_set.insert("X-Origin-Server".to_owned(), "masked".to_owned());
    response_set.insert("X-Served-By".to_owned(), "sdkwork".to_owned());
    let mut config = TunnelConfig::disabled();
    config.routes = Some(vec![TunnelRouteTemplate {
        name: "web".to_owned(),
        protocol: TunnelProtocolKind::Http,
        domain: Some("rewrite.tunnel.test".to_owned()),
        port: None,
        target: format!("127.0.0.1:{target_port}"),
        policy: Some(RoutePolicy {
            allow_public: true,
            ..RoutePolicy::private()
        }),
        host_header_rewrite: Some("backend.internal:8080".to_owned()),
        request_headers: Some(sdkwork_webserver_tunnel_core::HeaderSetConfig { set }),
        response_headers: Some(sdkwork_webserver_tunnel_core::HeaderSetConfig {
            set: response_set,
        }),
        ..TunnelRouteTemplate::default()
    }]);
    let device = Device::new(
        DeviceId::parse("dev_rewrite").expect("valid id"),
        "rewrite-test",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", tunnel_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            ca_pem_path: Some(cert_path.display().to_string()),
            ..tls::ClientTlsOptions::default()
        },
        device,
        token: "rewrite-t0ken".to_owned(),
        routes: config.routes_or_empty().to_vec(),
        network: TunnelConfig::disabled().network_or_default(),
        timeouts: config.timeout_or_default(),
        metrics: std::sync::Arc::new(TunnelMetrics::new()),
    });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .expect("agent never became ready");
        match tokio::time::timeout(remaining, agent.next_event()).await {
            Ok(Some(AgentEvent::Ready { rejected, .. })) => {
                assert!(rejected.is_empty(), "{rejected:?}");
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("agent stopped before ready"),
            Err(_) => panic!("timed out waiting for agent ready"),
        }
    }

    let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", data_plane_port))
        .await
        .expect("visitor connect");
    visitor
        .write_all(
            b"GET /preview HTTP/1.1\r\nHost: rewrite.tunnel.test\r\nX-From: visitor\r\nConnection: close\r\n\r\n",
        )
        .await
        .expect("request written");
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), visitor.read_to_end(&mut response))
        .await
        .expect("response within 5s")
        .expect("response read");
    let response_text = String::from_utf8_lossy(&response).to_string();
    assert!(response_text.contains("REWRITTEN"), "got: {response_text}");

    // Response side: the configured value replaced the upstream's, and an
    // additional header was added.
    let lowered = response_text.to_ascii_lowercase();
    assert!(
        lowered.contains("x-origin-server: masked"),
        "the upstream value must be replaced: {response_text}"
    );
    assert!(
        lowered.contains("x-served-by: sdkwork"),
        "the configured header must reach the visitor: {response_text}"
    );
    assert!(
        !lowered.contains("local-1"),
        "the replaced upstream value must not leak: {response_text}"
    );

    // Request side: the Host the local service sees is the configured one, and
    // the configured request header replaced the visitor's.
    let head = recorded.lock().await.clone();
    let head_lowered = head.to_ascii_lowercase();
    assert!(
        head_lowered.contains("host: backend.internal:8080"),
        "host rewrite missing: {head}"
    );
    assert!(
        !head_lowered.contains("rewrite.tunnel.test"),
        "the public host must not reach the local service: {head}"
    );
    assert!(
        head_lowered.contains("x-from: edge"),
        "request header rewrite missing: {head}"
    );
    assert!(
        head_lowered.contains("x-forwarded-for:"),
        "the visitor address is still propagated: {head}"
    );

    agent.shutdown().await;
    let _ = shutdown_tx.send(());
    let _ = data_plane.await;
    target_task.abort();
}

#[tokio::test]
async fn tunnel_disabled_leaves_data_plane_unchanged() {
    // PRD §77: without a tunnel section nothing tunnel-related starts and
    // the ordinary proxy path keeps working.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let target = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("target bind");
    let target_port = target.local_addr().expect("target addr").port();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = target.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buffer = [0_u8; 2048];
                let _ = socket.read(&mut buffer).await;
                let body = b"DIRECT";
                let response = format!("HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.write_all(body).await;
            });
        }
    });

    let data_plane_port = available_port();
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "tunnel-disabled-test",
        "limits": {
            "maxConcurrentRequests": 16,
            "requestTimeoutMs": 5_000,
            "drainTimeoutMs": 500,
            "maxConnections": 32
        },
        "listeners": [
            { "id": "main", "bind": "127.0.0.1", "port": data_plane_port, "protocols": ["http1"] }
        ],
        "resources": [
            { "id": "proxy", "type": "proxy", "upstreamRef": "local" }
        ],
        "virtualHosts": [
            {
                "id": "main-host",
                "listenerRefs": ["main"],
                "serverNames": ["main.test"],
                "routes": [
                    { "id": "root", "match": {"pathType": "prefix", "path": "/"}, "resourceRef": "proxy" }
                ]
            }
        ],
        "upstreams": [
            {
                "id": "local",
                "targets": [ { "url": format!("http://127.0.0.1:{target_port}") } ],
                "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]}
            }
        ]
    });
    let app: sdkwork_webserver_core::WebServerAppConfig =
        serde_json::from_value(config).expect("config parses");
    let compiled = load_and_compile_webserver_config_json(&app).expect("compiles");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let data_plane: DataPlaneTask = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", data_plane_port))
        .await
        .expect("visitor connect");
    visitor
        .write_all(b"GET / HTTP/1.1\r\nHost: main.test\r\nConnection: close\r\n\r\n")
        .await
        .expect("request written");
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), visitor.read_to_end(&mut response))
        .await
        .expect("response within 5s")
        .expect("response read");
    let response_text = String::from_utf8_lossy(&response).to_string();
    assert!(response_text.contains("DIRECT"), "got: {response_text}");

    let _ = shutdown_tx.send(());
    let _ = data_plane.await;
}

/// Same relay, but through the entry point the deployed edge runtime uses.
///
/// `sdkwork-webserver-website-delivery-edge-runtime serve` boots
/// `run_website_data_plane_from_config_until` → `run_website_data_plane_*` →
/// `run_data_plane_runtime_until(.., Some(executor), ..)`. The two tests above
/// use `run_data_plane_until`, which passes `None`. `serve_request` returns
/// *inside* the `Some(website_delivery)` branch, so the tunnel lookup that sits
/// below it is only reachable on the `None` entry point — a shape no binary
/// starts. This test pins the production shape: a host that matches a
/// registered tunnel route is relayed on the website-delivery entry too.
#[tokio::test]
async fn tunnel_domain_is_relayed_on_the_website_delivery_entry() {
    let _serial = tls_env_serial().await;
    use sdkwork_api_webserver_standalone_gateway::run_website_data_plane_with_operations_until;
    use sdkwork_webserver_core::website_runtime::{
        WebsiteRuntimeEnvironment, WebsiteRuntimeRegistry,
    };
    use sdkwork_webserver_delivery_runtime::{WebsiteDeliveryExecutor, WebsiteProviderRegistry};

    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    // Its own token: the tests in this file run in parallel threads.
    std::env::set_var("SDKWORK_TUNNEL_GATEWAY_TOKEN_WEBSITE", "dp-wd-t0ken");

    let target = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("target bind");
    let target_port = target.local_addr().expect("target addr").port();
    let target_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = target.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buffer = [0_u8; 2048];
                let _ = socket.read(&mut buffer).await;
                let body = b"HELLO-FROM-LOCAL";
                let response = format!("HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.write_all(body).await;
            });
        }
    });

    let material = tls::generate_self_signed(&["127.0.0.1".to_owned(), "localhost".to_owned()])
        .expect("material");
    let dir = tempfile::tempdir().expect("tempdir");
    let cert_path = dir.path().join("gateway-cert.pem");
    let key_path = dir.path().join("gateway-key.pem");
    std::fs::write(&cert_path, &material.cert_pem).expect("cert");
    std::fs::write(&key_path, &material.key_pem).expect("key");
    std::env::set_var(
        "SDKWORK_TUNNEL_TEST_TLS_CERT_WEBSITE",
        cert_path.display().to_string(),
    );
    std::env::set_var(
        "SDKWORK_TUNNEL_TEST_TLS_KEY_WEBSITE",
        key_path.display().to_string(),
    );

    let tunnel_port = available_port();
    let data_plane_port = available_port();

    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "tunnel-website-entry-test",
        "limits": {
            "maxConcurrentRequests": 16,
            "requestTimeoutMs": 5_000,
            "drainTimeoutMs": 500,
            "maxConnections": 32
        },
        "listeners": [
            {
                "id": "main",
                "bind": "127.0.0.1",
                "port": data_plane_port,
                "protocols": ["http1"],
                "defaultVirtualHostRef": "main-host"
            }
        ],
        "resources": [
            { "id": "proxy", "type": "proxy", "upstreamRef": "local" }
        ],
        "virtualHosts": [
            {
                "id": "main-host",
                "listenerRefs": ["main"],
                "serverNames": ["main.test"],
                "routes": [
                    { "id": "root", "match": {"pathType": "prefix", "path": "/"}, "resourceRef": "proxy" }
                ]
            }
        ],
        "upstreams": [
            {
                "id": "local",
                "targets": [ { "url": format!("http://127.0.0.1:{target_port}") } ],
                "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]}
            }
        ],
        "tunnel": {
            "enabled": true,
            "gateway": {
                "listen": format!("127.0.0.1:{tunnel_port}"),
                "agentTokenEnv": ["SDKWORK_TUNNEL_GATEWAY_TOKEN_WEBSITE"],
                "tlsCertPemEnv": "SDKWORK_TUNNEL_TEST_TLS_CERT_WEBSITE",
                "tlsKeyPemEnv": "SDKWORK_TUNNEL_TEST_TLS_KEY_WEBSITE"
            }
        }
    });
    let app: sdkwork_webserver_core::WebServerAppConfig =
        serde_json::from_value(config).expect("config parses");
    let compiled = load_and_compile_webserver_config_json(&app).expect("compiles");

    let executor = std::sync::Arc::new(
        WebsiteDeliveryExecutor::with_provider_runtime_limits(
            std::sync::Arc::new(WebsiteRuntimeRegistry::new(
                "node-1",
                WebsiteRuntimeEnvironment::Test,
            )),
            std::sync::Arc::new(WebsiteProviderRegistry::new()),
            1024,
            7,
        )
        .expect("website executor"),
    );

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let data_plane: DataPlaneTask = tokio::spawn(async move {
        run_website_data_plane_with_operations_until(compiled, executor, None, None, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut config = TunnelConfig::disabled();
    config.routes = Some(vec![TunnelRouteTemplate {
        name: "web".to_owned(),
        protocol: TunnelProtocolKind::Http,
        domain: Some("demo.tunnel.test".to_owned()),
        port: None,
        target: format!("127.0.0.1:{target_port}"),
        policy: Some(RoutePolicy {
            allow_public: true,
            ..RoutePolicy::private()
        }),
        proxy_protocol: None,
        locations: Vec::new(),
        ..TunnelRouteTemplate::default()
    }]);
    let device = Device::new(
        DeviceId::parse("dev_website").expect("valid id"),
        "website-entry-test",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", tunnel_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            ca_pem_path: Some(cert_path.display().to_string()),
            ..tls::ClientTlsOptions::default()
        },
        device,
        token: "dp-wd-t0ken".to_owned(),
        routes: config.routes_or_empty().to_vec(),
        network: TunnelConfig::disabled().network_or_default(),
        timeouts: config.timeout_or_default(),
        metrics: std::sync::Arc::new(TunnelMetrics::new()),
    });

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let ready = loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .expect("agent never became ready");
        match tokio::time::timeout(remaining, agent.next_event()).await {
            Ok(Some(AgentEvent::Ready { routes, rejected })) => {
                assert!(rejected.is_empty(), "{rejected:?}");
                break routes;
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("agent stopped before ready"),
            Err(_) => panic!("timed out waiting for agent ready"),
        }
    };
    assert_eq!(ready.len(), 1);

    let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", data_plane_port))
        .await
        .expect("visitor connect");
    visitor
        .write_all(b"GET /preview HTTP/1.1\r\nHost: demo.tunnel.test\r\nConnection: close\r\n\r\n")
        .await
        .expect("request written");
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), visitor.read_to_end(&mut response))
        .await
        .expect("response within 5s")
        .expect("response read");
    let response_text = String::from_utf8_lossy(&response).to_string();
    assert!(
        response_text.contains("HELLO-FROM-LOCAL"),
        "a registered tunnel host must be relayed on the website-delivery entry point \
         (the entry the deployed edge runtime uses); got: {response_text}"
    );

    agent.shutdown().await;
    let _ = shutdown_tx.send(());
    let _ = data_plane.await;
    target_task.abort();
}

/// A WebSocket session through the whole stack: visitor → data-plane listener →
/// tunnel (QUIC) → agent → local target. `docs/tunnel.md` promises the relay
/// handles upgrades, and `relay_tunnel_http` implements the pump, but the test
/// above only speaks plain HTTP — so this pins the upgrade path, and pins it at
/// the target: the handshake the local server receives is the only part of a
/// relayed WebSocket the edge can still rewrite (everything after the 101 is
/// opaque bytes), so the visitor's real address must be visible there.
///
/// This test deliberately uses its own environment variables for the gateway
/// token and TLS material: the two tests in this file run in parallel threads,
/// and sharing a name would let one point the gateway at the other's temporary
/// certificate.
#[tokio::test]
async fn data_plane_pumps_a_websocket_upgrade_through_the_tunnel() {
    let _serial = tls_env_serial().await;
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    std::env::set_var("SDKWORK_TUNNEL_GATEWAY_TOKEN_WS", "dp-ws-t0ken");

    // Local target: answers the upgrade, then echoes raw bytes.
    let target = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("target bind");
    let target_port = target.local_addr().expect("target addr").port();
    let handshakes = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let handshakes_task = std::sync::Arc::clone(&handshakes);
    let target_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = target.accept().await else {
                return;
            };
            let handshakes = std::sync::Arc::clone(&handshakes_task);
            tokio::spawn(async move {
                let mut bytes = Vec::with_capacity(1024);
                let mut buffer = [0_u8; 1024];
                let tail = loop {
                    let Ok(read) = socket.read(&mut buffer).await else {
                        return;
                    };
                    if read == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&buffer[..read]);
                    if let Some(position) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        break bytes.split_off(position + 4);
                    }
                    if bytes.len() > 64 * 1024 {
                        return;
                    }
                };
                handshakes
                    .lock()
                    .expect("handshake log lock")
                    .push(String::from_utf8_lossy(&bytes).to_string());
                let response = "HTTP/1.1 101 Switching Protocols\r\nConnection: upgrade\r\n\
                    Upgrade: websocket\r\nSec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\r\n";
                if socket.write_all(response.as_bytes()).await.is_err() {
                    return;
                }
                if !tail.is_empty() && socket.write_all(&tail).await.is_err() {
                    return;
                }
                loop {
                    match socket.read(&mut buffer).await {
                        Ok(0) | Err(_) => {
                            let _ = socket.shutdown().await;
                            return;
                        }
                        Ok(read) if socket.write_all(&buffer[..read]).await.is_err() => return,
                        Ok(_) => {}
                    }
                }
            });
        }
    });

    let material = tls::generate_self_signed(&["127.0.0.1".to_owned(), "localhost".to_owned()])
        .expect("material");
    let dir = tempfile::tempdir().expect("tempdir");
    let cert_path = dir.path().join("gateway-cert.pem");
    let key_path = dir.path().join("gateway-key.pem");
    std::fs::write(&cert_path, &material.cert_pem).expect("cert");
    std::fs::write(&key_path, &material.key_pem).expect("key");
    std::env::set_var(
        "SDKWORK_TUNNEL_TEST_TLS_CERT_WS",
        cert_path.display().to_string(),
    );
    std::env::set_var(
        "SDKWORK_TUNNEL_TEST_TLS_KEY_WS",
        key_path.display().to_string(),
    );

    let tunnel_port = available_port();
    let data_plane_port = available_port();
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "tunnel-websocket-test",
        "limits": {
            "maxConcurrentRequests": 16,
            "requestTimeoutMs": 5_000,
            "drainTimeoutMs": 500,
            "maxConnections": 32
        },
        "listeners": [
            {
                "id": "main",
                "bind": "127.0.0.1",
                "port": data_plane_port,
                "protocols": ["http1"],
                "defaultVirtualHostRef": "main-host"
            }
        ],
        "resources": [
            { "id": "proxy", "type": "proxy", "upstreamRef": "local" }
        ],
        "virtualHosts": [
            {
                "id": "main-host",
                "listenerRefs": ["main"],
                "serverNames": ["main.test"],
                "routes": [
                    { "id": "root", "match": {"pathType": "prefix", "path": "/"}, "resourceRef": "proxy" }
                ]
            }
        ],
        "upstreams": [
            {
                "id": "local",
                "targets": [ { "url": format!("http://127.0.0.1:{target_port}") } ],
                "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]}
            }
        ],
        "tunnel": {
            "enabled": true,
            "gateway": {
                "listen": format!("127.0.0.1:{tunnel_port}"),
                "agentTokenEnv": ["SDKWORK_TUNNEL_GATEWAY_TOKEN_WS"],
                "tlsCertPemEnv": "SDKWORK_TUNNEL_TEST_TLS_CERT_WS",
                "tlsKeyPemEnv": "SDKWORK_TUNNEL_TEST_TLS_KEY_WS"
            }
        }
    });
    let app: sdkwork_webserver_core::WebServerAppConfig =
        serde_json::from_value(config).expect("config parses");
    let compiled = load_and_compile_webserver_config_json(&app).expect("compiles");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let data_plane: DataPlaneTask = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let device = Device::new(
        DeviceId::parse("dev_dataplane_ws").expect("valid id"),
        "data-plane-ws-test",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", tunnel_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            ca_pem_path: Some(cert_path.display().to_string()),
            ..tls::ClientTlsOptions::default()
        },
        device,
        token: "dp-ws-t0ken".to_owned(),
        routes: vec![TunnelRouteTemplate {
            name: "ws".to_owned(),
            protocol: TunnelProtocolKind::Http,
            domain: Some("ws.tunnel.test".to_owned()),
            port: None,
            target: format!("127.0.0.1:{target_port}"),
            policy: Some(RoutePolicy {
                allow_public: true,
                ..RoutePolicy::private()
            }),
            proxy_protocol: None,
            locations: Vec::new(),
            ..TunnelRouteTemplate::default()
        }],
        network: Default::default(),
        timeouts: Default::default(),
        metrics: std::sync::Arc::new(TunnelMetrics::new()),
    });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .expect("agent never became ready");
        match tokio::time::timeout(remaining, agent.next_event()).await {
            Ok(Some(AgentEvent::Ready { routes, rejected })) => {
                assert!(rejected.is_empty(), "{rejected:?}");
                assert_eq!(routes.len(), 1);
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("agent stopped before ready"),
            Err(_) => panic!("timed out waiting for agent ready"),
        }
    }

    let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", data_plane_port))
        .await
        .expect("visitor connect");
    visitor
        .write_all(
            b"GET /socket HTTP/1.1\r\nHost: ws.tunnel.test\r\nConnection: keep-alive, Upgrade\r\n\
              Upgrade: websocket\r\nSec-WebSocket-Version: 13\r\n\
              Sec-WebSocket-Key: MDEyMzQ1Njc4OWFiY2RlZg==\r\n\r\n",
        )
        .await
        .expect("handshake written");

    let (head, mut buffered) = tokio::time::timeout(Duration::from_secs(5), async {
        let mut bytes = Vec::with_capacity(1024);
        let mut buffer = [0_u8; 1024];
        loop {
            let read = visitor.read(&mut buffer).await.expect("read head");
            assert_ne!(read, 0, "the relay closed before the 101");
            bytes.extend_from_slice(&buffer[..read]);
            if let Some(position) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let tail = bytes.split_off(position + 4);
                return (String::from_utf8_lossy(&bytes).to_string(), tail);
            }
            assert!(bytes.len() <= 64 * 1024, "head exceeds test bound");
        }
    })
    .await
    .expect("the 101 arrives through the tunnel within 5s");
    assert!(head.starts_with("HTTP/1.1 101"), "{head}");
    let lowered = head.to_ascii_lowercase();
    assert!(lowered.contains("connection: upgrade"), "{head}");
    assert!(lowered.contains("upgrade: websocket"), "{head}");

    // Bytes after the 101 are opaque and must round trip through both pumps.
    let payload = b"ping-through-the-tunnel";
    visitor.write_all(payload).await.expect("payload written");
    while buffered.len() < payload.len() {
        let mut chunk = [0_u8; 64];
        let read = tokio::time::timeout(Duration::from_secs(5), visitor.read(&mut chunk))
            .await
            .expect("the echo arrives within 5s")
            .expect("read echoed bytes");
        assert_ne!(read, 0, "the pump closed before the payload round tripped");
        buffered.extend_from_slice(&chunk[..read]);
    }
    assert_eq!(&buffered[..payload.len()], payload);

    let handshakes = handshakes.lock().expect("handshake log lock").clone();
    assert_eq!(
        handshakes.len(),
        1,
        "exactly one handshake reached the local target: {handshakes:?}"
    );
    assert!(
        handshakes[0]
            .to_ascii_lowercase()
            .contains("x-forwarded-for: 127.0.0.1"),
        "the relayed handshake carries the visitor's real address: {}",
        handshakes[0]
    );

    drop(visitor);
    agent.shutdown().await;
    let _ = shutdown_tx.send(());
    let _ = data_plane.await;
    target_task.abort();
}

/// The machine-management surface an admin console renders: the connector
/// machines the gateway manages, their liveness, routes and traffic, and the
/// operator switch — including the fail-closed path when no operations token
/// is configured.
#[tokio::test]
async fn tunnel_machines_surface_lists_and_switches_a_machine() {
    let _serial = tls_env_serial().await;
    use sdkwork_api_webserver_standalone_gateway::{
        run_data_plane_with_operations_until, DataPlaneOperationsConfig,
    };

    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    std::env::set_var("SDKWORK_TUNNEL_GATEWAY_TOKEN_MACHINES", "machines-t0ken");

    // A local HTTP target behind the machine's route.
    let target = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("target bind");
    let target_port = target.local_addr().expect("addr").port();
    let target_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = target.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buffer = [0_u8; 1024];
                let _ = socket.read(&mut buffer).await;
                let body = b"MACHINE-OK";
                let response = format!("HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.write_all(body).await;
            });
        }
    });

    let material = tls::generate_self_signed(&["127.0.0.1".to_owned(), "localhost".to_owned()])
        .expect("material");
    let dir = tempfile::tempdir().expect("tempdir");
    let cert_path = dir.path().join("gateway-cert.pem");
    let key_path = dir.path().join("gateway-key.pem");
    std::fs::write(&cert_path, &material.cert_pem).expect("cert");
    std::fs::write(&key_path, &material.key_pem).expect("key");
    std::env::set_var(
        "SDKWORK_TUNNEL_TEST_TLS_CERT_MACHINES",
        cert_path.display().to_string(),
    );
    std::env::set_var(
        "SDKWORK_TUNNEL_TEST_TLS_KEY_MACHINES",
        key_path.display().to_string(),
    );

    // The operations listener needs a bearer token to accept mutations; the
    // token file is the documented source, so the test configures it the way
    // an operator would.
    let token_path = dir.path().join("operations-token");
    std::fs::write(&token_path, "machine-operator-t0ken\n").expect("token file");
    let operations_port = available_port();
    std::env::set_var(
        "SDKWORK_WEBSERVER_DATA_PLANE_OPERATIONS_BIND",
        format!("127.0.0.1:{operations_port}"),
    );
    std::env::set_var(
        "SDKWORK_WEBSERVER_OPERATIONS_AUTH_TOKEN_FILE",
        token_path.display().to_string(),
    );
    let operations = DataPlaneOperationsConfig::from_env()
        .expect("operations config parses")
        .expect("operations listener is configured");

    let tunnel_port = available_port();
    let data_plane_port = available_port();
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "tunnel-machines-test",
        "limits": {
            "maxConcurrentRequests": 16,
            "requestTimeoutMs": 5_000,
            "drainTimeoutMs": 500,
            "maxConnections": 32
        },
        "listeners": [
            {
                "id": "main",
                "bind": "127.0.0.1",
                "port": data_plane_port,
                "protocols": ["http1"],
                "defaultVirtualHostRef": "main-host"
            }
        ],
        "resources": [
            { "id": "proxy", "type": "proxy", "upstreamRef": "local" }
        ],
        "virtualHosts": [
            {
                "id": "main-host",
                "listenerRefs": ["main"],
                "serverNames": ["main.test"],
                "routes": [
                    { "id": "root", "match": {"pathType": "prefix", "path": "/"}, "resourceRef": "proxy" }
                ]
            }
        ],
        "upstreams": [
            {
                "id": "local",
                "targets": [ { "url": format!("http://127.0.0.1:{target_port}") } ],
                "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]}
            }
        ],
        "tunnel": {
            "enabled": true,
            "gateway": {
                "listen": format!("127.0.0.1:{tunnel_port}"),
                "agentTokenEnv": ["SDKWORK_TUNNEL_GATEWAY_TOKEN_MACHINES"],
                "tlsCertPemEnv": "SDKWORK_TUNNEL_TEST_TLS_CERT_MACHINES",
                "tlsKeyPemEnv": "SDKWORK_TUNNEL_TEST_TLS_KEY_MACHINES"
            }
        }
    });
    let app: sdkwork_webserver_core::WebServerAppConfig =
        serde_json::from_value(config).expect("config parses");
    let compiled = load_and_compile_webserver_config_json(&app).expect("compiles");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let data_plane: DataPlaneTask = tokio::spawn(async move {
        run_data_plane_with_operations_until(compiled, Some(operations), async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let device = Device::new(
        DeviceId::parse("dev_machine_surface").expect("valid id"),
        "surface-machine",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", tunnel_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            ca_pem_path: Some(cert_path.display().to_string()),
            ..tls::ClientTlsOptions::default()
        },
        device,
        token: "machines-t0ken".to_owned(),
        routes: vec![TunnelRouteTemplate {
            name: "machine-web".to_owned(),
            protocol: TunnelProtocolKind::Http,
            domain: Some("machine.surface.test".to_owned()),
            target: format!("127.0.0.1:{target_port}"),
            policy: Some(RoutePolicy {
                allow_public: true,
                ..RoutePolicy::private()
            }),
            ..TunnelRouteTemplate::default()
        }],
        network: TunnelConfig::disabled().network_or_default(),
        timeouts: TunnelConfig::disabled().timeout_or_default(),
        metrics: std::sync::Arc::new(TunnelMetrics::new()),
    });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .expect("agent never became ready");
        match tokio::time::timeout(remaining, agent.next_event()).await {
            Ok(Some(AgentEvent::Ready { routes, rejected })) => {
                assert!(rejected.is_empty(), "{rejected:?}");
                assert_eq!(routes.len(), 1);
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("agent stopped before ready"),
            Err(_) => panic!("timed out waiting for agent ready"),
        }
    }

    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(0)
        .build()
        .expect("HTTP client");
    let base = format!("http://127.0.0.1:{operations_port}");
    let bearer = "machine-operator-t0ken";

    // The machine is listed with its route, liveness and identity.
    let machines: serde_json::Value = client
        .get(format!("{base}/tunnel/machines"))
        .bearer_auth(bearer)
        .send()
        .await
        .expect("machines request")
        .json()
        .await
        .expect("machines json");
    let listed = machines["machines"]
        .as_array()
        .expect("machines array")
        .clone();
    assert_eq!(listed.len(), 1, "one machine: {machines}");
    let machine = &listed[0];
    assert_eq!(machine["deviceName"], "surface-machine");
    assert_eq!(machine["online"], true);
    assert_eq!(machine["enabled"], true);
    assert_eq!(machine["routeCount"], 1);
    assert_eq!(machine["routes"][0]["domain"], "machine.surface.test");
    assert_eq!(machine["routes"][0]["allowPublic"], true);

    // Detail and the unknown-id path.
    let detail = client
        .get(format!("{base}/tunnel/machines/dev_machine_surface"))
        .bearer_auth(bearer)
        .send()
        .await
        .expect("detail request");
    assert_eq!(detail.status(), reqwest::StatusCode::OK);
    let missing = client
        .get(format!("{base}/tunnel/machines/dev_nope"))
        .bearer_auth(bearer)
        .send()
        .await
        .expect("missing request");
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);

    // The route surface of the same listener works too — it used to panic the
    // router at startup (`/tunnel/routes/:id` in an axum 0.8 router), which
    // only showed up once tunnel *and* operations were enabled together.
    let route_id = machine["routes"][0]["routeId"]
        .as_str()
        .expect("route id")
        .to_owned();
    let removed = client
        .delete(format!("{base}/tunnel/routes/{route_id}"))
        .bearer_auth(bearer)
        .send()
        .await
        .expect("route removal");
    assert_eq!(removed.status(), reqwest::StatusCode::NO_CONTENT);
    let after_removal: serde_json::Value = client
        .get(format!("{base}/tunnel/machines/dev_machine_surface"))
        .bearer_auth(bearer)
        .send()
        .await
        .expect("detail request")
        .json()
        .await
        .expect("detail json");
    assert_eq!(
        after_removal["routeCount"], 0,
        "a removed route stops being served: {after_removal}"
    );
    let missing_route = client
        .delete(format!("{base}/tunnel/routes/{route_id}"))
        .bearer_auth(bearer)
        .send()
        .await
        .expect("second removal");
    assert_eq!(missing_route.status(), reqwest::StatusCode::NOT_FOUND);

    // The switch is a mutation: without the token it is refused, and the
    // refusal is fail-closed rather than a silent no-op.
    let unauthenticated = client
        .put(format!(
            "{base}/tunnel/machines/dev_machine_surface/enabled"
        ))
        .json(&json!({ "enabled": false }))
        .send()
        .await
        .expect("unauthenticated switch");
    assert!(
        unauthenticated.status() == reqwest::StatusCode::UNAUTHORIZED
            || unauthenticated.status() == reqwest::StatusCode::FORBIDDEN,
        "an unauthenticated mutation must be refused, got {}",
        unauthenticated.status()
    );

    // A malformed body is a client error, not a crash.
    let malformed = client
        .put(format!(
            "{base}/tunnel/machines/dev_machine_surface/enabled"
        ))
        .bearer_auth(bearer)
        .body("not json")
        .send()
        .await
        .expect("malformed switch");
    assert_eq!(malformed.status(), reqwest::StatusCode::BAD_REQUEST);

    // Switching off takes effect on the live session.
    let switched = client
        .put(format!(
            "{base}/tunnel/machines/dev_machine_surface/enabled"
        ))
        .bearer_auth(bearer)
        .json(&json!({ "enabled": false }))
        .send()
        .await
        .expect("switch off");
    assert_eq!(switched.status(), reqwest::StatusCode::OK);
    let switched: serde_json::Value = switched.json().await.expect("switch json");
    assert_eq!(switched["enabled"], false);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let machine: serde_json::Value = client
            .get(format!("{base}/tunnel/machines/dev_machine_surface"))
            .bearer_auth(bearer)
            .send()
            .await
            .expect("detail request")
            .json()
            .await
            .expect("detail json");
        if machine["online"] == false {
            assert_eq!(
                machine["routeCount"], 0,
                "an offline machine serves nothing"
            );
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the disabled machine kept its session: {machine}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Switching it back on clears the switch, so a later reconnect is allowed.
    let restored = client
        .put(format!(
            "{base}/tunnel/machines/dev_machine_surface/enabled"
        ))
        .bearer_auth(bearer)
        .json(&json!({ "enabled": true }))
        .send()
        .await
        .expect("switch on");
    assert_eq!(restored.status(), reqwest::StatusCode::OK);
    let restored: serde_json::Value = restored.json().await.expect("switch json");
    assert_eq!(restored["enabled"], true);

    // With the tunnel disabled the whole surface 404s, like every other
    // tunnel-only path.
    agent.shutdown().await;
    let _ = shutdown_tx.send(());
    let _ = data_plane.await;
    target_task.abort();
}

/// A host the tunnel claims but cannot serve must answer 503.
///
/// 404 would say "nobody serves this host" and 502 "the relay broke"; neither
/// is true when every replica behind the route is down. The verdict travels
/// the whole chain here: the agent probes its own dead target, reports over
/// the control plane, the registry marks the route down, and the data plane
/// answers 503.
#[tokio::test]
async fn tunnel_route_without_a_healthy_replica_answers_503() {
    let _serial = tls_env_serial().await;
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    std::env::set_var("SDKWORK_TUNNEL_GATEWAY_TOKEN_SICK", "sick-t0ken");

    // The route's local target: a port nobody listens on, so every probe
    // fails and every relay would fail too.
    let dead_port = available_port();

    let material = tls::generate_self_signed(&["127.0.0.1".to_owned(), "localhost".to_owned()])
        .expect("material");
    let dir = tempfile::tempdir().expect("tempdir");
    let cert_path = dir.path().join("gateway-cert.pem");
    let key_path = dir.path().join("gateway-key.pem");
    std::fs::write(&cert_path, &material.cert_pem).expect("cert");
    std::fs::write(&key_path, &material.key_pem).expect("key");
    std::env::set_var(
        "SDKWORK_TUNNEL_TEST_TLS_CERT_SICK",
        cert_path.display().to_string(),
    );
    std::env::set_var(
        "SDKWORK_TUNNEL_TEST_TLS_KEY_SICK",
        key_path.display().to_string(),
    );

    let tunnel_port = available_port();
    let data_plane_port = available_port();
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "tunnel-sick-test",
        "limits": {
            "maxConcurrentRequests": 16,
            "requestTimeoutMs": 5_000,
            "drainTimeoutMs": 500,
            "maxConnections": 32
        },
        "listeners": [
            {
                "id": "main",
                "bind": "127.0.0.1",
                "port": data_plane_port,
                "protocols": ["http1"],
                "defaultVirtualHostRef": "main-host"
            }
        ],
        "resources": [
            { "id": "proxy", "type": "proxy", "upstreamRef": "local" }
        ],
        "virtualHosts": [
            {
                "id": "main-host",
                "listenerRefs": ["main"],
                "serverNames": ["main.test"],
                "routes": [
                    { "id": "root", "match": {"pathType": "prefix", "path": "/"}, "resourceRef": "proxy" }
                ]
            }
        ],
        "upstreams": [
            {
                "id": "local",
                "targets": [ { "url": format!("http://127.0.0.1:{dead_port}") } ],
                "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]}
            }
        ],
        "tunnel": {
            "enabled": true,
            "gateway": {
                "listen": format!("127.0.0.1:{tunnel_port}"),
                "agentTokenEnv": ["SDKWORK_TUNNEL_GATEWAY_TOKEN_SICK"],
                "tlsCertPemEnv": "SDKWORK_TUNNEL_TEST_TLS_CERT_SICK",
                "tlsKeyPemEnv": "SDKWORK_TUNNEL_TEST_TLS_KEY_SICK"
            }
        }
    });
    let app: sdkwork_webserver_core::WebServerAppConfig =
        serde_json::from_value(config).expect("config parses");
    let compiled = load_and_compile_webserver_config_json(&app).expect("compiles");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let data_plane: DataPlaneTask = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let device = Device::new(
        DeviceId::parse("dev_sick").expect("valid id"),
        "sick-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", tunnel_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            ca_pem_path: Some(cert_path.display().to_string()),
            ..tls::ClientTlsOptions::default()
        },
        device,
        token: "sick-t0ken".to_owned(),
        routes: vec![TunnelRouteTemplate {
            name: "sick".to_owned(),
            protocol: TunnelProtocolKind::Http,
            domain: Some("sick.tunnel.test".to_owned()),
            port: None,
            target: format!("127.0.0.1:{dead_port}"),
            policy: Some(RoutePolicy {
                allow_public: true,
                ..RoutePolicy::private()
            }),
            health_check: Some(sdkwork_webserver_tunnel_core::HealthCheckConfig {
                kind: sdkwork_webserver_tunnel_core::HealthCheckKind::Tcp,
                interval_seconds: 1,
                timeout_seconds: 1,
                max_failed: 1,
                path: None,
            }),
            ..TunnelRouteTemplate::default()
        }],
        network: TunnelConfig::disabled().network_or_default(),
        timeouts: TunnelConfig::disabled().timeout_or_default(),
        metrics: std::sync::Arc::new(TunnelMetrics::new()),
    });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .expect("agent never became ready");
        match tokio::time::timeout(remaining, agent.next_event()).await {
            Ok(Some(AgentEvent::Ready { routes, rejected })) => {
                assert!(rejected.is_empty(), "{rejected:?}");
                assert_eq!(routes.len(), 1);
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("agent stopped before ready"),
            Err(_) => panic!("timed out waiting for agent ready"),
        }
    }

    // Wait for the probe to report the route down. Until the first verdict
    // arrives the relay legitimately fails (502: the local target is dead);
    // what must never happen is 404, because the host *is* claimed. Once the
    // verdict lands, the answer is 503 and stays 503.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", data_plane_port))
            .await
            .expect("visitor connect");
        visitor
            .write_all(b"GET / HTTP/1.1\r\nHost: sick.tunnel.test\r\nConnection: close\r\n\r\n")
            .await
            .expect("request written");
        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_secs(5), visitor.read_to_end(&mut response))
            .await
            .expect("response within 5s")
            .expect("response read");
        let text = String::from_utf8_lossy(&response).to_string();
        assert!(
            !text.starts_with("HTTP/1.1 404"),
            "the host is claimed, so a down replica must not answer 404: {text}"
        );
        if text.starts_with("HTTP/1.1 503") {
            assert!(
                text.contains("no healthy replica"),
                "the 503 must say why: {text}"
            );
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the route never reported itself unhealthy: {text}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // Once unhealthy, the verdict holds for every later request.
    let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", data_plane_port))
        .await
        .expect("visitor connect");
    visitor
        .write_all(b"GET /again HTTP/1.1\r\nHost: sick.tunnel.test\r\nConnection: close\r\n\r\n")
        .await
        .expect("request written");
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), visitor.read_to_end(&mut response))
        .await
        .expect("response within 5s")
        .expect("response read");
    let text = String::from_utf8_lossy(&response).to_string();
    assert!(
        text.starts_with("HTTP/1.1 503"),
        "a down replica keeps answering 503: {text}"
    );

    agent.shutdown().await;
    let _ = shutdown_tx.send(());
    let _ = data_plane.await;
}
///
/// Every denial used to surface as "502 tunnel relay failed", which told the
/// visitor the tunnel was broken while it was behaving exactly as configured —
/// and hid a misconfigured policy behind what looks like an outage. The same
/// test covers the policy travelling through the control plane: the bearer
/// route below is protected by a token that only exists in the agent's
/// template, so a gateway that drops the policy would answer 403 for the
/// token-holding visitor instead of relaying.
#[tokio::test]
async fn tunnel_denials_answer_403_or_401_instead_of_502() {
    let _serial = tls_env_serial().await;
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    // Unique env names: several tests in this file share one process, and the
    // tunnel config names its environment variables (a token never touches a
    // config file), so a shared name would race.
    std::env::set_var("SDKWORK_TUNNEL_GATEWAY_TOKEN_DENY", "deny-t0ken");

    let target = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("target bind");
    let target_port = target.local_addr().expect("target addr").port();
    let target_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = target.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buffer = [0_u8; 2048];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                if read == 0 {
                    return;
                }
                let body = b"HELLO-FROM-LOCAL";
                let response = format!("HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.write_all(body).await;
            });
        }
    });

    let material = tls::generate_self_signed(&["127.0.0.1".to_owned(), "localhost".to_owned()])
        .expect("material");
    let dir = tempfile::tempdir().expect("tempdir");
    let cert_path = dir.path().join("gateway-cert.pem");
    let key_path = dir.path().join("gateway-key.pem");
    std::fs::write(&cert_path, &material.cert_pem).expect("cert");
    std::fs::write(&key_path, &material.key_pem).expect("key");
    std::env::set_var(
        "SDKWORK_TUNNEL_TEST_TLS_CERT_DENY",
        cert_path.display().to_string(),
    );
    std::env::set_var(
        "SDKWORK_TUNNEL_TEST_TLS_KEY_DENY",
        key_path.display().to_string(),
    );

    let tunnel_port = available_port();
    let data_plane_port = available_port();
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "tunnel-denial-test",
        "limits": {
            "maxConcurrentRequests": 16,
            "requestTimeoutMs": 5_000,
            "drainTimeoutMs": 500,
            "maxConnections": 32
        },
        "listeners": [
            {
                "id": "main",
                "bind": "127.0.0.1",
                "port": data_plane_port,
                "protocols": ["http1"],
                "defaultVirtualHostRef": "main-host"
            }
        ],
        "resources": [
            { "id": "proxy", "type": "proxy", "upstreamRef": "local" }
        ],
        "virtualHosts": [
            {
                "id": "main-host",
                "listenerRefs": ["main"],
                "serverNames": ["main.test"],
                "routes": [
                    { "id": "root", "match": {"pathType": "prefix", "path": "/"}, "resourceRef": "proxy" }
                ]
            }
        ],
        "upstreams": [
            {
                "id": "local",
                "targets": [ { "url": format!("http://127.0.0.1:{target_port}") } ],
                "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]}
            }
        ],
        "tunnel": {
            "enabled": true,
            "gateway": {
                "listen": format!("127.0.0.1:{tunnel_port}"),
                "agentTokenEnv": ["SDKWORK_TUNNEL_GATEWAY_TOKEN_DENY"],
                "tlsCertPemEnv": "SDKWORK_TUNNEL_TEST_TLS_CERT_DENY",
                "tlsKeyPemEnv": "SDKWORK_TUNNEL_TEST_TLS_KEY_DENY"
            }
        }
    });
    let app: sdkwork_webserver_core::WebServerAppConfig =
        serde_json::from_value(config).expect("config parses");
    let compiled = load_and_compile_webserver_config_json(&app).expect("compiles");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let data_plane: DataPlaneTask = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let routes = vec![
        // No admission channel at all: every visitor is refused.
        TunnelRouteTemplate {
            name: "closed".to_owned(),
            protocol: TunnelProtocolKind::Http,
            domain: Some("closed.tunnel.test".to_owned()),
            port: None,
            target: format!("127.0.0.1:{target_port}"),
            policy: Some(RoutePolicy::private()),
            proxy_protocol: None,
            locations: Vec::new(),
            ..TunnelRouteTemplate::default()
        },
        // Visitor token required; the token exists only in this template.
        TunnelRouteTemplate {
            name: "protected".to_owned(),
            protocol: TunnelProtocolKind::Http,
            domain: Some("protected.tunnel.test".to_owned()),
            port: None,
            target: format!("127.0.0.1:{target_port}"),
            policy: Some(RoutePolicy {
                allow_public: false,
                allowed_ips: Vec::new(),
                auth: sdkwork_webserver_tunnel_core::AuthPolicy::BearerToken,
                visitor_tokens: vec!["visitor-s3cret".to_owned()],
            }),
            proxy_protocol: None,
            locations: Vec::new(),
            ..TunnelRouteTemplate::default()
        },
    ];
    let device = Device::new(
        DeviceId::parse("dev_denial").expect("valid id"),
        "denial-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", tunnel_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            ca_pem_path: Some(cert_path.display().to_string()),
            ..tls::ClientTlsOptions::default()
        },
        device,
        token: "deny-t0ken".to_owned(),
        routes,
        network: TunnelConfig::disabled().network_or_default(),
        timeouts: TunnelConfig::disabled().timeout_or_default(),
        metrics: std::sync::Arc::new(TunnelMetrics::new()),
    });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .expect("agent never became ready");
        match tokio::time::timeout(remaining, agent.next_event()).await {
            Ok(Some(AgentEvent::Ready { routes, rejected })) => {
                assert!(rejected.is_empty(), "{rejected:?}");
                assert_eq!(routes.len(), 2);
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("agent stopped before ready"),
            Err(_) => panic!("timed out waiting for agent ready"),
        }
    }

    async fn visitor_request(
        port: u16,
        host: &str,
        authorization: Option<&str>,
    ) -> (String, Option<String>) {
        let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("visitor connect");
        let mut request = format!("GET / HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
        if let Some(value) = authorization {
            request.push_str(&format!("Authorization: {value}\r\n"));
        }
        request.push_str("\r\n");
        visitor
            .write_all(request.as_bytes())
            .await
            .expect("request written");
        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_secs(5), visitor.read_to_end(&mut response))
            .await
            .expect("response within 5s")
            .expect("response read");
        let text = String::from_utf8_lossy(&response).to_string();
        let lowered = text.to_ascii_lowercase();
        let challenge = lowered
            .lines()
            .find(|line| line.starts_with("www-authenticate:"))
            .map(str::to_owned);
        (text, challenge)
    }

    // 1. Policy with no admission channel: 403, not 502.
    let (response, _) = visitor_request(data_plane_port, "closed.tunnel.test", None).await;
    assert!(
        response.starts_with("HTTP/1.1 403"),
        "a private route must answer 403, got: {response}"
    );

    // 2. Bearer route without a token: 401 with a challenge.
    let (response, challenge) =
        visitor_request(data_plane_port, "protected.tunnel.test", None).await;
    assert!(
        response.starts_with("HTTP/1.1 401"),
        "a token-protected route must demand the token, got: {response}"
    );
    assert_eq!(
        challenge.as_deref(),
        Some("www-authenticate: bearer realm=\"tunnel\""),
        "the 401 must carry a bearer challenge"
    );

    // 3. Bearer route with a wrong token: still refused.
    let (response, _) = visitor_request(
        data_plane_port,
        "protected.tunnel.test",
        Some("Bearer wrong-token"),
    )
    .await;
    assert!(
        response.starts_with("HTTP/1.1 401"),
        "a wrong token must be refused, got: {response}"
    );

    // 4. The right token relays: proof the policy survived registration.
    let (response, _) = visitor_request(
        data_plane_port,
        "protected.tunnel.test",
        Some("Bearer visitor-s3cret"),
    )
    .await;
    // The status line's version is the *local target's* (it answers HTTP/1.0);
    // what matters here is that the relay happened instead of a denial.
    assert!(
        response.starts_with("HTTP/1.") && response.contains("200 OK"),
        "the right token must relay, got: {response}"
    );
    assert!(
        response.contains("HELLO-FROM-LOCAL"),
        "the relayed body must come from the local target: {response}"
    );

    agent.shutdown().await;
    let _ = shutdown_tx.send(());
    let _ = data_plane.await;
    target_task.abort();
}
