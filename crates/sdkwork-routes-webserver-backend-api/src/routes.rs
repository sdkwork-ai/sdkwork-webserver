use axum::{
    extract::{Path, Query, State},
    response::Response,
    routing::{get, post},
    Extension, Json, Router,
};
use sdkwork_webserver_contract::{
    CreateApplicationRequest, CreateDeploymentRequest, CreateDomainRequest,
    CreateListenerCertificateBindingRequest, CreateManagedDomainRequest, CreateNginxConfigRequest,
    CreateRootDomainHostnameRequest, CreateRootDomainRequest, CreateServerRequest,
    CreateSourceVersionRequest, ImportGitSourceVersionRequest, IssueCertificateRequest,
    ListApplicationsQuery, ListAuditLogsQuery, ListNginxConfigsQuery, ListRootDomainsQuery,
    RevokeCertificateRequest, UpdateApplicationRequest, UpdateCertificateRequest,
    UpdateDomainApplicationBindingRequest, UpdateNginxConfigRequest, WebBackendApi,
    WebBackendRequestContext,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::{agent_routes, auth::require_backend_context, cluster_routes, paths, traffic_usage_routes};
use sdkwork_routes_webserver_common::{
    accepted_async, created_resource, no_content, ok_application_page, ok_audit_log_page,
    ok_certificate_distribution_page, ok_certificate_page, ok_deployment_page, ok_domain_page,
    ok_listener_certificate_binding_page, ok_nginx_config_page, ok_resource, ok_root_domain_page,
    ok_server_page, ok_source_version_page, validate_pagination_query, WebApiError,
};

/// Router state shared by every handler of this surface.
///
/// `pub(crate)` because the surface's handlers live in sibling modules
/// (`traffic_usage_routes`) that are mounted into the same state; keeping the
/// state private would force those handlers onto the concrete service type and
/// bypass [`WebBackendApi`], which is the port the surface is built on.
#[derive(Clone)]
pub(crate) struct BackendState {
    pub(crate) api: Arc<dyn WebBackendApi>,
}

pub fn build_router_with_backend_api<A>(api: A) -> Router
where
    A: WebBackendApi + 'static,
{
    build_router_with_shared_backend_api(Arc::new(api))
}

pub fn build_router_with_shared_backend_api(api: Arc<dyn WebBackendApi>) -> Router {
    Router::new()
        .route(
            paths::APPLICATIONS,
            get(list_applications).post(create_application),
        )
        .route(
            paths::APPLICATION,
            get(retrieve_application)
                .patch(update_application)
                .delete(delete_application),
        )
        .route(paths::APPLICATION_ACTIVATE, post(activate_application))
        .route(paths::APPLICATION_PAUSE, post(pause_application))
        .route(
            paths::APPLICATION_DOMAINS,
            get(list_application_domains).post(create_application_domain),
        )
        .route(
            paths::APPLICATION_DOMAIN,
            axum::routing::delete(delete_application_domain),
        )
        .route(
            paths::APPLICATION_DOMAIN_VERIFY,
            post(verify_application_domain),
        )
        .route(
            paths::APPLICATION_DOMAIN_LISTENER_CERTIFICATE_BINDINGS,
            get(list_application_listener_certificate_bindings)
                .post(bind_application_listener_certificate),
        )
        .route(
            paths::APPLICATION_DOMAIN_LISTENER_CERTIFICATE_BINDING,
            axum::routing::delete(unbind_application_listener_certificate),
        )
        .route(
            paths::ROOT_DOMAINS,
            get(list_root_domains).post(create_root_domain),
        )
        .route(
            paths::ROOT_DOMAIN,
            get(retrieve_root_domain).delete(delete_root_domain),
        )
        .route(
            paths::ROOT_DOMAIN_SUBDOMAINS,
            get(list_root_domain_subdomains).post(create_root_domain_subdomain),
        )
        .route(
            paths::DOMAINS,
            get(list_managed_domains).post(create_managed_domain),
        )
        .route(paths::DOMAIN, axum::routing::delete(delete_managed_domain))
        .route(paths::DOMAIN_VERIFY, post(verify_managed_domain))
        .route(
            paths::DOMAIN_APPLICATION_BINDING,
            axum::routing::put(update_domain_application_binding)
                .delete(delete_domain_application_binding),
        )
        .route(
            paths::APPLICATION_SOURCE_VERSIONS,
            get(list_application_source_versions).post(create_application_source_version),
        )
        .route(
            paths::APPLICATION_SOURCE_VERSION_IMPORT_GIT,
            post(import_application_git_source_version),
        )
        .route(
            paths::APPLICATION_SOURCE_VERSION,
            get(retrieve_application_source_version),
        )
        .route(
            paths::APPLICATION_DEPLOYMENTS,
            get(list_application_deployments).post(create_application_deployment),
        )
        .route(
            paths::APPLICATION_DEPLOYMENT_ROLLBACK,
            post(rollback_application_deployment),
        )
        .route(paths::CERTIFICATES, get(list_managed_certificates))
        .route(paths::CERTIFICATES_ISSUE, post(issue_managed_certificate))
        .route(
            paths::CERTIFICATE_OPERATION,
            get(retrieve_managed_certificate_operation),
        )
        .route(
            paths::CERTIFICATE,
            axum::routing::put(update_managed_certificate).delete(delete_managed_certificate),
        )
        .route(paths::CERTIFICATE_RENEW, post(renew_managed_certificate))
        .route(paths::CERTIFICATE_REVOKE, post(revoke_managed_certificate))
        .route(
            paths::CERTIFICATE_DISTRIBUTION,
            get(list_certificate_distribution),
        )
        .route(
            paths::NGINX_CONFIGS,
            get(list_nginx_configs).post(create_nginx_config),
        )
        .route(
            paths::NGINX_CONFIG,
            get(retrieve_nginx_config).put(update_nginx_config),
        )
        .route(paths::NGINX_CONFIG_VALIDATE, post(validate_nginx_config))
        .route(paths::NGINX_CONFIG_DEPLOY, post(deploy_nginx_config))
        .route(paths::NGINX_RELOAD, post(reload_nginx))
        .route(paths::NGINX_STATUS, get(retrieve_nginx_status))
        .route(paths::SERVERS, get(list_servers).post(create_server))
        .route(paths::AUDIT_LOGS, get(list_audit_logs))
        .route(
            paths::CLUSTERS,
            get(cluster_routes::list_clusters).post(cluster_routes::create_cluster),
        )
        .route(
            paths::CLUSTER,
            get(cluster_routes::retrieve_cluster)
                .patch(cluster_routes::update_cluster)
                .delete(cluster_routes::delete_cluster),
        )
        .route(
            paths::CLUSTER_HOSTS,
            get(cluster_routes::list_cluster_hosts),
        )
        .route(
            paths::CLUSTER_HOST,
            get(cluster_routes::retrieve_cluster_host)
                .patch(cluster_routes::update_cluster_host)
                .delete(cluster_routes::delete_cluster_host),
        )
        .route(
            paths::CLUSTER_INSTANCES,
            get(cluster_routes::list_cluster_instances),
        )
        .route(
            paths::CLUSTER_SYNC_PUBLISH,
            post(cluster_routes::publish_cluster_sync),
        )
        .route(
            paths::CLUSTER_INSTANCE_METRICS_HISTORY,
            get(cluster_routes::list_instance_metrics_history),
        )
        .route(
            paths::CLUSTER_INSTANCE_PROBE,
            post(cluster_routes::probe_cluster_instance),
        )
        .route(
            paths::CLUSTER_INSTANCE_DRAIN,
            post(cluster_routes::drain_cluster_instance),
        )
        .route(
            paths::CLUSTER_INSTANCE_UNDRAIN,
            post(cluster_routes::undrain_cluster_instance),
        )
        .route(
            paths::CLUSTER_INSTANCE_CORDON,
            post(cluster_routes::cordon_cluster_instance),
        )
        .route(
            paths::CLUSTER_INSTANCE_UNCORDON,
            post(cluster_routes::uncordon_cluster_instance),
        )
        .route(
            paths::CLUSTER_INSTANCE,
            get(cluster_routes::retrieve_cluster_instance)
                .patch(cluster_routes::update_cluster_instance)
                .delete(cluster_routes::delete_cluster_instance),
        )
        .route(
            paths::CLUSTER_INSTANCE_HEARTBEATS,
            get(cluster_routes::list_instance_heartbeats),
        )
        .route(
            paths::CLUSTER_MESSAGES,
            post(cluster_routes::enqueue_cluster_messages),
        )
        .route(
            paths::CLUSTER_EVENTS,
            get(cluster_routes::list_cluster_events),
        )
        .route(
            paths::CLUSTER_OVERVIEW,
            get(cluster_routes::retrieve_cluster_overview),
        )
        .route(
            paths::TRAFFIC_USAGE,
            get(traffic_usage_routes::retrieve_tenant_traffic_usage),
        )
        .route(
            paths::PLATFORM_TRAFFIC_USAGE,
            get(traffic_usage_routes::retrieve_platform_traffic_usage),
        )
        .layer(axum::middleware::from_fn(validate_pagination_query))
        .with_state(BackendState { api })
}

/// Router containing only the Web Node agent routes (heartbeat/sync).
///
/// Agent routes authenticate `X-SDKWork-Agent-Token` through
/// WebFrameworkLayer + MachineCredentialResolverDecorator; the assembly
/// wraps this router in a machine-only framework layer so IAM user API keys
/// can never impersonate node credentials.
pub fn build_agent_router_with_shared_backend_api(api: Arc<dyn WebBackendApi>) -> Router {
    Router::new()
        .route(paths::AGENT_HEARTBEAT, post(agent_routes::agent_heartbeat))
        .route(paths::AGENT_SYNC, get(agent_routes::agent_sync))
        .with_state(BackendState { api })
}

#[derive(Debug, Deserialize)]
struct PageQuery {
    #[serde(default = "default_page")]
    page: i32,
    #[serde(default = "default_page_size")]
    page_size: i32,
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CertificatePageQuery {
    #[serde(default = "default_page")]
    page: i32,
    #[serde(default = "default_page_size")]
    page_size: i32,
    #[serde(rename = "domain_id")]
    domain_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeploymentPageQuery {
    #[serde(default = "default_page")]
    page: i32,
    #[serde(default = "default_page_size")]
    page_size: i32,
    status: Option<i32>,
    cursor: Option<String>,
}

fn default_page() -> i32 {
    1
}

fn default_page_size() -> i32 {
    20
}

async fn list_applications(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<ListApplicationsQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_application_page(state.api.list_applications(&context, &query).await)
}

async fn create_application(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Json(request): Json<CreateApplicationRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    created_resource(state.api.create_application(&context, &request).await)
}

async fn retrieve_application(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(application_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .retrieve_application(&context, &application_id)
            .await,
    )
}

async fn update_application(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(application_id): Path<String>,
    Json(request): Json<UpdateApplicationRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .update_application(&context, &application_id, &request)
            .await,
    )
}

async fn delete_application(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(application_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    no_content(
        state
            .api
            .delete_application(&context, &application_id)
            .await,
    )
}

async fn activate_application(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(application_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .activate_application(&context, &application_id)
            .await,
    )
}

async fn pause_application(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(application_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(state.api.pause_application(&context, &application_id).await)
}

async fn list_application_domains(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(application_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_domain_page(
        state
            .api
            .list_application_domains(&context, &application_id, query.page, query.page_size)
            .await,
        query.page,
        query.page_size,
    )
}

async fn create_application_domain(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(application_id): Path<String>,
    Json(request): Json<CreateDomainRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    created_resource(
        state
            .api
            .create_application_domain(&context, &application_id, &request)
            .await,
    )
}

async fn verify_application_domain(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path((application_id, domain_id)): Path<(String, String)>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .verify_application_domain(&context, &application_id, &domain_id)
            .await,
    )
}

async fn delete_application_domain(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path((application_id, domain_id)): Path<(String, String)>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    no_content(
        state
            .api
            .delete_application_domain(&context, &application_id, &domain_id)
            .await,
    )
}

async fn list_root_domains(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<ListRootDomainsQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_root_domain_page(
        state.api.list_root_domains(&context, &query).await,
        query.page,
        query.page_size,
    )
}

async fn create_root_domain(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Json(request): Json<CreateRootDomainRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    created_resource(state.api.create_root_domain(&context, &request).await)
}

async fn retrieve_root_domain(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(root_domain_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .retrieve_root_domain(&context, &root_domain_id)
            .await,
    )
}

async fn delete_root_domain(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(root_domain_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    no_content(
        state
            .api
            .delete_root_domain(&context, &root_domain_id)
            .await,
    )
}

async fn list_root_domain_subdomains(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(root_domain_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_domain_page(
        state
            .api
            .list_root_domain_hostnames(&context, &root_domain_id, query.page, query.page_size)
            .await,
        query.page,
        query.page_size,
    )
}

async fn create_root_domain_subdomain(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(root_domain_id): Path<String>,
    Json(request): Json<CreateRootDomainHostnameRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    created_resource(
        state
            .api
            .create_root_domain_hostname(&context, &root_domain_id, &request)
            .await,
    )
}

async fn list_managed_domains(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_domain_page(
        state
            .api
            .list_managed_domains(&context, query.page, query.page_size)
            .await,
        query.page,
        query.page_size,
    )
}

async fn create_managed_domain(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Json(request): Json<CreateManagedDomainRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    created_resource(state.api.create_managed_domain(&context, &request).await)
}

async fn delete_managed_domain(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(domain_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    no_content(state.api.delete_managed_domain(&context, &domain_id).await)
}

async fn verify_managed_domain(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(domain_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(state.api.verify_managed_domain(&context, &domain_id).await)
}

async fn update_domain_application_binding(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(domain_id): Path<String>,
    Json(request): Json<UpdateDomainApplicationBindingRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .update_domain_application_binding(&context, &domain_id, &request)
            .await,
    )
}

async fn delete_domain_application_binding(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(domain_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    no_content(
        state
            .api
            .delete_domain_application_binding(&context, &domain_id)
            .await,
    )
}

async fn list_application_source_versions(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(application_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_source_version_page(
        state
            .api
            .list_application_source_versions(
                &context,
                &application_id,
                query.page,
                query.page_size,
                query.cursor.as_deref(),
            )
            .await,
    )
}

async fn create_application_source_version(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(application_id): Path<String>,
    Json(request): Json<CreateSourceVersionRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    created_resource(
        state
            .api
            .create_application_source_version(&context, &application_id, &request)
            .await,
    )
}

async fn import_application_git_source_version(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(application_id): Path<String>,
    Json(request): Json<ImportGitSourceVersionRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    created_resource(
        state
            .api
            .import_application_git_source_version(&context, &application_id, &request)
            .await,
    )
}

async fn retrieve_application_source_version(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path((application_id, source_version_id)): Path<(String, String)>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .retrieve_application_source_version(&context, &application_id, &source_version_id)
            .await,
    )
}

async fn list_application_deployments(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(application_id): Path<String>,
    Query(query): Query<DeploymentPageQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_deployment_page(
        state
            .api
            .list_application_deployments(
                &context,
                &application_id,
                query.page,
                query.page_size,
                query.status,
                query.cursor.as_deref(),
            )
            .await,
    )
}

async fn create_application_deployment(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(application_id): Path<String>,
    Json(request): Json<CreateDeploymentRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    created_resource(
        state
            .api
            .create_application_deployment(&context, &application_id, &request)
            .await,
    )
}

async fn rollback_application_deployment(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path((application_id, deployment_id)): Path<(String, String)>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .rollback_application_deployment(&context, &application_id, &deployment_id)
            .await,
    )
}

async fn list_managed_certificates(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<CertificatePageQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_certificate_page(
        state
            .api
            .list_managed_certificates(
                &context,
                query.domain_id.as_deref(),
                query.page,
                query.page_size,
            )
            .await,
        query.page,
        query.page_size,
    )
}

async fn issue_managed_certificate(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Json(request): Json<IssueCertificateRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    accepted_async(
        state
            .api
            .issue_managed_certificate(&context, &request)
            .await,
    )
}

async fn retrieve_managed_certificate_operation(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(operation_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .retrieve_managed_certificate_operation(&context, &operation_id)
            .await,
    )
}

async fn list_application_listener_certificate_bindings(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path((application_id, domain_id)): Path<(String, String)>,
    Query(query): Query<PageQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_listener_certificate_binding_page(
        state
            .api
            .list_application_listener_certificate_bindings(
                &context,
                &application_id,
                &domain_id,
                query.page,
                query.page_size,
            )
            .await,
        query.page,
        query.page_size,
    )
}

async fn bind_application_listener_certificate(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path((application_id, domain_id)): Path<(String, String)>,
    Json(request): Json<CreateListenerCertificateBindingRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    created_resource(
        state
            .api
            .bind_application_listener_certificate(&context, &application_id, &domain_id, &request)
            .await,
    )
}

async fn unbind_application_listener_certificate(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path((application_id, domain_id, binding_id)): Path<(String, String, String)>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    no_content(
        state
            .api
            .unbind_application_listener_certificate(
                &context,
                &application_id,
                &domain_id,
                &binding_id,
            )
            .await,
    )
}

async fn update_managed_certificate(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(certificate_id): Path<String>,
    Json(request): Json<UpdateCertificateRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .update_managed_certificate(&context, &certificate_id, &request)
            .await,
    )
}

async fn renew_managed_certificate(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(certificate_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    accepted_async(
        state
            .api
            .renew_managed_certificate(&context, &certificate_id)
            .await,
    )
}

async fn revoke_managed_certificate(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(certificate_id): Path<String>,
    Json(request): Json<RevokeCertificateRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .revoke_managed_certificate(&context, &certificate_id, &request)
            .await,
    )
}

async fn delete_managed_certificate(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(certificate_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    no_content(
        state
            .api
            .delete_managed_certificate(&context, &certificate_id)
            .await,
    )
}

async fn list_certificate_distribution(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_certificate_distribution_page(
        state
            .api
            .list_certificate_distribution(&context, query.page, query.page_size)
            .await,
    )
}

async fn list_nginx_configs(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<ListNginxConfigsQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_nginx_config_page(state.api.list_nginx_configs(&context, &query).await)
}

async fn create_nginx_config(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Json(request): Json<CreateNginxConfigRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    created_resource(state.api.create_nginx_config(&context, &request).await)
}

async fn retrieve_nginx_config(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(config_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(state.api.retrieve_nginx_config(&context, &config_id).await)
}

async fn update_nginx_config(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(config_id): Path<String>,
    Json(request): Json<UpdateNginxConfigRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .update_nginx_config(&context, &config_id, &request)
            .await,
    )
}

async fn validate_nginx_config(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(config_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(state.api.validate_nginx_config(&context, &config_id).await)
}

async fn deploy_nginx_config(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(config_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(state.api.webserver_nginx_config(&context, &config_id).await)
}

async fn reload_nginx(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(state.api.reload_nginx(&context).await)
}

async fn retrieve_nginx_status(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(state.api.retrieve_nginx_status(&context).await)
}

async fn list_servers(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_server_page(
        state
            .api
            .list_servers(
                &context,
                query.page,
                query.page_size,
                query.cursor.as_deref(),
            )
            .await,
        query.page,
        query.page_size,
    )
}

async fn create_server(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Json(request): Json<CreateServerRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    created_resource(state.api.create_server(&context, &request).await)
}

async fn list_audit_logs(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<ListAuditLogsQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_audit_log_page(state.api.list_audit_logs(&context, &query).await)
}
