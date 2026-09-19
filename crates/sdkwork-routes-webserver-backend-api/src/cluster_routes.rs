//! Distributed cluster management HTTP routes (backend-admin surface).
//!
//! The cluster plane manages the webserver deployment itself: clusters group
//! hosts, hosts carry the machine identity (system basics, remote/local IPs,
//! machine code, MAC addresses), and instances are the webserver processes.
//! Per PRD-FR-030 the surface answers only the platform operator tenant
//! (tenant 0) through [`require_platform_operator`].

use std::sync::Arc;

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
    UpdateClusterInstanceRequest, UpdateClusterRequest, WebBackendRequestContext,
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
    created_resource(
        service
            .cluster_create(&context, &request)
            .await,
    )
}

pub(crate) async fn retrieve_cluster(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(cluster_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    ok_resource(
        service
            .cluster_retrieve(&context, &cluster_id)
            .await,
    )
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
    no_content(
        service
            .cluster_delete(&context, &cluster_id)
            .await,
    )
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
    )
}

pub(crate) async fn retrieve_cluster_host(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(host_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    ok_resource(
        service
            .cluster_host_retrieve(&context, &host_id)
            .await,
    )
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
    no_content(
        service
            .cluster_host_delete(&context, &host_id)
            .await,
    )
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
                query.page_size,
                query.cursor.as_deref(),
            )
            .await,
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
            .cluster_heartbeat_list(&context, &instance_id, query.page_size, query.cursor.as_deref())
            .await,
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
                query.page_size,
                query.cursor.as_deref(),
            )
            .await,
    )
}

pub(crate) async fn retrieve_cluster_overview(
    Extension(service): Extension<Arc<WebService>>,
    context: Option<Extension<WebBackendRequestContext>>,
) -> Result<Response, WebApiError> {
    let context = require_cluster_context(context)?;
    ok_resource(
        service
            .cluster_overview(&context)
            .await,
    )
}
