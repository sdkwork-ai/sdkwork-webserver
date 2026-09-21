//! End-to-end tunnel integration tests (PRD §73, §74, §75).
//!
//! Real loopback: QUIC gateway + agent + local HTTP/TCP targets, covering
//! the visitor → gateway → tunnel → agent → local-target path, the
//! reconnect flow, and authentication rejection.

use std::sync::Arc;
use std::time::Duration;

use sdkwork_webserver_tunnel::agent::{
    AgentEvent, AgentRuntime, AgentRuntimeOptions, RouteReadiness,
};
use sdkwork_webserver_tunnel::gateway::{RelayVisitor, TunnelGateway, TunnelGatewayOptions};
use sdkwork_webserver_tunnel::metrics::TunnelMetrics;
use sdkwork_webserver_tunnel::security::TokenAuthenticator;
use sdkwork_webserver_tunnel_core::{
    Device, DeviceId, DevicePlatform, RouteId, TunnelConfig, TunnelProtocolKind,
    TunnelRouteTemplate,
};
use sdkwork_webserver_tunnel_transport::{tls, RemoteEndpoint};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const TOKEN: &str = "e2e-t0ken";

fn crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

fn init_logs() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
        )
        .with_test_writer()
        .try_init();
}

fn tls_material() -> (Vec<u8>, Vec<u8>, String) {
    let material = tls::generate_self_signed(&["localhost".to_owned(), "127.0.0.1".to_owned()])
        .expect("self-signed material");
    (
        material.cert_pem.into_bytes(),
        material.key_pem.into_bytes(),
        material.sha256,
    )
}

fn http_template(domain: &str) -> TunnelRouteTemplate {
    TunnelRouteTemplate {
        name: "web".to_owned(),
        protocol: TunnelProtocolKind::Http,
        domain: Some(domain.to_owned()),
        port: None,
        target: "127.0.0.1:0".to_owned(),
        policy: Some(sdkwork_webserver_tunnel_core::RoutePolicy {
            allow_public: true,
            ..sdkwork_webserver_tunnel_core::RoutePolicy::private()
        }),
    }
}

/// Spawns a one-shot HTTP responder that answers every request with
/// `HELLO` and records the first request line it received.
async fn spawn_http_target() -> (
    u16,
    Arc<std::sync::Mutex<String>>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("http target bind");
    let port = listener.local_addr().expect("addr").port();
    let seen = Arc::new(std::sync::Mutex::new(String::new()));
    let seen_task = seen.clone();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let seen_task = seen_task.clone();
            tokio::spawn(async move {
                let mut buffer = [0_u8; 1024];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                if let Some(first_line) = request.lines().next() {
                    *seen_task.lock().expect("seen lock") = first_line.to_owned();
                }
                let body = b"HELLO";
                let response = format!("HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.write_all(body).await;
                let _ = socket.flush().await;
            });
        }
    });
    (port, seen, task)
}

async fn wait_for_ready(agent: &mut AgentRuntime) -> Vec<RouteReadiness> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let Some(remaining) = deadline.checked_duration_since(tokio::time::Instant::now()) else {
            panic!("agent never became ready");
        };
        match tokio::time::timeout(remaining, agent.next_event()).await {
            Ok(Some(event)) => {
                eprintln!("agent event: {event:?}");
                if let AgentEvent::Ready { routes, rejected } = event {
                    assert!(rejected.is_empty(), "unexpected rejections: {rejected:?}");
                    return routes;
                }
            }
            Ok(None) => panic!("agent event channel closed before ready"),
            Err(_) => panic!("timed out waiting for agent ready"),
        }
    }
}

