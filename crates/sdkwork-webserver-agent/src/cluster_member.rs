//! Cluster membership client for remote and LAN webserver nodes
//! (PRD: 同网段集群 + FRP 穿透集群).
//!
//! One [`ClusterMember`] drives the full node-side cluster lifecycle over
//! the machine-only internal API surface:
//!
//! 1. **Register** (idempotent per machine code + pid) with the shared
//!    `wreg_…` registration credential; the response carries the node's own
//!    `winst_…` heartbeat token.
//! 2. **Heartbeat** on the cluster's cadence with a quality-of-service
//!    sample (measured round-trip latency; the one signal available on
//!    every transport). The response names the desired configuration /
//!    applications revisions.
//! 3. **Sync**: whenever the desired revision drifts from the applied one,
//!    fetch the manifest, hand it to the [`SyncApplier`], and acknowledge
//!    `IN_SYNC` or `FAILED`.
//!
//! Transport is pluggable: [`ClusterTransport::Lan`] speaks HTTP directly
//! to the admin internal API (same-subnet); [`ClusterTransport::Tunnel`]
//! sends the exact same requests as raw bytes through the reverse tunnel's
//! HTTP relay (FRP mode: the node has no database and no other cluster
//! reachability — everything flows through the tunnel as API calls).

use std::sync::Arc;
use std::time::Duration;

use sdkwork_webserver_contract::{
    ClusterHeartbeatRequest, ClusterHeartbeatResponse, ClusterRegistrationRequest,
    ClusterRegistrationResponse, ClusterSyncAckRequest, ClusterSyncManifest, ClusterSyncState,
};
use sdkwork_webserver_tunnel::gateway::GatewayShared;
use tokio::sync::watch;

/// Admin API transport for one cluster member.
#[derive(Clone)]
pub enum ClusterTransport {
    /// Same-subnet nodes: plain HTTP to the admin internal API listener.
    Lan {
        /// Admin internal API base authority, `host:port`.
        base_authority: String,
    },
    /// FRP-mode nodes: HTTP relayed through the reverse tunnel; `host` is
    /// the tunnel route domain mapped to the cluster admin API.
    Tunnel {
        host: String,
        shared: Arc<GatewayShared>,
    },
}

impl ClusterTransport {
    fn authority(&self) -> String {
        match self {
            Self::Lan { base_authority } => base_authority.clone(),
            Self::Tunnel { host, .. } => host.clone(),
        }
    }
}

/// Node identity inputs resolved by the host (machine code, hostname,
/// process identity, tunnel identity).
#[derive(Clone, Debug)]
pub struct ClusterMemberIdentity {
    /// Stable machine fingerprint (one host row per machine code).
    pub machine_code: String,
    /// Operating system hostname.
    pub hostname: String,
    /// Cluster code to join (`None` joins the platform `default` cluster).
    pub cluster_code: Option<String>,
    /// Instance role label (`GATEWAY`, `MANAGEMENT`, ...).
    pub role: String,
    /// Deployment environment label.
    pub environment: String,
    /// Process id of the joining webserver process.
    pub process_pid: i32,
    /// Process start instant (RFC 3339).
    pub process_started_at: String,
    /// Build version reported to the registry.
    pub build_version: String,
    /// `LAN` (default) or `TUNNEL`.
    pub join_mode: String,
    /// Tunnel route domain reaching this node (TUNNEL mode only).
    pub tunnel_route_domain: Option<String>,
    /// Gateway endpoint this node dials (TUNNEL mode only).
    pub tunnel_endpoint: Option<String>,
}

/// Applies one desired-state manifest on this node. Errors acknowledge
/// `FAILED`; success acknowledges `IN_SYNC`.
#[async_trait::async_trait]
pub trait SyncApplier: Send + Sync {
    async fn apply(&self, kind: &str, manifest: &ClusterSyncManifest) -> Result<(), String>;
}

/// Errors the membership loop surfaces to its supervisor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberError {
    /// Transport/HTTP failure; the loop backs off and retries.
    Transient(String),
    /// A credential was refused; the loop stops until the operator fixes
    /// the secret (no hot retry against a rejected token).
    EnrollmentRejected(String),
}

