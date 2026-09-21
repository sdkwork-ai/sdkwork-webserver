use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    response::Response,
    routing::{get, post, put},
    Extension, Json, Router,
};
use sdkwork_routes_webserver_common::{created_resource, ok_resource, WebApiError};
use sdkwork_webserver_contract::{
    ClusterHeartbeatRequest, ClusterRegistrationRequest, ClusterSyncAckRequest,
    CreateRuntimeObservationRequest, PublishRuntimeAssignmentRequest, WebInternalApi,
    WebInternalRequestContext, MAX_WEBSITE_RUNTIME_SET_BYTES,
};
use serde::Deserialize;

use crate::{auth::require_internal_context, paths};

const PUBLISH_REQUEST_ENVELOPE_BYTES: usize = 1024 * 1024;
const OBSERVATION_REQUEST_BYTES: usize = 16 * 1024;
const REGISTRATION_REQUEST_BYTES: usize = 64 * 1024;
const HEARTBEAT_REQUEST_BYTES: usize = 64 * 1024;

#[derive(Clone)]
struct InternalState {
    api: Arc<dyn WebInternalApi>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClusterSyncManifestQuery {
    kind: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CurrentAssignmentQuery {
    environment: String,
    #[serde(default)]
    if_generation: Option<String>,
    #[serde(default)]
    if_snapshot_sha256: Option<String>,
}

pub fn build_router_with_internal_api<A>(api: A) -> Router
where
    A: WebInternalApi + 'static,
{
    build_router_with_shared_internal_api(Arc::new(api))
}

pub fn build_router_with_shared_internal_api(api: Arc<dyn WebInternalApi>) -> Router {
    Router::new()
        .route(
            paths::RUNTIME_ASSIGNMENT,
            put(publish_runtime_assignment).layer(DefaultBodyLimit::max(
                MAX_WEBSITE_RUNTIME_SET_BYTES + PUBLISH_REQUEST_ENVELOPE_BYTES,
            )),
        )
        .route(
            paths::CURRENT_RUNTIME_ASSIGNMENT,
            get(retrieve_current_runtime_assignment),
        )
        .route(
            paths::RUNTIME_OBSERVATIONS,
            post(create_runtime_observation)
                .layer(DefaultBodyLimit::max(OBSERVATION_REQUEST_BYTES)),
        )
        .route(
            paths::LATEST_RUNTIME_OBSERVATION,
            get(retrieve_latest_runtime_observation),
        )
        .route(
            paths::CLUSTER_REGISTER,
            post(cluster_register).layer(DefaultBodyLimit::max(REGISTRATION_REQUEST_BYTES)),
        )
        .route(
            paths::CLUSTER_HEARTBEAT,
            post(cluster_heartbeat).layer(DefaultBodyLimit::max(HEARTBEAT_REQUEST_BYTES)),
        )
        .route(paths::CLUSTER_PEERS, get(retrieve_cluster_peers))
        .route(
            paths::CLUSTER_SYNC_MANIFEST,
            get(retrieve_cluster_sync_manifest),
        )
        .route(
            paths::CLUSTER_SYNC_ACK,
            post(cluster_sync_ack).layer(DefaultBodyLimit::max(HEARTBEAT_REQUEST_BYTES)),
        )
        .route(paths::CLUSTER_DRAIN_COMPLETE, post(cluster_drain_complete))
        .with_state(InternalState { api })
}

async fn cluster_register(
    State(state): State<InternalState>,
    context: Option<Extension<WebInternalRequestContext>>,
    Json(request): Json<ClusterRegistrationRequest>,
) -> Result<Response, WebApiError> {
    let context = require_internal_context(context)?;
    created_resource(
        state
            .api
            .register_cluster_instance(&context, &request)
            .await,
    )
}

async fn cluster_heartbeat(
    State(state): State<InternalState>,
    context: Option<Extension<WebInternalRequestContext>>,
    Json(request): Json<ClusterHeartbeatRequest>,
) -> Result<Response, WebApiError> {
    let context = require_internal_context(context)?;
    ok_resource(state.api.record_cluster_heartbeat(&context, &request).await)
}

async fn retrieve_cluster_peers(
    State(state): State<InternalState>,
    context: Option<Extension<WebInternalRequestContext>>,
) -> Result<Response, WebApiError> {
    let context = require_internal_context(context)?;
    ok_resource(state.api.retrieve_cluster_peers(&context).await)
}

async fn retrieve_cluster_sync_manifest(
    State(state): State<InternalState>,
    context: Option<Extension<WebInternalRequestContext>>,
    Query(params): Query<ClusterSyncManifestQuery>,
) -> Result<Response, WebApiError> {
    let context = require_internal_context(context)?;
    ok_resource(
        state
            .api
            .retrieve_cluster_sync_manifest(&context, &params.kind)
            .await,
    )
}

async fn cluster_sync_ack(
    State(state): State<InternalState>,
    context: Option<Extension<WebInternalRequestContext>>,
    Json(request): Json<ClusterSyncAckRequest>,
) -> Result<Response, WebApiError> {
    let context = require_internal_context(context)?;
    ok_resource(state.api.record_cluster_sync_ack(&context, &request).await)
}

async fn cluster_drain_complete(
    State(state): State<InternalState>,
    context: Option<Extension<WebInternalRequestContext>>,
) -> Result<Response, WebApiError> {
    let context = require_internal_context(context)?;
    ok_resource(state.api.record_cluster_drain_complete(&context).await)
}

async fn publish_runtime_assignment(
    State(state): State<InternalState>,
    context: Option<Extension<WebInternalRequestContext>>,
    Path((node_uuid, environment)): Path<(String, String)>,
    Json(request): Json<PublishRuntimeAssignmentRequest>,
) -> Result<Response, WebApiError> {
    let context = require_internal_context(context)?;
    ok_resource(
        state
            .api
            .publish_runtime_assignment(&context, &node_uuid, &environment, &request)
            .await,
    )
}

async fn retrieve_current_runtime_assignment(
    State(state): State<InternalState>,
    context: Option<Extension<WebInternalRequestContext>>,
    Query(query): Query<CurrentAssignmentQuery>,
) -> Result<Response, WebApiError> {
    let context = require_internal_context(context)?;
    ok_resource(
        state
            .api
            .retrieve_current_runtime_assignment(
                &context,
                &query.environment,
                query.if_generation.as_deref(),
                query.if_snapshot_sha256.as_deref(),
            )
            .await,
    )
}

async fn create_runtime_observation(
    State(state): State<InternalState>,
    context: Option<Extension<WebInternalRequestContext>>,
    Path(snapshot_uuid): Path<String>,
    Json(request): Json<CreateRuntimeObservationRequest>,
) -> Result<Response, WebApiError> {
    let context = require_internal_context(context)?;
    created_resource(
        state
            .api
            .create_runtime_observation(&context, &snapshot_uuid, &request)
            .await,
    )
}

async fn retrieve_latest_runtime_observation(
    State(state): State<InternalState>,
    context: Option<Extension<WebInternalRequestContext>>,
    Path(snapshot_uuid): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_internal_context(context)?;
    ok_resource(
        state
            .api
            .retrieve_latest_runtime_observation(&context, &snapshot_uuid)
            .await,
    )
}
