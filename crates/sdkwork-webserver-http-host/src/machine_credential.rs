use std::sync::Arc;

use async_trait::async_trait;
use sdkwork_iam_web_adapter::IamWebRequestContextResolver;
use sdkwork_web_core::{
    JwtProductionClaimPolicy, ResolverProductionProfile, WebAuthLevel, WebDeploymentMode,
    WebEnvironment, WebFrameworkError, WebLoginScope, WebRequestContextResolver,
    WebRequestPrincipal, WebSubjectType,
};
use sdkwork_webserver_contract::MachineCredentialAuthenticator;

/// Prefix of Web Node machine credentials; credentials with this prefix must be
/// validated by the machine authenticator and never fall back to user API keys.
pub const MACHINE_CREDENTIAL_PREFIX: &str = "wagent_";

#[derive(Clone)]
pub struct MachineCredentialResolverDecorator<R>
where
    R: WebRequestContextResolver,
{
    inner: R,
    authenticator: Arc<dyn MachineCredentialAuthenticator>,
    production_profile: Option<ResolverProductionProfile>,
    machine_only: bool,
}

impl<R> MachineCredentialResolverDecorator<R>
where
    R: WebRequestContextResolver,
{
    pub fn new(inner: R, authenticator: Arc<dyn MachineCredentialAuthenticator>) -> Self {
        Self {
            inner,
            authenticator,
            production_profile: None,
            machine_only: false,
        }
    }

    /// Resolver for machine-only surfaces (the application-ingress internal API):
    /// credentials that are not validated as machine credentials are rejected and
    /// never fall back to the user API-key resolver.
    pub fn new_machine_only(
        inner: R,
        authenticator: Arc<dyn MachineCredentialAuthenticator>,
    ) -> Self {
        Self {
            inner,
            authenticator,
            // Machine credentials are tenant-bound (the authenticator returns
            // a tenant-scoped principal), so the machine-only surface reports
            // the tenant-bound production profile and passes the framework
            // production-assembly resolver validation.
            production_profile: Some(ResolverProductionProfile::TenantBoundBootstrap),
            machine_only: true,
        }
    }
}

impl MachineCredentialResolverDecorator<IamWebRequestContextResolver> {
    pub fn new_standalone_iam(
        inner: IamWebRequestContextResolver,
        authenticator: Arc<dyn MachineCredentialAuthenticator>,
    ) -> Self {
        Self {
            inner,
            authenticator,
            production_profile: Some(ResolverProductionProfile::TenantBoundBootstrap),
            machine_only: false,
        }
    }
}

