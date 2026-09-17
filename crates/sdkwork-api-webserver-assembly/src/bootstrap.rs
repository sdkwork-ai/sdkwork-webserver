//! Business-only gateway bootstrap for sdkwork-webserver.

use axum::{Extension, Router};
use sdkwork_intelligence_webserver_repository_sqlx::bootstrap_web_runtime_from_env;
use sdkwork_intelligence_webserver_service::WebService;
use sdkwork_routes_webserver_app_api::{
    gateway_mount as mount_app, gateway_route_manifest as app_route_manifest,
    web_app_domain_context_injectors,
};
use sdkwork_routes_webserver_backend_api::{
    agent_gateway_mount, gateway_mount as mount_backend,
    gateway_route_manifest as backend_route_manifest, web_backend_domain_context_injectors,
    wrap_agent_router_with_web_framework_from_env,
};
use sdkwork_routes_webserver_internal_api::{
    gateway_mount as mount_internal, gateway_route_manifest as internal_route_manifest,
    web_internal_domain_context_injectors, wrap_router_with_web_framework_from_env,
};
use sdkwork_web_bootstrap::{ApiAssemblyContribution, ReadinessCheck, ReadinessFuture, WebModule};
use sdkwork_web_core::{AuditEmitter, HttpRoute, HttpRouteManifest, SecurityEventEmitter};
use sdkwork_webserver_contract::MachineCredentialAuthenticator;
use std::sync::Arc;

