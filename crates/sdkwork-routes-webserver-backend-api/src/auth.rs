use axum::Extension;

use sdkwork_routes_webserver_common::WebApiError;
use sdkwork_utils_rust::SdkWorkResultCode;
use sdkwork_webserver_contract::{web_is_platform_operator_tenant, WebBackendRequestContext};

pub fn require_backend_context(
    context: Option<Extension<WebBackendRequestContext>>,
) -> Result<WebBackendRequestContext, WebApiError> {
    context.map(|Extension(context)| context).ok_or_else(|| {
        WebApiError::authentication_required("authenticated backend request context is required")
    })
}

/// Host-scoped administration surfaces (server files explorer, Web Server
/// configuration catalog, cluster plane) operate on the node's shared
/// filesystem and apply to every tenant at once. Per PRD-FR-030 a tenant-bound
/// backend context must never gain cross-tenant reach through a `web.*` grant,
/// so these surfaces answer only the platform operator tenant.
///
/// The platform operator tenant is resolved through
/// [`web_is_platform_operator_tenant`] so this guard and the IAM bootstrap
/// cannot disagree about which tenant that is. Route-level IAM authorization
/// still runs first: belonging to the operator tenant is necessary but not
/// sufficient, and a principal without the operation's `web.*` permission is
/// rejected by the framework layer before reaching here.
///
/// [`web_is_platform_operator_tenant`]: sdkwork_webserver_contract::web_is_platform_operator_tenant
pub fn require_platform_operator(
    context: &WebBackendRequestContext,
    surface: &'static str,
) -> Result<(), WebApiError> {
    let tenant_id = context.tenant_id.map(|id| id.to_string());
    if web_is_platform_operator_tenant(tenant_id.as_deref()) {
        return Ok(());
    }
    Err(WebApiError::new(
        SdkWorkResultCode::PermissionRequired,
        format!("{surface} is restricted to the platform operator tenant"),
    ))
}
