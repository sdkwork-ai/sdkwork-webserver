//! End-to-end tunnel integration tests (PRD §73, §74, §75).
//!
//! Real loopback: QUIC gateway + agent + local HTTP/TCP targets, covering
//! the visitor → gateway → tunnel → agent → local-target path, the
//! reconnect flow, and authentication rejection.

use std::sync::Arc;
use std::time::Duration;

use sdkwork_webserver_tunnel::agent::{
    AgentEvent, AgentRuntime, AgentRuntimeOptions, RouteReadiness, RouteRejection,
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
        proxy_protocol: None,
        locations: Vec::new(),
        ..TunnelRouteTemplate::default()
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

/// Waits for `Ready` without insisting that nothing was rejected.
///
/// Used by the quota tests: a rejected route is the *expected* answer there,
/// and a helper that panics on rejection would make the ceiling untestable.
async fn wait_for_ready_reporting_rejections(
    agent: &mut AgentRuntime,
) -> (Vec<RouteReadiness>, Vec<RouteRejection>) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let Some(remaining) = deadline.checked_duration_since(tokio::time::Instant::now()) else {
            panic!("agent never became ready");
        };
        match tokio::time::timeout(remaining, agent.next_event()).await {
            Ok(Some(AgentEvent::Ready { routes, rejected })) => return (routes, rejected),
            Ok(Some(_)) => continue,
            Ok(None) => panic!("agent event channel closed before ready"),
            Err(_) => panic!("timed out waiting for agent ready"),
        }
    }
}

#[tokio::test]
async fn route_annotations_travel_with_the_registration() {
    // FRP `annotations`: operator metadata that the gateway carries and lists
    // without interpreting. The point of the test is that it survives the wire
    // and the gateway's validation — and that the gateway bounds it, because
    // the map arrives from a peer.
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
    let quic_port = gateway.quic_port();
    let (http_port, _seen_http, http_task) = spawn_http_target().await;

    let mut template = http_template("annotated.tunnel.test");
    template.name = "annotated".to_owned();
    template.target = format!("127.0.0.1:{http_port}");
    template
        .annotations
        .insert("owner".to_owned(), "team-a".to_owned());
    template
        .annotations
        .insert("ticket".to_owned(), "OPS-1".to_owned());
    let device = Device::new(
        DeviceId::parse("dev_annotated").expect("valid id"),
        "annotated-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint),
            ..tls::ClientTlsOptions::default()
        },
        device,
        token: TOKEN.to_owned(),
        routes: vec![template],
        network: Default::default(),
        timeouts: Default::default(),
        metrics: std::sync::Arc::new(TunnelMetrics::new()),
    });
    let ready = wait_for_ready(&mut agent).await;
    assert_eq!(ready.len(), 1);

    // The gateway stored exactly what the operator declared.
    let routes = gateway.service().list_routes().await.expect("routes listing");
    let registered = routes
        .iter()
        .find(|route| route.name == "annotated")
        .expect("the route is registered");
    assert_eq!(
        registered.annotations.get("owner").map(String::as_str),
        Some("team-a")
    );
    assert_eq!(
        registered.annotations.get("ticket").map(String::as_str),
        Some("OPS-1")
    );

    agent.shutdown().await;
    http_task.abort();
    gateway.shutdown().await;
}

#[tokio::test]
async fn login_fail_exit_decides_whether_a_rejected_credential_stops_the_agent() {
    // FRP `loginFailExit`: a rejected credential is an operator error, not an
    // outage. With it on the agent reports why and stops; with it off (this
    // agent's default) it keeps retrying, because an agent is a service that
    // has to ride out a gateway problem.
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
    let quic_port = gateway.quic_port();
    let (http_port, _seen_http, http_task) = spawn_http_target().await;

    let agent_options = |login_fail_exit: bool| {
        let mut template = http_template("stopper.tunnel.test");
        template.target = format!("127.0.0.1:{http_port}");
        AgentRuntimeOptions {
            endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
            alternates: Vec::new(),
            login_fail_exit,
            tls: tls::ClientTlsOptions {
                pinned_server_sha256: Some(fingerprint.clone()),
                ..tls::ClientTlsOptions::default()
            },
            device: Device::new(
                DeviceId::parse("dev_stopper").expect("valid id"),
                "stopper",
                DevicePlatform::Linux,
            )
            .expect("valid device"),
            // A token the gateway does not accept.
            token: "not-the-token".to_owned(),
            routes: vec![template],
            network: Default::default(),
            timeouts: Default::default(),
            metrics: std::sync::Arc::new(TunnelMetrics::new()),
        }
    };

    // 1. `loginFailExit = true`: Disconnected carries the reason, then Stopped,
    //    and no reconnect is scheduled.
    let mut stopping = AgentRuntime::spawn(agent_options(true));
    let mut saw_disconnected = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .expect("the agent never stopped");
        match tokio::time::timeout(remaining, stopping.next_event()).await {
            Ok(Some(AgentEvent::Disconnected { reason })) => saw_disconnected = Some(reason),
            Ok(Some(AgentEvent::ReconnectingIn { .. })) => {
                panic!("a rejected credential must not schedule a retry when loginFailExit is set")
            }
            Ok(Some(AgentEvent::Stopped)) => break,
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => panic!("timed out waiting for the agent to stop"),
        }
    }
    let reason = saw_disconnected.expect("the operator is told why");
    assert!(
        reason.contains("authentication rejected") || reason.contains("rejected the session"),
        "the reason names the rejection: {reason}"
    );
    // Nothing further: the process is done, not retrying.
    let mut after_stop: Vec<String> = Vec::new();
    while let Ok(Some(event)) =
        tokio::time::timeout(Duration::from_millis(250), stopping.next_event()).await
    {
        after_stop.push(format!("{event:?}"));
    }
    assert!(after_stop.is_empty(), "no events after Stopped: {after_stop:?}");

    // 2. Default (switch off): the same rejection schedules a retry.
    let mut retrying = AgentRuntime::spawn(agent_options(false));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .expect("the agent never reported a retry");
        match tokio::time::timeout(remaining, retrying.next_event()).await {
            Ok(Some(AgentEvent::ReconnectingIn { after_ms })) => {
                assert!(after_ms > 0, "the backoff is a real delay");
                break;
            }
            Ok(Some(AgentEvent::Stopped)) => {
                panic!("without loginFailExit a rejected credential is ridden out, not fatal")
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("agent stopped reporting events"),
            Err(_) => panic!("timed out waiting for the retry"),
        }
    }

    stopping.shutdown().await;
    retrying.shutdown().await;
    http_task.abort();
    gateway.shutdown().await;
}

#[tokio::test]
async fn users_isolate_their_routes_and_their_quotas_are_enforced() {
    // FRP `user`, made enforceable: each team has its own token, its routes are
    // labelled with the user that token speaks for, its ceilings are counted per
    // user, and a machine cannot claim a team it holds no token for.
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    // Tokens come from the environment, as they always do; the variable names
    // are unique to this test so a parallel test cannot overwrite them.
    std::env::set_var("SDKWORK_TUNNEL_TEST_TEAM_A_TOKEN", "team-a-t0ken");
    std::env::set_var("SDKWORK_TUNNEL_TEST_TEAM_B_TOKEN", "team-b-t0ken");

    let config = TunnelConfig {
        enabled: true,
        gateway: Some(sdkwork_webserver_tunnel_core::TunnelGatewayConfig {
            users: vec![
                sdkwork_webserver_tunnel_core::TunnelUserConfig {
                    name: "team-a".to_owned(),
                    token_env: "SDKWORK_TUNNEL_TEST_TEAM_A_TOKEN".to_owned(),
                    // One route: enough to prove the ceiling.
                    max_routes: Some(1),
                    max_ports: Some(1),
                    max_devices: Some(2),
                },
                sdkwork_webserver_tunnel_core::TunnelUserConfig {
                    name: "team-b".to_owned(),
                    token_env: "SDKWORK_TUNNEL_TEST_TEAM_B_TOKEN".to_owned(),
                    max_routes: None,
                    max_ports: None,
                    max_devices: None,
                },
            ],
            ..sdkwork_webserver_tunnel_core::TunnelGatewayConfig::default()
        }),
        ..TunnelConfig::disabled()
    };
    config.validate().expect("the user table validates");

    let mut gateway_options = TunnelGatewayOptions::from_config(
        &config,
        "127.0.0.1:0".parse().expect("bind"),
        cert_pem.clone(),
        key_pem.clone(),
    );
    gateway_options.authenticator =
        sdkwork_webserver_tunnel::security::TokenAuthenticator::with_users(
            vec![TOKEN.to_owned()],
            vec![
                ("team-a".to_owned(), "team-a-t0ken".to_owned()),
                ("team-b".to_owned(), "team-b-t0ken".to_owned()),
            ],
        );
    let gateway = TunnelGateway::spawn(gateway_options, None)
        .await
        .expect("gateway spawns");
    let quic_port = gateway.quic_port();

    let (http_port, _seen_http, http_task) = spawn_http_target().await;

    // Team A: the second route exceeds its one-route budget.
    let mut first = http_template("a.tunnel.test");
    first.target = format!("127.0.0.1:{http_port}");
    let mut second = http_template("a2.tunnel.test");
    // A distinct template name: the route id is derived from it, and two
    // templates sharing a name are one route being re-registered — a
    // replacement, which no ceiling should refuse.
    second.name = "web-a2".to_owned();
    second.target = format!("127.0.0.1:{http_port}");
    let team_a = Device::new(
        DeviceId::parse("dev_team_a").expect("valid id"),
        "team-a-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device")
    .with_user(Some("team-a".to_owned()))
    .expect("valid user");
    let mut agent_a = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint.clone()),
            ..tls::ClientTlsOptions::default()
        },
        device: team_a,
        token: "team-a-t0ken".to_owned(),
        routes: vec![first, second],
        network: Default::default(),
        timeouts: Default::default(),
        metrics: std::sync::Arc::new(TunnelMetrics::new()),
    });
    let (accepted, rejected) = wait_for_ready_reporting_rejections(&mut agent_a).await;
    assert_eq!(accepted.len(), 1, "team-a registers exactly one route");
    assert_eq!(rejected.len(), 1, "the second route exceeds maxRoutes=1");
    assert!(
        rejected[0].error.contains("maxRoutes"),
        "the refusal names the ceiling: {}",
        rejected[0].error
    );

    // Team B: an independent budget, coexisting with team A's route.
    let mut team_b_first = http_template("b.tunnel.test");
    team_b_first.target = format!("127.0.0.1:{http_port}");
    let mut team_b_second = http_template("b2.tunnel.test");
    team_b_second.name = "web-b2".to_owned();
    team_b_second.target = format!("127.0.0.1:{http_port}");
    let team_b = Device::new(
        DeviceId::parse("dev_team_b").expect("valid id"),
        "team-b-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device")
    .with_user(Some("team-b".to_owned()))
    .expect("valid user");
    let mut agent_b = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint.clone()),
            ..tls::ClientTlsOptions::default()
        },
        device: team_b,
        token: "team-b-t0ken".to_owned(),
        routes: vec![team_b_first, team_b_second],
        network: Default::default(),
        timeouts: Default::default(),
        metrics: std::sync::Arc::new(TunnelMetrics::new()),
    });
    let ready_b = wait_for_ready(&mut agent_b).await;
    assert_eq!(ready_b.len(), 2, "team-b has no per-user ceiling");

    let status = gateway.service().status().await.expect("status");
    assert_eq!(status.routes, 3, "one route from team-a, two from team-b");

    // A machine claiming another team while holding team A's token is refused:
    // the claim is checked against the credential, never trusted.
    let impostor = Device::new(
        DeviceId::parse("dev_impostor").expect("valid id"),
        "impostor",
        DevicePlatform::Linux,
    )
    .expect("valid device")
    .with_user(Some("team-b".to_owned()))
    .expect("valid user");
    let mut refused = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint),
            ..tls::ClientTlsOptions::default()
        },
        device: impostor,
        token: "team-a-t0ken".to_owned(),
        routes: vec![{
            // A routable target: the refusal under test must come from the
            // credential check, not from a template the agent rejects locally.
            let mut template = http_template("impostor.tunnel.test");
            template.target = format!("127.0.0.1:{http_port}");
            template
        }],
        network: Default::default(),
        timeouts: Default::default(),
        metrics: std::sync::Arc::new(TunnelMetrics::new()),
    });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .expect("the impostor was never refused");
        match tokio::time::timeout(remaining, refused.next_event()).await {
            Ok(Some(AgentEvent::Ready { .. })) => {
                panic!("a machine may not claim a user its token does not speak for")
            }
            Ok(Some(AgentEvent::Disconnected { reason })) => {
                assert!(
                    reason.contains("authentication rejected")
                        || reason.contains("invalid credentials"),
                    "the refusal explains itself: {reason}"
                );
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("agent stopped before the refusal"),
            Err(_) => panic!("timed out waiting for the refusal"),
        }
    }

    agent_a.shutdown().await;
    agent_b.shutdown().await;
    refused.shutdown().await;
    http_task.abort();
    gateway.shutdown().await;
}

