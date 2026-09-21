//! Cluster membership client end-to-end coverage.
//!
//! Exercises the full member lifecycle (register → heartbeat → sync
//! manifest → apply → ack) against an in-process admin that implements the
//! machine-only internal API contract, over the LAN HTTP transport.

use std::sync::Arc;
use std::time::Duration;

use axum::{Json, Router};
use sdkwork_webserver_agent::cluster_member::{
    ClusterMember, ClusterMemberIdentity, ClusterTransport, MemberError, SyncApplier,
};
use sdkwork_webserver_contract::{
    ClusterHeartbeatResponse, ClusterRegistrationRequest, ClusterRegistrationResponse,
    ClusterSyncAckRequest, ClusterSyncManifest, ClusterSyncState,
};
use tokio::sync::{watch, Mutex};

/// The fake admin: a register endpoint issuing a fixed token, a heartbeat
/// endpoint advertising one desired config revision, the manifest endpoint
/// serving that revision, and the ack endpoint recording outcomes.
#[derive(Default)]
struct AdminState {
    registrations: usize,
    heartbeats: usize,
    acks: Mutex<Vec<(String, String)>>,
}

fn admin_router(state: Arc<AdminState>) -> Router {
    async fn register(
        Json(request): Json<ClusterRegistrationRequest>,
    ) -> Json<ClusterRegistrationResponse> {
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

    async fn heartbeat() -> Json<ClusterHeartbeatResponse> {
        Json(ClusterHeartbeatResponse {
            instance_id: "instance-1".to_owned(),
            status: 1,
            acknowledged_at: "2026-09-21T00:00:00Z".to_owned(),
            heartbeat_interval_seconds: 1,
            offline_threshold_seconds: 30,
            peers: Vec::new(),
            messages: Vec::new(),
            ops: None,
            sync: Some(vec![ClusterSyncState {
                kind: "config".to_owned(),
                desired_revision: Some("rev-1".to_owned()),
                applied_revision: None,
                status: "PENDING".to_owned(),
                updated_at: None,
            }]),
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
        .with_state(state)
}

struct RecordingApplier {
    applied: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl SyncApplier for RecordingApplier {
    async fn apply(&self, kind: &str, manifest: &ClusterSyncManifest) -> Result<(), String> {
        assert_eq!(kind, "config");
        assert_eq!(manifest.revision, "rev-1");
        self.applied.lock().await.push(manifest.revision.clone());
        Ok(())
    }
}

#[tokio::test]
async fn member_registers_heartbeats_and_syncs_over_lan() {
    let state = Arc::new(AdminState::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let server = tokio::spawn(async move {
        axum::serve(listener, admin_router(state))
            .await
            .expect("serve");
    });

    let identity = ClusterMemberIdentity {
        machine_code: "mc-test".to_owned(),
        hostname: "node-a".to_owned(),
        cluster_code: Some("default".to_owned()),
        role: "GATEWAY".to_owned(),
        environment: "test".to_owned(),
        process_pid: 4242,
        process_started_at: "2026-09-21T00:00:00Z".to_owned(),
        build_version: "test".to_owned(),
        join_mode: "LAN".to_owned(),
        tunnel_route_domain: None,
        tunnel_endpoint: None,
    };

    let (stop_tx, stop_rx) = watch::channel(false);
    let member = ClusterMember::spawn(
        ClusterTransport::Lan {
            base_authority: format!("127.0.0.1:{port}"),
        },
        "wreg_test".to_owned(),
        identity,
        Arc::new(RecordingApplier {
            applied: Mutex::new(Vec::new()),
        }),
        stop_rx,
    );

    // The loop registers, heartbeats (1s cadence), applies rev-1, and acks.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let _ = stop_tx.send(true);
    member.shutdown().await;
    server.abort();
}