#[async_trait]
impl<R> WebRequestContextResolver for MachineCredentialResolverDecorator<R>
where
    R: WebRequestContextResolver,
{
    fn resolver_production_profile(&self) -> ResolverProductionProfile {
        self.production_profile
            .unwrap_or_else(|| self.inner.resolver_production_profile())
    }

    fn jwt_production_claim_policy(&self) -> Option<JwtProductionClaimPolicy> {
        self.inner.jwt_production_claim_policy()
    }

    fn uses_default_api_key_lookup(&self) -> bool {
        self.inner.uses_default_api_key_lookup()
    }

    fn uses_default_oauth_token_lookup(&self) -> bool {
        self.inner.uses_default_oauth_token_lookup()
    }

    async fn resolve_api_key(
        &self,
        raw_api_key: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        let machine = self
            .authenticator
            .authenticate_machine_credential(raw_api_key)
            .await
            .map_err(|_| {
                WebFrameworkError::invalid_credentials("invalid or expired machine credential")
            })?;
        if let Some(machine) = machine {
            return Ok(WebRequestPrincipal::builder()
                .tenant_id(machine.tenant_id.to_string())
                .login_scope(WebLoginScope::Tenant)
                .user_id(machine.subject_id)
                .app_id(machine.app_id)
                .environment(WebEnvironment::Prod)
                .deployment_mode(WebDeploymentMode::Saas)
                .auth_level(WebAuthLevel::ApiKey)
                .data_scope(vec![])
                .permission_scope(machine.permission_scope)
                .subject_type(WebSubjectType::Service)
                .build());
        }
        if self.machine_only || raw_api_key.starts_with(MACHINE_CREDENTIAL_PREFIX) {
            // Machine-prefixed credentials must pass the machine authenticator;
            // falling back to the user API-key resolver would let user keys pose
            // as node credentials on agent/internal surfaces.
            return Err(WebFrameworkError::invalid_credentials(
                "machine credential required",
            ));
        }
        self.inner.resolve_api_key(raw_api_key).await
    }

    async fn resolve_dual_token(
        &self,
        raw_auth_token: &str,
        raw_access_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        self.inner
            .resolve_dual_token(raw_auth_token, raw_access_token)
            .await
    }

    async fn resolve_access_token(
        &self,
        raw_access_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        self.inner.resolve_access_token(raw_access_token).await
    }

    async fn resolve_oauth_bearer(
        &self,
        raw_bearer_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        self.inner.resolve_oauth_bearer(raw_bearer_token).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_webserver_contract::{
        AuthenticatedMachineCredential, WebServiceError, WebServiceResult,
    };

    #[derive(Clone, Default)]
    struct InnerResolver;

    #[async_trait]
    impl WebRequestContextResolver for InnerResolver {
        fn resolver_production_profile(&self) -> ResolverProductionProfile {
            ResolverProductionProfile::TenantBoundSaaS
        }

        fn uses_default_api_key_lookup(&self) -> bool {
            true
        }

        async fn resolve_api_key(
            &self,
            _raw_api_key: &str,
        ) -> Result<WebRequestPrincipal, WebFrameworkError> {
            Ok(principal("inner-subject", "inner-app"))
        }

        async fn resolve_dual_token(
            &self,
            _raw_auth_token: &str,
            _raw_access_token: &str,
        ) -> Result<WebRequestPrincipal, WebFrameworkError> {
            Ok(principal("inner-subject", "inner-app"))
        }

        async fn resolve_access_token(
            &self,
            _raw_access_token: &str,
        ) -> Result<WebRequestPrincipal, WebFrameworkError> {
            Ok(principal("inner-subject", "inner-app"))
        }
    }

    struct TestMachineAuthenticator;

    #[async_trait]
    impl MachineCredentialAuthenticator for TestMachineAuthenticator {
        async fn authenticate_machine_credential(
            &self,
            credential: &str,
        ) -> WebServiceResult<Option<AuthenticatedMachineCredential>> {
            match credential {
                "machine-valid" => Ok(Some(AuthenticatedMachineCredential {
                    tenant_id: 42,
                    subject_id: "node-42".to_owned(),
                    app_id: "sdkwork-webserver-agent".to_owned(),
                    permission_scope: vec!["web.agent.*".to_owned()],
                })),
                "machine-invalid" => Err(WebServiceError::Forbidden),
                _ => Ok(None),
            }
        }
    }

    #[tokio::test]
    async fn machine_credentials_are_mapped_and_other_api_keys_are_delegated() {
        let resolver = MachineCredentialResolverDecorator::new(
            InnerResolver,
            Arc::new(TestMachineAuthenticator),
        );
        let machine = resolver.resolve_api_key("machine-valid").await.unwrap();
        assert_eq!(machine.tenant_id(), "42");
        assert_eq!(machine.user_id(), "node-42");
        assert_eq!(machine.app_id(), "sdkwork-webserver-agent");
        assert_eq!(machine.scopes.permission_scope, vec!["web.agent.*"]);

        let delegated = resolver.resolve_api_key("iam-key").await.unwrap();
        assert_eq!(delegated.user_id(), "inner-subject");
        assert_eq!(delegated.app_id(), "inner-app");
        assert_eq!(
            resolver.resolver_production_profile(),
            ResolverProductionProfile::TenantBoundSaaS
        );
        assert!(resolver.uses_default_api_key_lookup());
        assert!(resolver.resolve_api_key("machine-invalid").await.is_err());
    }

    #[test]
    fn standalone_iam_resolver_reports_verified_bootstrap_profile() {
        let resolver = MachineCredentialResolverDecorator::new_standalone_iam(
            IamWebRequestContextResolver::from_database_pool(None),
            Arc::new(TestMachineAuthenticator),
        );

        assert_eq!(
            resolver.resolver_production_profile(),
            ResolverProductionProfile::TenantBoundBootstrap
        );
    }

    fn principal(subject_id: &str, app_id: &str) -> WebRequestPrincipal {
        WebRequestPrincipal::builder()
            .tenant_id("1")
            .login_scope(WebLoginScope::Tenant)
            .user_id(subject_id)
            .app_id(app_id)
            .environment(WebEnvironment::Prod)
            .deployment_mode(WebDeploymentMode::Saas)
            .auth_level(WebAuthLevel::ApiKey)
            .data_scope(vec![])
            .permission_scope(vec![])
            .subject_type(WebSubjectType::Service)
            .build()
    }
}