#[tokio::test]
async fn an_unreachable_primary_fails_over_to_the_next_gateway_address() {
    // FRP's ordered `serverAddr` list: the agent tries the primary, and when
    // that attempt never connects it moves to the next address instead of
    // retrying a dead port until an operator intervenes.
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

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

    // A *bound* UDP socket that never answers: holding it means the kernel
    // does not send an ICMP port-unreachable, so the primary fails by
    // handshake timeout — the same shape as a gateway that is down behind a
    // firewall, and deterministic across platforms.
    let dead_socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("reserve a dead udp port");
    let dead_port = dead_socket.local_addr().expect("dead addr").port();
    let (http_port, _seen_http, _http_task) = spawn_http_target().await;
    let mut template = http_template("failover.tunnel.test");
    template.target = format!("127.0.0.1:{http_port}");

    let device = Device::new(
        DeviceId::parse("dev_failover").expect("valid id"),
        "failover-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    // Short handshake budget so the failing attempt is quick; the heartbeat
    // keeps the established session alive past the idle window.
    let mut timeouts = TunnelConfig::disabled().timeout_or_default();
    timeouts.idle = 3;
    let mut network = TunnelConfig::disabled().network_or_default();
    network.heartbeat_interval_secs = 1;
    network.initial_backoff_secs = 1;
    network.max_backoff_secs = 1;
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", dead_port),
        alternates: vec![RemoteEndpoint::new("127.0.0.1", quic_port)],
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint),
            ..tls::ClientTlsOptions::default()
        },
        device,
        token: TOKEN.to_owned(),
        routes: vec![template],
        network,
        timeouts,
        metrics: std::sync::Arc::new(TunnelMetrics::new()),
    });

    // Observe the order: the primary is announced first, the fallback second,
    // and the session becomes ready on the fallback.
    let fallback = format!("127.0.0.1:{quic_port}");
    let primary = format!("127.0.0.1:{dead_port}");
    let mut attempts: Vec<String> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .expect("agent never became ready through the fallback");
        match tokio::time::timeout(remaining, agent.next_event()).await {
            Ok(Some(AgentEvent::Connecting { endpoint })) => attempts.push(endpoint),
            Ok(Some(AgentEvent::Ready { rejected, .. })) => {
                assert!(rejected.is_empty(), "unexpected rejections: {rejected:?}");
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("agent stopped before ready"),
            Err(_) => panic!("timed out waiting for the fallback to connect"),
        }
    }

    assert_eq!(
        attempts.first(),
        Some(&primary),
        "the primary must be tried first: {attempts:?}"
    );
    assert!(
        attempts.iter().any(|endpoint| endpoint == &fallback),
        "the fallback address must be tried after the primary failed: {attempts:?}"
    );

    agent.shutdown().await;
    gateway.shutdown().await;
    drop(dead_socket);
}

#[tokio::test]
async fn a_required_heartbeat_reauth_is_advertised_and_enforced() {
    // FRP `auth.additionalScopes = ["HeartBeats"]`: the gateway advertises the
    // scope, the real agent answers it, and a peer that stops re-presenting the
    // token is refused. All three halves matter — an advertisement nobody
    // enforces is decoration, and an enforcement nobody is told about locks
    // every agent out.
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();
    let config = TunnelConfig {
        enabled: true,
        gateway: Some(sdkwork_webserver_tunnel_core::TunnelGatewayConfig {
            auth_scopes: vec![sdkwork_webserver_tunnel_core::AuthScope::Heartbeats],
            ..sdkwork_webserver_tunnel_core::TunnelGatewayConfig::default()
        }),
        ..TunnelConfig::disabled()
    };
    config.validate().expect("the scope configuration is valid");

    let mut gateway_options = TunnelGatewayOptions::from_config(
        &config,
        "127.0.0.1:0".parse().expect("bind"),
        cert_pem.clone(),
        key_pem.clone(),
    );
    gateway_options.authenticator = TokenAuthenticator::new(vec![TOKEN.to_owned()]);
    let gateway = TunnelGateway::spawn(gateway_options, None)
        .await
        .expect("gateway spawns");
    let quic_port = gateway.quic_port();

    // 1. A real agent: it learns the requirement from `AuthResult` and keeps
    //    its session alive, which is what makes the scope usable at all.
    let (http_port, _seen_http, http_task) = spawn_http_target().await;
    let mut template = http_template("reauth.tunnel.test");
    template.target = format!("127.0.0.1:{http_port}");
    let device = Device::new(
        DeviceId::parse("dev_reauth").expect("valid id"),
        "reauth-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut network = TunnelConfig::disabled().network_or_default();
    // A one-second heartbeat keeps the test inside a second or two.
    network.heartbeat_interval_secs = 1;
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint.clone()),
            ..tls::ClientTlsOptions::default()
        },
        device,
        token: TOKEN.to_owned(),
        routes: vec![template],
        network,
        timeouts: Default::default(),
        metrics: std::sync::Arc::new(TunnelMetrics::new()),
    });
    let ready = wait_for_ready(&mut agent).await;
    assert_eq!(ready.len(), 1);
    // Outlive several heartbeat cycles: without a working re-presentation the
    // gateway would have closed the session by now.
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(
        gateway.service().status().await.expect("status").sessions >= 1,
        "the agent that re-authenticates must stay connected"
    );

    // 2. A raw peer that authenticates and then pings without re-presenting
    //    the token is refused.
    {
        use sdkwork_webserver_tunnel_transport::TunnelClientTransport;
        let client = sdkwork_webserver_tunnel_transport::QuicClientTransport::new(
            &sdkwork_webserver_tunnel_transport::tls::ClientTlsOptions {
                pinned_server_sha256: Some(fingerprint.clone()),
                ..sdkwork_webserver_tunnel_transport::tls::ClientTlsOptions::default()
            },
            sdkwork_webserver_tunnel_transport::TransportOptions::default(),
        )
        .expect("client");
        let connection = client
            .connect(
                &sdkwork_webserver_tunnel_transport::RemoteEndpoint::new("127.0.0.1", quic_port),
                "sdkwork-tunnel-gateway",
            )
            .await
            .expect("connect");
        let mut stream = connection.open_stream().await.expect("control stream");
        let hello = sdkwork_webserver_tunnel_protocol::ControlMessage::Hello(
            sdkwork_webserver_tunnel_protocol::Hello {
                protocol_versions: vec![
                    sdkwork_webserver_tunnel_protocol::ProtocolVersion::v1(),
                ],
                device_id: "dev_raw_reauth".to_owned(),
                device_name: "raw-reauth".to_owned(),
                platform: "linux".to_owned(),
                user: None,
            },
        );
        let authenticate = sdkwork_webserver_tunnel_protocol::ControlMessage::Authenticate(
            sdkwork_webserver_tunnel_protocol::Authenticate {
                token: TOKEN.to_owned(),
            },
        );
        use tokio::io::AsyncWriteExt;
        stream
            .write_all(&hello.to_frame().expect("frame"))
            .await
            .expect("hello written");
        stream
            .write_all(&authenticate.to_frame().expect("frame"))
            .await
            .expect("auth written");
        stream.flush().await.expect("flushed");
        let reply = tokio::time::timeout(
            Duration::from_secs(5),
            sdkwork_webserver_tunnel_protocol::frame::read_frame(
                &mut *stream,
                &mut bytes::BytesMut::new(),
            ),
        )
        .await
        .expect("auth result within 5s")
        .expect("auth result read");
        let decoded = sdkwork_webserver_tunnel_protocol::ControlMessage::from_frame(&reply)
            .expect("decodes");
        match decoded {
            sdkwork_webserver_tunnel_protocol::ControlMessage::AuthResult(result) => {
                assert!(result.ok);
                assert_eq!(
                    result.required_scopes,
                    Some(vec!["heartbeats".to_owned()]),
                    "the gateway must advertise what it enforces"
                );
            }
            other => panic!("unexpected handshake reply: {other:?}"),
        }

        // A heartbeat with no re-presentation: the reply is an authentication
        // error, and the session does not survive it.
        let heartbeat =
            sdkwork_webserver_tunnel_protocol::ControlMessage::Heartbeat;
        stream
            .write_all(&heartbeat.to_frame().expect("frame"))
            .await
            .expect("heartbeat written");
        stream.flush().await.expect("flushed");
        let reply = tokio::time::timeout(
            Duration::from_secs(5),
            sdkwork_webserver_tunnel_protocol::frame::read_frame(
                &mut *stream,
                &mut bytes::BytesMut::new(),
            ),
        )
        .await
        .expect("the refusal arrives")
        .expect("reply read");
        let decoded = sdkwork_webserver_tunnel_protocol::ControlMessage::from_frame(&reply)
            .expect("decodes");
        match decoded {
            sdkwork_webserver_tunnel_protocol::ControlMessage::Error(error) => {
                assert_eq!(
                    error.code,
                    sdkwork_webserver_tunnel_protocol::ErrorCode::AuthenticationFailed
                );
                assert!(
                    !error.message.contains(TOKEN),
                    "the refusal never echoes the token: {}",
                    error.message
                );
            }
            other => panic!("expected an authentication error, got {other:?}"),
        }
    }

    agent.shutdown().await;
    http_task.abort();
    gateway.shutdown().await;
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
        alternates: Vec::new(),
        login_fail_exit: false,
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
            "",
            RelayVisitor {
                ip: "203.0.113.5".parse().expect("ip"),
                port: 0,
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
            user: None,
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
        alternates: Vec::new(),
        login_fail_exit: false,
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
        proxy_protocol: None,
        locations: Vec::new(),
        ..TunnelRouteTemplate::default()
    };
    let device = Device::new(
        DeviceId::parse("dev_tcp").expect("valid id"),
        "tcp-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
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

#[tokio::test]
async fn proxy_bind_addr_places_route_listeners_on_the_configured_address() {
    // FRP `proxyBindAddr`: a host with several addresses decides where
    // agent-registered ports appear. The pair of assertions is the proof —
    // reachable on the configured address, *not* reachable on another one that
    // a wildcard bind would have accepted.
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

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

    let config = TunnelConfig {
        enabled: true,
        gateway: Some(sdkwork_webserver_tunnel_core::TunnelGatewayConfig {
            proxy_bind_addr: "127.0.0.1".to_owned(),
            ..sdkwork_webserver_tunnel_core::TunnelGatewayConfig::default()
        }),
        ..TunnelConfig::disabled()
    };
    config.validate().expect("the bind address validates");

    let mut gateway_options = TunnelGatewayOptions::from_config(
        &config,
        "127.0.0.1:0".parse().expect("bind"),
        cert_pem,
        key_pem,
    );
    gateway_options.authenticator = TokenAuthenticator::new(vec![TOKEN.to_owned()]);
    let gateway = TunnelGateway::spawn(gateway_options, None)
        .await
        .expect("gateway spawns");
    let quic_port = gateway.quic_port();
    let public_port = available_port();

    let template = TunnelRouteTemplate {
        name: "bound".to_owned(),
        protocol: TunnelProtocolKind::Tcp,
        domain: None,
        port: Some(public_port),
        target: format!("127.0.0.1:{echo_port}"),
        policy: Some(sdkwork_webserver_tunnel_core::RoutePolicy {
            allow_public: true,
            ..sdkwork_webserver_tunnel_core::RoutePolicy::private()
        }),
        proxy_protocol: None,
        locations: Vec::new(),
        ..TunnelRouteTemplate::default()
    };
    let device = Device::new(
        DeviceId::parse("dev_bound").expect("valid id"),
        "bound-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
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

    // 1. The configured address serves the route.
    let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", public_port))
        .await
        .expect("the configured address accepts the visitor");
    visitor
        .write_all(b"bound-address")
        .await
        .expect("ping written");
    let mut buffer = [0_u8; 32];
    let read = tokio::time::timeout(Duration::from_secs(5), visitor.read(&mut buffer))
        .await
        .expect("echo within timeout")
        .expect("echo read");
    assert_eq!(&buffer[..read], b"bound-address");

    // 2. Another address on the same host does not: a wildcard bind would have
    //    accepted here, which is exactly what the knob prevents.
    let refused = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::TcpStream::connect(("127.0.0.2", public_port)),
    )
    .await
    .expect("the probe returns within the timeout");
    assert!(
        refused.is_err(),
        "a listener bound to 127.0.0.1 must not answer on 127.0.0.2"
    );

    agent.shutdown().await;
    echo_task.abort();
    gateway.shutdown().await;
}

#[tokio::test]
async fn dial_server_local_ip_decides_the_source_of_target_dials() {
    // FRP `transport.dialServerLocalIP`: a multi-homed agent chooses which of
    // its addresses the local service sees. The proof is the peer address at
    // the target — the same route, run twice, reaching the service as two
    // different clients.
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    // A target that reports who connected.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("target bind");
    let target_port = listener.local_addr().expect("target addr").port();
    let (peer_tx, mut peer_rx) = tokio::sync::mpsc::unbounded_channel();
    let target_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, peer)) = listener.accept().await else {
                return;
            };
            let _ = peer_tx.send(peer);
            tokio::spawn(async move {
                let mut buffer = [0_u8; 256];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                if read > 0 {
                    let _ = socket.write_all(&buffer[..read]).await;
                }
            });
        }
    });

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
    let quic_port = gateway.quic_port();

    // Two sessions from the same machine: one letting the kernel choose, one
    // pinned to another address of the loopback range.
    let mut observed = Vec::new();
    for (index, source) in [None, Some("127.0.0.2".parse::<std::net::IpAddr>().expect("ip"))]
        .into_iter()
        .enumerate()
    {
        let public_port = available_port();
        let template = TunnelRouteTemplate {
            name: format!("dialer-{index}"),
            protocol: TunnelProtocolKind::Tcp,
            domain: None,
            port: Some(public_port),
            target: format!("127.0.0.1:{target_port}"),
            policy: Some(sdkwork_webserver_tunnel_core::RoutePolicy {
                allow_public: true,
                ..sdkwork_webserver_tunnel_core::RoutePolicy::private()
            }),
            proxy_protocol: None,
            locations: Vec::new(),
            ..TunnelRouteTemplate::default()
        };
        let mut network = TunnelConfig::disabled().network_or_default();
        network.dial_local_addr = source;
        let device = Device::new(
            DeviceId::parse(&format!("dev_dialer_{index}")).expect("valid id"),
            "dialer",
            DevicePlatform::Linux,
        )
        .expect("valid device");
        let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
            endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
            alternates: Vec::new(),
            login_fail_exit: false,
            tls: tls::ClientTlsOptions {
                pinned_server_sha256: Some(fingerprint.clone()),
                ..tls::ClientTlsOptions::default()
            },
            device,
            token: TOKEN.to_owned(),
            routes: vec![template],
            network,
            timeouts: Default::default(),
            metrics: Arc::new(TunnelMetrics::new()),
        });
        let ready = wait_for_ready(&mut agent).await;
        assert_eq!(ready.len(), 1);

        let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", public_port))
            .await
            .expect("visitor connect");
        visitor.write_all(b"who-am-i").await.expect("written");
        let mut echo = [0_u8; 16];
        tokio::time::timeout(Duration::from_secs(5), visitor.read(&mut echo))
            .await
            .expect("echo within timeout")
            .expect("echo read");

        let peer = tokio::time::timeout(Duration::from_secs(5), peer_rx.recv())
            .await
            .expect("the target is reached")
            .expect("the target reports its peer");
        observed.push((source, peer.ip()));

        agent.shutdown().await;
    }

    assert_eq!(
        observed[0].1,
        "127.0.0.1".parse::<std::net::IpAddr>().expect("ip"),
        "without the knob the kernel picks the egress address"
    );
    assert_eq!(
        observed[1].1,
        "127.0.0.2".parse::<std::net::IpAddr>().expect("ip"),
        "with the knob the target sees the configured source address"
    );

    target_task.abort();
    gateway.shutdown().await;
}

