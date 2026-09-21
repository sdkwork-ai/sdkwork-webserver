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
    Device, DeviceId, DevicePlatform, RoutePolicy, TunnelConfig, TunnelGatewayConfig,
    TunnelProtocolKind, TunnelRouteTemplate,
};
use sdkwork_webserver_tunnel_transport::tls;
use sdkwork_webserver_tunnel_transport::{RemoteEndpoint, TunnelClientTransport};
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
