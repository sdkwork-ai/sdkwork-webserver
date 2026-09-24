//! Cluster membership client end-to-end coverage.
//!
//! Exercises the full member lifecycle (register → heartbeat → sync
//! manifest → apply → ack) against an in-process admin that implements the
//! machine-only internal API contract, over both transports the member
//! speaks: direct HTTP (`LAN`) and HTTP relayed through the reverse tunnel
//! (`TUNNEL`).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::{Json, Router};
use sdkwork_webserver_agent::cluster_member::{
    ClusterMember, ClusterMemberIdentity, ClusterTransport, SyncApplier,
};
use sdkwork_webserver_contract::{
    ClusterHeartbeatResponse, ClusterRegistrationRequest, ClusterRegistrationResponse,
    ClusterSyncAckRequest, ClusterSyncManifest, ClusterSyncState,
};
use sdkwork_webserver_tunnel::agent::{
    AgentEvent, AgentRuntime, AgentRuntimeOptions, RouteReadiness,
};
use sdkwork_webserver_tunnel::gateway::{TunnelGateway, TunnelGatewayOptions};
use sdkwork_webserver_tunnel::metrics::TunnelMetrics;
use sdkwork_webserver_tunnel::security::TokenAuthenticator;
use sdkwork_webserver_tunnel_core::{
    Device, DeviceId, DevicePlatform, RoutePolicy, TunnelConfig, TunnelProtocolKind,
    TunnelRouteTemplate,
};
use sdkwork_webserver_tunnel_transport::{tls, RemoteEndpoint};
use tokio::sync::{watch, Mutex};

/// The fake admin: a register endpoint issuing a fixed token, a heartbeat
/// endpoint advertising one desired config revision, the manifest endpoint
/// serving that revision, and the ack endpoint recording outcomes.
///
/// `drain_requested` flips the heartbeat into the cordon/drain posture the
/// registry uses to ask a node to leave service, and `drain_completes` counts
/// the node's completion reports.
#[derive(Default)]
struct AdminState {
    registrations: AtomicUsize,
    heartbeats: AtomicUsize,
    acks: Mutex<Vec<(String, String)>>,
    /// The `(joinMode, tunnelRouteDomain)` pair each registration advertised,
    /// so a test can prove the wire carried the mode it meant to.
    advertised_modes: Mutex<Vec<(String, Option<String>)>>,
    drain_requested: bool,
    drain_completes: AtomicUsize,
}