#[tokio::test]
async fn http_and_tcp_traffic_relay_end_to_end() {
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    // Gateway on an ephemeral QUIC port.
    let mut gateway_options = TunnelGatewayOptions::from_config(
        &TunnelConfig::disabled(),
        "127.0.0.1:0".parse().expect("bind"),
        cert_pem.clone(),
        key_pem.clone(),
    );
    gateway_options.authenticator = TokenAuthenticator::new(vec![TOKEN.to_owned()]);
    let gateway = TunnelGateway::spawn(gateway_options, None)
        .await
        .expect("gateway spawns");
    let quic_port = gateway.quic_port();

    // Local HTTP target.
    let (http_port, seen_http, http_task) = spawn_http_target().await;

    // Agent with one HTTP route pointing at the local target.
    let mut template = http_template("demo.tunnel.test");
    template.target = format!("127.0.0.1:{http_port}");
    let device = Device::new(
        DeviceId::parse("dev_e2e").expect("valid id"),
        "e2e-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint),
            ..tls::ClientTlsOptions::default()
        },
        device,
        token: TOKEN.to_owned(),
        routes: vec![template],
        network: Default::default(),
        timeouts: Default::default(),
        metrics: Arc::new(TunnelMetrics::new()),
    });
    let ready = wait_for_ready(&mut agent).await;
    assert_eq!(ready.len(), 1);
    assert_eq!(
        ready[0].public_url.as_deref(),
        Some("https://demo.tunnel.test")
    );

    // Visitor side: resolve the host through the gateway relay and speak
    // raw HTTP/1.0 over the tunnel.
    let shared = gateway.shared();
    let mut relayed = shared
        .connect_http_stream(
            "demo.tunnel.test",
            RelayVisitor {
                ip: "203.0.113.5".parse().expect("ip"),
                bearer: None,
            },
        )
        .await
        .expect("relay opens");
    relayed
        .write_all(b"GET /preview HTTP/1.0\r\nHost: demo.tunnel.test\r\n\r\n")
        .await
        .expect("request written");
    let mut response = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !response
        .windows(4)
        .any(|window| window == b"\r\n\r\nHELLO"[..4].split_at(0).0)
        && tokio::time::Instant::now() < deadline
    {
        let mut chunk = [0_u8; 512];
        match tokio::time::timeout(Duration::from_secs(5), relayed.read(&mut chunk)).await {
            Ok(Ok(0)) | Err(_) => break,
            Ok(Ok(read)) => response.extend_from_slice(&chunk[..read]),
            Ok(Err(error)) => panic!("relay read failed: {error}"),
        }
        if String::from_utf8_lossy(&response).contains("HELLO") {
            break;
        }
    }
    let response_text = String::from_utf8_lossy(&response).to_string();
    assert!(response_text.contains("200 OK"), "got: {response_text}");
    assert!(response_text.contains("HELLO"), "got: {response_text}");
    assert_eq!(
        seen_http.lock().expect("seen lock").as_str(),
        "GET /preview HTTP/1.0"
    );

    // Session visible on the gateway with the route attached.
    let status = gateway.service().status().await.expect("status");
    assert_eq!(status.sessions, 1);
    assert_eq!(status.routes, 1);
    let sessions = gateway.service().list_sessions().await.expect("sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(
        sessions[0].route_ids,
        vec![RouteId::parse("route_web").expect("valid id").to_string()]
    );

    agent.shutdown().await;
    http_task.abort();
    gateway.shutdown().await;
}

fn gateway_quic_port(gateway: &TunnelGateway) -> u16 {
    gateway.quic_port()
}

#[tokio::test]
async fn raw_client_through_real_gateway_reads_hello() {
    // Bisect inside this crate: a raw transport client against the real
    // TunnelGateway; the gateway must see the Hello bytes.
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();
    let mut gateway_options = TunnelGatewayOptions::from_config(
        &TunnelConfig::disabled(),
        "127.0.0.1:0".parse().expect("bind"),
        cert_pem,
        key_pem,
    );
    gateway_options.authenticator = TokenAuthenticator::new(vec![TOKEN.to_owned()]);
    let gateway = TunnelGateway::spawn(gateway_options, None)
        .await
        .expect("gateway spawns");
    let port = gateway.quic_port();

    use sdkwork_webserver_tunnel_transport::TunnelClientTransport;
    let client = sdkwork_webserver_tunnel_transport::QuicClientTransport::new(
        &sdkwork_webserver_tunnel_transport::tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint),
            ..sdkwork_webserver_tunnel_transport::tls::ClientTlsOptions::default()
        },
        sdkwork_webserver_tunnel_transport::TransportOptions::default(),
    )
    .expect("client");
    let connection = client
        .connect(
            &sdkwork_webserver_tunnel_transport::RemoteEndpoint::new("127.0.0.1", port),
            "sdkwork-tunnel-gateway",
        )
        .await
        .expect("connect");
    let mut stream = connection.open_stream().await.expect("control stream");
    let hello = sdkwork_webserver_tunnel_protocol::ControlMessage::Hello(
        sdkwork_webserver_tunnel_protocol::Hello {
            protocol_versions: vec![sdkwork_webserver_tunnel_protocol::ProtocolVersion::v1()],
            device_id: "dev_probe".to_owned(),
            device_name: "probe".to_owned(),
            platform: "linux".to_owned(),
        },
    );
    let authenticate = sdkwork_webserver_tunnel_protocol::ControlMessage::Authenticate(
        sdkwork_webserver_tunnel_protocol::Authenticate {
            token: TOKEN.to_owned(),
        },
    );
    let frame = hello.to_frame().expect("frame");
    let auth_frame = authenticate.to_frame().expect("auth frame");
    use tokio::io::AsyncWriteExt;
    stream.write_all(&frame).await.expect("hello written");
    stream.flush().await.expect("hello flushed");
    stream.write_all(&auth_frame).await.expect("auth written");
    stream.flush().await.expect("auth flushed");
    // If the gateway control loop is healthy it will answer with an
    // AuthResult; read it through the real protocol decode path.
    let reply = tokio::time::timeout(
        Duration::from_secs(5),
        sdkwork_webserver_tunnel_protocol::frame::read_frame(
            &mut *stream,
            &mut bytes::BytesMut::new(),
        ),
    )
    .await
    .expect("gateway replies within 5s")
    .expect("gateway reply frame");
    let message = sdkwork_webserver_tunnel_protocol::ControlMessage::from_frame(&reply)
        .expect("decode reply");
    assert!(
        matches!(
            message,
            sdkwork_webserver_tunnel_protocol::ControlMessage::AuthResult(_)
        ),
        "expected AuthResult, got {message:?}"
    );
    gateway.shutdown().await;
}

