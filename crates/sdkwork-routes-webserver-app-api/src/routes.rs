use axum::{
    extract::{Path, Query, State},
    response::Response,
    routing::{get, post},
    Extension, Json, Router,
};
use sdkwork_webserver_contract::{
    CreateApplicationRequest, CreateDeploymentRequest, CreateDomainRequest,
    CreateEnvVariableRequest, CreateHealthCheckRequest, CreateListenerCertificateBindingRequest,
    CreateSourceVersionRequest, ImportGitSourceVersionRequest, ListApplicationsQuery,
    UpdateApplicationRequest, UpdateEnvVariableRequest, WebAppApi, WebAppRequestContext,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::{auth::require_app_context, paths};
use sdkwork_routes_webserver_common::{
    created_resource, no_content, ok_application_page, ok_deployment_page, ok_domain_page,
    ok_env_variable_page, ok_health_check_page, ok_listener_certificate_binding_page, ok_resource,
    ok_source_version_page, validate_pagination_query, WebApiError,
};

#[derive(Clone)]
struct AppState {
    api: Arc<dyn WebAppApi>,
}

pub fn build_router_with_app_api<A>(api: A) -> Router
where
    A: WebAppApi + 'static,
{
    build_router_with_shared_app_api(Arc::new(api))
}

pub fn build_router_with_shared_app_api(api: Arc<dyn WebAppApi>) -> Router {
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
            get(list_domains).post(create_domain),
        )
        .route(
            paths::APPLICATION_DOMAIN,
            get(retrieve_domain).delete(delete_domain),
        )
        .route(paths::APPLICATION_DOMAIN_VERIFY, post(verify_domain))
        .route(paths::DOMAINS, get(list_certificate_domains))
        .route(
            paths::APPLICATION_DOMAIN_LISTENER_CERTIFICATE_BINDINGS,
            get(list_listener_certificate_bindings).post(bind_listener_certificate),
        )
        .route(
            paths::APPLICATION_DOMAIN_LISTENER_CERTIFICATE_BINDING,
            axum::routing::delete(unbind_listener_certificate),
        )
        .route(
            paths::APPLICATION_SOURCE_VERSIONS,
            get(list_source_versions).post(create_source_version),
        )
        .route(
            paths::APPLICATION_SOURCE_VERSION_GIT_IMPORT,
            post(import_git_source_version),
        )
        .route(
            paths::APPLICATION_SOURCE_VERSION,
            get(retrieve_source_version),
        )
        .route(
            paths::APPLICATION_DEPLOYMENTS,
            get(list_deployments).post(create_deployment),
        )
        .route(paths::APPLICATION_DEPLOYMENT, get(retrieve_deployment))
        .route(
            paths::APPLICATION_DEPLOYMENT_ROLLBACK,
            post(rollback_deployment),
        )
        .route(
            paths::APPLICATION_ENV_VARIABLES,
            get(list_env_variables).post(create_env_variable),
        )
        .route(
            paths::APPLICATION_ENV_VARIABLE,
            axum::routing::patch(update_env_variable).delete(delete_env_variable),
        )
        .route(
            paths::APPLICATION_HEALTH_CHECKS,
            get(list_health_checks).post(create_health_check),
        )
        .layer(axum::middleware::from_fn(validate_pagination_query))
        .with_state(AppState { api })
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
struct DeploymentListQuery {
    #[serde(default = "default_page")]
    page: i32,
    #[serde(default = "default_page_size")]
    page_size: i32,
    status: Option<i32>,
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EnvVariableListQuery {
    environment: Option<String>,
}

fn default_page() -> i32 {
    1
}

fn default_page_size() -> i32 {
    20
}

async fn list_applications(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Query(query): Query<ListApplicationsQuery>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_application_page(state.api.list_applications(&context, &query).await)
}

async fn create_application(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Json(request): Json<CreateApplicationRequest>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    created_resource(state.api.create_application(&context, &request).await)
}

async fn retrieve_application(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path(application_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_resource(
        state
            .api
            .retrieve_application(&context, &application_id)
            .await,
    )
}

async fn update_application(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path(application_id): Path<String>,
    Json(request): Json<UpdateApplicationRequest>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_resource(
        state
            .api
            .update_application(&context, &application_id, &request)
            .await,
    )
}

async fn delete_application(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path(application_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    no_content(
        state
            .api
            .delete_application(&context, &application_id)
            .await,
    )
}

async fn activate_application(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path(application_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_resource(
        state
            .api
            .activate_application(&context, &application_id)
            .await,
    )
}

async fn pause_application(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path(application_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_resource(state.api.pause_application(&context, &application_id).await)
}

async fn list_domains(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path(application_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_domain_page(
        state
            .api
            .list_domains(&context, &application_id, query.page, query.page_size)
            .await,
        query.page,
        query.page_size,
    )
}

async fn list_certificate_domains(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Query(query): Query<PageQuery>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_domain_page(
        state
            .api
            .list_certificate_domains(&context, query.page, query.page_size)
            .await,
        query.page,
        query.page_size,
    )
}

async fn create_domain(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path(application_id): Path<String>,
    Json(request): Json<CreateDomainRequest>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    created_resource(
        state
            .api
            .create_domain(&context, &application_id, &request)
            .await,
    )
}

async fn retrieve_domain(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path((application_id, domain_id)): Path<(String, String)>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_resource(
        state
            .api
            .retrieve_domain(&context, &application_id, &domain_id)
            .await,
    )
}

async fn delete_domain(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path((application_id, domain_id)): Path<(String, String)>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    no_content(
        state
            .api
            .delete_domain(&context, &application_id, &domain_id)
            .await,
    )
}

async fn verify_domain(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path((application_id, domain_id)): Path<(String, String)>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_resource(
        state
            .api
            .verify_domain(&context, &application_id, &domain_id)
            .await,
    )
}

async fn list_deployments(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path(application_id): Path<String>,
    Query(query): Query<DeploymentListQuery>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_deployment_page(
        state
            .api
            .list_deployments(
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

async fn list_source_versions(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path(application_id): Path<String>,
    Query(query): Query<PageQuery>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_source_version_page(
        state
            .api
            .list_source_versions(
                &context,
                &application_id,
                query.page,
                query.page_size,
                query.cursor.as_deref(),
            )
            .await,
    )
}

async fn create_source_version(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path(application_id): Path<String>,
    Json(request): Json<CreateSourceVersionRequest>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    created_resource(
        state
            .api
            .create_source_version(&context, &application_id, &request)
            .await,
    )
}

async fn import_git_source_version(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path(application_id): Path<String>,
    Json(request): Json<ImportGitSourceVersionRequest>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    created_resource(
        state
            .api
            .import_git_source_version(&context, &application_id, &request)
            .await,
    )
}

async fn retrieve_source_version(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path((application_id, source_version_id)): Path<(String, String)>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_resource(
        state
            .api
            .retrieve_source_version(&context, &application_id, &source_version_id)
            .await,
    )
}

async fn create_deployment(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path(application_id): Path<String>,
    Json(request): Json<CreateDeploymentRequest>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    created_resource(
        state
            .api
            .create_deployment(&context, &application_id, &request)
            .await,
    )
}

async fn retrieve_deployment(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path((application_id, deployment_id)): Path<(String, String)>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_resource(
        state
            .api
            .retrieve_deployment(&context, &application_id, &deployment_id)
            .await,
    )
}

async fn rollback_deployment(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path((application_id, deployment_id)): Path<(String, String)>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_resource(
        state
            .api
            .rollback_deployment(&context, &application_id, &deployment_id)
            .await,
    )
}

async fn list_env_variables(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path(application_id): Path<String>,
    Query(query): Query<EnvVariableListQuery>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_env_variable_page(
        state
            .api
            .list_env_variables(&context, &application_id, query.environment.as_deref())
            .await,
    )
}

async fn create_env_variable(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path(application_id): Path<String>,
    Json(request): Json<CreateEnvVariableRequest>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    created_resource(
        state
            .api
            .create_env_variable(&context, &application_id, &request)
            .await,
    )
}

async fn update_env_variable(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path((application_id, variable_id)): Path<(String, String)>,
    Json(request): Json<UpdateEnvVariableRequest>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_resource(
        state
            .api
            .update_env_variable(&context, &application_id, &variable_id, &request)
            .await,
    )
}

async fn delete_env_variable(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path((application_id, variable_id)): Path<(String, String)>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    no_content(
        state
            .api
            .delete_env_variable(&context, &application_id, &variable_id)
            .await,
    )
}

async fn list_listener_certificate_bindings(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path((application_id, domain_id)): Path<(String, String)>,
    Query(query): Query<PageQuery>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_listener_certificate_binding_page(
        state
            .api
            .list_listener_certificate_bindings(
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

async fn bind_listener_certificate(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path((application_id, domain_id)): Path<(String, String)>,
    Json(request): Json<CreateListenerCertificateBindingRequest>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    created_resource(
        state
            .api
            .bind_listener_certificate(&context, &application_id, &domain_id, &request)
            .await,
    )
}

async fn unbind_listener_certificate(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path((application_id, domain_id, binding_id)): Path<(String, String, String)>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    no_content(
        state
            .api
            .unbind_listener_certificate(&context, &application_id, &domain_id, &binding_id)
            .await,
    )
}

async fn list_health_checks(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path(application_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    ok_health_check_page(
        state
            .api
            .list_health_checks(&context, &application_id)
            .await,
    )
}

async fn create_health_check(
    State(state): State<AppState>,
    context: Option<Extension<WebAppRequestContext>>,
    Path(application_id): Path<String>,
    Json(request): Json<CreateHealthCheckRequest>,
) -> Result<Response, WebApiError> {
    let context = require_app_context(context)?;
    created_resource(
        state
            .api
            .create_health_check(&context, &application_id, &request)
            .await,
    )
}
