use async_trait::async_trait;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use sdkwork_iam_web_adapter::IamWebRequestContextResolver;
use sdkwork_routes_webserver_app_api::{
    build_router_with_shared_app_api, web_bootstrap::wrap_router_with_iam_database_web_framework,
    wrap_router_with_web_framework_and_metrics,
};
use sdkwork_web_bootstrap::{service_router, ServiceRouterConfig};
use sdkwork_web_core::{
    access_token_jwt, auth_token_jwt, auth_token_jwt_with_permissions,
    DefaultWebRequestContextResolver, HttpMetricsRegistry,
};
use sdkwork_webserver_contract::{
    ApplicationPage, ListApplicationsQuery, WebAppApi, WebAppRequestContext, WebServiceResult,
};
use std::sync::Arc;
use tower::util::ServiceExt;

#[tokio::test]
async fn app_router_web_framework_rejects_unauthenticated_requests() {
    let app = wrap_router_with_iam_database_web_framework(
        IamWebRequestContextResolver::new(None),
        build_router_with_shared_app_api(Arc::new(StubAppApi)),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/app/v3/api/applications")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn app_router_enforces_manifest_permissions_before_business_logic() {
    let app = wrap_router_with_web_framework_and_metrics(
        DefaultWebRequestContextResolver::default(),
        build_router_with_shared_app_api(Arc::new(StubAppApi)),
        HttpMetricsRegistry::new(),
    );

    let denied = app
        .clone()
        .oneshot(authorized_request(auth_token_jwt(
            "42",
            "7",
            "session-1",
            "web",
        )))
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    let allowed = app
        .oneshot(authorized_request(auth_token_jwt_with_permissions(
            "42",
            "7",
            "session-1",
            "web",
            "web.applications.read",
        )))
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);
}

#[tokio::test]
async fn app_router_rejects_non_canonical_or_out_of_range_pagination() {
    let app = build_router_with_shared_app_api(Arc::new(StubAppApi));
    for query in [
        "page=0",
        "page_size=201",
        "pageSize=20",
        "limit=20",
        "page=1&page=2",
        "page=1&cursor=opaque",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/app/v3/api/applications?{query}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect bounded problem response")
            .to_bytes();
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("40001"), "{query}: {body}");
    }
}

fn authorized_request(auth_token: String) -> Request<Body> {
    Request::builder()
        .uri("/app/v3/api/applications")
        .header(header::AUTHORIZATION, format!("Bearer {auth_token}"))
        .header(
            "access-token",
            access_token_jwt("42", "7", "session-1", "web"),
        )
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn app_router_handles_sdk_browser_preflight_through_the_framework_policy() {
    let app = wrap_router_with_web_framework_and_metrics(
        DefaultWebRequestContextResolver::default(),
        build_router_with_shared_app_api(Arc::new(StubAppApi)),
        HttpMetricsRegistry::new(),
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/app/v3/api/applications")
                .header(header::ORIGIN, "http://127.0.0.1:5182")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .header(
                    header::ACCESS_CONTROL_REQUEST_HEADERS,
                    "authorization,access-token,content-type,idempotency-key,x-content-sha256,x-idempotency-fingerprint",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("http://127.0.0.1:5182")
    );
    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .and_then(|value| value.to_str().ok()),
        Some("true")
    );
    assert!(response
        .headers()
        .get(header::VARY)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value
            .split(',')
            .any(|part| part.trim().eq_ignore_ascii_case("origin"))));

    let rejected = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/app/v3/api/applications")
                .header(header::ORIGIN, "https://evil.example.com")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::FORBIDDEN);
    assert!(rejected
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .is_none());
}