#[tokio::test]
async fn wrong_token_is_rejected_and_route_never_registers() {
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();
    let mut gateway_options = TunnelGatewayOptions::from_config(
        &TunnelConfig::disabled(),
        "127.0.0.1:0".parse().expect("bind"),
        cert_pem,
        key_pem,
    );
    gateway_options.authenticator = TokenAuthenticator::new(vec![TOKEN.to_owned()]);
    let gateway = TunnelGateway::spawn(gateway_options, None)
        .await
        .expect("gateway spawns");
    let quic_port = gateway_quic_port(&gateway);

    let (http_port, _seen, http_task) = spawn_http_target().await;
    let mut template = http_template("denied.tunnel.test");
    template.target = format!("127.0.0.1:{http_port}");
    let device = Device::new(
        DeviceId::parse("dev_bad").expect("valid id"),
        "bad-actor",
        DevicePlatform::Windows,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint),
            ..tls::ClientTlsOptions::default()
        },
        device,
        token: "WRONG".to_owned(),
        routes: vec![template],
        network: sdkwork_webserver_tunnel_core::TunnelNetworkConfig {
            reconnect: true,
            ..Default::default()
        },
        timeouts: Default::default(),
        metrics: Arc::new(TunnelMetrics::new()),
    });
    // The agent must see rejections and never reach Ready with routes.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
    let mut saw_rejection = false;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_secs(6), agent.next_event()).await {
            Ok(Some(AgentEvent::Ready { routes, .. })) if !routes.is_empty() => {
                panic!("rejected agent must not become ready with routes")
            }
            Ok(Some(AgentEvent::Disconnected { reason })) => {
                assert!(reason.contains("authentication"), "reason: {reason}");
                saw_rejection = true;
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => break,
        }
    }
    assert!(saw_rejection, "expected an authentication rejection event");
    let status = gateway.service().status().await.expect("status");
    assert_eq!(status.sessions, 0, "no session may survive");
    assert_eq!(status.routes, 0);
    let metrics = gateway.shared().metrics.snapshot();
    assert!(metrics.auth_failures >= 1, "auth failures counted");
    agent.shutdown().await;
    http_task.abort();
    gateway.shutdown().await;
}

