use std::sync::Arc;

use sdkwork_web_bootstrap::{ApiModuleRegistry, ComposedApiAssembly, WebModule};
use sdkwork_web_core::{AuditEmitter, SecurityEventEmitter};
use sdkwork_webserver_contract::MachineCredentialAuthenticator;

const DEPENDENCY_UNAVAILABLE_CODE: i32 = 50301;

pub(crate) struct StandaloneApiProfile {
    pub assembly: ComposedApiAssembly,
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
    #[error("standalone API assembly composition is invalid: {detail}")]
    InvalidComposition { detail: String },
}

impl StandaloneProfileError {
    /// Typed dependency-unavailable error (code 50301). Shared with
    /// `dependency_assembly` so a missing composition prerequisite is reported
    /// the same way as a dependency bootstrap failure.
    pub(crate) fn assembly_unavailable(owner: &'static str, detail: impl Into<String>) -> Self {
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
            Self::InvalidComposition { .. } => None,
        }
    }
}

pub(crate) async fn assemble_standalone_profile(
) -> Result<StandaloneApiProfile, StandaloneProfileError> {
    let web = sdkwork_api_webserver_assembly::assemble_api_router(
        sdkwork_api_webserver_assembly::ApiAssemblyContext::default(),
    )
    .await
    .map_err(|error| {
        StandaloneProfileError::assembly_unavailable("sdkwork-webserver", error.to_string())
    })?;
    // Drive (`/app/v3/api/drive/*` + `/backend/v3/api/drive/storage/*`) and IAM
    // (`/app/v3/api/iam/*` + `/backend/v3/api/iam/*` + the IAM open surface) are
    // each exactly one contributed module: both owners ship their surfaces as
    // one already-composed contribution, because the drive and IAM assembly
    // crates are outside the webserver assembly crate graph (API_ASSEMBLY_SPEC
    // §4.1.1). Fetching either owner surface-by-surface here would drop the
    // second surface through the registry's duplicate-owner tolerance.
    let dependency_modules = crate::dependency_assembly::same_origin_dependency_modules().await?;
    let machine_authenticator = web.machine_credential_authenticator.clone();
    let audit_emitter = web.audit_emitter.clone();
    let security_event_emitter = web.security_event_emitter.clone();

    // One module per served owner (API_ASSEMBLY_SPEC §4.1.1). Registering the
    // same owner as two modules makes the registry drop the second one and
    // serve a partial route surface.
    let mut modules = vec![WebModule::from_contribution(web.into_contribution())];
    modules.extend(dependency_modules);

    compose_owner_modules(
        modules,
        machine_authenticator,
        audit_emitter,
        security_event_emitter,
    )
}