/// A stub DNS server answering one name with one address (FRP `dnsServer`).
///
/// Answers a single `A` question and ignores everything else, which is enough
/// to prove that the agent used *this* resolver: the name it is asked for does
/// not exist in any system resolver.
async fn spawn_stub_dns_server(answer: Option<std::net::Ipv4Addr>) -> (u16, tokio::task::JoinHandle<()>) {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("dns bind");
    let port = socket.local_addr().expect("dns addr").port();
    let task = tokio::spawn(async move {
        let mut buffer = [0_u8; 512];
        loop {
            let Ok((read, peer)) = socket.recv_from(&mut buffer).await else {
                return;
            };
            if read < 12 {
                continue;
            }
            let id = [buffer[0], buffer[1]];
            // Echo the question back, then answer it — or fail it.
            let mut reply = Vec::from(&buffer[..read]);
            reply[2] = 0x81;
            reply[3] = match answer {
                Some(_) => 0x80,
                // NXDOMAIN
                None => 0x83,
            };
            reply[6] = 0x00;
            reply[7] = u8::from(answer.is_some());
            reply[8] = 0;
            reply[9] = 0;
            reply[10] = 0;
            reply[11] = 0;
            if let Some(address) = answer {
                // Name pointer to the question, type A, class IN, TTL 0, rdlen 4.
                reply.extend_from_slice(&[0xC0, 0x0C]);
                reply.extend_from_slice(&1_u16.to_be_bytes());
                reply.extend_from_slice(&1_u16.to_be_bytes());
                reply.extend_from_slice(&0_u32.to_be_bytes());
                reply.extend_from_slice(&4_u16.to_be_bytes());
                reply.extend_from_slice(&address.octets());
            }
            debug_assert_eq!(&reply[0..2], &id);
            let _ = socket.send_to(&reply, peer).await;
        }
    });
    (port, task)
}

#[tokio::test]
async fn dns_server_resolves_the_gateway_name_the_system_cannot() {
    // FRP `dnsServer`: a split-horizon or offline network publishes the gateway
    // name in its own resolver. The name below exists nowhere else, so a
    // successful session *is* the proof that the configured server was used —
    // and the NXDOMAIN case proves the agent does not quietly fall back to the
    // system resolver.
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
    let quic_port = gateway.quic_port();
    let (http_port, _seen_http, http_task) = spawn_http_target().await;

    let agent_options = |dns_port: u16| {
        let mut template = http_template("resolved.tunnel.test");
        template.target = format!("127.0.0.1:{http_port}");
        let mut network = TunnelConfig::disabled().network_or_default();
        network.dns_server = Some(format!("127.0.0.1:{dns_port}"));
        AgentRuntimeOptions {
            // A name no system resolver knows.
            endpoint: RemoteEndpoint::new("gw.tunnel.test", quic_port),
            alternates: Vec::new(),
            login_fail_exit: false,
            tls: tls::ClientTlsOptions {
                pinned_server_sha256: Some(fingerprint.clone()),
                ..tls::ClientTlsOptions::default()
            },
            device: Device::new(
                DeviceId::parse("dev_dns").expect("valid id"),
                "dns-runner",
                DevicePlatform::Linux,
            )
            .expect("valid device"),
            token: TOKEN.to_owned(),
            routes: vec![template],
            network,
            timeouts: Default::default(),
            metrics: Arc::new(TunnelMetrics::new()),
        }
    };

    // 1. A resolver that answers: the session comes up.
    let (answering_port, answering_task) =
        spawn_stub_dns_server(Some(std::net::Ipv4Addr::LOCALHOST)).await;
    let mut agent = AgentRuntime::spawn(agent_options(answering_port));
    let ready = wait_for_ready(&mut agent).await;
    assert_eq!(ready.len(), 1, "the gateway name resolved through dnsServer");
    agent.shutdown().await;
    answering_task.abort();

    // 2. A resolver that says NXDOMAIN: the agent reports an address-related
    //    failure and never reaches the gateway (no silent system fallback).
    let (failing_port, failing_task) = spawn_stub_dns_server(None).await;
    let mut refused = AgentRuntime::spawn(agent_options(failing_port));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .expect("the agent never reported the failure");
        match tokio::time::timeout(remaining, refused.next_event()).await {
            Ok(Some(AgentEvent::Connected { .. })) | Ok(Some(AgentEvent::Ready { .. })) => {
                panic!("NXDOMAIN must not fall back to the system resolver")
            }
            Ok(Some(AgentEvent::Disconnected { reason })) => {
                assert!(
                    reason.contains("NXDOMAIN") || reason.contains("dnsServer"),
                    "the failure names the resolver: {reason}"
                );
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("agent stopped reporting events"),
            Err(_) => panic!("timed out waiting for the resolution failure"),
        }
    }
    refused.shutdown().await;
    failing_task.abort();

    http_task.abort();
    gateway.shutdown().await;
}

/// Hands out a free TCP port, never twice in one test process.
///
/// Asking the OS for port 0 and dropping the listener answers "free *now*",
/// which two tests running in parallel can answer identically — one of them
/// then fails when the other binds first. Candidates therefore come from a
/// per-process counter, each verified by an actual bind; when that window is
/// unusable (Windows excludes whole port ranges for Hyper-V/WSL, so a range can
/// be unbindable end to end) the OS allocator answers instead, and every port
/// handed out is remembered so the same number never comes back twice.
fn available_port() -> u16 {
    static NEXT_OFFSET: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    // A per-process base keeps concurrent *test binaries* (e.g. the gateway
    // and tunnel e2e suites) out of each other's range as well.
    let base = 20_000 + (std::process::id() % 20_000) as u16;
    for _ in 0..200 {
        let offset = NEXT_OFFSET.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as u16;
        let candidate = match base.checked_add(offset) {
            Some(candidate) if candidate > 10_000 => candidate,
            _ => break,
        };
        if !handed_out_tcp(&candidate)
            && std::net::TcpListener::bind(("127.0.0.1", candidate)).is_ok()
        {
            return candidate;
        }
    }
    // Fall back to the OS allocator, still guaranteeing uniqueness in-process.
    for _ in 0..1000 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);
        if !handed_out_tcp(&port) {
            return port;
        }
    }
    panic!("no free TCP port could be reserved");
}

/// Reserves a free **UDP** port.
///
/// TCP and UDP port spaces are independent, so a TCP reservation says nothing
/// about whether the UDP bind will succeed; the allocation rule is otherwise
/// the same as [`available_port`].
fn available_udp_port() -> u16 {
    static NEXT_OFFSET: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let base = 40_000 + (std::process::id() % 15_000) as u16;
    for _ in 0..200 {
        let offset = NEXT_OFFSET.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as u16;
        let candidate = match base.checked_add(offset) {
            Some(candidate) if candidate > 10_000 => candidate,
            _ => break,
        };
        if !handed_out_udp(&candidate)
            && std::net::UdpSocket::bind(("127.0.0.1", candidate)).is_ok()
        {
            return candidate;
        }
    }
    for _ in 0..1000 {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("reserve udp port");
        let port = socket.local_addr().expect("local addr").port();
        drop(socket);
        if !handed_out_udp(&port) {
            return port;
        }
    }
    panic!("no free UDP port could be reserved");
}

/// Records a handed-out TCP port; false when it was already taken.
fn handed_out_tcp(port: &u16) -> bool {
    use std::sync::{Mutex, OnceLock};
    static PORTS: OnceLock<Mutex<std::collections::HashSet<u16>>> = OnceLock::new();
    let mut ports = PORTS
        .get_or_init(|| Mutex::new(std::collections::HashSet::new()))
        .lock()
        .expect("port registry lock");
    !ports.insert(*port)
}

/// Records a handed-out UDP port; false when it was already taken.
fn handed_out_udp(port: &u16) -> bool {
    use std::sync::{Mutex, OnceLock};
    static PORTS: OnceLock<Mutex<std::collections::HashSet<u16>>> = OnceLock::new();
    let mut ports = PORTS
        .get_or_init(|| Mutex::new(std::collections::HashSet::new()))
        .lock()
        .expect("port registry lock");
    !ports.insert(*port)
}

/// A secret **datagram** route (FRP `sudp`): the provider's UDP service is
/// reachable through the visitor's local UDP port and nowhere else.
#[tokio::test]
async fn a_visitor_runtime_publishes_a_secret_datagram_route() {
    use sdkwork_webserver_tunnel::visitor::{
        VisitorEntry, VisitorEvent, VisitorRuntime, VisitorRuntimeOptions,
    };

    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    const SECRET_ENV: &str = "SDKWORK_TUNNEL_TEST_SECRET_SUDP";
    const SECRET: &str = "sudp-key-789";
    std::env::set_var(SECRET_ENV, SECRET);

    // The provider's private datagram service: echoes every datagram back with
    // a marker so the test can tell a round trip from a local echo.
    let service = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("service bind");
    let service_port = service.local_addr().expect("service addr").port();
    let service_task = tokio::spawn(async move {
        let mut buffer = [0_u8; 2048];
        loop {
            let Ok((length, peer)) = service.recv_from(&mut buffer).await else {
                return;
            };
            let mut reply = b"reply:".to_vec();
            reply.extend_from_slice(&buffer[..length]);
            let _ = service.send_to(&reply, peer).await;
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

    let provider_template = TunnelRouteTemplate {
        name: "private-dns".to_owned(),
        protocol: TunnelProtocolKind::Sudp,
        target: format!("127.0.0.1:{service_port}"),
        secret_key_env: Some(SECRET_ENV.to_owned()),
        ..TunnelRouteTemplate::default()
    };
    // A datagram secret route is a template the agent accepts: the protocol,
    // the name matcher and the udp target all agree.
    provider_template
        .validate()
        .expect("sudp template validates");
    let mut provider = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint.clone()),
            ..tls::ClientTlsOptions::default()
        },
        device: Device::new(
            DeviceId::parse("dev_sudp_provider").expect("valid id"),
            "provider",
            DevicePlatform::Linux,
        )
        .expect("valid device"),
        token: TOKEN.to_owned(),
        routes: vec![provider_template],
        network: Default::default(),
        timeouts: Default::default(),
        metrics: Arc::new(TunnelMetrics::new()),
    });
    assert_eq!(wait_for_ready(&mut provider).await.len(), 1);

    let mut visitor = VisitorRuntime::spawn(VisitorRuntimeOptions::from_config(
        &TunnelConfig::disabled(),
        RemoteEndpoint::new("127.0.0.1", quic_port),
        tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint),
            ..tls::ClientTlsOptions::default()
        },
        Device::new(
            DeviceId::parse("dev_sudp_visitor").expect("valid id"),
            "visitor",
            DevicePlatform::Linux,
        )
        .expect("valid device"),
        TOKEN.to_owned(),
        vec![VisitorEntry::loopback_datagram("private-dns", SECRET, 0)],
        Arc::new(TunnelMetrics::new()),
    ));
    let local_port = loop {
        match tokio::time::timeout(Duration::from_secs(10), visitor.next_event()).await {
            Ok(Some(VisitorEvent::Listening { name, local_addr })) => {
                assert_eq!(name, "private-dns");
                break local_addr.port();
            }
            Ok(Some(VisitorEvent::BindFailed { name, error })) => {
                panic!("visitor failed to bind {name}: {error}")
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("the visitor runtime ended before it listened"),
            Err(_) => panic!("the visitor runtime never opened its local port"),
        }
    };

    // A local datagram client reaches the provider's private service.
    let client = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("client bind");
    client
        .send_to(b"datagram-one", ("127.0.0.1", local_port))
        .await
        .expect("datagram sent");
    let mut buffer = [0_u8; 2048];
    let (length, _) = tokio::time::timeout(Duration::from_secs(10), client.recv_from(&mut buffer))
        .await
        .expect("the provider's datagram service answers")
        .expect("reply read");
    assert_eq!(&buffer[..length], b"reply:datagram-one");

    // The same client keeps working on the same tunnel stream, and a second
    // client gets its own (FRP `sudp` keeps one stream per local client).
    client
        .send_to(b"datagram-two", ("127.0.0.1", local_port))
        .await
        .expect("second datagram sent");
    let (length, _) = tokio::time::timeout(Duration::from_secs(10), client.recv_from(&mut buffer))
        .await
        .expect("second reply")
        .expect("reply read");
    assert_eq!(&buffer[..length], b"reply:datagram-two");

    let other = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("second client bind");
    other
        .send_to(b"datagram-three", ("127.0.0.1", local_port))
        .await
        .expect("third datagram sent");
    let (length, _) = tokio::time::timeout(Duration::from_secs(10), other.recv_from(&mut buffer))
        .await
        .expect("third reply")
        .expect("reply read");
    assert_eq!(&buffer[..length], b"reply:datagram-three");

    visitor.shutdown().await;
    provider.shutdown().await;
    service_task.abort();
    gateway.shutdown().await;
}

