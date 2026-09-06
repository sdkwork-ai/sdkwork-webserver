//! Web Server configuration management HTTP routes.
//!
//! Exposes the online configuration-editing plane for the *current* server
//! deployment, backed by [`sdkwork_webserver_config_service`]. The surface is
//! intentionally catalog-addressed:
//!
//! 1. `GET /backend/v3/api/webserver_configs` returns the managed catalog
//!    (default config, `imports.d/` import plane, and read-only
//!    sibling-module sidecar configs).
//! 2. `GET /backend/v3/api/webserver_configs/{configId}` reads one entry
//!    with its SHA-256 content digest.
//! 3. `PUT /backend/v3/api/webserver_configs/{configId}` validates and
//!    atomically overwrites one entry (timestamped backup, optimistic
//!    concurrency through `expectedSha256`).
//!
//! Only catalog-enumerated files are addressable; there is no arbitrary
//! path parameter anywhere on this surface.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    response::Response,
    routing::get,
    Extension, Json, Router,
};
use sdkwork_routes_webserver_common::WebApiError;
use sdkwork_utils_rust::SdkWorkResultCode;
use sdkwork_webserver_config_service::{
    WebserverConfigError, WebserverConfigService, WebserverConfigServiceConfig,
};
use sdkwork_webserver_contract::WebBackendRequestContext;
use serde::Deserialize;

use crate::{auth::require_backend_context, paths, server_files_routes::ok_json};

#[derive(Clone)]
struct WebserverConfigState {
    service: Arc<WebserverConfigService>,
}

/// Resolve the Web Server runtime configuration root.
///
/// `SDKWORK_WEBSERVER_CONFIG_ROOT` wins; otherwise the installed default
/// `/etc/sdkwork/webserver` (container/deb layout) applies.
pub fn default_webserver_config_root() -> String {
    if let Ok(configured) = std::env::var("SDKWORK_WEBSERVER_CONFIG_ROOT") {
        if !configured.trim().is_empty() {
            return configured;
        }
    }
    "/etc/sdkwork/webserver".to_string()
}

/// Build the Web Server configuration management router.
pub fn build_webserver_config_router(service: WebserverConfigService) -> Router {
    Router::new()
        .route(paths::WEBSERVER_CONFIGS, get(list_webserver_configs))
        .route(
            paths::WEBSERVER_CONFIG,
            get(read_webserver_config).put(update_webserver_config),
        )
        .with_state(WebserverConfigState {
            service: Arc::new(service),
        })
}

/// Compose the service from the standard environment variables.
///
/// `SDKWORK_DEPLOY_ROOT` additionally enables the read-only sibling-module
/// sidecar group.
pub fn webserver_config_service_from_env() -> Result<WebserverConfigService, WebApiError> {
    let deploy_root = std::env::var("SDKWORK_DEPLOY_ROOT")
        .ok()
        .filter(|root| !root.trim().is_empty());
    WebserverConfigService::new(WebserverConfigServiceConfig {
        config_root: default_webserver_config_root(),
        deploy_root,
        ..WebserverConfigServiceConfig::default()
    })
    .map_err(|_error| {
        WebApiError::new(
            SdkWorkResultCode::InternalError,
            "the webserver configuration root is invalid",
        )
    })
}

async fn list_webserver_configs(
    State(state): State<WebserverConfigState>,
    context: Option<Extension<WebBackendRequestContext>>,
) -> Result<Response, WebApiError> {
    require_backend_context(context)?;
    let catalog = state
        .service
        .catalog()
        .await
        .map_err(webserver_config_error)?;
    Ok(ok_json(&catalog))
}

async fn read_webserver_config(
    State(state): State<WebserverConfigState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(config_id): Path<String>,
) -> Result<Response, WebApiError> {
    require_backend_context(context)?;
    let file = state
        .service
        .read(&config_id)
        .await
        .map_err(webserver_config_error)?;
    Ok(ok_json(&file))
}

#[derive(Debug, Deserialize)]
struct UpdateWebserverConfigRequest {
    content: String,
    #[serde(rename = "expectedSha256")]
    expected_sha256: Option<String>,
}

async fn update_webserver_config(
    State(state): State<WebserverConfigState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(config_id): Path<String>,
    Json(request): Json<UpdateWebserverConfigRequest>,
) -> Result<Response, WebApiError> {
    require_backend_context(context)?;
    let result = state
        .service
        .write(
            &config_id,
            &request.content,
            request.expected_sha256.as_deref(),
        )
        .await
        .map_err(webserver_config_error)?;
    Ok(ok_json(&result))
}

fn webserver_config_error(error: WebserverConfigError) -> WebApiError {
    match &error {
        WebserverConfigError::NotFound => {
            WebApiError::new(SdkWorkResultCode::NotFound, error.to_string())
        }
        WebserverConfigError::TooLarge | WebserverConfigError::WriteTooLarge => {
            WebApiError::new(SdkWorkResultCode::PayloadTooLarge, error.to_string())
        }
        WebserverConfigError::ReadOnlyEntry | WebserverConfigError::InvalidContent(_) => {
            WebApiError::new(SdkWorkResultCode::ValidationError, error.to_string())
        }
        WebserverConfigError::Conflict { .. } => {
            WebApiError::new(SdkWorkResultCode::Conflict, error.to_string())
        }
        WebserverConfigError::Containment(_) => WebApiError::new(
            SdkWorkResultCode::ValidationError,
            "path is outside the authorized directory",
        ),
        WebserverConfigError::Io(_) => {
            WebApiError::new(SdkWorkResultCode::InternalError, error.to_string())
        }
    }
}