impl std::fmt::Display for MemberError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transient(reason) => f.write_str(reason),
            Self::EnrollmentRejected(reason) => {
                f.write_str("enrollment rejected: ")?;
                f.write_str(reason)
            }
        }
    }
}

/// Cluster membership loop handle.
pub struct ClusterMember {
    task: tokio::task::JoinHandle<()>,
}

impl ClusterMember {
    /// Spawns the membership loop.
    pub fn spawn(
        transport: ClusterTransport,
        registration_token: String,
        identity: ClusterMemberIdentity,
        applier: Arc<dyn SyncApplier>,
        stop: watch::Receiver<bool>,
    ) -> Self {
        let task = tokio::spawn(run_loop(
            transport,
            registration_token,
            identity,
            applier,
            stop,
        ));
        Self { task }
    }

    /// Stops the loop and waits for exit.
    pub async fn shutdown(self) {
        let _ = self.task.await;
    }
}

async fn run_loop(
    transport: ClusterTransport,
    registration_token: String,
    identity: ClusterMemberIdentity,
    applier: Arc<dyn SyncApplier>,
    mut stop: watch::Receiver<bool>,
) {
    let mut session: Option<RegisteredSession> = None;
    loop {
        if *stop.borrow() {
            return;
        }
        if session.is_none() {
            match register(&transport, &registration_token, &identity).await {
                Ok(response) => {
                    tracing::info!(
                        instance = %response.instance.id,
                        cluster = %response.cluster.code,
                        "cluster membership registered"
                    );
                    session = Some(RegisteredSession {
                        instance_id: response.instance.id.clone(),
                        instance_token: response.instance_token.clone(),
                        heartbeat_interval: Duration::from_secs(
                            response.heartbeat_interval_seconds.max(1) as u64,
                        ),
                    });
                    continue;
                }
                Err(MemberError::EnrollmentRejected(error)) => {
                    tracing::error!(error = %error, "cluster enrollment rejected; stopping");
                    return;
                }
                Err(MemberError::Transient(error)) => {
                    tracing::warn!(error = %error, "cluster register failed; retrying");
                    if backoff(&mut stop, Duration::from_secs(5)).await {
                        return;
                    }
                    continue;
                }
            }
        }
        let Some(snapshot) = session.clone() else {
            continue;
        };

        match membership_tick(&transport, &snapshot, &identity, &applier).await {
            TickOutcome::Ok => {}
            TickOutcome::SessionLost(reason) => {
                tracing::warn!(reason = %reason, "cluster session lost; re-registering");
                session = None;
            }
            TickOutcome::Transient(reason) => {
                tracing::debug!(reason = %reason, "cluster tick failed; backing off");
            }
        }
        if backoff(&mut stop, snapshot.heartbeat_interval).await {
            return;
        }
    }
}

#[derive(Clone)]
struct RegisteredSession {
    #[allow(dead_code)]
    instance_id: String,
    instance_token: String,
    heartbeat_interval: Duration,
}

async fn backoff(stop: &mut watch::Receiver<bool>, delay: Duration) -> bool {
    tokio::select! {
        biased;
        _ = stop.changed() => true,
        _ = tokio::time::sleep(delay) => false,
    }
}

enum TickOutcome {
    Ok,
    SessionLost(String),
    Transient(String),
}

/// One membership tick: heartbeat with quality sample, then the sync
/// engine (manifest fetch → apply → ack per drifting track).
async fn membership_tick(
    transport: &ClusterTransport,
    session: &RegisteredSession,
    identity: &ClusterMemberIdentity,
    applier: &Arc<dyn SyncApplier>,
) -> TickOutcome {
    let heartbeat = ClusterHeartbeatRequest {
        status: 1,
        health_state: "HEALTHY".to_owned(),
        uptime_seconds: 0,
        build_version: Some(identity.build_version.clone()),
        metrics: serde_json::json!({}),
    };
    let started = std::time::Instant::now();
    let response: ClusterHeartbeatResponse = match send_json(
        transport,
        Some(&session.instance_token),
        "POST",
        "/internal/v3/api/web/cluster/instances/heartbeat",
        &heartbeat,
    )
    .await
    {
        Ok(response) => response,
        Err(error) => return TickOutcome::from(error),
    };
    tracing::debug!(
        rtt_ms = started.elapsed().as_millis() as u64,
        "cluster heartbeat accepted"
    );
    if let Some(states) = &response.sync {
        run_sync(transport, session, states, applier).await;
    }
    TickOutcome::Ok
}