/// The visitor **runtime** (what `visitor` on the CLI runs) turns a secret
/// route into a local port: no public surface, no custom client code.
#[tokio::test]
async fn a_visitor_runtime_publishes_a_secret_route_on_a_local_port() {
    use sdkwork_webserver_tunnel::visitor::{
        VisitorEntry, VisitorEvent, VisitorRuntime, VisitorRuntimeOptions,
    };

    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    const SECRET_ENV: &str = "SDKWORK_TUNNEL_TEST_SECRET_VISITOR";
    const SECRET: &str = "visitor-key-456";
    std::env::set_var(SECRET_ENV, SECRET);

    // The provider's private service: an echo target only reachable from the
    // provider's own network.
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

    // Nothing is bound publicly for a secret route: the gateway has no port
    // and no domain for it, which is asserted below through the route listing.
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

    let provider_template = TunnelRouteTemplate {
        name: "private-ssh".to_owned(),
        protocol: TunnelProtocolKind::Stcp,
        target: format!("127.0.0.1:{echo_port}"),
        secret_key_env: Some(SECRET_ENV.to_owned()),
        ..TunnelRouteTemplate::default()
    };
    let mut provider = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint.clone()),
            ..tls::ClientTlsOptions::default()
        },
        device: Device::new(
            DeviceId::parse("dev_visitor_provider").expect("valid id"),
            "provider",
            DevicePlatform::Linux,
        )
        .expect("valid device"),
        token: TOKEN.to_owned(),
        routes: vec![provider_template],
        network: Default::default(),
        timeouts: Default::default(),
        metrics: Arc::new(TunnelMetrics::new()),
    });
    assert_eq!(wait_for_ready(&mut provider).await.len(), 1);

    // A secret route has no public surface at all: the matcher is a name, and
    // there is neither a host nor a port an outsider could dial.
    let routes = gateway.service().list_routes().await.expect("list");
    let listed = routes
        .iter()
        .find(|route| route.name == "private-ssh")
        .expect("the secret route is registered");
    assert_eq!(listed.protocol, TunnelProtocolKind::Stcp);
    assert!(
        listed.matcher.as_domain().is_none() && listed.matcher.as_port().is_none(),
        "a secret route must not claim a public surface: {:?}",
        listed.matcher
    );

    // The visitor runs where the *consumer* is; port 0 lets the OS choose, and
    // the runtime reports what it bound.
    let mut visitor = VisitorRuntime::spawn(VisitorRuntimeOptions::from_config(
        &TunnelConfig::disabled(),
        RemoteEndpoint::new("127.0.0.1", quic_port),
        tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint),
            ..tls::ClientTlsOptions::default()
        },
        Device::new(
            DeviceId::parse("dev_visitor_client").expect("valid id"),
            "visitor",
            DevicePlatform::Linux,
        )
        .expect("valid device"),
        TOKEN.to_owned(),
        vec![VisitorEntry::loopback("private-ssh", SECRET, 0)],
        Arc::new(TunnelMetrics::new()),
    ));
    let local_port = loop {
        match tokio::time::timeout(Duration::from_secs(10), visitor.next_event()).await {
            Ok(Some(VisitorEvent::Listening { name, local_addr })) => {
                assert_eq!(name, "private-ssh");
                break local_addr.port();
            }
            Ok(Some(VisitorEvent::BindFailed { name, error })) => {
                panic!("visitor failed to bind {name}: {error}")
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("the visitor runtime ended before it listened"),
            Err(_) => panic!("the visitor runtime never opened its local port"),
        }
    };

    // A local client — SSH, a browser, anything — reaches the provider's
    // private service through the visitor's loopback port.
    let mut local = tokio::net::TcpStream::connect(("127.0.0.1", local_port))
        .await
        .expect("local client connects");
    local
        .write_all(b"visitor-runtime")
        .await
        .expect("local write");
    let mut echoed = [0_u8; 15];
    tokio::time::timeout(Duration::from_secs(10), local.read_exact(&mut echoed))
        .await
        .expect("the provider's service answers")
        .expect("echo read");
    assert_eq!(&echoed, b"visitor-runtime");

    visitor.shutdown().await;
    provider.shutdown().await;
    echo_task.abort();
    gateway.shutdown().await;
}

/// A secret route (FRP `stcp`) opens **no public port**: only a visitor that
/// names the route *and* presents its shared key reaches the provider's local
/// target, and every other attempt is refused with nothing learned.
#[tokio::test]
async fn a_secret_route_serves_only_a_visitor_that_holds_the_key() {
    use sdkwork_webserver_tunnel_protocol::{
        frame, AuthResult, Authenticate, ControlMessage, Hello, ProtocolVersion,
        VisitorStreamHeader,
    };
    use sdkwork_webserver_tunnel_transport::{QuicClientTransport, TunnelClientTransport};

    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    const SECRET_ENV: &str = "SDKWORK_TUNNEL_TEST_SECRET_STCP";
    const SECRET: &str = "s3cret-key-123";
    std::env::set_var(SECRET_ENV, SECRET);

    // Provider: a live echo target behind the secret route.
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

    let provider_template = TunnelRouteTemplate {
        name: "private-ssh".to_owned(),
        protocol: TunnelProtocolKind::Stcp,
        domain: None,
        port: None,
        target: format!("127.0.0.1:{echo_port}"),
        policy: None,
        secret_key_env: Some(SECRET_ENV.to_owned()),
        ..TunnelRouteTemplate::default()
    };
    provider_template
        .validate()
        .expect("secret template validates");

    let mut provider = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint.clone()),
            ..tls::ClientTlsOptions::default()
        },
        device: Device::new(
            DeviceId::parse("dev_provider").expect("valid id"),
            "provider",
            DevicePlatform::Linux,
        )
        .expect("valid device"),
        token: TOKEN.to_owned(),
        routes: vec![provider_template],
        network: Default::default(),
        timeouts: Default::default(),
        metrics: Arc::new(TunnelMetrics::new()),
    });
    let ready = wait_for_ready(&mut provider).await;
    assert_eq!(ready.len(), 1, "the secret route registers: {ready:?}");

    // The gateway must hold the route under its secret name, and the key must
    // never appear in the route listing.
    let shared = gateway.shared();
    assert_eq!(shared.registry.candidates_secret("private-ssh").len(), 1);
    let listed = serde_json::to_string(&gateway.service().list_routes().await.expect("list"))
        .expect("serialize");
    assert!(
        !listed.contains(SECRET),
        "the shared key must never leak into a listing: {listed}"
    );

    // A visitor client: authenticate like any other client, then open one
    // stream per connection it serves.
    let visitor_endpoint = RemoteEndpoint::new("127.0.0.1", quic_port);
    let transport = QuicClientTransport::new(
        &tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint),
            ..tls::ClientTlsOptions::default()
        },
        Default::default(),
    )
    .expect("transport");
    let connection = transport
        .connect(&visitor_endpoint, &tls::server_name_for(&visitor_endpoint))
        .await
        .expect("visitor connects");
    let mut control = connection.open_stream().await.expect("control stream");
    for message in [
        ControlMessage::Hello(Hello {
            protocol_versions: vec![ProtocolVersion::v1()],
            device_id: "dev_visitor".to_owned(),
            device_name: "visitor".to_owned(),
            platform: "linux".to_owned(),
            user: None,
        }),
        ControlMessage::Authenticate(Authenticate {
            token: TOKEN.to_owned(),
        }),
    ] {
        use tokio::io::AsyncWriteExt;
        let framed = frame::frame_bytes(&message.encode().expect("encode")).expect("frame");
        control.write_all(&framed).await.expect("handshake write");
        control.flush().await.expect("flush");
    }
    {
        let payload = frame::read_exact_frame(&mut control)
            .await
            .expect("auth reply frame");
        match ControlMessage::decode(&payload).expect("decode") {
            ControlMessage::AuthResult(AuthResult { ok: true, .. }) => {}
            other => panic!("visitor authentication failed: {other:?}"),
        }
    }

    // The visitor opens a stream, names the route, and presents a key.
    async fn open_visitor_stream(
        connection: &Box<dyn sdkwork_webserver_tunnel_transport::TunnelConnection>,
        name: &str,
        key: &str,
        protocol: &str,
    ) -> Box<dyn sdkwork_webserver_tunnel_transport::TunnelStream> {
        use tokio::io::AsyncWriteExt;
        let mut stream = connection.open_stream().await.expect("visitor stream");
        let header = VisitorStreamHeader {
            name: name.to_owned(),
            secret_key: key.to_owned(),
            protocol: protocol.to_owned(),
            stream_id: 1,
        };
        let framed = header.to_frame().expect("frame");
        stream.write_all(&framed).await.expect("header written");
        stream.flush().await.expect("flush");
        stream
    }

    // 1. The right key reaches the provider's local target.
    let mut stream = open_visitor_stream(&connection, "private-ssh", SECRET, "stcp").await;
    {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        stream.write_all(b"ping-over-secret").await.expect("write");
        let mut echoed = [0_u8; 16];
        tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut echoed))
            .await
            .expect("the provider's target answers through the tunnel")
            .expect("echo read");
        assert_eq!(&echoed, b"ping-over-secret");
    }

    // 2. A wrong key is refused: the stream closes without serving anything.
    let mut wrong = open_visitor_stream(&connection, "private-ssh", "wrong-key-999", "stcp").await;
    {
        use tokio::io::AsyncReadExt;
        let mut buffer = [0_u8; 16];
        let read = tokio::time::timeout(Duration::from_secs(5), wrong.read(&mut buffer))
            .await
            .expect("refusal is prompt")
            .expect("read");
        assert_eq!(read, 0, "a wrong key must not be served");
    }

    // 3. Asking as the wrong protocol is refused too: a datagram visitor must
    //    never be handed a stream.
    let mut mismatched = open_visitor_stream(&connection, "private-ssh", SECRET, "sudp").await;
    {
        use tokio::io::AsyncReadExt;
        let mut buffer = [0_u8; 16];
        let read = tokio::time::timeout(Duration::from_secs(5), mismatched.read(&mut buffer))
            .await
            .expect("refusal is prompt")
            .expect("read");
        assert_eq!(read, 0, "a protocol mismatch must not be served");
    }

    let snapshot = shared.metrics.snapshot();
    assert_eq!(
        snapshot.visitor_streams, 1,
        "exactly one visitor stream was bridged"
    );
    assert_eq!(
        snapshot.visitor_auth_failures, 1,
        "a wrong key counts as a visitor authentication failure, not an agent one"
    );
    assert_eq!(snapshot.auth_failures, 0);

    drop(stream);
    drop(control);
    drop(connection);
    provider.shutdown().await;
    echo_task.abort();
    gateway.shutdown().await;
}