use crate::framework_observability::{WebFrameworkAuditEmitter, WebFrameworkSecurityEventEmitter};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ApiAssemblyProfile {
    #[default]
    Standalone,
    CloudGateway,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct ApiAssemblyContext {
    profile: ApiAssemblyProfile,
}

impl ApiAssemblyContext {
    /// Selects the Web Server service-to-service surface for the platform cloud gateway.
    /// Standalone Site, certificate, Nginx, and server management routes remain excluded.
    pub const fn cloud_gateway() -> Self {
        Self {
            profile: ApiAssemblyProfile::CloudGateway,
        }
    }

    const fn includes_standalone_control_plane(self) -> bool {
        matches!(self.profile, ApiAssemblyProfile::Standalone)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ApiAssemblyError {
    #[error("Web Server API assembly initialization failed: {detail}")]
    Initialization { detail: String },
    #[error("Web Server database migration failed: {detail}")]
    DatabaseMigration { detail: String },
}

impl ApiAssemblyError {
    pub const SERVICE_UNAVAILABLE_CODE: i32 = 50301;

    pub fn code(&self) -> i32 {
        Self::SERVICE_UNAVAILABLE_CODE
    }
}

pub struct ApiAssembly {
    pub contribution: ApiAssemblyContribution,
    pub machine_credential_authenticator: Arc<dyn MachineCredentialAuthenticator>,
    pub audit_emitter: Arc<dyn AuditEmitter>,
    pub security_event_emitter: Arc<dyn SecurityEventEmitter>,
}

impl ApiAssembly {
    /// Returns the complete host-neutral contribution consumed by both the
    /// platform cloud gateway and the standalone host.
    pub fn into_contribution(self) -> ApiAssemblyContribution {
        self.contribution
    }
}

struct CombinedReadinessCheck {
    checks: Vec<Arc<dyn ReadinessCheck>>,
}

impl ReadinessCheck for CombinedReadinessCheck {
    fn check(&self) -> ReadinessFuture<'_> {
        let checks = self.checks.clone();
        Box::pin(async move {
            for check in checks {
                check.check().await?;
            }
            Ok(())
        })
    }
}

struct WebServiceReadinessCheck {
    service: Arc<WebService>,
}

impl ReadinessCheck for WebServiceReadinessCheck {
    fn check(&self) -> ReadinessFuture<'_> {
        let service = self.service.clone();
        Box::pin(async move {
            service
                .ready_check()
                .await
                .map_err(|error| error.to_string())
        })
    }
}

pub async fn assemble_business_routes(
    context: ApiAssemblyContext,
) -> Result<ApiAssembly, ApiAssemblyError> {
    let runtime = bootstrap_web_runtime_from_env()
        .await
        .map_err(|detail| ApiAssemblyError::Initialization { detail })?;
    let service = Arc::new(runtime.service);
    let audit_emitter: Arc<dyn AuditEmitter> =
        Arc::new(WebFrameworkAuditEmitter::new(service.clone()));
    let security_event_emitter: Arc<dyn SecurityEventEmitter> =
        Arc::new(WebFrameworkSecurityEventEmitter::new(service.clone()));
    let route_manifest = selected_route_manifest(context);
    let mut router = Router::new();
    let mut domain_context_injectors = Vec::new();
    let mut readiness_checks: Vec<Arc<dyn ReadinessCheck>> =
        vec![Arc::new(WebServiceReadinessCheck {
            service: service.clone(),
        })];
    if context.includes_standalone_control_plane() {
        // Same-origin dependency surfaces are deliberately **not** composed
        // here. API_ASSEMBLY_SPEC §6.1 makes the standalone gateway the
        // composition point for every dependency it declares as same-origin and
        // forbids the host assembly from reclassifying dependency ownership by
        // folding dependency routes into this application's own contribution.
        // The gateway installs the Deployments domain/certificate blocks, Skills,
        // MCP, and Drive each as their own `WebModule` beside this one (see the
        // gateway's `dependency_assembly`), so every dependency keeps its own
        // owner, route manifest, OpenAPI document, and permission catalog.
        router = router
            .merge(mount_app(service.clone()))
            .merge(mount_backend(service.clone()))
            // Web Node agent routes authenticate through the shared api-key
            // path; wrap them in a machine-only framework layer so IAM user
            // API keys can never impersonate node credentials.
            .merge(
                wrap_agent_router_with_web_framework_from_env(
                    agent_gateway_mount(service.clone()),
                    service.clone(),
                    audit_emitter.clone(),
                    security_event_emitter.clone(),
                )
                .await,
            );
        domain_context_injectors.extend(web_app_domain_context_injectors());
        domain_context_injectors.extend(web_backend_domain_context_injectors());
    }
    // The internal (machine-to-machine) surface must remain machine-only on
    // every composed surface, including the standalone gateway where the
    // outer framework layer also accepts IAM user API keys. Wrapping the
    // internal router in its own `new_machine_only` framework layer here
    // guarantees that user API keys can never reach internal routes, and
    // `wagent_`-prefixed credentials never fall back to user-key resolution.
    router = router.merge(
        wrap_router_with_web_framework_from_env(
            mount_internal(service.clone()),
            service.clone(),
            audit_emitter.clone(),
            security_event_emitter.clone(),
        )
        .await,
    );
    domain_context_injectors.extend(web_internal_domain_context_injectors());
    let permission_catalog = permission_catalog(route_manifest.routes());
    let openapi = sdkwork_web_contract::build_openapi_document(
        "SDKWork Web Server API",
        route_manifest.routes(),
    );
    let readiness_check: Arc<dyn ReadinessCheck> = if readiness_checks.len() == 1 {
        readiness_checks.pop().expect("readiness checks non-empty")
    } else {
        Arc::new(CombinedReadinessCheck {
            checks: readiness_checks,
        })
    };
    let contribution = ApiAssemblyContribution::try_new(
        "sdkwork-webserver",
        router.layer(Extension(service.clone())),
        route_manifest,
        openapi,
        permission_catalog,
        domain_context_injectors,
        readiness_check,
    )
    .map_err(|detail| ApiAssemblyError::Initialization { detail })?;
    Ok(ApiAssembly {
        contribution,
        machine_credential_authenticator: service,
        audit_emitter,
        security_event_emitter,
    })
}

pub async fn assemble_api_router(
    context: ApiAssemblyContext,
) -> Result<ApiAssembly, ApiAssemblyError> {
    assemble_business_routes(context).await
}

/// Seed the SDKWork space repository under the deployment root (owner API
/// integration point).
///
/// The standalone gateway must not depend on service crates directly; this
/// assembly-owned wrapper forwards to the Server Files service so the gateway
/// only consumes the owner assembly (API_ASSEMBLY_SPEC §4/§6).
pub async fn seed_space_repository() -> Result<std::path::PathBuf, String> {
    let deployment_root = std::env::var("SDKWORK_DEPLOY_ROOT").ok();
    sdkwork_routes_webserver_backend_api::ensure_space_repository(deployment_root.as_deref())
        .await
        .map_err(|error| error.to_string())
}

pub async fn migrate_database_from_env() -> Result<(), ApiAssemblyError> {
    // Migrate every in-process database module in startup order
    // (DATABASE_FRAMEWORK_SPEC §4.3): the Web module first, then the
    // Deployments domain/certificate blocks, Skills, and MCP modules that the
    // standalone gateway composes as same-origin dependencies. Each module's
    // baseline bootstraps empty databases; versioned forward migrations converge
    // existing ones.
    std::env::set_var("SDKWORK_DATABASE_AUTO_MIGRATE", "true");
    sdkwork_webserver_database_host::bootstrap_web_database_from_env()
        .await
        .map(|_| ())
        .map_err(|detail| ApiAssemblyError::DatabaseMigration { detail })?;
    sdkwork_api_deployments_assembly::migrate_database_from_env()
        .await
        .map_err(|detail| ApiAssemblyError::DatabaseMigration { detail })?;
    sdkwork_api_skills_assembly::bootstrap_database_from_env()
        .await
        .map_err(|detail| ApiAssemblyError::DatabaseMigration { detail })?;
    sdkwork_api_mcp_assembly::bootstrap_database_from_env()
        .await
        .map_err(|detail| ApiAssemblyError::DatabaseMigration { detail })?;
    Ok(())
}

fn permission_catalog(routes: &[HttpRoute]) -> Vec<&'static str> {
    let mut permissions = std::collections::BTreeSet::new();
    for route in routes {
        if let Some(permission) = route.required_permission {
            permissions.insert(permission);
        }
        if let Some(alternate_permissions) = route.alternate_permissions {
            permissions.extend(alternate_permissions.iter().copied());
        }
    }
    permissions.into_iter().collect()
}