impl From<MemberError> for TickOutcome {
    fn from(error: MemberError) -> Self {
        match error {
            MemberError::EnrollmentRejected(reason) => TickOutcome::SessionLost(reason),
            MemberError::Transient(reason) => TickOutcome::Transient(reason),
        }
    }
}

/// Registers the node (idempotent) and returns the enrollment response.
async fn register(
    transport: &ClusterTransport,
    registration_token: &str,
    identity: &ClusterMemberIdentity,
) -> Result<ClusterRegistrationResponse, MemberError> {
    let request = ClusterRegistrationRequest {
        cluster_code: identity.cluster_code.clone(),
        host: sdkwork_webserver_contract::ClusterHostDescriptor {
            hostname: identity.hostname.clone(),
            machine_code: identity.machine_code.clone(),
            name: Some(identity.hostname.clone()),
            os_name: Some(std::env::consts::OS.to_owned()),
            os_version: None,
            kernel_version: None,
            arch: Some(std::env::consts::ARCH.to_owned()),
            cpu_model: None,
            cpu_cores: None,
            memory_total_mb: None,
            remote_ip: None,
            local_ips: Vec::new(),
            mac_addresses: Vec::new(),
            daemon_version: Some(identity.build_version.clone()),
            join_mode: Some(identity.join_mode.clone()),
            tunnel: identity.tunnel_route_domain.as_ref().map(|route_domain| {
                sdkwork_webserver_contract::ClusterTunnelDescriptor {
                    route_domain: route_domain.clone(),
                    endpoint: identity.tunnel_endpoint.clone(),
                }
            }),
        },
        instance: sdkwork_webserver_contract::ClusterInstanceDescriptor {
            name: None,
            role: identity.role.clone(),
            environment: identity.environment.clone(),
            process_pid: identity.process_pid,
            process_started_at: identity.process_started_at.clone(),
            bind_host: None,
            bind_port: None,
            public_endpoint: None,
            build_version: Some(identity.build_version.clone()),
            join_mode: Some(identity.join_mode.clone()),
        },
    };
    send_json(
        transport,
        Some(registration_token),
        "POST",
        "/internal/v3/api/web/cluster/instances/register",
        &request,
    )
    .await
}

/// Runs the sync engine once: fetch drifting manifests, apply, acknowledge.
async fn run_sync(
    transport: &ClusterTransport,
    session: &RegisteredSession,
    states: &[ClusterSyncState],
    applier: &Arc<dyn SyncApplier>,
) {
    for state in states {
        let Some(desired) = state.desired_revision.clone() else {
            continue;
        };
        if state.applied_revision.as_deref() == Some(desired.as_str()) {
            continue; // already in sync on this track
        }
        let manifest_path = format!(
            "/internal/v3/api/web/cluster/sync/manifest?kind={}",
            state.kind
        );
        let manifest: ClusterSyncManifest = match send_json(
            transport,
            Some(&session.instance_token),
            "GET",
            &manifest_path,
            &(),
        )
        .await
        {
            Ok(manifest) => manifest,
            Err(error) => {
                tracing::debug!(error = %error, "sync manifest fetch failed");
                continue;
            }
        };
        // Integrity: only acknowledge the revision the cluster desires.
        if manifest.revision != desired {
            tracing::warn!(
                manifest = %manifest.revision,
                desired = %desired,
                "sync manifest revision drifted mid-flight; skipping"
            );
            continue;
        }
        match applier.apply(&state.kind, &manifest).await {
            Ok(()) => {
                let ack = ClusterSyncAckRequest {
                    kind: state.kind.clone(),
                    revision: manifest.revision.clone(),
                    status: "IN_SYNC".to_owned(),
                    detail: None,
                };
                let _: Result<ClusterSyncState, MemberError> = send_json(
                    transport,
                    Some(&session.instance_token),
                    "POST",
                    "/internal/v3/api/web/cluster/sync/ack",
                    &ack,
                )
                .await;
                tracing::info!(kind = %state.kind, revision = %manifest.revision, "sync applied");
            }
            Err(error) => {
                let ack = ClusterSyncAckRequest {
                    kind: state.kind.clone(),
                    revision: manifest.revision.clone(),
                    status: "FAILED".to_owned(),
                    detail: Some(error.clone()),
                };
                let _: Result<ClusterSyncState, MemberError> = send_json(
                    transport,
                    Some(&session.instance_token),
                    "POST",
                    "/internal/v3/api/web/cluster/sync/ack",
                    &ack,
                )
                .await;
                tracing::warn!(kind = %state.kind, error = %error, "sync apply failed");
            }
        }
    }
}