/// One multiplexed ingress port reaches every route (FRP `tcpmux` parity):
/// an HTTP route by host and a `tcp` route by port, both through HTTP
/// `CONNECT`, with the route's own policy still deciding.
#[tokio::test]
async fn one_multiplexed_port_reaches_http_and_tcp_routes() {
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    // Two independent local services: an HTTP one and a raw-TCP one.
    let http = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("http target bind");
    let http_port = http.local_addr().expect("addr").port();
    let http_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = http.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buffer = [0_u8; 1024];
                let _ = socket.read(&mut buffer).await;
                let body = b"HTTP-TARGET";
                let response = format!("HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.write_all(body).await;
            });
        }
    });
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("tcp target bind");
    let tcp_port = tcp.local_addr().expect("addr").port();
    let tcp_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = tcp.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buffer = [0_u8; 1024];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                let _ = socket.write_all(&buffer[..read]).await;
            });
        }
    });

    let gateway_port = available_port();
    let mut gateway_options = TunnelGatewayOptions::from_config(
        &TunnelConfig::disabled(),
        "127.0.0.1:0".parse().expect("bind"),
        cert_pem.clone(),
        key_pem.clone(),
    );
    gateway_options.authenticator = TokenAuthenticator::new(vec![TOKEN.to_owned()]);
    gateway_options.tcpmux_port = Some(gateway_port);
    let gateway = TunnelGateway::spawn(gateway_options, None)
        .await
        .expect("gateway spawns");
    let quic_port = gateway.quic_port();
    assert_eq!(gateway.tcpmux_port(), Some(gateway_port));
    // The status surface must report the port that actually listens, so an
    // operator can discover a requested port of 0 rather than guess.
    assert_eq!(
        gateway
            .service()
            .status()
            .await
            .expect("status")
            .multiplexer_port,
        Some(gateway_port)
    );

    // One agent, two routes: an HTTP route with a token-protected policy and a
    // tcp route on a public port. Neither needs its own ingress here.
    let mut http_template = http_template("mux.tunnel.test");
    http_template.name = "mux-web".to_owned();
    http_template.target = format!("127.0.0.1:{http_port}");
    http_template.policy = Some(sdkwork_webserver_tunnel_core::RoutePolicy {
        auth: sdkwork_webserver_tunnel_core::AuthPolicy::BearerToken,
        visitor_tokens: vec!["visitor-token".to_owned()],
        ..sdkwork_webserver_tunnel_core::RoutePolicy::private()
    });
    let tcp_route_port = available_port();
    let mut tcp_template = TunnelRouteTemplate {
        name: "mux-ssh".to_owned(),
        protocol: TunnelProtocolKind::Tcp,
        port: Some(tcp_route_port),
        target: format!("127.0.0.1:{tcp_port}"),
        policy: None,
        ..TunnelRouteTemplate::default()
    };
    tcp_template.policy = Some(sdkwork_webserver_tunnel_core::RoutePolicy {
        allow_public: true,
        ..sdkwork_webserver_tunnel_core::RoutePolicy::private()
    });

    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint),
            ..tls::ClientTlsOptions::default()
        },
        device: Device::new(
            DeviceId::parse("dev_mux").expect("valid id"),
            "mux",
            DevicePlatform::Linux,
        )
        .expect("valid device"),
        token: TOKEN.to_owned(),
        routes: vec![http_template, tcp_template],
        network: Default::default(),
        timeouts: Default::default(),
        metrics: Arc::new(TunnelMetrics::new()),
    });
    let ready = wait_for_ready(&mut agent).await;
    assert_eq!(ready.len(), 2, "both routes register: {ready:?}");

    /// Sends one CONNECT request and returns the first response line plus any
    /// bytes that follow the head.
    async fn connect_through(
        mux_port: u16,
        authority: &str,
        bearer: Option<&str>,
    ) -> (String, Vec<u8>, tokio::net::TcpStream) {
        use tokio::io::AsyncReadExt;
        let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", mux_port))
            .await
            .expect("mux connect");
        let auth = match bearer {
            Some(token) => format!("Proxy-Authorization: Bearer {token}\r\n"),
            None => String::new(),
        };
        let request = format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n{auth}\r\n");
        visitor
            .write_all(request.as_bytes())
            .await
            .expect("request written");
        let mut response = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline {
            let mut chunk = [0_u8; 512];
            let read = tokio::time::timeout(Duration::from_secs(5), visitor.read(&mut chunk))
                .await
                .expect("response within 5s")
                .expect("response read");
            if read == 0 {
                break;
            }
            response.extend_from_slice(&chunk[..read]);
            if let Some(head_end) = response.windows(4).position(|window| window == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&response[..head_end]).to_string();
                let first_line = head.lines().next().unwrap_or_default().to_owned();
                let rest = response[head_end + 4..].to_vec();
                return (first_line, rest, visitor);
            }
            if response.len() > 64 * 1024 {
                break;
            }
        }
        let first_line = String::from_utf8_lossy(&response)
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned();
        (first_line, Vec::new(), visitor)
    }

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // 1. An HTTP route is reachable by host, carrying its bearer token.
    let (status, rest, mut visitor) =
        connect_through(gateway_port, "mux.tunnel.test:443", Some("visitor-token")).await;
    assert_eq!(status, "HTTP/1.1 200 Connection Established");
    visitor
        .write_all(b"GET / HTTP/1.0\r\nHost: mux.tunnel.test\r\n\r\n")
        .await
        .expect("request through the tunnel");
    let mut payload = rest;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        let mut chunk = [0_u8; 256];
        match tokio::time::timeout(Duration::from_secs(5), visitor.read(&mut chunk)).await {
            Ok(Ok(0)) | Err(_) => break,
            Ok(Ok(read)) => payload.extend_from_slice(&chunk[..read]),
            Ok(Err(error)) => panic!("relay read failed: {error}"),
        }
        if String::from_utf8_lossy(&payload).contains("HTTP-TARGET") {
            break;
        }
    }
    assert!(
        String::from_utf8_lossy(&payload).contains("HTTP-TARGET"),
        "the host-routed route must answer through the multiplexer"
    );

    // 2. The same port reaches the tcp route by port, with no bearer token —
    //    the tcp route's own policy is public.
    let (status, _, mut visitor) = connect_through(
        gateway_port,
        &format!("anything.test:{tcp_route_port}"),
        None,
    )
    .await;
    assert_eq!(status, "HTTP/1.1 200 Connection Established");
    visitor
        .write_all(b"raw-tcp-through-mux")
        .await
        .expect("raw write");
    let mut echoed = [0_u8; 19];
    tokio::time::timeout(Duration::from_secs(5), visitor.read_exact(&mut echoed))
        .await
        .expect("the tcp target answers through the multiplexer")
        .expect("echo read");
    assert_eq!(&echoed, b"raw-tcp-through-mux");

    // 3. A token-protected route refuses a CONNECT without the token, and the
    //    refusal is the CONNECT-specific 407 (with a challenge), not a 403.
    let (status, body, _visitor) = connect_through(gateway_port, "mux.tunnel.test:443", None).await;
    assert_eq!(
        status, "HTTP/1.1 407 Proxy Authentication Required",
        "a bearer-protected route must ask for the token"
    );
    assert!(
        String::from_utf8_lossy(&body).contains("refused"),
        "the refusal explains itself"
    );

    // 4. An authority nothing claims is a 404, not a hang.
    let (status, _, _visitor) = connect_through(
        gateway_port,
        "nobody.tunnel.test:443",
        Some("visitor-token"),
    )
    .await;
    assert_eq!(status, "HTTP/1.1 404 Not Found");

    // 5. A plain GET on the multiplexer port says what the port is for.
    let mut plain = tokio::net::TcpStream::connect(("127.0.0.1", gateway_port))
        .await
        .expect("connect");
    plain
        .write_all(b"GET / HTTP/1.1\r\nHost: mux.tunnel.test\r\n\r\n")
        .await
        .expect("write");
    let mut response = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(5), plain.read_to_end(&mut response)).await;
    let text = String::from_utf8_lossy(&response).to_string();
    assert!(
        text.starts_with("HTTP/1.1 405"),
        "only CONNECT is served here, got: {text}"
    );

    agent.shutdown().await;
    http_task.abort();
    tcp_task.abort();
    gateway.shutdown().await;
}

/// The `ssh -R` recipe, verified: a **server-first** protocol (SSH sends its
/// banner before the client says anything) reached through a secret route and
/// the visitor runtime.
///
/// This is the repository's answer to FRP's `sshTunnelGateway`: the owner keeps
/// no public port open, and the consumer runs one `visitor` command and then
/// uses the stock `ssh` client against `127.0.0.1`. It only works if nothing in
/// the chain waits for the visitor's first byte, which is what this test pins
/// down.
#[tokio::test]
async fn a_secret_route_carries_a_server_first_protocol_like_ssh() {
    use sdkwork_webserver_tunnel::visitor::{
        VisitorEntry, VisitorEvent, VisitorRuntime, VisitorRuntimeOptions,
    };

    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    const SECRET_ENV: &str = "SDKWORK_TUNNEL_TEST_SECRET_SSH";
    const SECRET: &str = "ssh-shared-key-1";
    const BANNER: &[u8] = b"SSH-2.0-sdkwork_test\r\n";
    std::env::set_var(SECRET_ENV, SECRET);

    // An SSH-shaped service: it greets first (RFC 4253 §4.2), then echoes the
    // client's identification string — the ordering is what breaks a relay
    // that lazily dials on the visitor's first byte.
    let service = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("ssh-like service bind");
    let service_port = service.local_addr().expect("addr").port();
    let service_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = service.accept().await else {
                return;
            };
            tokio::spawn(async move {
                if socket.write_all(BANNER).await.is_err() {
                    return;
                }
                let mut line = Vec::new();
                let mut byte = [0_u8; 1];
                while let Ok(1) = socket.read(&mut byte).await {
                    line.push(byte[0]);
                    if line.ends_with(b"\n") {
                        break;
                    }
                }
                let _ = socket.write_all(&line).await;
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

    // Owner side: expose the local SSH service privately. No port, no domain —
    // an stcp route has no public surface at all.
    let owner_template = TunnelRouteTemplate {
        name: "private-ssh".to_owned(),
        protocol: TunnelProtocolKind::Stcp,
        target: format!("127.0.0.1:{service_port}"),
        secret_key_env: Some(SECRET_ENV.to_owned()),
        ..TunnelRouteTemplate::default()
    };
    let mut owner = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint.clone()),
            ..tls::ClientTlsOptions::default()
        },
        device: Device::new(
            DeviceId::parse("dev_ssh_owner").expect("valid id"),
            "ssh-owner",
            DevicePlatform::Linux,
        )
        .expect("valid device"),
        token: TOKEN.to_owned(),
        routes: vec![owner_template],
        network: Default::default(),
        timeouts: Default::default(),
        metrics: Arc::new(TunnelMetrics::new()),
    });
    assert_eq!(wait_for_ready(&mut owner).await.len(), 1);

    // Consumer side: one command, no agent, no public port.
    let mut visitor = VisitorRuntime::spawn(VisitorRuntimeOptions::from_config(
        &TunnelConfig::disabled(),
        RemoteEndpoint::new("127.0.0.1", quic_port),
        tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint),
            ..tls::ClientTlsOptions::default()
        },
        Device::new(
            DeviceId::parse("dev_ssh_consumer").expect("valid id"),
            "ssh-consumer",
            DevicePlatform::Linux,
        )
        .expect("valid device"),
        TOKEN.to_owned(),
        vec![VisitorEntry::loopback("private-ssh", SECRET, 0)],
        Arc::new(TunnelMetrics::new()),
    ));
    let local_port = loop {
        match tokio::time::timeout(Duration::from_secs(10), visitor.next_event()).await {
            Ok(Some(VisitorEvent::Listening { local_addr, .. })) => break local_addr.port(),
            Ok(Some(VisitorEvent::BindFailed { name, error })) => {
                panic!("visitor failed to bind {name}: {error}")
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("the visitor runtime ended before it listened"),
            Err(_) => panic!("the visitor runtime never opened its local port"),
        }
    };

    // The client behaves like `ssh`: it says nothing and expects the banner.
    let mut client = tokio::net::TcpStream::connect(("127.0.0.1", local_port))
        .await
        .expect("ssh client connects");
    let mut banner = vec![0_u8; BANNER.len()];
    tokio::time::timeout(Duration::from_secs(10), client.read_exact(&mut banner))
        .await
        .expect("the banner arrives before the client speaks")
        .expect("banner read");
    assert_eq!(banner, BANNER);

    // Then the normal version exchange completes through the same stream.
    client
        .write_all(b"SSH-2.0-sdkwork_client\r\n")
        .await
        .expect("client identification written");
    let mut echoed = vec![0_u8; b"SSH-2.0-sdkwork_client\r\n".len()];
    tokio::time::timeout(Duration::from_secs(10), client.read_exact(&mut echoed))
        .await
        .expect("the service answer arrives")
        .expect("answer read");
    assert_eq!(echoed, b"SSH-2.0-sdkwork_client\r\n");

    // And nothing about the route is public: no host, no port, no listing leak.
    let listed = gateway.service().list_routes().await.expect("list");
    let route = listed
        .iter()
        .find(|route| route.name == "private-ssh")
        .expect("the secret route is registered");
    assert!(route.matcher.as_domain().is_none() && route.matcher.as_port().is_none());
    assert!(!serde_json::to_string(&listed)
        .expect("serialize")
        .contains(SECRET));

    visitor.shutdown().await;
    owner.shutdown().await;
    service_task.abort();
    gateway.shutdown().await;
}

/// A live agent can be reconfigured without a restart (FRP `frpc reload`
/// parity): added routes become reachable, removed routes stop being routed,
/// and traffic already flowing keeps flowing.
#[tokio::test]
async fn a_live_agent_reloads_its_route_set() {
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    // Two local services, one per route.
    async fn spawn_echo(marker: &'static [u8]) -> (u16, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("echo bind");
        let port = listener.local_addr().expect("addr").port();
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buffer = [0_u8; 1024];
                    let read = socket.read(&mut buffer).await.unwrap_or(0);
                    let _ = socket.write_all(marker).await;
                    let _ = read;
                });
            }
        });
        (port, task)
    }
    let (first_port, first_task) = spawn_echo(b"FIRST").await;
    let (second_port, second_task) = spawn_echo(b"SECOND").await;

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

    let mut first = http_template("first.tunnel.test");
    first.name = "first".to_owned();
    first.target = format!("127.0.0.1:{first_port}");
    let agent_options = |routes: Vec<TunnelRouteTemplate>| AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint.clone()),
            ..tls::ClientTlsOptions::default()
        },
        device: Device::new(
            DeviceId::parse("dev_reload").expect("valid id"),
            "reload",
            DevicePlatform::Linux,
        )
        .expect("valid device"),
        token: TOKEN.to_owned(),
        routes,
        network: Default::default(),
        timeouts: Default::default(),
        metrics: Arc::new(TunnelMetrics::new()),
    };
    let mut agent = AgentRuntime::spawn(agent_options(vec![first.clone()]));
    assert_eq!(wait_for_ready(&mut agent).await.len(), 1);

    let shared = gateway.shared();
    assert!(shared
        .registry
        .match_domain_path("first.tunnel.test", "/")
        .is_some());

    // Reload: keep `first` (with a new target) and add `second`.
    let mut second = http_template("second.tunnel.test");
    second.name = "second".to_owned();
    second.target = format!("127.0.0.1:{second_port}");
    let mut first_moved = first.clone();
    first_moved.target = format!("127.0.0.1:{second_port}");
    let report = agent
        .reload(vec![first_moved.clone(), second.clone()])
        .await
        .expect("reload applies");
    assert!(!report.applied_offline, "the session was live");
    let mut added = report.added.clone();
    added.sort();
    assert_eq!(added, vec!["first".to_owned(), "second".to_owned()]);
    assert!(report.removed.is_empty(), "{:?}", report.removed);
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);

    // The new route is routable immediately, without a reconnect.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while shared
        .registry
        .match_domain_path("second.tunnel.test", "/")
        .is_none()
    {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the added route never became routable"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let mut stream = shared
        .connect_http_stream(
            "second.tunnel.test",
            "/",
            RelayVisitor {
                ip: "203.0.113.9".parse().expect("ip"),
                port: 0,
                bearer: None,
            },
        )
        .await
        .expect("the added route relays");
    stream.write_all(b"hello").await.expect("request written");
    let mut answer = [0_u8; 6];
    tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut answer))
        .await
        .expect("added route answers")
        .expect("read");
    assert_eq!(&answer, b"SECOND");
    drop(stream);

    // The changed route follows the new target too.
    let mut stream = shared
        .connect_http_stream(
            "first.tunnel.test",
            "/",
            RelayVisitor {
                ip: "203.0.113.9".parse().expect("ip"),
                port: 0,
                bearer: None,
            },
        )
        .await
        .expect("the changed route relays");
    stream.write_all(b"hello").await.expect("request written");
    tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut answer))
        .await
        .expect("changed route answers")
        .expect("read");
    assert_eq!(
        &answer, b"SECOND",
        "a reload must move the route to its new target"
    );
    drop(stream);

    // Removing a route takes it off the gateway while the session stays up.
    let report = agent
        .reload(vec![second.clone()])
        .await
        .expect("reload applies");
    assert_eq!(report.removed, vec!["first".to_owned()]);
    assert!(report.added.is_empty(), "{:?}", report.added);
    assert_eq!(report.unchanged, vec!["second".to_owned()]);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while shared
        .registry
        .match_domain_path("first.tunnel.test", "/")
        .is_some()
    {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the removed route is still registered"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(matches!(
        shared
            .connect_http_stream(
                "first.tunnel.test",
                "/",
                RelayVisitor {
                    ip: "203.0.113.9".parse().expect("ip"),
                    port: 0,
                    bearer: None,
                },
            )
            .await,
        Err(sdkwork_webserver_tunnel_core::TunnelError::RouteNotFound)
    ));

    // And the surviving route is untouched by the removal.
    let mut stream = shared
        .connect_http_stream(
            "second.tunnel.test",
            "/",
            RelayVisitor {
                ip: "203.0.113.9".parse().expect("ip"),
                port: 0,
                bearer: None,
            },
        )
        .await
        .expect("the surviving route still relays");
    stream.write_all(b"hello").await.expect("request written");
    tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut answer))
        .await
        .expect("surviving route answers")
        .expect("read");
    assert_eq!(&answer, b"SECOND");
    drop(stream);

    // An unusable configuration is refused without disturbing the live one.
    let mut broken = second.clone();
    broken.target = "not-an-address".to_owned();
    let error = agent
        .reload(vec![broken])
        .await
        .expect_err("an invalid route set is refused");
    assert!(error.contains("IP:port"), "{error}");
    assert!(
        shared
            .registry
            .match_domain_path("second.tunnel.test", "/")
            .is_some(),
        "a refused reload must leave the running configuration alone"
    );

    agent.shutdown().await;
    first_task.abort();
    second_task.abort();
    gateway.shutdown().await;
}