#[tokio::test]
async fn app_router_records_requests_into_the_injected_bounded_registry() {
    let metrics = HttpMetricsRegistry::new();
    let app = service_router(
        wrap_router_with_web_framework_and_metrics(
            DefaultWebRequestContextResolver::default(),
            build_router_with_shared_app_api(Arc::new(StubAppApi)),
            metrics.clone(),
        ),
        ServiceRouterConfig::default()
            .with_always_ready()
            .with_metrics(metrics.clone()),
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/app/v3/api/applications")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let metrics_response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(metrics_response.status(), StatusCode::OK);
    let rendered = String::from_utf8(
        metrics_response
            .into_body()
            .collect()
            .await
            .expect("collect bounded metrics response")
            .to_bytes()
            .to_vec(),
    )
    .expect("metrics are UTF-8");
    assert!(rendered.contains("sdkwork_http_requests_total{"));
    assert!(
        rendered.contains("sdkwork_http_requests_total{service=\"sdkwork-web-framework\"")
            || rendered.contains("sdkwork_http_requests_total{service=")
    );
    assert!(rendered.contains("route=\"/app/v3/api/applications\""));
    assert!(rendered.contains("operation_id=\"applications.list\""));
    assert!(rendered.contains("status=\"401\""));
}

struct StubAppApi;

#[async_trait]
impl WebAppApi for StubAppApi {
    async fn list_applications(
        &self,
        _context: &WebAppRequestContext,
        _query: &ListApplicationsQuery,
    ) -> WebServiceResult<ApplicationPage> {
        Ok(ApplicationPage::default())
    }

    async fn create_application(
        &self,
        _context: &WebAppRequestContext,
        _request: &sdkwork_webserver_contract::CreateApplicationRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::ApplicationResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn retrieve_application(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::ApplicationResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn update_application(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _request: &sdkwork_webserver_contract::UpdateApplicationRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::ApplicationResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn delete_application(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
    ) -> WebServiceResult<()> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn activate_application(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::ApplicationResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn pause_application(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::ApplicationResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn list_domains(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _page: i32,
        _page_size: i32,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainPage> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn list_certificate_domains(
        &self,
        _context: &WebAppRequestContext,
        _page: i32,
        _page_size: i32,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainPage> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn create_domain(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _request: &sdkwork_webserver_contract::CreateDomainRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn retrieve_domain(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _domain_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn delete_domain(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _domain_id: &str,
    ) -> WebServiceResult<()> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn verify_domain(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _domain_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::DomainVerifyResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn create_platform_target(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _request: &sdkwork_webserver_contract::CreatePlatformTargetRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::PlatformTargetResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn list_platform_targets(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _page: i32,
        _page_size: i32,
    ) -> WebServiceResult<sdkwork_webserver_contract::PlatformTargetPage> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn retrieve_platform_target(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _platform_target_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::PlatformTargetResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn list_deployments(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _page: i32,
        _page_size: i32,
        _status: Option<i32>,
        _cursor: Option<&str>,
    ) -> WebServiceResult<sdkwork_webserver_contract::DeploymentPage> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn create_deployment(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _request: &sdkwork_webserver_contract::CreateDeploymentRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::DeploymentResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn retrieve_deployment(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _deployment_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::DeploymentResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn rollback_deployment(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _deployment_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::DeploymentResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn list_env_variables(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _environment: Option<&str>,
    ) -> WebServiceResult<sdkwork_webserver_contract::EnvVariablePage> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn create_env_variable(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _request: &sdkwork_webserver_contract::CreateEnvVariableRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::EnvVariableResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn update_env_variable(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _variable_id: &str,
        _request: &sdkwork_webserver_contract::UpdateEnvVariableRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::EnvVariableResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn delete_env_variable(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _variable_id: &str,
    ) -> WebServiceResult<()> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn list_certificates(
        &self,
        _context: &WebAppRequestContext,
        _application_id: Option<&str>,
        _domain_id: Option<&str>,
        _page: i32,
        _page_size: i32,
    ) -> WebServiceResult<sdkwork_webserver_contract::CertificatePage> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn issue_certificate(
        &self,
        _context: &WebAppRequestContext,
        _request: &sdkwork_webserver_contract::IssueCertificateRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::CertificateOperationAcceptedResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn retrieve_certificate_operation(
        &self,
        _context: &WebAppRequestContext,
        _operation_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::CertificateOperationResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn list_listener_certificate_bindings(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _domain_id: &str,
        _page: i32,
        _page_size: i32,
    ) -> WebServiceResult<sdkwork_webserver_contract::ListenerCertificateBindingPage> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn bind_listener_certificate(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _domain_id: &str,
        _request: &sdkwork_webserver_contract::CreateListenerCertificateBindingRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::ListenerCertificateBindingResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn unbind_listener_certificate(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _domain_id: &str,
        _binding_id: &str,
    ) -> WebServiceResult<()> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn list_health_checks(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
    ) -> WebServiceResult<sdkwork_webserver_contract::HealthCheckPage> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }

    async fn create_health_check(
        &self,
        _context: &WebAppRequestContext,
        _application_id: &str,
        _request: &sdkwork_webserver_contract::CreateHealthCheckRequest,
    ) -> WebServiceResult<sdkwork_webserver_contract::HealthCheckResponse> {
        Err(sdkwork_webserver_contract::WebServiceError::Internal(
            "not implemented".into(),
        ))
    }
}