fn admin_router(state: Arc<AdminState>) -> Router {
    async fn register(
        axum::extract::State(state): axum::extract::State<Arc<AdminState>>,
        Json(request): Json<ClusterRegistrationRequest>,
    ) -> Json<ClusterRegistrationResponse> {
        state.registrations.fetch_add(1, Ordering::SeqCst);
        state.advertised_modes.lock().await.push((
            request.instance.join_mode.clone().unwrap_or_default(),
            request
                .host
                .tunnel
                .as_ref()
                .map(|tunnel| tunnel.route_domain.clone()),
        ));
        Json(ClusterRegistrationResponse {
            cluster: sdkwork_webserver_contract::ClusterRef {
                id: "cluster-1".to_owned(),
                name: "default".to_owned(),
                code: request.cluster_code.unwrap_or_else(|| "default".to_owned()),
            },
            host: sdkwork_webserver_contract::ClusterHostRef {
                id: "host-1".to_owned(),
                name: request.host.hostname.clone(),
                hostname: request.host.hostname,
            },
            instance: sdkwork_webserver_contract::ClusterInstanceRef {
                id: "instance-1".to_owned(),
                name: "gateway".to_owned(),
                role: "GATEWAY".to_owned(),
            },
            instance_token: "winst_test-token".to_owned(),
            heartbeat_interval_seconds: 1,
            offline_threshold_seconds: 30,
            peers: Vec::new(),
        })
    }

    async fn heartbeat(
        axum::extract::State(state): axum::extract::State<Arc<AdminState>>,
    ) -> Json<ClusterHeartbeatResponse> {
        state.heartbeats.fetch_add(1, Ordering::SeqCst);
        Json(ClusterHeartbeatResponse {
            instance_id: "instance-1".to_owned(),
            status: 1,
            acknowledged_at: "2026-09-21T00:00:00Z".to_owned(),
            heartbeat_interval_seconds: 1,
            offline_threshold_seconds: 30,
            peers: Vec::new(),
            messages: Vec::new(),
            ops: state.drain_requested.then(|| {
                sdkwork_webserver_contract::ClusterInstanceOpsDirectives {
                    routing_enabled: false,
                    drain_requested: true,
                }
            }),
            sync: Some(vec![ClusterSyncState {
                kind: "config".to_owned(),
                desired_revision: Some("rev-1".to_owned()),
                applied_revision: None,
                status: "PENDING".to_owned(),
                updated_at: None,
            }]),
        })
    }

    /// The node's drain-completion acknowledgment route. Takes no request body
    /// and authenticates with the node's own `winst_` token, exactly like the
    /// heartbeat.
    async fn drain_complete(
        axum::extract::State(state): axum::extract::State<Arc<AdminState>>,
    ) -> Json<sdkwork_webserver_contract::ClusterDrainCompleteResponse> {
        state.drain_completes.fetch_add(1, Ordering::SeqCst);
        Json(sdkwork_webserver_contract::ClusterDrainCompleteResponse {
            acknowledged_at: "2026-09-21T00:00:00Z".to_owned(),
        })
    }

    async fn manifest() -> Json<ClusterSyncManifest> {
        Json(ClusterSyncManifest {
            cluster_id: "cluster-1".to_owned(),
            kind: "config".to_owned(),
            revision: "rev-1".to_owned(),
            sha256: "abc".to_owned(),
            payload: serde_json::json!({ "listeners": [] }),
            created_at: "2026-09-21T00:00:00Z".to_owned(),
        })
    }

    async fn ack(
        state: axum::extract::State<Arc<AdminState>>,
        Json(request): Json<ClusterSyncAckRequest>,
    ) -> Json<ClusterSyncState> {
        state
            .acks
            .lock()
            .await
            .push((request.revision.clone(), request.status.clone()));
        Json(ClusterSyncState {
            kind: request.kind,
            desired_revision: Some(request.revision.clone()),
            applied_revision: Some(request.revision),
            status: request.status,
            updated_at: None,
        })
    }

    Router::new()
        .route(
            "/internal/v3/api/web/cluster/instances/register",
            axum::routing::post(register),
        )
        .route(
            "/internal/v3/api/web/cluster/instances/heartbeat",
            axum::routing::post(heartbeat),
        )
        .route(
            "/internal/v3/api/web/cluster/sync/manifest",
            axum::routing::get(manifest),
        )
        .route(
            "/internal/v3/api/web/cluster/sync/ack",
            axum::routing::post(ack),
        )
        .route(
            "/internal/v3/api/web/cluster/instances/drain_complete",
            axum::routing::post(drain_complete),
        )
        .with_state(state)
}

struct RecordingApplier {
    applied: Mutex<Vec<String>>,
    drains: AtomicUsize,
}

#[async_trait::async_trait]
impl SyncApplier for RecordingApplier {
    async fn apply(&self, kind: &str, manifest: &ClusterSyncManifest) -> Result<(), String> {
        assert_eq!(kind, "config");
        assert_eq!(manifest.revision, "rev-1");
        self.applied.lock().await.push(manifest.revision.clone());
        Ok(())
    }

