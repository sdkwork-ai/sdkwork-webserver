use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use axum::Router;
use sdkwork_web_bootstrap::{CompositeReadinessCheck, ReadinessCheck};
use sdkwork_web_contract::{
    merge_openapi_documents, route_inventory_from_openapi, route_inventory_from_routes, HttpRoute,
    OpenApiMergeError,
};
use sdkwork_web_core::{
    AuditEmitter, DomainContextInjector, HttpRouteManifest, SecurityEventEmitter,
};
use sdkwork_webserver_contract::MachineCredentialAuthenticator;

const DEPENDENCY_UNAVAILABLE_CODE: i32 = 50301;

pub(crate) struct StandaloneApiProfile {
    pub router: Router,
    pub route_manifest: HttpRouteManifest,
    pub openapi: serde_json::Value,
    pub permission_catalog: Vec<&'static str>,
    pub domain_context_injectors: Vec<Arc<dyn DomainContextInjector>>,
    pub readiness_check: Arc<dyn ReadinessCheck>,
    pub machine_authenticator: Arc<dyn MachineCredentialAuthenticator>,
    pub audit_emitter: Arc<dyn AuditEmitter>,
    pub security_event_emitter: Arc<dyn SecurityEventEmitter>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum StandaloneProfileError {
    #[error("selected API assembly `{owner}` is unavailable (code {code}): {detail}")]
    AssemblyUnavailable {
        owner: &'static str,
        code: i32,
        detail: String,
    },
    #[error("API contribution `{owner}` is invalid: {detail}")]
    InvalidContribution { owner: &'static str, detail: String },
    #[error("standalone API route collision: {detail}")]
    RouteCollision { detail: String },
    #[error("standalone OpenAPI contribution merge failed: {0}")]
    OpenApiMerge(#[from] OpenApiMergeError),
}

impl StandaloneProfileError {
    fn assembly_unavailable(owner: &'static str, detail: impl Into<String>) -> Self {
        Self::AssemblyUnavailable {
            owner,
            code: DEPENDENCY_UNAVAILABLE_CODE,
            detail: detail.into(),
        }
    }

    #[cfg(test)]
    fn code(&self) -> Option<i32> {
        match self {
            Self::AssemblyUnavailable { code, .. } => Some(*code),
            _ => None,
        }
    }
}

struct OwnerApiContribution {
    owner: &'static str,
    router: Router,
    route_manifest: HttpRouteManifest,
    openapi: serde_json::Value,
    permission_catalog: Vec<&'static str>,
    domain_context_injectors: Vec<Arc<dyn DomainContextInjector>>,
    readiness_check: Arc<dyn ReadinessCheck>,
}

pub(crate) async fn assemble_standalone_profile(
) -> Result<StandaloneApiProfile, StandaloneProfileError> {
    let web = sdkwork_api_webserver_assembly::assemble_api_router(
        sdkwork_api_webserver_assembly::ApiAssemblyContext::default(),
    )
    .await
    .map_err(|error| {
        StandaloneProfileError::assembly_unavailable("sdkwork-web-server", error.to_string())
    })?;
    let web_iam_manifest = crate::iam_module_bootstrap::web_iam_module_manifest_path()
        .map_err(|error| StandaloneProfileError::assembly_unavailable("sdkwork-iam", error))?;
    let iam = sdkwork_api_iam_assembly::assemble_app_api_contribution_with_module_manifests(&[
        web_iam_manifest,
    ])
    .await
    .map_err(|error| StandaloneProfileError::assembly_unavailable("sdkwork-iam", error))?;
    let drive = sdkwork_api_drive_assembly::assemble_app_api_contribution()
        .await
        .map_err(|error| StandaloneProfileError::assembly_unavailable("sdkwork-drive", error))?;
    let skills = sdkwork_api_skills_assembly::assemble_app_api_contribution()
        .await
        .map_err(|error| StandaloneProfileError::assembly_unavailable("sdkwork-skills", error))?;
    let mcp = sdkwork_api_mcp_assembly::assemble_app_api_contribution()
        .await
        .map_err(|error| StandaloneProfileError::assembly_unavailable("sdkwork-mcp", error))?;
    let machine_authenticator = web.machine_credential_authenticator.clone();
    let audit_emitter = web.audit_emitter.clone();
    let security_event_emitter = web.security_event_emitter.clone();

    compose_owner_contributions(
        vec![
            OwnerApiContribution {
                owner: "sdkwork-web-server",
                router: web.router,
                route_manifest: web.route_manifest,
                openapi: web.openapi,
                permission_catalog: web.permission_catalog,
                domain_context_injectors: web.domain_context_injectors,
                readiness_check: web.readiness_check,
            },
            OwnerApiContribution {
                owner: "sdkwork-iam",
                router: iam.router,
                route_manifest: iam.route_manifest,
                openapi: iam.openapi,
                permission_catalog: iam.permission_catalog,
                domain_context_injectors: iam.domain_context_injectors,
                readiness_check: iam.readiness_check,
            },
            OwnerApiContribution {
                owner: "sdkwork-drive",
                router: drive.router,
                route_manifest: drive.route_manifest,
                openapi: drive.openapi,
                permission_catalog: drive.permission_catalog,
                domain_context_injectors: drive.domain_context_injectors,
                readiness_check: drive.readiness_check,
            },
            OwnerApiContribution {
                owner: "sdkwork-skills",
                router: skills.router,
                route_manifest: skills.route_manifest,
                openapi: skills.openapi,
                permission_catalog: skills.permission_catalog,
                domain_context_injectors: skills.domain_context_injectors,
                readiness_check: skills.readiness_check,
            },
            OwnerApiContribution {
                owner: "sdkwork-mcp",
                router: mcp.router,
                route_manifest: mcp.route_manifest,
                openapi: mcp.openapi,
                permission_catalog: mcp.permission_catalog,
                domain_context_injectors: mcp.domain_context_injectors,
                readiness_check: mcp.readiness_check,
            },
        ],
        machine_authenticator,
        audit_emitter,
        security_event_emitter,
    )
}

fn compose_owner_contributions(
    contributions: Vec<OwnerApiContribution>,
    machine_authenticator: Arc<dyn MachineCredentialAuthenticator>,
    audit_emitter: Arc<dyn AuditEmitter>,
    security_event_emitter: Arc<dyn SecurityEventEmitter>,
) -> Result<StandaloneApiProfile, StandaloneProfileError> {
    for contribution in &contributions {
        validate_owner_contribution(contribution)?;
    }
    validate_no_route_collisions(&contributions)?;
    let openapi = merge_openapi_documents(
        "SDKWork Web Server Standalone API",
        contributions
            .iter()
            .map(|contribution| (contribution.owner, &contribution.openapi)),
    )?;

    let mut router = Router::new();
    let mut routes = Vec::new();
    let mut permissions = BTreeSet::new();
    let mut domain_context_injectors = Vec::new();
    let mut readiness_checks = Vec::new();
    for contribution in contributions {
        router = router.merge(contribution.router);
        routes.extend_from_slice(contribution.route_manifest.routes());
        permissions.extend(contribution.permission_catalog);
        domain_context_injectors.extend(contribution.domain_context_injectors);
        readiness_checks.push(contribution.readiness_check);
    }

    let route_manifest = HttpRouteManifest::from_owned_routes(routes);
    let manifest_inventory = route_inventory_from_routes(route_manifest.routes());
    let openapi_inventory = route_inventory_from_openapi(&openapi).map_err(|detail| {
        StandaloneProfileError::InvalidContribution {
            owner: "standalone-composition",
            detail,
        }
    })?;
    if manifest_inventory != openapi_inventory {
        return Err(StandaloneProfileError::InvalidContribution {
            owner: "standalone-composition",
            detail: "combined route manifest and merged OpenAPI inventories differ".to_owned(),
        });
    }

    Ok(StandaloneApiProfile {
        router,
        route_manifest,
        openapi,
        permission_catalog: permissions.into_iter().collect(),
        domain_context_injectors,
        readiness_check: Arc::new(CompositeReadinessCheck::new(readiness_checks)),
        machine_authenticator,
        audit_emitter,
        security_event_emitter,
    })
}

fn validate_owner_contribution(
    contribution: &OwnerApiContribution,
) -> Result<(), StandaloneProfileError> {
    let manifest_inventory = route_inventory_from_routes(contribution.route_manifest.routes());
    let openapi_inventory =
        route_inventory_from_openapi(&contribution.openapi).map_err(|detail| {
            StandaloneProfileError::InvalidContribution {
                owner: contribution.owner,
                detail: format!("OpenAPI inventory is invalid: {detail}"),
            }
        })?;
    if manifest_inventory != openapi_inventory {
        return Err(StandaloneProfileError::InvalidContribution {
            owner: contribution.owner,
            detail: "route manifest and OpenAPI inventories differ".to_owned(),
        });
    }
    if contribution.permission_catalog != permission_catalog(contribution.route_manifest.routes()) {
        return Err(StandaloneProfileError::InvalidContribution {
            owner: contribution.owner,
            detail: "permission catalog differs from its route manifest".to_owned(),
        });
    }
    Ok(())
}

fn validate_no_route_collisions(
    contributions: &[OwnerApiContribution],
) -> Result<(), StandaloneProfileError> {
    let mut routes = BTreeMap::<(String, String, String), (&str, String)>::new();
    for contribution in contributions {
        for route in route_inventory_from_routes(contribution.route_manifest.routes()) {
            let identity = (
                route.surface.clone(),
                route.method.clone(),
                route.normalized_path.clone(),
            );
            if let Some((existing_owner, existing_operation)) = routes.insert(
                identity.clone(),
                (contribution.owner, route.operation_id.clone()),
            ) {
                return Err(StandaloneProfileError::RouteCollision {
                    detail: format!(
                        "{} {} {}: {} ({}) conflicts with {} ({})",
                        identity.0,
                        identity.1,
                        identity.2,
                        existing_owner,
                        existing_operation,
                        contribution.owner,
                        route.operation_id
                    ),
                });
            }
        }
    }
    Ok(())
}

fn permission_catalog(routes: &[HttpRoute]) -> Vec<&'static str> {
    let mut permissions = BTreeSet::new();
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use sdkwork_web_bootstrap::{AlwaysReady, ReadinessFuture};
    use sdkwork_web_contract::{build_openapi_document, HttpMethod};
    use sdkwork_web_core::{NoOpAuditEmitter, NoOpSecurityEventEmitter, WebRequestContext};
    use sdkwork_webserver_contract::{AuthenticatedMachineCredential, WebServiceResult};

    struct NoopMachineAuthenticator;

    #[async_trait]
    impl MachineCredentialAuthenticator for NoopMachineAuthenticator {
        async fn authenticate_machine_credential(
            &self,
            _credential: &str,
        ) -> WebServiceResult<Option<AuthenticatedMachineCredential>> {
            Ok(None)
        }
    }

    struct FailingReadiness;

    impl ReadinessCheck for FailingReadiness {
        fn check(&self) -> ReadinessFuture<'_> {
            Box::pin(async { Err("synthetic dependency unavailable".to_owned()) })
        }
    }

    struct MarkerInjector;

    impl DomainContextInjector for MarkerInjector {
        fn inject(&self, _request: &mut axum::extract::Request, _context: &WebRequestContext) {}
    }

    fn route(
        path: &'static str,
        operation_id: &'static str,
        permission: &'static str,
    ) -> HttpRoute {
        HttpRoute::dual_token(HttpMethod::Get, path, "test", operation_id)
            .with_required_permission(permission)
    }

    fn contribution(
        owner: &'static str,
        route: HttpRoute,
        readiness_check: Arc<dyn ReadinessCheck>,
    ) -> OwnerApiContribution {
        let route_manifest = HttpRouteManifest::from_owned_routes(vec![route]);
        OwnerApiContribution {
            owner,
            router: Router::new(),
            openapi: build_openapi_document(owner, route_manifest.routes()),
            permission_catalog: permission_catalog(route_manifest.routes()),
            route_manifest,
            domain_context_injectors: vec![Arc::new(MarkerInjector)],
            readiness_check,
        }
    }

    fn authenticator() -> Arc<dyn MachineCredentialAuthenticator> {
        Arc::new(NoopMachineAuthenticator)
    }

    fn compose_test_contributions(
        contributions: Vec<OwnerApiContribution>,
    ) -> Result<StandaloneApiProfile, StandaloneProfileError> {
        compose_owner_contributions(
            contributions,
            authenticator(),
            Arc::new(NoOpAuditEmitter),
            Arc::new(NoOpSecurityEventEmitter),
        )
    }

    fn expect_profile_error(
        result: Result<StandaloneApiProfile, StandaloneProfileError>,
        message: &str,
    ) -> StandaloneProfileError {
        match result {
            Ok(_) => panic!("{message}"),
            Err(error) => error,
        }
    }

    #[test]
    fn dependency_initialization_error_is_typed_as_50301() {
        let error = StandaloneProfileError::assembly_unavailable("sdkwork-drive", "offline");
        assert_eq!(error.code(), Some(50301));
        assert!(error.to_string().contains("sdkwork-drive"));
        assert!(error.to_string().contains("50301"));
    }

    #[test]
    fn route_collisions_fail_before_router_merge() {
        let error = expect_profile_error(
            compose_test_contributions(vec![
                contribution(
                    "first",
                    route("/app/v3/api/tests", "tests.list", "tests.read"),
                    Arc::new(AlwaysReady),
                ),
                contribution(
                    "second",
                    route("/app/v3/api/tests", "other.list", "other.read"),
                    Arc::new(AlwaysReady),
                ),
            ]),
            "duplicate route must fail",
        );
        assert!(matches!(
            error,
            StandaloneProfileError::RouteCollision { .. }
        ));
    }

    #[test]
    fn manifest_openapi_mismatch_fails_closed() {
        let mut contribution = contribution(
            "owner",
            route("/app/v3/api/tests", "tests.list", "tests.read"),
            Arc::new(AlwaysReady),
        );
        contribution.openapi["paths"] = serde_json::json!({});
        let error = expect_profile_error(
            compose_test_contributions(vec![contribution]),
            "OpenAPI drift must fail",
        );
        assert!(matches!(
            error,
            StandaloneProfileError::InvalidContribution { owner: "owner", .. }
        ));
    }

    #[test]
    fn permission_mismatch_fails_closed() {
        let mut contribution = contribution(
            "owner",
            route("/app/v3/api/tests", "tests.list", "tests.read"),
            Arc::new(AlwaysReady),
        );
        contribution.permission_catalog.clear();
        let error = expect_profile_error(
            compose_test_contributions(vec![contribution]),
            "permission drift must fail",
        );
        assert!(error.to_string().contains("permission catalog"));
    }

    #[test]
    fn owner_openapi_extensions_and_injectors_are_preserved() {
        let mut first = contribution(
            "first",
            route("/app/v3/api/first", "first.list", "first.read"),
            Arc::new(AlwaysReady),
        );
        first.openapi["components"]["schemas"]["FirstMarker"] =
            serde_json::json!({"type": "string"});
        first.openapi["x-sdkwork-first-owner"] = serde_json::json!(true);
        let second = contribution(
            "second",
            route("/app/v3/api/second", "second.list", "second.read"),
            Arc::new(AlwaysReady),
        );

        let profile = compose_test_contributions(vec![first, second]).expect("valid contributions");
        assert_eq!(profile.domain_context_injectors.len(), 2);
        assert_eq!(
            profile.permission_catalog,
            vec!["first.read", "second.read"]
        );
        assert_eq!(
            profile
                .openapi
                .pointer("/components/schemas/FirstMarker/type"),
            Some(&serde_json::json!("string"))
        );
        assert_eq!(profile.openapi["x-sdkwork-first-owner"], true);
    }

    #[tokio::test]
    async fn dependency_readiness_failure_is_retained() {
        let profile = compose_test_contributions(vec![
            contribution(
                "first",
                route("/app/v3/api/first", "first.list", "first.read"),
                Arc::new(AlwaysReady),
            ),
            contribution(
                "second",
                route("/app/v3/api/second", "second.list", "second.read"),
                Arc::new(FailingReadiness),
            ),
        ])
        .expect("valid contributions");
        let error = profile
            .readiness_check
            .check()
            .await
            .expect_err("dependency readiness must fail");
        assert_eq!(error, "synthetic dependency unavailable");
    }
}
