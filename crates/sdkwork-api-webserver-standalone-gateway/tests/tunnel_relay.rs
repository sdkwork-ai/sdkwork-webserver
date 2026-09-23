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

#[tokio::test]
async fn data_plane_relays_tunnel_domain_requests() {
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
    }]);
    let device = Device::new(
        DeviceId::parse("dev_dataplane").expect("valid id"),
        "data-plane-test",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", tunnel_port),
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
    }]);
    let device = Device::new(
        DeviceId::parse("dev_website").expect("valid id"),
        "website-entry-test",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", tunnel_port),
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