    async fn drain(&self) -> Result<(), String> {
        self.drains.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// The node identity every test registers with; only the fields the LAN
/// transport consumes are meaningful here.
fn test_identity() -> ClusterMemberIdentity {
    ClusterMemberIdentity {
        machine_code: "mc-test".to_owned(),
        hostname: "node-a".to_owned(),
        cluster_code: Some("default".to_owned()),
        role: "GATEWAY".to_owned(),
        environment: "test".to_owned(),
        process_pid: 4242,
        process_started_at: "2026-09-21T00:00:00Z".to_owned(),
        // The node's slot: what makes a restart land on the same instance
        // instead of registering a second one.
        bind_host: Some("0.0.0.0".to_owned()),
        bind_port: Some(3800),
        build_version: "test".to_owned(),
        join_mode: "LAN".to_owned(),
        tunnel_route_domain: None,
        tunnel_endpoint: None,
    }
}

async fn spawn_fake_admin(state: Arc<AdminState>) -> (u16, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let server = tokio::spawn(async move {
        axum::serve(listener, admin_router(state))
            .await
            .expect("serve");
    });
    (port, server)
}

fn lan_transport(port: u16) -> ClusterTransport {
    ClusterTransport::Lan {
        base_authority: format!("127.0.0.1:{port}"),
    }
}

fn recording_applier() -> Arc<RecordingApplier> {
    Arc::new(RecordingApplier {
        applied: Mutex::new(Vec::new()),
        drains: AtomicUsize::new(0),
    })
}

/// Coerces a concrete applier into the trait object the membership loop takes.
/// Explicit call site beats an `as` cast, which the workspace lints flag.
fn as_applier<T: SyncApplier + 'static>(applier: Arc<T>) -> Arc<dyn SyncApplier> {
    applier
}

#[tokio::test]
async fn member_registers_heartbeats_and_syncs_over_lan() {
    let state = Arc::new(AdminState::default());
    let (port, server) = spawn_fake_admin(Arc::clone(&state)).await;
    let applier = recording_applier();

    let (stop_tx, stop_rx) = watch::channel(false);
    let member = ClusterMember::spawn(
        lan_transport(port),
        "wreg_test".to_owned(),
        test_identity(),
        as_applier(Arc::clone(&applier)),
        stop_rx,
    );

    // The loop registers, heartbeats (1s cadence), applies rev-1, and acks.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let _ = stop_tx.send(true);
    member.shutdown().await;

    assert_eq!(
        state.registrations.load(Ordering::SeqCst),
        1,
        "registration is idempotent per tick"
    );
    assert!(
        state.heartbeats.load(Ordering::SeqCst) >= 1,
        "at least one heartbeat reached the registry"
    );
    let applied = applier.applied.lock().await;
    assert!(
        !applied.is_empty(),
        "the desired revision was fetched and applied"
    );
    assert!(applied.iter().all(|revision| revision == "rev-1"));
    drop(applied);
    let acks = state.acks.lock().await.clone();
    assert!(!acks.is_empty(), "the applied revision was acknowledged");
    assert!(
        acks.iter()
            .all(|(revision, status)| revision == "rev-1" && status == "IN_SYNC"),
        "only in-sync acknowledgements: {acks:?}"
    );
    assert_eq!(
        state.drain_completes.load(Ordering::SeqCst),
        0,
        "no drain was requested, so nothing may report completion"
    );
    server.abort();
}

/// Cordon/drain, node half: the registry answers `ops.drainRequested`, the
/// host finishes in-flight work, and the node closes the loop. The heartbeat
/// must then *stop* — heartbeat carries `status: 1`, so one more tick would
/// put a stopped instance back online and leave the operator's drain open
/// forever.
#[tokio::test]
async fn drain_request_finishes_work_reports_completion_and_stops_heartbeating() {
    let state = Arc::new(AdminState {
        drain_requested: true,
        ..Default::default()
    });
    let (port, server) = spawn_fake_admin(Arc::clone(&state)).await;
    let applier = recording_applier();

    let (stop_tx, stop_rx) = watch::channel(false);
    let member = ClusterMember::spawn(
        lan_transport(port),
        "wreg_test".to_owned(),
        test_identity(),
        as_applier(Arc::clone(&applier)),
        stop_rx,
    );

    // Register → first heartbeat carries the directive → host drains → the
    // node reports completion and leaves the loop.
    tokio::time::sleep(Duration::from_millis(1500)).await;

    assert_eq!(
        applier.drains.load(Ordering::SeqCst),
        1,
        "the host was asked to finish in-flight work"
    );
    assert_eq!(
        state.drain_completes.load(Ordering::SeqCst),
        1,
        "the registry's drain loop was closed exactly once"
    );

    // The loop must be gone: at a 1s cadence another tick would land here.
    let heartbeats_after_drain = state.heartbeats.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        state.heartbeats.load(Ordering::SeqCst),
        heartbeats_after_drain,
        "a drained instance must not heartbeat itself back online"
    );