/// Machine management (the admin console's "FRP machines" view): a device is
/// listed while it serves, remembered after it disconnects with its routes
/// gone and its traffic kept, and switched off by an operator with the switch
/// taking effect on the live session *and* on the next reconnect.
#[tokio::test]
async fn machines_are_listed_remembered_and_switchable() {
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    let echo = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("target bind");
    let echo_port = echo.local_addr().expect("addr").port();
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

    let mut template = http_template("machine.tunnel.test");
    template.name = "machine-web".to_owned();
    template.target = format!("127.0.0.1:{echo_port}");
    let agent_options = || AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint.clone()),
            ..tls::ClientTlsOptions::default()
        },
        device: Device::new(
            DeviceId::parse("dev_machine_1").expect("valid id"),
            "edge-machine-1",
            DevicePlatform::Linux,
        )
        .expect("valid device"),
        token: TOKEN.to_owned(),
        routes: vec![template.clone()],
        network: Default::default(),
        timeouts: Default::default(),
        metrics: Arc::new(TunnelMetrics::new()),
    };
    let mut agent = AgentRuntime::spawn(agent_options());
    assert_eq!(wait_for_ready(&mut agent).await.len(), 1);

    let device_id = DeviceId::parse("dev_machine_1").expect("valid id");
    let service = gateway.service();

    // Online: identity, liveness and the route it serves.
    let machines = service.list_devices().await.expect("list machines");
    assert_eq!(machines.len(), 1, "one machine is managed");
    let machine = &machines[0];
    assert_eq!(machine.device.name, "edge-machine-1");
    assert!(machine.online, "a live session means online");
    assert!(machine.enabled);
    assert!(machine.session_id.is_some());
    assert_eq!(machine.route_count(), 1);
    assert_eq!(machine.healthy_route_count(), 1);
    assert_eq!(
        machine.routes[0].matcher.as_domain(),
        Some("machine.tunnel.test")
    );

    // Traffic is credited to the machine that served it.
    let mut stream = gateway
        .shared()
        .connect_http_stream(
            "machine.tunnel.test",
            "/",
            RelayVisitor {
                ip: "203.0.113.30".parse().expect("ip"),
                port: 0,
                bearer: None,
            },
        )
        .await
        .expect("relay opens");
    stream.write_all(b"machine-traffic").await.expect("write");
    let mut echoed = [0_u8; 15];
    tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut echoed))
        .await
        .expect("echo")
        .expect("read");
    drop(stream);
    let machine = service.device(&device_id).await.expect("detail");
    assert!(
        machine.bytes_in >= 15 && machine.bytes_out >= 15,
        "relayed bytes are credited to the machine: in={} out={}",
        machine.bytes_in,
        machine.bytes_out
    );

    // Offline: still managed, but its routes are gone (they died with the
    // session) and the history is kept.
    agent.shutdown().await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let machine = service.device(&device_id).await.expect("detail");
        if !machine.online {
            assert_eq!(
                machine.route_count(),
                0,
                "an offline machine serves nothing"
            );
            assert!(machine.disconnected_at.is_some());
            assert_eq!(machine.device.name, "edge-machine-1");
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the machine never went offline"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // An unknown machine cannot be switched off: identity comes from the
    // inventory, so a typo cannot pre-emptively block a stranger.
    assert!(matches!(
        service
            .set_device_enabled(&DeviceId::parse("dev_unknown").expect("id"), false)
            .await,
        Err(sdkwork_webserver_tunnel_core::TunnelError::DeviceNotFound)
    ));

    // Switching the machine off makes it visible as disabled and refuses the
    // reconnect that follows.
    let switched = service
        .set_device_enabled(&device_id, false)
        .await
        .expect("switch off");
    assert!(!switched.enabled);
    let mut refused = AgentRuntime::spawn(agent_options());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        // Every arm either diverges or leaves the loop, so the deadline is the
        // only other way out: the refusal must arrive before it.
        match tokio::time::timeout(Duration::from_secs(5), refused.next_event()).await {
            Ok(Some(AgentEvent::Ready { .. })) => panic!("a disabled machine must not register"),
            Ok(Some(AgentEvent::Disconnected { reason })) => {
                assert!(
                    reason.contains("disabled") || reason.contains("invalid credentials"),
                    "the refusal explains itself: {reason}"
                );
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "the disabled machine never stopped retrying"
                );
                break;
            }
        }
    }
    refused.shutdown().await;

    // Switching it back on restores service.
    let switched = service
        .set_device_enabled(&device_id, true)
        .await
        .expect("switch on");
    assert!(switched.enabled);
    let mut restored = AgentRuntime::spawn(agent_options());
    let ready = wait_for_ready(&mut restored).await;
    assert_eq!(ready.len(), 1, "an enabled machine registers again");

    restored.shutdown().await;
    echo_task.abort();
    gateway.shutdown().await;
}

/// A route's bandwidth ceiling must actually pace the relay, not merely be
/// recorded: the same bytes take measurably longer through a limited route
/// (FRP `transport.bandwidthLimit` parity).
///
/// The assertion is a lower bound on elapsed time, which no scheduler can
/// violate from above — a flaky upper bound would be the alternative, and it
/// would fail on a loaded machine.
#[tokio::test]
async fn a_bandwidth_limited_route_paces_the_relay() {
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    // Echo target.
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
                let mut buffer = [0_u8; 4096];
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
    let public_port = available_port();

    /// Ceiling and payload chosen so the paced transfer takes seconds while
    /// the unpaced one takes milliseconds.
    const LIMIT_BYTES_PER_SECOND: u64 = 32 * 1024;
    const PAYLOAD: usize = 64 * 1024;

    let template = TunnelRouteTemplate {
        name: "metered".to_owned(),
        protocol: TunnelProtocolKind::Tcp,
        domain: None,
        port: Some(public_port),
        target: format!("127.0.0.1:{echo_port}"),
        policy: Some(sdkwork_webserver_tunnel_core::RoutePolicy {
            allow_public: true,
            ..sdkwork_webserver_tunnel_core::RoutePolicy::private()
        }),
        bandwidth_limit_bytes_per_second: Some(LIMIT_BYTES_PER_SECOND),
        ..TunnelRouteTemplate::default()
    };
    let device = Device::new(
        DeviceId::parse("dev_metered").expect("valid id"),
        "metered-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
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

    // The gateway's listing must show the ceiling an operator configured.
    let registered = gateway
        .shared()
        .registry
        .match_port(public_port)
        .expect("route registered");
    assert_eq!(
        registered.route.bandwidth_limit_bytes_per_second,
        Some(LIMIT_BYTES_PER_SECOND)
    );

    let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", public_port))
        .await
        .expect("visitor connect");
    let payload = vec![0x5a_u8; PAYLOAD];
    let started = tokio::time::Instant::now();
    visitor
        .write_all(&payload)
        .await
        .expect("payload written through a paced stream");
    let mut echoed = vec![0_u8; PAYLOAD];
    tokio::time::timeout(Duration::from_secs(30), visitor.read_exact(&mut echoed))
        .await
        .expect("the paced echo arrives within 30s")
        .expect("echo read");
    let elapsed = started.elapsed();

    assert_eq!(echoed, payload, "pacing must not corrupt the relayed bytes");
    // 64 KiB through a 32 KiB/s bucket (one second of burst) needs at least a
    // second in eachdirection; assert the conservative half of that.
    assert!(
        elapsed >= Duration::from_millis(1_000),
        "a limited route relayed {PAYLOAD} bytes in {elapsed:?}, faster than its ceiling allows"
    );

    agent.shutdown().await;
    echo_task.abort();
    gateway.shutdown().await;
}

/// A load-balancer group serves from its healthy replica: the agent behind the
/// broken replica *reports* it down (FRP `healthCheck`), the gateway steers
/// traffic to the survivor, and losing the survivor too yields "no healthy
/// replica" rather than "not found" (FRP E2 + E3 parity).
#[tokio::test]
async fn a_load_balancer_group_serves_only_from_healthy_replicas() {
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    // Replica A: a live HTTP target.
    let live = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("live target bind");
    let live_port = live.local_addr().expect("addr").port();
    let live_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = live.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buffer = [0_u8; 1024];
                let _ = socket.read(&mut buffer).await;
                let body = b"REPLICA-A";
                let response = format!("HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.write_all(body).await;
            });
        }
    });

    // Replica B: a port nobody listens on, so its probe fails.
    let dead_port = available_port();

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

    // Both replicas declare the same host and the same group, which is what
    // lets two devices share one surface at all.
    let probe = sdkwork_webserver_tunnel_core::HealthCheckConfig {
        kind: sdkwork_webserver_tunnel_core::HealthCheckKind::Tcp,
        interval_seconds: 1,
        timeout_seconds: 1,
        max_failed: 1,
        path: None,
    };
    let mut healthy_template = http_template("lb.tunnel.test");
    // Replicas share a group, not a route id: the id is the registry key and
    // must stay unique, which is also FRP's rule for proxy names.
    healthy_template.name = "web-a".to_owned();
    healthy_template.target = format!("127.0.0.1:{live_port}");
    healthy_template.load_balancer_group = Some("web".to_owned());
    healthy_template.health_check = Some(probe.clone());
    let mut broken_template = http_template("lb.tunnel.test");
    broken_template.name = "web-b".to_owned();
    broken_template.target = format!("127.0.0.1:{dead_port}");
    broken_template.load_balancer_group = Some("web".to_owned());
    broken_template.health_check = Some(probe);

    let spawn_agent = |device_id: &'static str, template: TunnelRouteTemplate| {
        let fingerprint = fingerprint.clone();
        async move {
            let device = Device::new(
                DeviceId::parse(device_id).expect("valid id"),
                "lb-runner",
                DevicePlatform::Linux,
            )
            .expect("valid device");
            AgentRuntime::spawn(AgentRuntimeOptions {
                endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
                alternates: Vec::new(),
                login_fail_exit: false,
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
            })
        }
    };
    let mut healthy_agent = spawn_agent("dev_lb_a", healthy_template).await;
    let mut broken_agent = spawn_agent("dev_lb_b", broken_template).await;
    let healthy_ready = wait_for_ready(&mut healthy_agent).await;
    let broken_ready = wait_for_ready(&mut broken_agent).await;
    assert_eq!(healthy_ready.len(), 1);
    assert_eq!(broken_ready.len(), 1);

    // Both replicas claim the host; the registry keeps both as candidates.
    let shared = gateway.shared();
    assert_eq!(
        shared
            .registry
            .candidates_domain_path("lb.tunnel.test", "/")
            .len(),
        2,
        "both replicas must be registered as candidates"
    );

    // Wait for the broken replica's own probe to report it down.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let any_unhealthy = shared
            .registry
            .candidates_domain_path("lb.tunnel.test", "/")
            .iter()
            .any(|route| !route.health.is_healthy());
        if any_unhealthy {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the broken replica never reported itself unhealthy"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Every relay must now reach the live replica.
    for attempt in 0..4 {
        let mut stream = shared
            .connect_http_stream(
                "lb.tunnel.test",
                "/",
                RelayVisitor {
                    ip: "203.0.113.5".parse().expect("ip"),
                    port: 0,
                    bearer: None,
                },
            )
            .await
            .unwrap_or_else(|error| panic!("attempt {attempt} failed: {error}"));
        stream
            .write_all(b"GET / HTTP/1.0\r\nHost: lb.tunnel.test\r\n\r\n")
            .await
            .expect("request written");
        let mut response = Vec::new();
        let read_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < read_deadline {
            let mut chunk = [0_u8; 256];
            match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk)).await {
                Ok(Ok(0)) | Err(_) => break,
                Ok(Ok(read)) => response.extend_from_slice(&chunk[..read]),
                Ok(Err(error)) => panic!("relay read failed: {error}"),
            }
            if String::from_utf8_lossy(&response).contains("REPLICA-A") {
                break;
            }
        }
        let text = String::from_utf8_lossy(&response).to_string();
        assert!(
            text.contains("REPLICA-A"),
            "attempt {attempt} must reach the healthy replica, got: {text}"
        );
    }

    // Losing the healthy replica leaves a claimed host with nothing to serve
    // it: that is "no healthy replica", not "not found".
    healthy_agent.shutdown().await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let outcome = shared
            .connect_http_stream(
                "lb.tunnel.test",
                "/",
                RelayVisitor {
                    ip: "203.0.113.5".parse().expect("ip"),
                    port: 0,
                    bearer: None,
                },
            )
            .await;
        match outcome {
            Err(sdkwork_webserver_tunnel_core::TunnelError::NoHealthyReplica) => break,
            Ok(_) => {}
            Err(other) => panic!("unexpected relay error: {other}"),
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the group never reported itself without a healthy replica"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    broken_agent.shutdown().await;
    live_task.abort();
    gateway.shutdown().await;
}

