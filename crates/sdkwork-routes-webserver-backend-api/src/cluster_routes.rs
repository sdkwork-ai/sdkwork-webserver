//! Distributed cluster management HTTP routes (backend-admin surface).
//!
//! The cluster plane manages the webserver deployment itself: clusters group
//! hosts, hosts carry the machine identity (system basics, remote/local IPs,
//! machine code, MAC addresses), and instances are the webserver processes.
//! Per PRD-FR-030 the surface answers only the platform operator tenant
//! through [`require_platform_operator`].

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, Query},
    response::Response,
    Extension, Json,
};
use sdkwork_intelligence_webserver_service::WebService;
use sdkwork_routes_webserver_common::{
    created_resource, no_content, ok_cluster_event_page, ok_cluster_heartbeat_page,
    ok_cluster_host_page, ok_cluster_instance_page, ok_cluster_page, ok_resource, WebApiError,
};
use sdkwork_webserver_contract::{
    CreateClusterRequest, EnqueueClusterPeerMessagesRequest, UpdateClusterHostRequest,
    UpdateClusterInstanceRequest, UpdateClusterRequest, WebBackendApi, WebBackendRequestContext,
};
use serde::Deserialize;

use crate::auth::{require_backend_context, require_platform_operator};

const CLUSTER_SURFACE: &str = "the Web Server cluster management surface";