    let _ = stop_tx.send(true);
    member.shutdown().await;
    server.abort();
}

/// The drain report is the *last* obligation: if the host still has in-flight
/// work, the node stays registered and never claims completion, so the
/// registry keeps the drain open instead of marking a busy instance stopped.
#[tokio::test]
async fn a_host_that_cannot_drain_yet_keeps_the_instance_registered() {
    struct BusyApplier;

    #[async_trait::async_trait]
    impl SyncApplier for BusyApplier {
        async fn apply(&self, _kind: &str, _manifest: &ClusterSyncManifest) -> Result<(), String> {
            Ok(())
        }

        async fn drain(&self) -> Result<(), String> {
            Err("in-flight requests are still open".to_owned())
        }
    }

    let state = Arc::new(AdminState {
        drain_requested: true,
        ..Default::default()
    });
    let (port, server) = spawn_fake_admin(Arc::clone(&state)).await;

    let (stop_tx, stop_rx) = watch::channel(false);
    let member = ClusterMember::spawn(
        lan_transport(port),
        "wreg_test".to_owned(),
        test_identity(),
        as_applier(Arc::new(BusyApplier)),
        stop_rx,
    );

    tokio::time::sleep(Duration::from_millis(1500)).await;

    assert_eq!(
        state.drain_completes.load(Ordering::SeqCst),
        0,
        "unfinished work must not be reported as a completed drain"
    );
    assert!(
        state.heartbeats.load(Ordering::SeqCst) >= 1,
        "the node stays registered and keeps retrying the drain"
    );

    let _ = stop_tx.send(true);
    member.shutdown().await;
    server.abort();
}

/// The `TUNNEL` join mode, end to end. A node behind NAT has no direct TCP
/// path to the control plane, so every register/heartbeat/manifest/ack call
/// has to travel through the reverse tunnel: this node's own tunnel gateway
/// relays the HTTP, and the route is registered by an agent whose local target
/// is the cluster admin API. Nothing here reaches the admin over plain TCP —
/// the only socket the member opens is the relayed stream — so a regression
/// that silently fell back to the LAN path would show up as a connect failure
/// rather than as a passing test.
const TUNNEL_TOKEN: &str = "e2e-tunnel-t0ken";
const TUNNEL_ROUTE_DOMAIN: &str = "cluster-admin.tunnel.test";

fn tunnel_route_template(admin_port: u16) -> TunnelRouteTemplate {
    TunnelRouteTemplate {
        name: "cluster-admin".to_owned(),
        protocol: TunnelProtocolKind::Http,
        domain: Some(TUNNEL_ROUTE_DOMAIN.to_owned()),
        port: None,
        target: format!("127.0.0.1:{admin_port}"),
        policy: Some(RoutePolicy {
            allow_public: true,
            ..RoutePolicy::private()
        }),
    }
}

async fn wait_for_route_ready(agent: &mut AgentRuntime) -> Vec<RouteReadiness> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let Some(remaining) = deadline.checked_duration_since(tokio::time::Instant::now()) else {
            panic!("the tunnel agent never became ready");
        };
        match tokio::time::timeout(remaining, agent.next_event()).await {
            Ok(Some(AgentEvent::Ready { routes, rejected })) => {
                assert!(
                    rejected.is_empty(),
                    "unexpected route rejections: {rejected:?}"
                );
                return routes;
            }
            Ok(Some(_)) => {}
            Ok(None) => panic!("agent event channel closed before ready"),
            Err(_) => panic!("timed out waiting for the tunnel agent to become ready"),
        }
    }
}

fn tunnel_identity() -> ClusterMemberIdentity {
    ClusterMemberIdentity {
        join_mode: "TUNNEL".to_owned(),
        tunnel_route_domain: Some(TUNNEL_ROUTE_DOMAIN.to_owned()),
        tunnel_endpoint: None,
        ..test_identity()
    }
}

