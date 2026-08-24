use std::sync::Arc;

use axum::{Extension, Router};
use sdkwork_iam_web_adapter::IamAuthorizationPolicy;
use sdkwork_intelligence_webserver_service::WebService;
use sdkwork_routes_webserver_common::{
    web_auth_mode_from_env, web_framework_runtime_policy_from_env, with_problem_correlation,
    MachineCredentialResolverDecorator, ProductionFailClosedResolver, WebAuthMode,
    WebServerTenantIsolationPolicy,
};
use sdkwork_web_axum::{with_web_request_context, WebFrameworkLayer};
use sdkwork_web_bootstrap::WebFrameworkBuilder;
use sdkwork_web_core::{
    DefaultWebRequestContextResolver, DomainContextInjector, HttpMetricsRegistry, WebEnvironment,
    WebFrameworkOptionalFeatures, WebRequestContext, WebRequestContextProfile,
    WebRequestContextResolver,
};
use sdkwork_webserver_contract::{MachineCredentialAuthenticator, WebBackendRequestContext};

use crate::http_route_manifest::backend_route_manifest;
use crate::paths;

#[derive(Clone, Default)]
struct WebBackendContextInjector;

impl DomainContextInjector for WebBackendContextInjector {
    fn inject(&self, request: &mut axum::extract::Request, context: &WebRequestContext) {
        if let Some(backend_context) = web_backend_context_from_web_request(context) {
            request.extensions_mut().insert(backend_context);
        }
    }
}

pub fn domain_context_injectors() -> Vec<Arc<dyn DomainContextInjector>> {
    vec![Arc::new(WebBackendContextInjector)]
}

fn web_backend_context_from_web_request(
    context: &WebRequestContext,
) -> Option<WebBackendRequestContext> {
    let principal = context.principal.as_ref()?;
    let tenant_id: i64 = principal.tenant_id().parse().ok()?;
    // Machine principals use an opaque Web Node UUID, so an operator id is optional.
    let operator_id = principal.user_id().parse().ok();
    let subject_id = Some(principal.user_id().to_owned());
    Some(WebBackendRequestContext {
        operator_id,
        tenant_id: Some(tenant_id),
        subject_id,
        idempotency_key: context.idempotency_key().map(str::to_owned),
    })
}

fn build_web_backend_api_framework_layer<R>(
    resolver: R,
    metrics: Option<Arc<HttpMetricsRegistry>>,
    audit_emitter: Arc<dyn sdkwork_web_core::AuditEmitter>,
    security_event_emitter: Arc<dyn sdkwork_web_core::SecurityEventEmitter>,
) -> WebFrameworkLayer<R>
where
    R: WebRequestContextResolver + Clone,
{
    let (environment, security_policy) = web_framework_runtime_policy_from_env();
    // Control-plane surfaces (agent/backend/internal) assemble with the
    // standalone control-plane production features in production so the
    // framework production validation accepts the tenant-bound machine
    // resolver and the SQLx-backed stores.
    let production_control_plane = matches!(environment, WebEnvironment::Prod);
    let route_manifest = backend_route_manifest();
    let mut builder = WebFrameworkBuilder::new(resolver)
        .profile(WebRequestContextProfile {
            backend_api_prefix: paths::PREFIX.to_owned(),
            environment,
            ..WebRequestContextProfile::default()
        })
        .security_policy(security_policy)
        .route_manifest(route_manifest.clone())
        .authorization_policy(Arc::new(IamAuthorizationPolicy::new(route_manifest)))
        .tenant_isolation_policy(Arc::new(WebServerTenantIsolationPolicy))
        .domain_injector(Arc::new(WebBackendContextInjector))
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
    service: Arc<WebService>,
    audit_emitter: Arc<dyn sdkwork_web_core::AuditEmitter>,
    security_event_emitter: Arc<dyn sdkwork_web_core::SecurityEventEmitter>,
) -> Router {
    wrap_router_with_web_framework_from_env_and_optional_metrics(
        router,
        service,
        None,
        audit_emitter,
        security_event_emitter,
    )
    .await
}

pub async fn wrap_router_with_web_framework_from_env_and_metrics(
    router: Router,
    service: Arc<WebService>,
    metrics: Arc<HttpMetricsRegistry>,
    audit_emitter: Arc<dyn sdkwork_web_core::AuditEmitter>,
    security_event_emitter: Arc<dyn sdkwork_web_core::SecurityEventEmitter>,
) -> Router {
    wrap_router_with_web_framework_from_env_and_optional_metrics(
        router,
        service,
        Some(metrics),
        audit_emitter,
        security_event_emitter,
    )
    .await
}

/// Wraps the Web Node agent router in a machine-only framework layer.
///
/// Agent routes authenticate `X-SDKWork-Agent-Token` through the shared
/// api-key path, so the resolver must never fall back to IAM user API keys;
/// only `wagent_`-prefixed machine credentials are accepted on this surface.
pub async fn wrap_agent_router_with_web_framework_from_env(
    router: Router,
    service: Arc<WebService>,
    audit_emitter: Arc<dyn sdkwork_web_core::AuditEmitter>,
    security_event_emitter: Arc<dyn sdkwork_web_core::SecurityEventEmitter>,
) -> Router {
    let machine_authenticator: Arc<dyn MachineCredentialAuthenticator> = service.clone();
    let correlated = with_problem_correlation(router).layer(Extension(service));
    match web_auth_mode_from_env().await {
        WebAuthMode::DevInline => with_web_request_context(
            correlated,
            build_web_backend_api_framework_layer(
                MachineCredentialResolverDecorator::new_machine_only(
                    DefaultWebRequestContextResolver::default(),
                    machine_authenticator,
                ),
                None,
                audit_emitter,
                security_event_emitter,
            ),
        ),
        WebAuthMode::ProductionFailClosed => with_web_request_context(
            correlated,
            build_web_backend_api_framework_layer(
                MachineCredentialResolverDecorator::new_machine_only(
                    ProductionFailClosedResolver,
                    machine_authenticator,
                ),
                None,
                audit_emitter,
                security_event_emitter,
            ),
        ),
        WebAuthMode::IamDatabase(resolver) => with_web_request_context(
            correlated,
            build_web_backend_api_framework_layer(
                MachineCredentialResolverDecorator::new_machine_only(
                    resolver,
                    machine_authenticator,
                ),
                None,
                audit_emitter,
                security_event_emitter,
            ),
        ),
    }
}

