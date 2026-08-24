use std::sync::Arc;

use axum::Router;
use sdkwork_routes_webserver_common::{
    web_auth_mode_from_env, web_framework_runtime_policy_from_env, with_problem_correlation,
    MachineCredentialResolverDecorator, ProductionFailClosedResolver, WebAuthMode,
    WebServerTenantIsolationPolicy,
};
use sdkwork_web_axum::{with_web_request_context, WebFrameworkLayer};
use sdkwork_web_bootstrap::WebFrameworkBuilder;
use sdkwork_web_core::{
    DefaultWebRequestContextResolver, DomainContextInjector, HttpMetricsRegistry,
    ManifestAuthorizationPolicy, WebEnvironment, WebFrameworkOptionalFeatures, WebRequestContext,
    WebRequestContextProfile, WebRequestContextResolver,
};
use sdkwork_webserver_contract::{MachineCredentialAuthenticator, WebInternalRequestContext};

use crate::http_route_manifest::internal_route_manifest;

const WEB_AGENT_APP_ID: &str = "sdkwork-web-agent";
const RUNTIME_ASSIGNMENT_WRITE_PERMISSION: &str = "web.runtimeAssignments.write";

#[derive(Clone, Default)]
struct WebInternalContextInjector;

impl DomainContextInjector for WebInternalContextInjector {
    fn inject(&self, request: &mut axum::extract::Request, context: &WebRequestContext) {
        if let Some(internal_context) = web_internal_context_from_web_request(context) {
            request.extensions_mut().insert(internal_context);
        }
    }
}

pub fn domain_context_injectors() -> Vec<Arc<dyn DomainContextInjector>> {
    vec![Arc::new(WebInternalContextInjector)]
}

fn web_internal_context_from_web_request(
    context: &WebRequestContext,
) -> Option<WebInternalRequestContext> {
    let principal = context.principal.as_ref()?;
    let tenant_id = principal.tenant_id().parse().ok()?;
    let is_web_agent = principal.app_id() == WEB_AGENT_APP_ID;
    Some(WebInternalRequestContext {
        tenant_id,
        subject_id: principal.user_id().to_owned(),
        agent_node_uuid: is_web_agent.then(|| principal.user_id().to_owned()),
        can_publish_cross_tenant: context.has_permission(RUNTIME_ASSIGNMENT_WRITE_PERMISSION),
    })
}

fn build_web_internal_api_framework_layer<R>(
    resolver: R,
    metrics: Option<Arc<HttpMetricsRegistry>>,
    audit_emitter: Arc<dyn sdkwork_web_core::AuditEmitter>,
    security_event_emitter: Arc<dyn sdkwork_web_core::SecurityEventEmitter>,
) -> WebFrameworkLayer<R>
where
    R: WebRequestContextResolver + Clone,
{
    let (environment, security_policy) = web_framework_runtime_policy_from_env();
    // Control-plane surface (machine-to-machine) assembles with the
    // standalone control-plane production features in production so the
    // framework production validation accepts the tenant-bound machine
    // resolver.
    let production_control_plane = matches!(environment, WebEnvironment::Prod);
    let route_manifest = internal_route_manifest();
    let mut builder = WebFrameworkBuilder::new(resolver)
        .profile(WebRequestContextProfile {
            environment,
            ..WebRequestContextProfile::default()
        })
        .security_policy(security_policy)
        .route_manifest(route_manifest.clone())
        .authorization_policy(Arc::new(ManifestAuthorizationPolicy::new(route_manifest)))
        .tenant_isolation_policy(Arc::new(WebServerTenantIsolationPolicy))
        .domain_injector(Arc::new(WebInternalContextInjector))
        .audit_emitter(audit_emitter)
        .security_event_emitter(security_event_emitter);
    if production_control_plane {
        builder = builder.optional_features(
            WebFrameworkOptionalFeatures::production_sqlx().control_plane_standalone(),
        );
    }
    if let Some(metrics) = metrics {
        builder = builder.metrics_registry(metrics);
    }
    builder.build().into_layer()
}