#[tokio::test]
async fn tcp_route_relays_raw_bytes_through_gateway_port() {
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    // Local echo target.
    let echo = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("echo bind");
    let echo_port = echo.local_addr().expect("echo addr").port();
    let echo_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = echo.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buffer = [0_u8; 1024];
                loop {
                    let read = socket.read(&mut buffer).await.unwrap_or(0);
                    if read == 0 {
                        return;
                    }
                    if socket.write_all(&buffer[..read]).await.is_err() {
                        return;
                    }
                }
            });
        }
    });

    let mut gateway_options = TunnelGatewayOptions::from_config(
        &TunnelConfig::disabled(),
        "127.0.0.1:0".parse().expect("bind"),
        cert_pem.clone(),
        key_pem.clone(),
    );
    gateway_options.authenticator = TokenAuthenticator::new(vec![TOKEN.to_owned()]);
    let gateway = TunnelGateway::spawn(gateway_options, None)
        .await
        .expect("gateway spawns");
    let quic_port = gateway.quic_port();
    // Reserve the public gateway port for the TCP route.
    let public_port = available_port();

    let template = TunnelRouteTemplate {
        name: "ssh".to_owned(),
        protocol: TunnelProtocolKind::Tcp,
        domain: None,
        port: Some(public_port),
        target: format!("127.0.0.1:{echo_port}"),
        policy: Some(sdkwork_webserver_tunnel_core::RoutePolicy {
            allow_public: true,
            ..sdkwork_webserver_tunnel_core::RoutePolicy::private()
        }),
    };
    let device = Device::new(
        DeviceId::parse("dev_tcp").expect("valid id"),
        "tcp-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint),
            ..tls::ClientTlsOptions::default()
        },
        device,
        token: TOKEN.to_owned(),
        routes: vec![template],
        network: Default::default(),
        timeouts: Default::default(),
        metrics: Arc::new(TunnelMetrics::new()),
    });
    let ready = wait_for_ready(&mut agent).await;
    assert_eq!(ready.len(), 1);

    // Visitor connects to the gateway's public TCP port and echoes bytes.
    let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", public_port))
        .await
        .expect("visitor connect");
    visitor
        .write_all(b"ping-through-tunnel")
        .await
        .expect("ping written");
    let mut buffer = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut chunk = [0_u8; 256];
        loop {
            let read = visitor.read(&mut chunk).await.expect("echo read");
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            if buffer.len() >= b"ping-through-tunnel".len() {
                break;
            }
        }
    })
    .await
    .expect("echo within timeout");
    assert_eq!(buffer, b"ping-through-tunnel");

    // Gateway status shows the TCP route with its served port.
    let status = gateway.service().status().await.expect("status");
    assert_eq!(status.routes, 1);
    assert_eq!(status.ports, vec![public_port]);

    agent.shutdown().await;
    echo_task.abort();
    gateway.shutdown().await;
}

fn available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve port");
    listener.local_addr().expect("local addr").port()
}

#[tokio::test]
async fn udp_route_relays_datagrams_through_gateway() {
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    // Local UDP echo target.
    let echo = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("echo bind");
    let echo_port = echo.local_addr().expect("echo addr").port();
    let echo_task = tokio::spawn(async move {
        let mut buffer = [0_u8; 2048];
        loop {
            let Ok((size, peer)) = echo.recv_from(&mut buffer).await else {
                return;
            };
            let _ = echo.send_to(&buffer[..size], peer).await;
        }
    });

    let mut gateway_options = TunnelGatewayOptions::from_config(
        &TunnelConfig::disabled(),
        "127.0.0.1:0".parse().expect("bind"),
        cert_pem.clone(),
        key_pem.clone(),
    );
    gateway_options.authenticator = TokenAuthenticator::new(vec![TOKEN.to_owned()]);
    let gateway = TunnelGateway::spawn(gateway_options, None)
        .await
        .expect("gateway spawns");
    let quic_port = gateway.quic_port();
    let public_port = available_port();

    let template = TunnelRouteTemplate {
        name: "dns".to_owned(),
        protocol: TunnelProtocolKind::Udp,
        domain: None,
        port: Some(public_port),
        target: format!("127.0.0.1:{echo_port}"),
        policy: Some(sdkwork_webserver_tunnel_core::RoutePolicy {
            allow_public: true,
            ..sdkwork_webserver_tunnel_core::RoutePolicy::private()
        }),
    };
    let device = Device::new(
        DeviceId::parse("dev_udp").expect("valid id"),
        "udp-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint),
            ..tls::ClientTlsOptions::default()
        },
        device,
        token: TOKEN.to_owned(),
        routes: vec![template],
        network: Default::default(),
        timeouts: Default::default(),
        metrics: Arc::new(TunnelMetrics::new()),
    });
    let ready = wait_for_ready(&mut agent).await;
    assert_eq!(ready.len(), 1);

    // Visitor sends a datagram to the gateway UDP port; the echo target
    // replies through the tunnel.
    let visitor = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("visitor bind");
    let mut replies = Vec::new();
    for attempt in 0..5 {
        visitor
            .send_to(b"udp-ping", ("127.0.0.1", public_port))
            .await
            .expect("datagram sent");
        let mut buffer = [0_u8; 256];
        match tokio::time::timeout(Duration::from_secs(2), visitor.recv_from(&mut buffer)).await {
            Ok(Ok((size, _))) => {
                replies.extend_from_slice(&buffer[..size]);
                break;
            }
            _ if attempt < 4 => continue,
            other => panic!("no udp reply: {other:?}"),
        }
    }
    assert_eq!(replies, b"udp-ping");

    let status = gateway.service().status().await.expect("status");
    assert_eq!(status.routes, 1);
    assert_eq!(status.ports, vec![public_port]);

    agent.shutdown().await;
    echo_task.abort();
    gateway.shutdown().await;
}