fn selected_route_manifest(context: ApiAssemblyContext) -> HttpRouteManifest {
    let mut routes = Vec::new();
    if context.includes_standalone_control_plane() {
        routes.extend_from_slice(app_route_manifest().routes());
        routes.extend_from_slice(backend_route_manifest().routes());
    }
    routes.extend_from_slice(internal_route_manifest().routes());
    HttpRouteManifest::from_owned_routes(routes)
}

/// Installs this application as a Web Module with caller-supplied assembly
/// context (API_ASSEMBLY_SPEC §4.1.1).
pub async fn web_module_with_context(context: ApiAssemblyContext) -> Result<WebModule, String> {
    let assembly = assemble_api_router(context)
        .await
        .map_err(|error| error.to_string())?;
    Ok(WebModule::from_contribution(assembly.contribution))
}

/// Canonical Web Module definition for this application
/// (API_ASSEMBLY_SPEC §4.1.1): the complete HTTP surface — every route,
/// manifest, and OpenAPI document of this owner — as one installable module.
///
/// The default context is the standalone profile, which also mounts the
/// Site, certificate, Nginx, and server management control plane. The platform
/// cloud gateway installs [`web_module_with_context`] with
/// [`ApiAssemblyContext::cloud_gateway`] instead.
pub async fn web_module() -> Result<WebModule, String> {
    web_module_with_context(ApiAssemblyContext::default()).await
}

#[cfg(test)]
mod tests {
    use super::{selected_route_manifest, ApiAssemblyContext};

    #[test]
    fn cloud_gateway_profile_exposes_only_web_internal_routes() {
        let manifest = selected_route_manifest(ApiAssemblyContext::cloud_gateway());

        assert!(!manifest.routes().is_empty());
        assert!(manifest
            .routes()
            .iter()
            .all(|route| route.path.starts_with("/internal/v3/api/web/")));
    }

    #[test]
    fn standalone_manifest_and_openapi_inventories_match() {
        // API_ASSEMBLY_SPEC §4: this assembly publishes its route inventory and
        // the served OpenAPI from the same routes; the two must be identical
        // (no duplicates, no orphans).
        let manifest = selected_route_manifest(ApiAssemblyContext::default());
        let openapi = sdkwork_web_contract::build_openapi_document("debug", manifest.routes());

        assert_eq!(
            sdkwork_web_contract::route_inventory_from_routes(manifest.routes()),
            sdkwork_web_contract::route_inventory_from_openapi(&openapi).unwrap(),
            "route manifest and OpenAPI inventories diverged"
        );
    }

    #[test]
    fn standalone_manifest_owns_no_same_origin_dependency_routes() {
        // API_ASSEMBLY_SPEC §6.1: the standalone gateway composes same-origin
        // dependencies as their own modules and must not reclassify dependency
        // ownership by folding dependency routes into this application's
        // contribution. A dependency route present here would be composed twice
        // by the gateway and fail route-collision validation, or — through the
        // module registry's duplicate-owner tolerance — silently drop a surface
        // the gateway contract declares as served.
        let manifest = selected_route_manifest(ApiAssemblyContext::default());

        for prefix in [
            "/app/v3/api/skills",
            "/backend/v3/api/skills",
            "/app/v3/api/mcp",
            "/backend/v3/api/mcp",
            "/app/v3/api/drive",
            "/backend/v3/api/drive",
            "/app/v3/api/domain_zones",
            "/app/v3/api/certificates",
        ] {
            assert!(
                !manifest
                    .routes()
                    .iter()
                    .any(|route| route.path.starts_with(prefix)),
                "the web server assembly must not own same-origin dependency route {prefix}"
            );
        }
    }

    #[test]
    fn standalone_profile_retains_control_plane_routes() {
        let manifest = selected_route_manifest(ApiAssemblyContext::default());

        assert!(manifest
            .routes()
            .iter()
            .any(|route| route.path.starts_with("/app/v3/api/applications")));
        assert!(manifest
            .routes()
            .iter()
            .any(|route| route.path.starts_with("/backend/v3/api/nginx")));
        assert!(manifest
            .routes()
            .iter()
            .any(|route| route.path.starts_with("/internal/v3/api/web/")));
    }
}
