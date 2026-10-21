use axum::{
    extract::{Path, Query, State},
    response::Response,
    routing::{get, post},
    Extension, Json, Router,
};
use sdkwork_webserver_contract::{
    CreateApplicationRequest, CreateDomainDnsRecordRequest, CreateDomainRequest,
    CreateListenerCertificateBindingRequest, CreateManagedDomainRequest, CreateNginxConfigRequest,
    CreatePluginRequest, CreateRootDomainHostnameRequest, CreateRootDomainRequest,
    CreateServerRequest, CreateSourceVersionRequest, DomainDnsRecordStatusRequest,
    ImportGitSourceVersionRequest, IssueCertificateRequest, ListApplicationsQuery,
    ListAuditLogsQuery, ListDomainDnsRecordsQuery, ListNginxConfigsQuery, ListPluginsQuery,
    ListRootDomainsQuery, RevokeCertificateRequest, UpdateApplicationRequest,
    UpdateCertificateRequest, UpdateDomainApplicationBindingRequest, UpdateDomainDnsRecordRequest,
    UpdateNginxConfigRequest, UpdatePluginRequest, UpdateRootDomainRequest,
    UpsertDomainWechatVerificationRequest, WebBackendApi, WebBackendRequestContext,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    agent_routes, auth::require_backend_context, cluster_routes, metrics_summary_routes, paths,
    traffic_usage_routes, tunnel_machine_routes,
};
use sdkwork_routes_webserver_common::{
    accepted_async, created_resource, no_content, ok_application_page, ok_audit_log_page,
    ok_certificate_distribution_page, ok_certificate_page, ok_deployment_page, ok_dns_account_page,
    ok_domain_dns_record_page, ok_domain_page, ok_listener_certificate_binding_page,
    ok_nginx_config_page, ok_nginx_config_revision, ok_nginx_config_revision_page, ok_plugin_page,
    ok_resource, ok_root_domain_page, ok_server_page, ok_source_version_page,
    validate_pagination_query, WebApiError,
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
            get(retrieve_root_domain)
                .patch(update_root_domain)
                .delete(delete_root_domain),
        )
        .route(
            paths::ROOT_DOMAIN_SUBDOMAINS,
            get(list_root_domain_subdomains).post(create_root_domain_subdomain),
        )
        .route(
            paths::ROOT_DOMAIN_DNS_RECORDS,
            get(list_root_domain_dns_records).post(create_root_domain_dns_record),
        )
        .route(
            paths::ROOT_DOMAIN_DNS_RECORD,
            axum::routing::patch(update_root_domain_dns_record)
                .delete(delete_root_domain_dns_record),
        )
        .route(
            paths::ROOT_DOMAIN_DNS_RECORD_STATUS,
            axum::routing::patch(set_root_domain_dns_record_status),
        )
        .route(
            paths::ROOT_DOMAIN_WECHAT_VERIFICATION,
            get(retrieve_root_domain_wechat_verification)
                .put(upsert_root_domain_wechat_verification)
                .delete(delete_root_domain_wechat_verification),
        )
        .route(
            paths::ROOT_DOMAIN_WECHAT_VERIFICATION_CHECK,
            post(check_root_domain_wechat_verification),
        )
        .route(
            paths::ROOT_DOMAIN_DNS_RECORDS_SYNC,
            post(sync_root_domain_dns_records),
        )
        .route(
            paths::DOMAINS,
            get(list_managed_domains).post(create_managed_domain),
        )
        .route(
            paths::DOMAIN,
            get(retrieve_managed_domain).delete(delete_managed_domain),
        )
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
            get(list_application_deployments),
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
        .route(paths::DNS_ACCOUNTS, get(list_dns_accounts))
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
        .route(
            paths::NGINX_CONFIG_REVISIONS,
            get(list_nginx_config_revisions),
        )
        .route(
            paths::NGINX_CONFIG_REVISION,
            get(retrieve_nginx_config_revision),
        )
        .route(
            paths::NGINX_CONFIG_REVISION_ROLLBACK,
            post(rollback_nginx_config_revision),
        )
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
            paths::TUNNEL_MACHINES,
            get(tunnel_machine_routes::list_tunnel_machines),
        )
        .route(
            paths::TUNNEL_MACHINE,
            get(tunnel_machine_routes::retrieve_tunnel_machine),
        )
        .route(
            paths::TUNNEL_MACHINE_ENABLED,
            axum::routing::put(tunnel_machine_routes::set_tunnel_machine_enabled),
        )
        .route(
            paths::TUNNEL_MACHINE_ROUTES,
            post(tunnel_machine_routes::create_tunnel_machine_route),
        )
        .route(
            paths::TUNNEL_MACHINE_ROUTE,
            axum::routing::delete(tunnel_machine_routes::delete_tunnel_machine_route),
        )
        .route(
            paths::TRAFFIC_USAGE,
            get(traffic_usage_routes::retrieve_tenant_traffic_usage),
        )
        .route(
            paths::PLATFORM_TRAFFIC_USAGE,
            get(traffic_usage_routes::retrieve_platform_traffic_usage),
        )
        .route(
            paths::METRICS_SUMMARY,
            get(metrics_summary_routes::retrieve_metrics_summary),
        )
        .route(
            paths::PLATFORM_METRICS_SUMMARY,
            get(metrics_summary_routes::retrieve_platform_metrics_summary),
        )
        .route(paths::PLUGINS, get(list_plugins).post(create_plugin))
        .route(
            paths::PLUGIN,
            get(retrieve_plugin)
                .patch(update_plugin)
                .delete(delete_plugin),
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
pub(crate) struct PageQuery {
    #[serde(default = "default_page")]
    pub(crate) page: i32,
    #[serde(default = "default_page_size")]
    pub(crate) page_size: i32,
    pub(crate) cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CertificatePageQuery {
    #[serde(default = "default_page")]
    page: i32,
    #[serde(default = "default_page_size")]
    page_size: i32,
    #[serde(rename = "domain_id")]
    domain_id: Option<String>,
    cursor: Option<String>,
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

async fn update_root_domain(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(root_domain_id): Path<String>,
    Json(request): Json<UpdateRootDomainRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .update_root_domain(&context, &root_domain_id, &request)
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

/// Reads the Zone's synced resolution records, optionally restricted to one
/// subdomain, one 主机记录 keyword, or one record type. Store-only: the page
/// renders the snapshot and never waits on a provider.
async fn list_root_domain_dns_records(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(root_domain_id): Path<String>,
    Query(query): Query<ListDomainDnsRecordsQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_domain_dns_record_page(
        state
            .api
            .list_root_domain_dns_records(&context, &root_domain_id, &query)
            .await,
        query.page,
        query.page_size,
    )
}

/// Creates one resolution record of any managed type through the Zone's cloud
/// account and joins it to the snapshot. The contract idempotency key dedupes
/// a retried request; two distinct creates produce two vendor records — the
/// provider consoles' own semantics.
async fn create_root_domain_dns_record(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(root_domain_id): Path<String>,
    Json(request): Json<CreateDomainDnsRecordRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    created_resource(
        state
            .api
            .create_root_domain_dns_record(&context, &root_domain_id, &request)
            .await,
    )
}

/// Replaces one record in place on the provider, then in the snapshot.
async fn update_root_domain_dns_record(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path((root_domain_id, record_id)): Path<(String, String)>,
    Json(request): Json<UpdateDomainDnsRecordRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .update_root_domain_dns_record(&context, &root_domain_id, &record_id, &request)
            .await,
    )
}

/// Deletes one record on the provider and removes the stored row.
async fn delete_root_domain_dns_record(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path((root_domain_id, record_id)): Path<(String, String)>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    no_content(
        state
            .api
            .delete_root_domain_dns_record(&context, &root_domain_id, &record_id)
            .await,
    )
}

/// Pauses one record (暂停解析) or resumes it.
async fn set_root_domain_dns_record_status(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path((root_domain_id, record_id)): Path<(String, String)>,
    Json(request): Json<DomainDnsRecordStatusRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .set_root_domain_dns_record_status(&context, &root_domain_id, &record_id, &request)
            .await,
    )
}

async fn retrieve_root_domain_wechat_verification(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(root_domain_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .retrieve_root_domain_wechat_verification(&context, &root_domain_id)
            .await,
    )
}

async fn upsert_root_domain_wechat_verification(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(root_domain_id): Path<String>,
    Json(request): Json<UpsertDomainWechatVerificationRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .upsert_root_domain_wechat_verification(&context, &root_domain_id, &request)
            .await,
    )
}

async fn delete_root_domain_wechat_verification(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(root_domain_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    no_content(
        state
            .api
            .delete_root_domain_wechat_verification(&context, &root_domain_id)
            .await,
    )
}

async fn check_root_domain_wechat_verification(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(root_domain_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .check_root_domain_wechat_verification(&context, &root_domain_id)
            .await,
    )
}

/// Re-reads the Zone's inventory through its cloud account and replaces the
/// stored snapshot. Idempotent in effect — two concurrent syncs replace the
/// snapshot with equivalent answers — so the route carries the same
/// idempotency contract as the other write routes.
async fn sync_root_domain_dns_records(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(root_domain_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .sync_root_domain_dns_records(&context, &root_domain_id)
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

async fn retrieve_managed_domain(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(domain_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .retrieve_managed_domain(&context, &domain_id)
            .await,
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
    let cursor_request = query.cursor.is_some();
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
        cursor_request,
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
    let cursor_request = query.cursor.is_some();
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
        cursor_request,
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
                query.cursor.as_deref(),
            )
            .await,
        query.page,
        query.page_size,
        query.cursor.is_some(),
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

async fn list_dns_accounts(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_dns_account_page(
        state
            .api
            .list_dns_accounts(&context, query.page, query.page_size)
            .await,
        query.page,
        query.page_size,
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

async fn list_nginx_config_revisions(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(config_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_nginx_config_revision_page(
        state
            .api
            .list_nginx_config_revisions(&context, &config_id, query.page, query.page_size)
            .await,
    )
}

async fn retrieve_nginx_config_revision(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path((config_id, version_no)): Path<(String, i32)>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_nginx_config_revision(
        state
            .api
            .retrieve_nginx_config_revision(&context, &config_id, version_no)
            .await,
    )
}

async fn rollback_nginx_config_revision(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path((config_id, version_no)): Path<(String, i32)>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .rollback_nginx_config_revision(&context, &config_id, version_no)
            .await,
    )
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
    let cursor_request = query.cursor.is_some();
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
        cursor_request,
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

async fn list_plugins(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<ListPluginsQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_plugin_page(state.api.list_plugins(&context, &query).await)
}

async fn create_plugin(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Json(request): Json<CreatePluginRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    created_resource(state.api.create_plugin(&context, &request).await)
}

async fn retrieve_plugin(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(plugin_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(state.api.retrieve_plugin(&context, &plugin_id).await)
}

async fn update_plugin(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(plugin_id): Path<String>,
    Json(request): Json<UpdatePluginRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .update_plugin(&context, &plugin_id, &request)
            .await,
    )
}

async fn delete_plugin(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(plugin_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    no_content(state.api.delete_plugin(&context, &plugin_id).await)
}