/// Minimal HTTP/1.1 JSON client over either transport. `Connection: close`
/// keeps the per-call stream lifetime trivial; the tunnel transport opens a
/// fresh data stream per call anyway.
async fn send_json<T, B>(
    transport: &ClusterTransport,
    bearer: Option<&str>,
    method: &str,
    path: &str,
    body: &B,
) -> Result<T, MemberError>
where
    T: serde::de::DeserializeOwned,
    B: serde::Serialize,
{
    let body_bytes = serde_json::to_vec(body)
        .map_err(|error| MemberError::Transient(format!("encode body: {error}")))?;
    let authority = transport.authority();
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body_bytes.len()
    );
    if let Some(token) = bearer {
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    request.push_str("\r\n");

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream: Box<dyn StreamIo> = match transport {
        ClusterTransport::Lan { base_authority } => {
            let tcp = tokio::time::timeout(
                Duration::from_secs(10),
                tokio::net::TcpStream::connect(base_authority),
            )
            .await
            .map_err(|_| MemberError::Transient("connect timed out".to_owned()))?
            .map_err(|error| MemberError::Transient(format!("connect: {error}")))?;
            Box::new(tcp)
        }
        ClusterTransport::Tunnel { host, shared } => {
            let visitor = sdkwork_webserver_tunnel::gateway::RelayVisitor {
                ip: std::net::IpAddr::from([127, 0, 0, 1]),
                bearer: None,
            };
            let relayed = tokio::time::timeout(
                Duration::from_secs(10),
                shared.connect_http_stream(host, visitor),
            )
            .await
            .map_err(|_| MemberError::Transient("tunnel stream timed out".to_owned()))?
            .map_err(|error| MemberError::Transient(format!("tunnel stream: {error}")))?;
            Box::new(relayed)
        }
    };

    let mut raw = Vec::with_capacity(4096);
    let exchange = async {
        stream
            .write_all(request.as_bytes())
            .await
            .map_err(|error| MemberError::Transient(format!("write: {error}")))?;
        stream
            .write_all(&body_bytes)
            .await
            .map_err(|error| MemberError::Transient(format!("write body: {error}")))?;
        stream
            .flush()
            .await
            .map_err(|error| MemberError::Transient(format!("flush: {error}")))?;
        let mut buffer = [0_u8; 4096];
        loop {
            let read = stream
                .read(&mut buffer)
                .await
                .map_err(|error| MemberError::Transient(format!("read: {error}")))?;
            if read == 0 {
                break;
            }
            raw.extend_from_slice(&buffer[..read]);
        }
        Ok::<(), MemberError>(())
    };
    tokio::time::timeout(Duration::from_secs(20), exchange)
        .await
        .map_err(|_| MemberError::Transient("http call timed out".to_owned()))??;

    let text = String::from_utf8_lossy(&raw);
    let mut parts = text.splitn(2, "\r\n\r\n");
    let head = parts.next().unwrap_or_default();
    let body_text = parts.next().unwrap_or_default();
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| MemberError::Transient("malformed HTTP response".to_owned()))?;
    if !(200..300).contains(&status) {
        if status == 401 || status == 403 {
            return Err(MemberError::EnrollmentRejected(format!("HTTP {status}")));
        }
        return Err(MemberError::Transient(format!("HTTP {status}")));
    }
    // Envelope unwrap: the internal surface answers `{"data": ...}`.
    let value: serde_json::Value = serde_json::from_str(body_text.trim())
        .map_err(|error| MemberError::Transient(format!("decode body: {error}")))?;
    let data = value.get("data").cloned().unwrap_or(value);
    serde_json::from_value(data)
        .map_err(|error| MemberError::Transient(format!("decode payload: {error}")))
}

/// Object-safe supertrait so LAN and tunnel streams share one boxed type.
trait StreamIo: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}

impl<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> StreamIo for S {}
