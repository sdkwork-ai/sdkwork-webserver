use axum::Extension;

use sdkwork_routes_webserver_common::WebApiError;
use sdkwork_utils_rust::SdkWorkResultCode;
use sdkwork_webserver_contract::WebBackendRequestContext;

pub fn require_backend_context(
    context: Option<Extension<WebBackendRequestContext>>,
) -> Result<WebBackendRequestContext, WebApiError> {
    context.map(|Extension(context)| context).ok_or_else(|| {
        WebApiError::authentication_required("authenticated backend request context is required")
    })
}

/// Host-scoped administration surfaces (server files explorer, Web Server
/// configuration catalog) operate on the node's shared filesystem and apply
/// to every tenant at once. Per PRD-FR-030 a tenant-bound backend context
/// must never gain cross-tenant reach through a `web.*` grant, so these
/// surfaces answer only the platform operator tenant (tenant 0).
pub fn require_platform_operator(
    context: &WebBackendRequestContext,
    surface: &'static str,
) -> Result<(), WebApiError> {
    if context.tenant_id == Some(0) {
        return Ok(());
    }
    Err(WebApiError::new(
        SdkWorkResultCode::PermissionRequired,
        format!("{surface} is restricted to the platform operator tenant"),
    ))
}