/// Two routes on one hostname, distinguished by path prefix (FRP `locations`
/// parity): `/api` reaches one local service, `/static` another, and a path
/// nobody claimed is not served by the tunnel at all.
#[tokio::test]
async fn location_claims_on_one_host_dispatch_to_their_own_targets() {
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    /// One-shot HTTP responder that answers with `body`.
    async fn spawn_target(body: &'static str) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("target bind");
        let port = listener.local_addr().expect("addr").port();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buffer = [0_u8; 1024];
                    let _ = socket.read(&mut buffer).await;
                    let response =
                        format!("HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.write_all(body.as_bytes()).await;
                });
            }
        });
        port
    }

    let api_port = spawn_target("API-TARGET").await;
    let static_port = spawn_target("STATIC-TARGET").await;

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

    let mut api_template = http_template("split.tunnel.test");
    api_template.name = "api".to_owned();
    api_template.target = format!("127.0.0.1:{api_port}");
    api_template.locations = vec!["/api".to_owned()];
    let mut static_template = http_template("split.tunnel.test");
    static_template.name = "assets".to_owned();
    static_template.target = format!("127.0.0.1:{static_port}");
    static_template.locations = vec!["/static".to_owned()];

    let device = Device::new(
        DeviceId::parse("dev_split").expect("valid id"),
        "split-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint),
            ..tls::ClientTlsOptions::default()
        },
        device,
        token: TOKEN.to_owned(),
        routes: vec![api_template, static_template],
        network: Default::default(),
        timeouts: Default::default(),
        metrics: Arc::new(TunnelMetrics::new()),
    });
    let ready = wait_for_ready(&mut agent).await;
    assert_eq!(ready.len(), 2, "both claims registered: {ready:?}");

    let shared = gateway.shared();
    let relay = |path: &'static str| {
        let shared = shared.clone();
        async move {
            let mut stream = shared
                .connect_http_stream(
                    "split.tunnel.test",
                    path,
                    RelayVisitor {
                        ip: "203.0.113.5".parse().expect("ip"),
                        port: 0,
                        bearer: None,
                    },
                )
                .await
                .expect("relay opens");
            stream
                .write_all(
                    format!("GET {path} HTTP/1.0\r\nHost: split.tunnel.test\r\n\r\n").as_bytes(),
                )
                .await
                .expect("request written");
            let mut response = Vec::new();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
            while tokio::time::Instant::now() < deadline {
                let mut chunk = [0_u8; 256];
                match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk)).await {
                    Ok(Ok(0)) | Err(_) => break,
                    Ok(Ok(read)) => response.extend_from_slice(&chunk[..read]),
                    Ok(Err(error)) => panic!("relay read failed: {error}"),
                }
                let text = String::from_utf8_lossy(&response);
                if text.contains("API-TARGET") || text.contains("STATIC-TARGET") {
                    break;
                }
            }
            String::from_utf8_lossy(&response).to_string()
        }
    };

    let api = relay("/api/users").await;
    assert!(api.contains("API-TARGET"), "got: {api}");
    let assets = relay("/static/app.js").await;
    assert!(assets.contains("STATIC-TARGET"), "got: {assets}");

    // A path nobody claimed is not the tunnel's to answer.
    let unclaimed = shared
        .connect_http_stream(
            "split.tunnel.test",
            "/other",
            RelayVisitor {
                ip: "203.0.113.5".parse().expect("ip"),
                port: 0,
                bearer: None,
            },
        )
        .await;
    assert!(
        matches!(
            unclaimed,
            Err(sdkwork_webserver_tunnel_core::TunnelError::RouteNotFound)
        ),
        "an unclaimed path must not be relayed, got {unclaimed:?}"
    );

    // The gateway's listing shows each claim, so operations can tell which
    // service owns which path.
    let listed = gateway.service().list_routes().await.expect("list");
    let mut locations: Vec<Vec<String>> =
        listed.iter().map(|route| route.locations.clone()).collect();
    locations.sort();
    assert_eq!(
        locations,
        vec![vec!["/api".to_owned()], vec!["/static".to_owned()]]
    );

    agent.shutdown().await;
    gateway.shutdown().await;
}