fn compose_owner_modules(
    modules: Vec<WebModule>,
    machine_authenticator: Arc<dyn MachineCredentialAuthenticator>,
    audit_emitter: Arc<dyn AuditEmitter>,
    security_event_emitter: Arc<dyn SecurityEventEmitter>,
) -> Result<StandaloneApiProfile, StandaloneProfileError> {
    let mut module_registry = ApiModuleRegistry::new();
    module_registry.add_modules(modules);
    // `ApiModuleRegistry` ignores a module owner that is registered twice
    // instead of failing composition. That silently removes the second
    // registration's routes and reaches the operator as a 404 on a surface this
    // gateway's component contract declares as served, which API_ASSEMBLY_SPEC
    // §6.1 forbids. Fail closed: a duplicate owner is a startup error, never a
    // partial route surface.
    let ignored_duplicates: Vec<&'static str> = module_registry.ignored_duplicates().to_vec();
    if !ignored_duplicates.is_empty() {
        return Err(StandaloneProfileError::InvalidComposition {
            detail: format!(
                "module owners registered more than once: {}; each served owner must install exactly one module (API_ASSEMBLY_SPEC §4.1.1)",
                ignored_duplicates.join(", ")
            ),
        });
    }
    let assembly = module_registry
        .try_compose("SDKWork Web Server Standalone API")
        .map_err(|detail| StandaloneProfileError::InvalidComposition { detail })?;
    Ok(StandaloneApiProfile {
        assembly,
        machine_authenticator,
        audit_emitter,
        security_event_emitter,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use axum::Router;
    use sdkwork_web_bootstrap::{
        AlwaysReady, ApiAssemblyContribution, ReadinessCheck, ReadinessFuture,
    };
    use sdkwork_web_contract::{HttpMethod, HttpRoute};
    use sdkwork_web_core::{
        DomainContextInjector, HttpRouteManifest, NoOpAuditEmitter, NoOpSecurityEventEmitter,
        WebRequestContext,
    };
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

    fn contribution(
        owner: &'static str,
        path: &'static str,
        operation_id: &'static str,
        permission: &'static str,
        readiness_check: Arc<dyn ReadinessCheck>,
    ) -> ApiAssemblyContribution {
        let route = HttpRoute::dual_token(HttpMethod::Get, path, "test", operation_id)
            .with_required_permission(permission);
        ApiAssemblyContribution::from_manifest(
            owner,
            owner,
            Router::new(),
            HttpRouteManifest::from_owned_routes(vec![route]),
            vec![Arc::new(MarkerInjector)],
            readiness_check,
        )
        .expect("valid test contribution")
    }

    fn one_module_per_contribution(contributions: Vec<ApiAssemblyContribution>) -> Vec<WebModule> {
        contributions
            .into_iter()
            .map(WebModule::from_contribution)
            .collect()
    }

    fn compose_test_modules(
        modules: Vec<WebModule>,
    ) -> Result<StandaloneApiProfile, StandaloneProfileError> {
        compose_owner_modules(
            modules,
            Arc::new(NoopMachineAuthenticator),
            Arc::new(NoOpAuditEmitter),
            Arc::new(NoOpSecurityEventEmitter),
        )
    }

    fn compose_test_contributions(
        contributions: Vec<ApiAssemblyContribution>,
    ) -> Result<StandaloneApiProfile, StandaloneProfileError> {
        compose_test_modules(one_module_per_contribution(contributions))
    }

    #[test]
    fn one_owner_split_across_two_contributions_fails_closed() {
        // API_ASSEMBLY_SPEC §4.1.1 keeps one indivisible contribution per
        // served owner. An application's app surface and backend surface must
        // therefore arrive already composed as one contribution; splitting one
        // owner across two contributions must fail instead of mounting a
        // partial route surface.
        let app_surface = contribution(
            "sdkwork-drive",
            "/app/v3/api/drive/files",
            "drive.files.list",
            "drive.files.read",
            Arc::new(AlwaysReady),
        );
        let backend_surface = contribution(
            "sdkwork-drive",
            "/backend/v3/api/drive/storage/provider-kinds",
            "drive.storage.providerKinds.list",
            "drive.storage.read",
            Arc::new(AlwaysReady),
        );
        let module = WebModule::new("sdkwork-drive", "SDKWork Drive API")
            .with_surface(app_surface)
            .with_surface(backend_surface);

        let error = compose_test_modules(vec![module])
            .err()
            .expect("one owner split across two contributions must fail closed");
        assert!(
            error.to_string().contains("sdkwork-drive"),
            "the offending owner must be named: {error}"
        );
        assert!(
            error.to_string().contains("selected more than once"),
            "the failure must name the duplicate-owner cause: {error}"
        );
    }

    #[test]
    fn duplicate_module_owner_fails_closed_instead_of_dropping_routes() {
        // A second module for an owner the registry already mounted would have
        // its routes dropped silently, leaving a partial route surface that
        // returns 404 for a declared dependency (API_ASSEMBLY_SPEC §6.1).
        let app_module = WebModule::from_contribution(contribution(
            "sdkwork-drive",
            "/app/v3/api/drive/files",
            "drive.files.list",
            "drive.files.read",
            Arc::new(AlwaysReady),
        ));
        let storage_module = WebModule::from_contribution(contribution(
            "sdkwork-drive",
            "/backend/v3/api/drive/storage/provider-kinds",
            "drive.storage.providerKinds.list",
            "drive.storage.read",
            Arc::new(AlwaysReady),
        ));

        let error = compose_test_modules(vec![app_module, storage_module])
            .err()
            .expect("a duplicate module owner must fail closed");
        assert!(
            error.to_string().contains("sdkwork-drive"),
            "the duplicate owner must be named: {error}"
        );
        assert!(
            error.to_string().contains("registered more than once"),
            "the failure must name the duplicate-registration cause: {error}"
        );
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
        let error = compose_test_contributions(vec![
            contribution(
                "sdkwork-first",
                "/app/v3/api/tests",
                "tests.list",
                "tests.read",
                Arc::new(AlwaysReady),
            ),
            contribution(
                "sdkwork-second",
                "/app/v3/api/tests",
                "other.list",
                "other.read",
                Arc::new(AlwaysReady),
            ),
        ])
        .err()
        .expect("duplicate route must fail");
        assert!(matches!(
            error,
            StandaloneProfileError::InvalidComposition { .. }
        ));
    }

    #[test]
    fn manifest_openapi_mismatch_fails_closed() {
        let mut contribution = contribution(
            "sdkwork-owner",
            "/app/v3/api/tests",
            "tests.list",
            "tests.read",
            Arc::new(AlwaysReady),
        );
        contribution.openapi["paths"] = serde_json::json!({});
        let error = compose_test_contributions(vec![contribution])
            .err()
            .expect("OpenAPI drift must fail");
        assert!(error.to_string().contains("inventories differ"));
    }

    #[test]
    fn permission_mismatch_fails_closed() {
        let mut contribution = contribution(
            "sdkwork-owner",
            "/app/v3/api/tests",
            "tests.list",
            "tests.read",
            Arc::new(AlwaysReady),
        );
        contribution.permission_catalog.clear();
        let error = compose_test_contributions(vec![contribution])
            .err()
            .expect("permission drift must fail");
        assert!(error.to_string().contains("permission catalog"));
    }

    #[test]
    fn owner_metadata_and_injectors_are_preserved() {
        let mut first = contribution(
            "sdkwork-first",
            "/app/v3/api/first",
            "first.list",
            "first.read",
            Arc::new(AlwaysReady),
        );
        first.openapi["x-sdkwork-first-owner"] = serde_json::json!(true);
        let second = contribution(
            "sdkwork-second",
            "/app/v3/api/second",
            "second.list",
            "second.read",
            Arc::new(AlwaysReady),
        );

        let profile = compose_test_contributions(vec![first, second]).expect("valid contributions");
        assert_eq!(profile.assembly.domain_context_injectors.len(), 2);
        assert_eq!(
            profile.assembly.permission_catalog,
            vec!["first.read", "second.read"]
        );
        assert_eq!(profile.assembly.openapi["x-sdkwork-first-owner"], true);
    }

    #[tokio::test]
    async fn dependency_readiness_failure_is_retained() {
        let profile = compose_test_contributions(vec![
            contribution(
                "sdkwork-first",
                "/app/v3/api/first",
                "first.list",
                "first.read",
                Arc::new(AlwaysReady),
            ),
            contribution(
                "sdkwork-second",
                "/app/v3/api/second",
                "second.list",
                "second.read",
                Arc::new(FailingReadiness),
            ),
        ])
        .expect("valid contributions");
        let error = profile
            .assembly
            .readiness_check
            .check()
            .await
            .expect_err("dependency readiness must fail");
        assert_eq!(error, "synthetic dependency unavailable");
    }
}
