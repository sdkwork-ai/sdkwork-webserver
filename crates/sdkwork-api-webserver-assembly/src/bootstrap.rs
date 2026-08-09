//! Business-only gateway bootstrap for sdkwork-web-server.

use axum::{Extension, Router};
use sdkwork_api_deployments_assembly::assemble_domain_certificate_blocks;
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
use sdkwork_web_bootstrap::{ReadinessCheck, ReadinessFuture};
use sdkwork_web_core::{
    AuditEmitter, DomainContextInjector, HttpRoute, HttpRouteManifest, SecurityEventEmitter,
};
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
    pub router: Router,
    pub route_manifest: HttpRouteManifest,
    pub openapi: serde_json::Value,
    pub permission_catalog: Vec<&'static str>,
    pub domain_context_injectors: Vec<Arc<dyn DomainContextInjector>>,
    pub readiness_check: Arc<dyn ReadinessCheck>,
    pub machine_credential_authenticator: Arc<dyn MachineCredentialAuthenticator>,
    pub audit_emitter: Arc<dyn AuditEmitter>,
    pub security_event_emitter: Arc<dyn SecurityEventEmitter>,
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
    let mut route_manifest = selected_route_manifest(context);
    let mut router = Router::new();
    let mut domain_context_injectors = Vec::new();
    let mut readiness_checks: Vec<Arc<dyn ReadinessCheck>> =
        vec![Arc::new(WebServiceReadinessCheck {
            service: service.clone(),
        })];
    if context.includes_standalone_control_plane() {
        // SDKWork Deployments domain/certificate management composes the Web
        // Server standalone surface as a same-origin dependency assembly
        // (API_ASSEMBLY_SPEC §6.1). The Deployments service host runs inside
        // this process; its composable blocks merge before the single Web
        // Framework layer is installed and authenticate through it. The
        // Deployments assembly is profile-selected (standalone embeds it;
        // cloud keeps the declared external base URL).
        let deploy_blocks = assemble_domain_certificate_blocks()
            .await
            .map_err(|detail| ApiAssemblyError::Initialization { detail })?;
        route_manifest = compose_route_manifests(&route_manifest, &deploy_blocks.route_manifest);
        domain_context_injectors.extend(deploy_blocks.domain_context_injectors);
        readiness_checks.push(deploy_blocks.readiness_check);
        router = router
            .merge(deploy_blocks.router)
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
    Ok(ApiAssembly {
        router: router.layer(Extension(service.clone())),
        route_manifest,
        openapi,
        permission_catalog,
        domain_context_injectors,
        readiness_check: if readiness_checks.len() == 1 {
            readiness_checks.pop().expect("readiness checks non-empty")
        } else {
            Arc::new(CombinedReadinessCheck {
                checks: readiness_checks,
            })
        },
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

pub async fn migrate_database_from_env() -> Result<(), ApiAssemblyError> {
    // Migrate every in-process database module in startup order
    // (DATABASE_FRAMEWORK_SPEC §4.3): the Web module first, then the
    // Deployments domain/certificate blocks composed by the standalone
    // gateway, then the Skills and MCP modules. Each module's baseline
    // bootstraps empty databases; versioned forward migrations converge
    // existing ones.
    std::env::set_var("SDKWORK_DATABASE_AUTO_MIGRATE", "true");
    sdkwork_webserver_database_host::bootstrap_web_database_from_env()
        .await
        .map(|_| ())
        .map_err(|detail| ApiAssemblyError::DatabaseMigration { detail })?;
    sdkwork_api_deployments_assembly::migrate_database_from_env()
        .await
        .map_err(|detail| ApiAssemblyError::DatabaseMigration { detail })?;
    sdkwork_skills_database_host::bootstrap_skills_database_from_env()
        .await
        .map(|_| ())
        .map_err(|detail| ApiAssemblyError::DatabaseMigration { detail })?;
    sdkwork_mcp_database_host::bootstrap_mcp_database_from_env()
        .await
        .map(|_| ())
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

/// Combines the host route inventory with a dependency assembly contribution
/// (API_ASSEMBLY_SPEC §4/§6.1). The host builds one combined manifest before
/// the single Web Framework layer is installed; OpenAPI and the permission
/// catalog are derived from this combined inventory.
fn compose_route_manifests(
    base: &HttpRouteManifest,
    dependency: &HttpRouteManifest,
) -> HttpRouteManifest {
    HttpRouteManifest::from_owned_routes(
        base.routes()
            .iter()
            .copied()
            .chain(dependency.routes().iter().copied())
            .collect(),
    )
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

#[cfg(test)]
mod tests {
    use super::{compose_route_manifests, selected_route_manifest, ApiAssemblyContext};
    use sdkwork_web_core::HttpMethod;

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
    fn composed_manifest_and_openapi_inventories_match() {
        // API_ASSEMBLY_SPEC §4: the host builds the served OpenAPI from the
        // same combined inventory as the executable router; the two
        // inventories must be identical (no duplicates, no orphans).
        let base = selected_route_manifest(ApiAssemblyContext::default());
        let dependency = sdkwork_api_deployments_assembly::domain_certificate_route_manifest();
        let composed = compose_route_manifests(&base, &dependency);
        let openapi = sdkwork_web_contract::build_openapi_document("debug", composed.routes());
        let manifest_inventory =
            sdkwork_web_contract::route_inventory_from_routes(composed.routes());
        let openapi_inventory =
            sdkwork_web_contract::route_inventory_from_openapi(&openapi).unwrap();
        assert_eq!(
            manifest_inventory, openapi_inventory,
            "combined route manifest and OpenAPI inventories diverged"
        );
        // The dependency operation keeps its permission metadata through the
        // combined inventory into the served OpenAPI. The Deployments
        // contract marks the blocks permission-free (`x-sdkwork-permission:
        // false` in its API source), so the served OpenAPI must not invent a
        // permission extension for them.
        assert!(
            openapi["paths"]["/app/v3/api/domain_zones"]["get"]["x-sdkwork-permission"].is_null()
        );
    }

    #[test]
    fn composed_manifest_includes_the_deployments_dependency_blocks() {
        // API_ASSEMBLY_SPEC §6.1: the standalone gateway combines the
        // dependency-owned assembly contribution (domain/certificate blocks)
        // into one route inventory before framework installation. The blocks
        // are open to every authenticated user: no permission is required.
        let base = selected_route_manifest(ApiAssemblyContext::default());
        let dependency = sdkwork_api_deployments_assembly::domain_certificate_route_manifest();
        let composed = compose_route_manifests(&base, &dependency);

        for (path, method) in [
            ("/app/v3/api/domain_zones", HttpMethod::Get),
            ("/app/v3/api/certificates", HttpMethod::Get),
        ] {
            let route = composed
                .routes()
                .iter()
                .find(|route| route.path == path && route.method == method)
                .unwrap_or_else(|| panic!("missing composed route {path}"));
            assert_eq!(
                route.required_permission, None,
                "{path} must not require a permission"
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