#[tokio::test]
async fn member_registers_heartbeats_and_syncs_over_the_tunnel() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // The node-side gateway: one QUIC listener, one shared token.
    let material = tls::generate_self_signed(&["localhost".to_owned(), "127.0.0.1".to_owned()])
        .expect("self-signed material");
    let mut gateway_options = TunnelGatewayOptions::from_config(
        &TunnelConfig::disabled(),
        "127.0.0.1:0".parse().expect("bind"),
        material.cert_pem.clone().into_bytes(),
        material.key_pem.clone().into_bytes(),
    );
    gateway_options.authenticator = TokenAuthenticator::new(vec![TUNNEL_TOKEN.to_owned()]);
    let gateway = TunnelGateway::spawn(gateway_options, None)
        .await
        .expect("gateway spawns");
    let quic_port = gateway.quic_port();

    let state = Arc::new(AdminState::default());
    let (admin_port, server) = spawn_fake_admin(Arc::clone(&state)).await;
    let applier = recording_applier();

    // The route owner: an agent whose local target is the cluster admin API.
    let device = Device::new(
        DeviceId::parse("dev_tunnel_e2e").expect("valid device id"),
        "tunnel-e2e-runner",
        DevicePlatform::Linux,
    )
    .expect("valid device");
    let mut agent = AgentRuntime::spawn(AgentRuntimeOptions {
        endpoint: RemoteEndpoint::new("127.0.0.1", quic_port),
        tls: tls::ClientTlsOptions {
            pinned_server_sha256: Some(material.sha256.clone()),
            ..tls::ClientTlsOptions::default()
        },
        device,
        token: TUNNEL_TOKEN.to_owned(),
        routes: vec![tunnel_route_template(admin_port)],
        network: Default::default(),
        timeouts: Default::default(),
        metrics: Arc::new(TunnelMetrics::new()),
    });
    let ready = wait_for_route_ready(&mut agent).await;
    assert_eq!(ready.len(), 1, "the cluster-admin route registered");
    assert_eq!(
        ready[0].public_url.as_deref(),
        Some("https://cluster-admin.tunnel.test")
    );

    let (stop_tx, stop_rx) = watch::channel(false);
    let member = ClusterMember::spawn(
        ClusterTransport::Tunnel {
            host: TUNNEL_ROUTE_DOMAIN.to_owned(),
            shared: gateway.shared(),
        },
        "wreg_test".to_owned(),
        tunnel_identity(),
        as_applier(Arc::clone(&applier)),
        stop_rx,
    );

    // Same lifecycle as the LAN case, one relayed request at a time.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let registered = state.registrations.load(Ordering::SeqCst) >= 1;
        let acked = !state.acks.lock().await.is_empty();
        if registered && acked {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the tunnelled membership never completed: registrations={}, acks={:?}",
            state.registrations.load(Ordering::SeqCst),
            state.acks.lock().await.clone()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let _ = stop_tx.send(true);
    member.shutdown().await;

    assert_eq!(
        state.registrations.load(Ordering::SeqCst),
        1,
        "registration is idempotent per tick, tunnel or not"
    );
    assert!(
        state.heartbeats.load(Ordering::SeqCst) >= 1,
        "at least one heartbeat reached the registry through the relay"
    );
    let applied = applier.applied.lock().await;
    assert_eq!(
        applied.as_slice(),
        ["rev-1".to_owned()],
        "the desired revision was fetched through the relay and applied once"
    );
    drop(applied);
    let acks = state.acks.lock().await.clone();
    assert!(
        acks.iter()
            .all(|(revision, status)| revision == "rev-1" && status == "IN_SYNC"),
        "only in-sync acknowledgements: {acks:?}"
    );
    let advertised = state.advertised_modes.lock().await.clone();
    assert_eq!(
        advertised,
        vec![("TUNNEL".to_owned(), Some(TUNNEL_ROUTE_DOMAIN.to_owned()))],
        "the wire carried the TUNNEL mode with its route domain"
    );

    agent.shutdown().await;
    gateway.shutdown().await;
    server.abort();
}