#[derive(Debug, Deserialize)]
pub(crate) struct ClusterListQuery {
    #[serde(default = "default_page")]
    page: i32,
    #[serde(default = "default_page_size")]
    page_size: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClusterHostListQuery {
    #[serde(default = "default_page_size")]
    page_size: i32,
    cursor: Option<String>,
    cluster_id: Option<String>,
    status: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClusterInstanceListQuery {
    #[serde(default = "default_page_size")]
    page_size: i32,
    cursor: Option<String>,
    cluster_id: Option<String>,
    host_id: Option<String>,
    status: Option<i32>,
    health_state: Option<String>,
    /// `0` = LAN (same-subnet), `1` = TUNNEL (API-only reverse tunnel).
    #[serde(rename = "joinMode", default)]
    join_mode: Option<i32>,
    /// `0` = unknown, `1` = in sync, `2` = pending, `3` = failed.
    #[serde(rename = "syncStatus", default)]
    sync_status: Option<i32>,
    /// Label selector `k1=v1,k2=v2` (every pair must match).
    #[serde(default)]
    labels: Option<String>,
    /// Free-text search over name / endpoint / hostname.
    #[serde(default)]
    search: Option<String>,
    /// Exact build version (version-skew management).
    #[serde(rename = "buildVersion", default)]
    build_version: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClusterHeartbeatListQuery {
    #[serde(default = "default_page_size")]
    page_size: i32,
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClusterEventListQuery {
    #[serde(default = "default_page_size")]
    page_size: i32,
    cursor: Option<String>,
    cluster_id: Option<String>,
    severity: Option<String>,
    /// Detail-page filter: events recorded for one instance.
    #[serde(rename = "instanceId", default)]
    instance_id: Option<String>,
}

fn default_page() -> i32 {
    1
}

fn default_page_size() -> i32 {
    20
}

fn require_cluster_context(
    context: Option<Extension<WebBackendRequestContext>>,
) -> Result<WebBackendRequestContext, WebApiError> {
    let context = require_backend_context(context)?;
    require_platform_operator(&context, CLUSTER_SURFACE)?;
    Ok(context)
}

pub(crate) async fn list_clusters(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<ClusterListQuery>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    ok_cluster_page(
        service
            .cluster_list(&context, query.page, query.page_size)
            .await,
        query.page,
        query.page_size,
    )
}

pub(crate) async fn create_cluster(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Json(request): Json<CreateClusterRequest>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    created_resource(service.cluster_create(&context, &request).await)
}

pub(crate) async fn retrieve_cluster(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(cluster_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    ok_resource(service.cluster_retrieve(&context, &cluster_id).await)
}

pub(crate) async fn update_cluster(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(cluster_id): Path<String>,
    Json(request): Json<UpdateClusterRequest>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    ok_resource(
        service
            .cluster_update(&context, &cluster_id, &request)
            .await,
    )
}

pub(crate) async fn delete_cluster(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(cluster_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    no_content(service.cluster_delete(&context, &cluster_id).await)
}

pub(crate) async fn list_cluster_hosts(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<ClusterHostListQuery>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    ok_cluster_host_page(
        service
            .cluster_host_list(
                &context,
                query.cluster_id.as_deref(),
                query.status,
                query.page_size,
                query.cursor.as_deref(),
            )
            .await,
        query.page_size,
    )
}

pub(crate) async fn retrieve_cluster_host(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(host_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    ok_resource(service.cluster_host_retrieve(&context, &host_id).await)
}

pub(crate) async fn update_cluster_host(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(host_id): Path<String>,
    Json(request): Json<UpdateClusterHostRequest>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    ok_resource(
        service
            .cluster_host_update(&context, &host_id, &request)
            .await,
    )
}

pub(crate) async fn delete_cluster_host(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(host_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    no_content(service.cluster_host_delete(&context, &host_id).await)
}

pub(crate) async fn list_cluster_instances(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<ClusterInstanceListQuery>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    ok_cluster_instance_page(
        service
            .cluster_instance_list(
                &context,
                query.cluster_id.as_deref(),
                query.host_id.as_deref(),
                query.status,
                query.health_state.as_deref(),
                query.join_mode,
                query.sync_status,
                query.labels.as_deref(),
                query.search.as_deref(),
                query.build_version.as_deref(),
                query.page_size,
                query.cursor.as_deref(),
            )
            .await,
        query.page_size,
    )
}

pub(crate) async fn retrieve_cluster_instance(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(instance_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    ok_resource(
        service
            .cluster_instance_retrieve(&context, &instance_id)
            .await,
    )
}

pub(crate) async fn update_cluster_instance(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(instance_id): Path<String>,
    Json(request): Json<UpdateClusterInstanceRequest>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    ok_resource(
        service
            .cluster_instance_update(&context, &instance_id, &request)
            .await,
    )
}

pub(crate) async fn delete_cluster_instance(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(instance_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    no_content(
        service
            .cluster_instance_delete(&context, &instance_id)
            .await,
    )
}

pub(crate) async fn list_instance_heartbeats(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(instance_id): Path<String>,
    Query(query): Query<ClusterHeartbeatListQuery>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    ok_cluster_heartbeat_page(
        service
            .cluster_heartbeat_list(
                &context,
                &instance_id,
                query.page_size,
                query.cursor.as_deref(),
            )
            .await,
        query.page_size,
    )
}

pub(crate) async fn enqueue_cluster_messages(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Json(request): Json<EnqueueClusterPeerMessagesRequest>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    created_resource(service.cluster_message_enqueue(&context, &request).await)
}

pub(crate) async fn list_cluster_events(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<ClusterEventListQuery>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    ok_cluster_event_page(
        service
            .cluster_events_list(
                &context,
                query.cluster_id.as_deref(),
                query.severity.as_deref(),
                query.instance_id.as_deref(),
                query.page_size,
                query.cursor.as_deref(),
            )
            .await,
        query.page_size,
    )
}

pub(crate) async fn retrieve_cluster_overview(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    ok_resource(service.cluster_overview(&context).await)
}

/// `POST /backend/v3/api/clusters/{clusterId}/sync` — publishes one
/// desired-state revision (config or applications) to every instance of
/// the cluster (PRD: complete data synchronization).
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClusterSyncPublishBody {
    kind: String,
    payload: serde_json::Value,
}

pub(crate) async fn publish_cluster_sync(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(cluster_id): Path<String>,
    Json(body): Json<ClusterSyncPublishBody>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    let manifest = service
        .cluster_sync_publish(&context, &cluster_id, &body.kind, body.payload)
        .await?;
    ok_resource(Ok(manifest))
}

macro_rules! instance_action_route {
    ($fn_name:ident, $trait_method:ident) => {
        pub(crate) async fn $fn_name(
            Extension(service): Extension<Arc<WebService>>,
            context: Option<Extension<WebBackendRequestContext>>,
            Path(instance_id): Path<String>,
        ) -> Result<Response, WebApiError> {
            let context = require_cluster_context(context)?;
            ok_resource(service.$trait_method(&context, &instance_id).await)
        }
    };
}

instance_action_route!(drain_cluster_instance, drain_cluster_instance);
instance_action_route!(undrain_cluster_instance, undrain_cluster_instance);
instance_action_route!(cordon_cluster_instance, cordon_cluster_instance);
instance_action_route!(uncordon_cluster_instance, uncordon_cluster_instance);

/// `GET /backend/v3/api/clusters/instances/{instanceId}/metrics/history` —
/// per-instance heartbeat metric samples for the detail page. Growing
/// time-series collection: cursor/keyset pagination (PAGINATION_SPEC),
/// newest first, same contract as the heartbeat list.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct InstanceMetricsHistoryQuery {
    #[serde(default = "default_page_size")]
    page_size: i32,
    cursor: Option<String>,
}

pub(crate) async fn list_instance_metrics_history(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(instance_id): Path<String>,
    Query(query): Query<InstanceMetricsHistoryQuery>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    ok_cluster_heartbeat_page(
        service
            .cluster_instance_metrics_history(
                &context,
                &instance_id,
                query.page_size,
                query.cursor.as_deref(),
            )
            .await,
        query.page_size,
    )
}

// ---------------------------------------------------------------------------
// Instance connectivity probe (industry "Test connection" action)
// ---------------------------------------------------------------------------

/// Raw HTTP GET health probe over TCP: an HTTP/1.0 style request with a
/// bounded timeout, exactly the nginx-style active health check. Any valid
/// HTTP status line counts as healthy; the probe measures round-trip
/// latency.
async fn http_probe(authority: &str, path: &str, timeout: Duration) -> Result<u64, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let started = std::time::Instant::now();
    let tcp = tokio::time::timeout(timeout, tokio::net::TcpStream::connect(authority))
        .await
        .map_err(|_| "connect timed out".to_owned())?
        .map_err(|error| format!("connect: {error}"))?;
    let mut socket = tcp;
    socket
        .write_all(
            format!(
                "GET {path} HTTP/1.0
Host: {authority}
Connection: close

"
            )
            .as_bytes(),
        )
        .await
        .map_err(|error| format!("write: {error}"))?;
    let mut response = Vec::with_capacity(128);
    let mut chunk = [0_u8; 256];
    let read = tokio::time::timeout(timeout, socket.read(&mut chunk))
        .await
        .map_err(|_| "read timed out".to_owned())?
        .map_err(|error| format!("read: {error}"))?;
    response.extend_from_slice(&chunk[..read.min(32)]);
    let head = String::from_utf8_lossy(&response);
    if !head.starts_with("HTTP") {
        return Err("non-HTTP response".to_owned());
    }
    Ok(started.elapsed().as_millis() as u64)
}

/// `POST /backend/v3/api/clusters/instances/{instanceId}/probe` — on-demand
/// connectivity probe: runs one active health check against the instance
/// (probeUrl override, else the bind endpoint, else the public endpoint),
/// records the outcome through the auto-eject / auto-recover port, and
/// returns the result for the operator.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct InstanceProbeBody {
    #[serde(default = "default_probe_path")]
    path: String,
    #[serde(default = "default_probe_timeout")]
    timeout_ms: u64,
}

fn default_probe_path() -> String {
    "/".to_owned()
}

fn default_probe_timeout() -> u64 {
    3_000
}

pub(crate) async fn probe_cluster_instance(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(instance_id): Path<String>,
    Json(body): Json<InstanceProbeBody>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    let detail = service
        .cluster_instance_retrieve(&context, &instance_id)
        .await?;
    // Probe target precedence: probeUrl override > bind host:port > public
    // endpoint (strip scheme for the raw hop).
    let target = detail
        .probe_url
        .clone()
        .or_else(|| {
            detail
                .bind_host
                .zip(detail.bind_port)
                .map(|(host, port)| format!("http://{host}:{port}"))
        })
        .or_else(|| detail.public_endpoint.clone())
        .ok_or_else(|| {
            WebApiError::new(
                sdkwork_utils_rust::SdkWorkResultCode::InvalidParameter,
                "instance has no probeable endpoint",
            )
        })?;
    let authority = target
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_owned();
    let probe_path = if body.path.is_empty() {
        "/".to_owned()
    } else {
        body.path
    };
    let timeout = Duration::from_millis(body.timeout_ms.clamp(100, 10_000));

    let probe = http_probe(&authority, &probe_path, timeout).await;
    let latency_ms = probe
        .as_ref()
        .copied()
        .unwrap_or(u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX));
    let outcome = service
        .cluster_instance_probe_outcome(&instance_id, probe.is_ok())
        .await?;
    ok_resource(Ok(sdkwork_webserver_contract::ClusterProbeRunResponse {
        healthy: probe.is_ok(),
        latency_ms,
        failures: outcome.failures,
        ejected: outcome.ejected,
        recovered: outcome.recovered,
    }))
}
