use std::sync::Arc;

use sdkwork_web_bootstrap::{ApiAssemblyContribution, ComposedApiAssembly};
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
    let federated_iam_manifests =
        crate::iam_module_bootstrap::federated_iam_module_manifest_paths().map_err(|error| {
            StandaloneProfileError::assembly_unavailable("sdkwork-iam", error)
        })?;
    let iam = sdkwork_api_iam_assembly::assemble_app_api_contribution_with_module_manifests(
        &federated_iam_manifests,
    )
    .await
    .map_err(|error| StandaloneProfileError::assembly_unavailable("sdkwork-iam", error))?;
    let drive = sdkwork_api_drive_assembly::assemble_app_api_contribution()
        .await
        .map_err(|error| StandaloneProfileError::assembly_unavailable("sdkwork-drive", error))?;
    let dependency_contributions =
        crate::dependency_assembly::optional_same_origin_dependency_contributions().await?;
    let machine_authenticator = web.machine_credential_authenticator.clone();
    let audit_emitter = web.audit_emitter.clone();
    let security_event_emitter = web.security_event_emitter.clone();

    let mut contributions = vec![web.into_contribution(), iam, drive];
    contributions.extend(dependency_contributions);

    compose_owner_contributions(
        contributions,
        machine_authenticator,
        audit_emitter,
        security_event_emitter,
    )
}

fn compose_owner_contributions(
    contributions: Vec<ApiAssemblyContribution>,
    machine_authenticator: Arc<dyn MachineCredentialAuthenticator>,
    audit_emitter: Arc<dyn AuditEmitter>,
    security_event_emitter: Arc<dyn SecurityEventEmitter>,
) -> Result<StandaloneApiProfile, StandaloneProfileError> {
    let assembly =
        ComposedApiAssembly::try_compose("SDKWork Web Server Standalone API", contributions)
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
    use sdkwork_web_bootstrap::{AlwaysReady, ReadinessCheck, ReadinessFuture};
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

    fn compose_test_contributions(
        contributions: Vec<ApiAssemblyContribution>,
    ) -> Result<StandaloneApiProfile, StandaloneProfileError> {
        compose_owner_contributions(
            contributions,
            Arc::new(NoopMachineAuthenticator),
            Arc::new(NoOpAuditEmitter),
            Arc::new(NoOpSecurityEventEmitter),
        )
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