pub async fn wrap_router_with_web_framework_from_env(
    router: Router,
    machine_authenticator: Arc<dyn MachineCredentialAuthenticator>,
    audit_emitter: Arc<dyn sdkwork_web_core::AuditEmitter>,
    security_event_emitter: Arc<dyn sdkwork_web_core::SecurityEventEmitter>,
) -> Router {
    wrap_router_with_web_framework_from_env_and_optional_metrics(
        router,
        machine_authenticator,
        None,
        audit_emitter,
        security_event_emitter,
    )
    .await
}

pub async fn wrap_router_with_web_framework_from_env_and_metrics(
    router: Router,
    machine_authenticator: Arc<dyn MachineCredentialAuthenticator>,
    metrics: Arc<HttpMetricsRegistry>,
    audit_emitter: Arc<dyn sdkwork_web_core::AuditEmitter>,
    security_event_emitter: Arc<dyn sdkwork_web_core::SecurityEventEmitter>,
) -> Router {
    wrap_router_with_web_framework_from_env_and_optional_metrics(
        router,
        machine_authenticator,
        Some(metrics),
        audit_emitter,
        security_event_emitter,
    )
    .await
}

async fn wrap_router_with_web_framework_from_env_and_optional_metrics(
    router: Router,
    machine_authenticator: Arc<dyn MachineCredentialAuthenticator>,
    metrics: Option<Arc<HttpMetricsRegistry>>,
    audit_emitter: Arc<dyn sdkwork_web_core::AuditEmitter>,
    security_event_emitter: Arc<dyn sdkwork_web_core::SecurityEventEmitter>,
) -> Router {
    let correlated = with_problem_correlation(router);
    match web_auth_mode_from_env().await {
        WebAuthMode::DevInline => with_web_request_context(
            correlated,
            build_web_internal_api_framework_layer(
                MachineCredentialResolverDecorator::new_machine_only(
                    DefaultWebRequestContextResolver::default(),
                    machine_authenticator,
                ),
                metrics,
                audit_emitter.clone(),
                security_event_emitter.clone(),
            ),
        ),
        WebAuthMode::ProductionFailClosed => with_web_request_context(
            correlated,
            build_web_internal_api_framework_layer(
                MachineCredentialResolverDecorator::new_machine_only(
                    ProductionFailClosedResolver,
                    machine_authenticator,
                ),
                metrics,
                audit_emitter.clone(),
                security_event_emitter.clone(),
            ),
        ),
        WebAuthMode::IamDatabase(resolver) => with_web_request_context(
            correlated,
            build_web_internal_api_framework_layer(
                MachineCredentialResolverDecorator::new_machine_only(
                    resolver,
                    machine_authenticator,
                ),
                metrics,
                audit_emitter,
                security_event_emitter,
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::build_web_internal_api_framework_layer;
    use crate::web_bootstrap::MachineCredentialResolverDecorator;
    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use sdkwork_web_axum::with_web_request_context;
    use sdkwork_web_core::{
        access_token_jwt, NoOpAuditEmitter, NoOpSecurityEventEmitter, WebAuthLevel,
        WebDeploymentMode, WebEnvironment, WebFrameworkError, WebLoginScope,
        WebRequestContextResolver, WebRequestPrincipal, WebSubjectType,
    };
    use sdkwork_webserver_contract::{
        AuthenticatedMachineCredential, MachineCredentialAuthenticator, WebServiceResult,
    };
    use tower::ServiceExt;

    /// Machine authenticator that never validates credentials; used to exercise
    /// the framework permission/tenant logic via user-style resolution.
    #[derive(Clone)]
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

    #[derive(Clone)]
    struct TestResolver {
        tenant_id: &'static str,
        permissions: Vec<String>,
    }

    #[async_trait]
    impl WebRequestContextResolver for TestResolver {
        async fn resolve_api_key(
            &self,
            _raw_api_key: &str,
        ) -> Result<WebRequestPrincipal, WebFrameworkError> {
            Ok(self.principal())
        }

        async fn resolve_dual_token(
            &self,
            _raw_auth_token: &str,
            _raw_access_token: &str,
        ) -> Result<WebRequestPrincipal, WebFrameworkError> {
            Ok(self.principal())
        }

        async fn resolve_access_token(
            &self,
            _raw_access_token: &str,
        ) -> Result<WebRequestPrincipal, WebFrameworkError> {
            Ok(self.principal())
        }
    }

    impl TestResolver {
        fn principal(&self) -> WebRequestPrincipal {
            WebRequestPrincipal::builder()
                .tenant_id(self.tenant_id)
                .login_scope(WebLoginScope::Tenant)
                .user_id("node-1")
                .app_id("sdkwork-web-agent")
                .environment(WebEnvironment::Dev)
                .deployment_mode(WebDeploymentMode::Local)
                .auth_level(WebAuthLevel::ApiKey)
                .permission_scope(self.permissions.clone())
                .subject_type(WebSubjectType::Service)
                .build()
        }
    }

    #[tokio::test]
    async fn internal_framework_enforces_permission_and_tenant_scope() {
        assert_eq!(
            call_current_assignment(TestResolver {
                tenant_id: "42",
                permissions: Vec::new(),
            })
            .await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call_current_assignment(TestResolver {
                tenant_id: "",
                permissions: vec!["web.agent.read".to_owned()],
            })
            .await,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            call_current_assignment(TestResolver {
                tenant_id: "42",
                permissions: vec!["web.agent.read".to_owned()],
            })
            .await,
            StatusCode::OK
        );
    }

    async fn call_current_assignment(resolver: TestResolver) -> StatusCode {
        let app = with_web_request_context(
            Router::new().route(
                "/internal/v3/api/web/runtime_assignments/current",
                get(|| async { StatusCode::OK }),
            ),
            build_web_internal_api_framework_layer(
                resolver,
                None,
                std::sync::Arc::new(NoOpAuditEmitter),
                std::sync::Arc::new(NoOpSecurityEventEmitter),
            ),
        );
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/internal/v3/api/web/runtime_assignments/current")
                    .header("x-sdkwork-ingress-token", "ingress-test")
                    .header(
                        "access-token",
                        access_token_jwt("42", "7", "session-1", "web"),
                    )
                    .body(Body::empty())
                    .expect("valid internal request"),
            )
            .await
            .expect("internal framework response");
        response.status()
    }

    #[tokio::test]
    async fn machine_only_internal_surface_rejects_unvalidated_credentials() {
        let app = with_web_request_context(
            Router::new().route(
                "/internal/v3/api/web/runtime_assignments/current",
                get(|| async { StatusCode::OK }),
            ),
            build_web_internal_api_framework_layer(
                MachineCredentialResolverDecorator::new_machine_only(
                    TestResolver {
                        tenant_id: "42",
                        permissions: vec!["web.agent.read".to_owned()],
                    },
                    std::sync::Arc::new(NoopMachineAuthenticator),
                ),
                None,
                std::sync::Arc::new(NoOpAuditEmitter),
                std::sync::Arc::new(NoOpSecurityEventEmitter),
            ),
        );
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/internal/v3/api/web/runtime_assignments/current")
                    .header("x-sdkwork-ingress-token", "ingress-test")
                    .header(
                        "access-token",
                        access_token_jwt("42", "7", "session-1", "web"),
                    )
                    .body(Body::empty())
                    .expect("valid internal request"),
            )
            .await
            .expect("internal framework response");
        // The credential is not a validated machine credential, so the
        // machine-only surface must reject it instead of falling back to
        // user API-key resolution.
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