async fn wrap_router_with_web_framework_from_env_and_optional_metrics(
    router: Router,
    service: Arc<WebService>,
    metrics: Option<Arc<HttpMetricsRegistry>>,
    audit_emitter: Arc<dyn sdkwork_web_core::AuditEmitter>,
    security_event_emitter: Arc<dyn sdkwork_web_core::SecurityEventEmitter>,
) -> Router {
    // Clone service for the resolver decorator before moving the original into Extension.
    let service_for_resolver = service.clone();
    let machine_authenticator: Arc<dyn MachineCredentialAuthenticator> = service_for_resolver;
    // Extension(service) is applied inside the framework layer so machine routes
    // can extract Arc<WebService> alongside the framework-injected WebBackendRequestContext.
    let correlated = with_problem_correlation(router).layer(Extension(service));
    match web_auth_mode_from_env().await {
        WebAuthMode::DevInline => with_web_request_context(
            correlated,
            build_web_backend_api_framework_layer(
                MachineCredentialResolverDecorator::new(
                    DefaultWebRequestContextResolver::default(),
                    machine_authenticator.clone(),
                ),
                metrics,
                audit_emitter.clone(),
                security_event_emitter.clone(),
            ),
        ),
        WebAuthMode::ProductionFailClosed => with_web_request_context(
            correlated,
            build_web_backend_api_framework_layer(
                MachineCredentialResolverDecorator::new(
                    ProductionFailClosedResolver,
                    machine_authenticator.clone(),
                ),
                metrics,
                audit_emitter.clone(),
                security_event_emitter.clone(),
            ),
        ),
        WebAuthMode::IamDatabase(resolver) => with_web_request_context(
            correlated,
            build_web_backend_api_framework_layer(
                MachineCredentialResolverDecorator::new(resolver, machine_authenticator),
                metrics,
                audit_emitter,
                security_event_emitter,
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::build_web_backend_api_framework_layer;
    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use sdkwork_web_axum::with_web_request_context;
    use sdkwork_web_core::{
        access_token_jwt, auth_token_jwt, NoOpAuditEmitter, NoOpSecurityEventEmitter, WebAuthLevel,
        WebDeploymentMode, WebEnvironment, WebFrameworkError, WebLoginScope,
        WebRequestContextResolver, WebRequestPrincipal, WebSubjectType,
    };
    use tower::ServiceExt;

    #[derive(Clone)]
    struct TestResolver {
        tenant_id: &'static str,
        login_scope: WebLoginScope,
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
            let organization_id =
                (self.login_scope == WebLoginScope::Organization).then(|| "9".to_owned());
            WebRequestPrincipal::builder()
                .tenant_id(self.tenant_id)
                .organization_id(organization_id)
                .login_scope(self.login_scope.clone())
                .user_id("7")
                .session_id(Some("session-1".to_owned()))
                .app_id("web")
                .environment(WebEnvironment::Dev)
                .deployment_mode(WebDeploymentMode::Local)
                .auth_level(WebAuthLevel::Password)
                .permission_scope(self.permissions.clone())
                .subject_type(WebSubjectType::User)
                .build()
        }
    }

    #[tokio::test]
    async fn backend_framework_enforces_permission_and_tenant_scope() {
        assert_eq!(
            call_applications(TestResolver {
                tenant_id: "42",
                login_scope: WebLoginScope::Organization,
                permissions: Vec::new(),
            })
            .await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call_applications(TestResolver {
                tenant_id: "",
                login_scope: WebLoginScope::Organization,
                permissions: vec!["web.sites.read".to_owned()],
            })
            .await,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            call_applications(TestResolver {
                tenant_id: "42",
                login_scope: WebLoginScope::Tenant,
                permissions: Vec::new(),
            })
            .await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call_applications(TestResolver {
                tenant_id: "42",
                login_scope: WebLoginScope::Organization,
                permissions: vec!["web.sites.read".to_owned()],
            })
            .await,
            StatusCode::OK
        );
    }

    async fn call_applications(resolver: TestResolver) -> StatusCode {
        let app = with_web_request_context(
            Router::new().route(
                "/backend/v3/api/applications",
                get(|| async { StatusCode::OK }),
            ),
            build_web_backend_api_framework_layer(
                resolver,
                None,
                std::sync::Arc::new(NoOpAuditEmitter),
                std::sync::Arc::new(NoOpSecurityEventEmitter),
            ),
        );
        app.oneshot(
            Request::builder()
                .uri("/backend/v3/api/applications")
                .header(
                    header::AUTHORIZATION,
                    format!("Bearer {}", auth_token_jwt("42", "7", "session-1", "web")),
                )
                .header(
                    "access-token",
                    access_token_jwt("42", "7", "session-1", "web"),
                )
                .body(Body::empty())
                .expect("valid backend request"),
        )
        .await
        .expect("backend framework response")
        .status()
    }
}