/// A route that declares PROXY protocol must hand the visitor's real address
/// to the local service *before* any application byte (FRP
/// `transport.proxyProtocolVersion` parity).
///
/// The address can only come from the gateway — the agent dials the local
/// target itself — so this test also pins the stream-header field: without
/// it, the header would either be missing or carry a made-up address, and a
/// service that authorizes on PROXY-protocol source addresses would see the
/// wrong client.
#[tokio::test]
async fn a_proxy_protocol_route_hands_the_visitor_address_to_the_local_target() {
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    // The local target records the first line it receives, then echoes.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("target bind");
    let target_port = listener.local_addr().expect("addr").port();
    let first_line = Arc::new(std::sync::Mutex::new(String::new()));
    let first_line_task = first_line.clone();
    let target_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let first_line = first_line_task.clone();
            tokio::spawn(async move {
                let mut buffer = [0_u8; 512];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                if read == 0 {
                    return;
                }
                let text = String::from_utf8_lossy(&buffer[..read]).to_string();
                let line = text.lines().next().unwrap_or_default().to_owned();
                *first_line.lock().expect("lock") = line;
                let _ = socket.write_all(b"OK\n").await;
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
    let public_port = available_port();

    let template = TunnelRouteTemplate {
        name: "audited".to_owned(),
        protocol: TunnelProtocolKind::Tcp,
        domain: None,
        port: Some(public_port),
        target: format!("127.0.0.1:{target_port}"),
        policy: Some(sdkwork_webserver_tunnel_core::RoutePolicy {
            allow_public: true,
            ..sdkwork_webserver_tunnel_core::RoutePolicy::private()
        }),
        proxy_protocol: Some(sdkwork_webserver_tunnel_core::ProxyProtocolVersion::V1),
        locations: Vec::new(),
        ..TunnelRouteTemplate::default()
    };
    let device = Device::new(
        DeviceId::parse("dev_proxy").expect("valid id"),
        "proxy-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
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

    // The registered route must advertise the header it will emit, so the
    // gateway's listing (operations REST) tells the truth about the service.
    let registered = gateway
        .shared()
        .registry
        .match_port(public_port)
        .expect("route registered");
    assert_eq!(
        registered.route.proxy_protocol,
        Some(sdkwork_webserver_tunnel_core::ProxyProtocolVersion::V1)
    );

    let visitor = tokio::net::TcpStream::connect(("127.0.0.1", public_port))
        .await
        .expect("visitor connect");
    let visitor_port = visitor.local_addr().expect("visitor addr").port();
    let mut visitor = visitor;
    let mut reply = [0_u8; 3];
    tokio::time::timeout(Duration::from_secs(5), visitor.read_exact(&mut reply))
        .await
        .expect("target reply arrives")
        .expect("reply read");
    assert_eq!(&reply, b"OK\n");

    let line = first_line.lock().expect("lock").clone();
    assert_eq!(
        line,
        format!("PROXY TCP4 127.0.0.1 127.0.0.1 {visitor_port} {target_port}"),
        "the local target must receive the visitor's own address and port"
    );

    agent.shutdown().await;
    target_task.abort();
    gateway.shutdown().await;
}

/// A route without a PROXY-protocol declaration must stay byte-for-byte raw:
/// a service that never asked for a header would otherwise parse it as data.
#[tokio::test]
async fn a_route_without_proxy_protocol_receives_no_header() {
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("target bind");
    let target_port = listener.local_addr().expect("addr").port();
    let seen = Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let seen_task = seen.clone();
    let target_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let seen = seen_task.clone();
            tokio::spawn(async move {
                let mut buffer = [0_u8; 512];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                seen.lock()
                    .expect("lock")
                    .extend_from_slice(&buffer[..read]);
                let _ = socket.write_all(b"OK\n").await;
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
    let public_port = available_port();

    let mut template = http_template("raw.tunnel.test");
    template.protocol = TunnelProtocolKind::Tcp;
    template.domain = None;
    template.port = Some(public_port);
    template.target = format!("127.0.0.1:{target_port}");
    let device = Device::new(
        DeviceId::parse("dev_raw").expect("valid id"),
        "raw-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
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

    let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", public_port))
        .await
        .expect("visitor connect");
    visitor
        .write_all(b"GET /raw HTTP/1.0\r\n\r\n")
        .await
        .expect("request written");
    let mut reply = [0_u8; 3];
    tokio::time::timeout(Duration::from_secs(5), visitor.read_exact(&mut reply))
        .await
        .expect("target reply arrives")
        .expect("reply read");
    assert_eq!(&reply, b"OK\n");
    assert_eq!(
        seen.lock().expect("lock").as_slice(),
        b"GET /raw HTTP/1.0\r\n\r\n",
        "no header may be injected into a route that did not ask for one"
    );

    agent.shutdown().await;
    target_task.abort();
    gateway.shutdown().await;
}

/// A `unix:` template target must actually be dialed: the variant existed in
/// the domain model with no producer, so "support" was an enum value nothing
/// could reach. POSIX-only, because a Unix domain socket is.
#[cfg(unix)]
#[tokio::test]
async fn tcp_route_can_relay_to_a_unix_socket_target() {
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    // Local Unix socket target that answers a banner before reading, like a
    // container runtime socket.
    let socket_dir = std::env::temp_dir().join(format!("sdkwork-tunnel-{}", std::process::id()));
    std::fs::create_dir_all(&socket_dir).expect("socket dir");
    let socket_path = socket_dir.join("target.sock");
    let _ = std::fs::remove_file(&socket_path);
    let listener = tokio::net::UnixListener::bind(&socket_path).expect("unix bind");
    let socket_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let _ = socket.write_all(b"UNIX-OK\n").await;
                let mut buffer = [0_u8; 256];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                if read > 0 {
                    let _ = socket.write_all(&buffer[..read]).await;
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
    let public_port = available_port();

    let template = TunnelRouteTemplate {
        name: "runtime".to_owned(),
        protocol: TunnelProtocolKind::Tcp,
        domain: None,
        port: Some(public_port),
        target: format!("unix:{}", socket_path.display()),
        policy: Some(sdkwork_webserver_tunnel_core::RoutePolicy {
            allow_public: true,
            ..sdkwork_webserver_tunnel_core::RoutePolicy::private()
        }),
        proxy_protocol: None,
        locations: Vec::new(),
        ..TunnelRouteTemplate::default()
    };
    let device = Device::new(
        DeviceId::parse("dev_unix").expect("valid id"),
        "unix-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
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

    let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", public_port))
        .await
        .expect("visitor connect");
    let mut banner = [0_u8; 9];
    tokio::time::timeout(Duration::from_secs(5), visitor.read_exact(&mut banner))
        .await
        .expect("the unix socket's banner must arrive")
        .expect("banner read");
    assert_eq!(&banner, b"UNIX-OK\n");

    visitor
        .write_all(b"ping")
        .await
        .expect("visitor payload written");
    let mut echoed = [0_u8; 4];
    tokio::time::timeout(Duration::from_secs(5), visitor.read_exact(&mut echoed))
        .await
        .expect("echo within timeout")
        .expect("echo read");
    assert_eq!(&echoed, b"ping");

    agent.shutdown().await;
    socket_task.abort();
    gateway.shutdown().await;
    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_dir(&socket_dir);
}

#[tokio::test]
async fn a_datagram_above_the_configured_packet_size_is_dropped_and_counted() {
    // FRP `transport.udpPacketSize`: the ceiling decides what the relay
    // accepts. A datagram above it is dropped whole — never truncated, because
    // a shortened datagram would reach the service as corrupt input — and the
    // drop is counted so an operator can see the ceiling is too low.
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    let echo = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("echo bind");
    let echo_port = echo.local_addr().expect("echo addr").port();
    let echo_task = tokio::spawn(async move {
        let mut buffer = [0_u8; 4096];
        loop {
            let Ok((size, peer)) = echo.recv_from(&mut buffer).await else {
                return;
            };
            let _ = echo.send_to(&buffer[..size], peer).await;
        }
    });

    let mut network = TunnelConfig::disabled().network_or_default();
    network.udp_packet_size = Some(512);
    network.validate_network().expect("512 is a usable ceiling");
    let mut gateway_options = TunnelGatewayOptions::from_config(
        &TunnelConfig::disabled(),
        "127.0.0.1:0".parse().expect("bind"),
        cert_pem.clone(),
        key_pem.clone(),
    );
    gateway_options.authenticator = TokenAuthenticator::new(vec![TOKEN.to_owned()]);
    gateway_options.network = network.clone();
    // The drop happens at the gateway, so the gateway's counters are the ones
    // to assert on (the agent never sees the datagram).
    let gateway_metrics = std::sync::Arc::new(TunnelMetrics::new());
    gateway_options.metrics = gateway_metrics.clone();
    let gateway = TunnelGateway::spawn(gateway_options, None)
        .await
        .expect("gateway spawns");
    let quic_port = gateway.quic_port();
    let public_port = available_udp_port();

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
        proxy_protocol: None,
        locations: Vec::new(),
        ..TunnelRouteTemplate::default()
    };
    let device = Device::new(
        DeviceId::parse("dev_udp_size").expect("valid id"),
        "udp-size-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let metrics = std::sync::Arc::new(TunnelMetrics::new());
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(fingerprint),
            ..tls::ClientTlsOptions::default()
        },
        device,
        token: TOKEN.to_owned(),
        routes: vec![template],
        network,
        timeouts: Default::default(),
        metrics: metrics.clone(),
    });
    let ready = wait_for_ready(&mut agent).await;
    assert_eq!(ready.len(), 1);

    let visitor = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("visitor bind");

    // At the ceiling: forwarded and echoed back.
    let small = [7_u8; 512];
    let mut echoed = None;
    for attempt in 0..5 {
        visitor
            .send_to(&small, ("127.0.0.1", public_port))
            .await
            .expect("datagram sent");
        let mut buffer = [0_u8; 1024];
        match tokio::time::timeout(Duration::from_secs(2), visitor.recv_from(&mut buffer)).await {
            Ok(Ok((size, _))) => {
                echoed = Some(size);
                break;
            }
            _ if attempt < 4 => continue,
            other => panic!("no udp reply at the ceiling: {other:?}"),
        }
    }
    assert_eq!(echoed, Some(512));

    // The retries above may have left echoes in flight (every datagram the
    // visitor sent before the reply arrived is still answered). Drain them, or
    // they would be mistaken for a reply to the oversized datagram below.
    let mut drain = [0_u8; 1024];
    while tokio::time::timeout(Duration::from_millis(300), visitor.recv_from(&mut drain))
        .await
        .is_ok()
    {}

    // Above the ceiling: dropped, and the visitor gets nothing back.
    let large = [9_u8; 900];
    visitor
        .send_to(&large, ("127.0.0.1", public_port))
        .await
        .expect("oversized datagram sent");
    let mut buffer = [0_u8; 1024];
    match tokio::time::timeout(Duration::from_millis(1500), visitor.recv_from(&mut buffer)).await {
        Ok(Ok((size, _))) => panic!("an oversized datagram must not be relayed, got {size} bytes"),
        Ok(Err(error)) => panic!("unexpected socket error: {error}"),
        Err(_) => {}
    }
    assert!(
        gateway_metrics.udp_datagrams_too_large() >= 1,
        "the drop must be counted: {}",
        gateway_metrics.udp_datagrams_too_large()
    );

    agent.shutdown().await;
    echo_task.abort();
    gateway.shutdown().await;
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
    let public_port = available_udp_port();

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
        proxy_protocol: None,
        locations: Vec::new(),
        ..TunnelRouteTemplate::default()
    };
    let device = Device::new(
        DeviceId::parse("dev_udp").expect("valid id"),
        "udp-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
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

#[tokio::test]
async fn wildcard_http_route_matches_subdomains_only() {
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

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

    let (http_port, seen_http, http_task) = spawn_http_target().await;

    let mut template = http_template("*.wild.test");
    template.target = format!("127.0.0.1:{http_port}");
    let device = Device::new(
        DeviceId::parse("dev_wild").expect("valid id"),
        "wild-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
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

    let shared = gateway.shared();

    // A subdomain of the wildcard relays to the local target.
    let mut relayed = shared
        .connect_http_stream(
            "app.wild.test",
            "",
            RelayVisitor {
                ip: "203.0.113.5".parse().expect("ip"),
                port: 0,
                bearer: None,
            },
        )
        .await
        .expect("wildcard subdomain relays");
    relayed
        .write_all(
            b"GET / HTTP/1.0
Host: app.wild.test

",
        )
        .await
        .expect("request written");
    let mut buffer = [0_u8; 256];
    let read = tokio::time::timeout(Duration::from_secs(5), relayed.read(&mut buffer))
        .await
        .expect("response within budget")
        .expect("relay read");
    assert!(read > 0, "expected a relayed response");

    // The bare suffix and foreign hosts do not match the wildcard.
    for host in ["wild.test", "app.other.test"] {
        let outcome = shared
            .connect_http_stream(
                host,
                "",
                RelayVisitor {
                    ip: "203.0.113.5".parse().expect("ip"),
                    port: 0,
                    bearer: None,
                },
            )
            .await;
        assert!(
            matches!(
                outcome,
                Err(sdkwork_webserver_tunnel_core::TunnelError::RouteNotFound)
            ),
            "host {host} must not match the wildcard route"
        );
    }

    drop(seen_http.lock().expect("seen lock"));
    agent.shutdown().await;
    http_task.abort();
    gateway.shutdown().await;
}

#[tokio::test]
async fn private_udp_route_denies_anonymous_visitors() {
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    // Local UDP recorder: echoes AND records every datagram it receives.
    let echo = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("echo bind");
    let echo_port = echo.local_addr().expect("echo addr").port();
    let received = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let received_task = received.clone();
    let echo_task = tokio::spawn(async move {
        let mut buffer = [0_u8; 2048];
        loop {
            let Ok((size, peer)) = echo.recv_from(&mut buffer).await else {
                return;
            };
            received_task.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
    let public_port = available_udp_port();

    // A PRIVATE udp route: anonymous datagrams must be refused by the same
    // admission gate the HTTP and TCP planes use.
    let template = TunnelRouteTemplate {
        name: "secret-udp".to_owned(),
        protocol: TunnelProtocolKind::Udp,
        domain: None,
        port: Some(public_port),
        target: format!("127.0.0.1:{echo_port}"),
        policy: Some(sdkwork_webserver_tunnel_core::RoutePolicy::private()),
        proxy_protocol: None,
        locations: Vec::new(),
        ..TunnelRouteTemplate::default()
    };
    let device = Device::new(
        DeviceId::parse("dev_udp_priv").expect("valid id"),
        "udp-private-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
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

    let visitor = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("visitor bind");
    for _ in 0..3 {
        visitor
            .send_to(b"denied-ping", ("127.0.0.1", public_port))
            .await
            .expect("datagram sent");
    }
    // Give any (incorrect) relay a chance to arrive, then assert silence.
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(
        received.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "private udp routes must not relay anonymous datagrams"
    );

    agent.shutdown().await;
    echo_task.abort();
    gateway.shutdown().await;
}

/// SSH over a TCP route, pinned at the properties SSH actually depends on.
///
/// The echo test above has the *visitor* speak first, which every relay gets
/// right by accident. SSH does not: RFC 4253 §4.2 has the server send its
/// identification string immediately on connect, before the client writes a
/// single byte. A relay that only dials its local target once client data
/// arrives deadlocks there — the client waits for a banner that waits for the
/// client. So this test reads the banner first and sends nothing until it has
/// it, then checks the three other properties a terminal session needs:
/// byte transparency (SSH packets are binary, not UTF-8), full duplex at rate,
/// and half-close propagation when the client ends its side.
#[tokio::test]
async fn tcp_route_relays_a_server_first_binary_protocol() {
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

    const BANNER: &[u8] = b"SSH-2.0-SDKWorkTest_1.0\r\n";
    const CLIENT_ID: &[u8] = b"SSH-2.0-ClientTest\r\n";
    /// Every byte value, so a text-oriented hop is caught.
    fn binary_frame() -> Vec<u8> {
        (0..=255_u8).collect()
    }
    const BULK: usize = 128 * 1024;

    // The local target, shaped like an SSH server: banner first, then a
    // binary frame, then echo, then surface the client's EOF.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("target bind");
    let target_port = listener.local_addr().expect("addr").port();
    let saw_eof = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let saw_eof_task = Arc::clone(&saw_eof);
    let target_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let saw_eof = Arc::clone(&saw_eof_task);
            tokio::spawn(async move {
                // Server-first: nothing has been read yet.
                if socket.write_all(BANNER).await.is_err() {
                    return;
                }
                // Read the client's own identification string.
                let mut received = Vec::new();
                let mut buffer = [0_u8; 512];
                while !received.windows(2).any(|window| window == b"\r\n") {
                    let Ok(read) = socket.read(&mut buffer).await else {
                        return;
                    };
                    if read == 0 {
                        return;
                    }
                    received.extend_from_slice(&buffer[..read]);
                    if received.len() > 4096 {
                        return;
                    }
                }
                let frame = binary_frame();
                if socket.write_all(&frame).await.is_err() {
                    return;
                }
                // Then plain echo until the client half-closes.
                loop {
                    let read = socket.read(&mut buffer).await.unwrap_or(0);
                    if read == 0 {
                        saw_eof.store(true, std::sync::atomic::Ordering::SeqCst);
                        let _ = socket.shutdown().await;
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
    let public_port = available_port();

    let template = TunnelRouteTemplate {
        name: "ssh".to_owned(),
        protocol: TunnelProtocolKind::Tcp,
        domain: None,
        port: Some(public_port),
        target: format!("127.0.0.1:{target_port}"),
        policy: Some(sdkwork_webserver_tunnel_core::RoutePolicy {
            allow_public: true,
            ..sdkwork_webserver_tunnel_core::RoutePolicy::private()
        }),
        proxy_protocol: None,
        locations: Vec::new(),
        ..TunnelRouteTemplate::default()
    };
    let device = Device::new(
        DeviceId::parse("dev_ssh").expect("valid id"),
        "ssh-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
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

    let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", public_port))
        .await
        .expect("visitor connect");
    // The whole point: read before writing anything.
    let mut banner = vec![0_u8; BANNER.len()];
    tokio::time::timeout(Duration::from_secs(5), visitor.read_exact(&mut banner))
        .await
        .expect("a server-first banner must arrive without the visitor writing first")
        .expect("banner read");
    assert_eq!(banner, BANNER);

    visitor
        .write_all(CLIENT_ID)
        .await
        .expect("client identification written");
    let frame = binary_frame();
    let mut received_frame = vec![0_u8; frame.len()];
    tokio::time::timeout(
        Duration::from_secs(5),
        visitor.read_exact(&mut received_frame),
    )
    .await
    .expect("binary frame within timeout")
    .expect("binary frame read");
    assert_eq!(received_frame, frame, "the relay is byte-transparent");

    // Full duplex at rate: 128 KiB each way.
    let bulk: Vec<u8> = (0..BULK).map(|index| (index % 251) as u8).collect();
    visitor
        .write_all(&bulk)
        .await
        .expect("bulk payload written");
    let mut echoed = vec![0_u8; BULK];
    tokio::time::timeout(Duration::from_secs(10), visitor.read_exact(&mut echoed))
        .await
        .expect("bulk echo within timeout")
        .expect("bulk echo read");
    assert_eq!(echoed, bulk);

    // Half-close: an SSH client ends its side at logout, and the local target
    // must see the EOF rather than a connection that lingers until timeout.
    visitor.shutdown().await.expect("visitor half-close");
    let mut trailing = [0_u8; 1];
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), visitor.read(&mut trailing))
            .await
            .expect("the local target's EOF reaches the visitor")
            .expect("read propagated half-close"),
        0
    );
    assert!(
        saw_eof.load(std::sync::atomic::Ordering::SeqCst),
        "the local target observed the client's end of stream"
    );

    agent.shutdown().await;
    target_task.abort();
    gateway.shutdown().await;
}

/// The visitor-token policy must survive the control plane: the agent builds
/// the route from its local template, the policy travels in the registration,
/// and the gateway enforces it at admission (FRP `httpUser`/`httpPassword`
/// parity).
///
/// Before the policy crossed the wire, the gateway rebuilt every registration
/// as `auth = none` while keeping `allow_public = false`, so this route denied
/// every visitor — including the one holding the right token. The test asserts
/// both directions: the token is required *and* it works.
#[tokio::test]
async fn http_route_visitor_tokens_are_enforced_at_the_gateway() {
    crypto_provider();
    init_logs();
    let (cert_pem, key_pem, fingerprint) = tls_material();

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

    let (http_port, _seen, http_task) = spawn_http_target().await;

    let mut template = http_template("protected.tunnel.test");
    template.target = format!("127.0.0.1:{http_port}");
    template.policy = Some(sdkwork_webserver_tunnel_core::RoutePolicy {
        allow_public: false,
        allowed_ips: Vec::new(),
        auth: sdkwork_webserver_tunnel_core::AuthPolicy::BearerToken,
        visitor_tokens: vec!["visitor-s3cret".to_owned()],
    });
    let device = Device::new(
        DeviceId::parse("dev_bearer").expect("valid id"),
        "bearer-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        alternates: Vec::new(),
        login_fail_exit: false,
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

    // The gateway must have registered the token policy, not a private route
    // with no admission channel.
    let shared = gateway.shared();
    let registered = shared
        .registry
        .match_domain("protected.tunnel.test")
        .expect("route is registered");
    assert_eq!(
        registered.route.policy.auth,
        sdkwork_webserver_tunnel_core::AuthPolicy::BearerToken,
        "the visitor-token policy must survive the registration"
    );
    assert_eq!(
        registered.route.policy.visitor_tokens,
        vec!["visitor-s3cret".to_owned()]
    );

    // Anonymous visitor: denied.
    let anonymous = shared
        .connect_http_stream(
            "protected.tunnel.test",
            "",
            RelayVisitor {
                ip: "203.0.113.5".parse().expect("ip"),
                port: 0,
                bearer: None,
            },
        )
        .await;
    assert!(
        matches!(
            anonymous,
            Err(sdkwork_webserver_tunnel_core::TunnelError::AuthorizationDenied)
        ),
        "an anonymous visitor must be denied, got {anonymous:?}"
    );

    // Wrong token: denied.
    let wrong = shared
        .connect_http_stream(
            "protected.tunnel.test",
            "",
            RelayVisitor {
                ip: "203.0.113.5".parse().expect("ip"),
                port: 0,
                bearer: Some("not-the-token"),
            },
        )
        .await;
    assert!(
        matches!(
            wrong,
            Err(sdkwork_webserver_tunnel_core::TunnelError::AuthorizationDenied)
        ),
        "a wrong token must be denied, got {wrong:?}"
    );

    // Right token: relayed to the local target.
    let mut relayed = shared
        .connect_http_stream(
            "protected.tunnel.test",
            "",
            RelayVisitor {
                ip: "203.0.113.5".parse().expect("ip"),
                port: 0,
                bearer: Some("visitor-s3cret"),
            },
        )
        .await
        .expect("the right token admits the visitor");
    relayed
        .write_all(b"GET /protected HTTP/1.0\r\nHost: protected.tunnel.test\r\n\r\n")
        .await
        .expect("request written");
    let mut response = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
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
    let text = String::from_utf8_lossy(&response).to_string();
    assert!(text.contains("200 OK"), "got: {text}");
    assert!(text.contains("HELLO"), "got: {text}");

    // The denial counter is what an operator alerts on; it must not be
    // confused with agent credential failures.
    let snapshot = gateway.shared().metrics.snapshot();
    assert_eq!(snapshot.relay_denied, 2, "two denials were counted");
    assert_eq!(snapshot.auth_failures, 0);

    agent.shutdown().await;
    http_task.abort();
    gateway.shutdown().await;
}
