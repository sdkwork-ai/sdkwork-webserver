//! Process-wide HTTP host adapters for SDKWork Web Server gateways.

mod correlation;
mod machine_credential;
mod tenant_isolation;

use async_trait::async_trait;
use axum::http::Uri;
use sdkwork_iam_web_adapter::IamWebRequestContextResolver;
use sdkwork_web_core::{
    SecurityPolicy, WebEnvironment, WebFrameworkError, WebRequestContextResolver,
    WebRequestPrincipal,
};
use sdkwork_webserver_contract::{
    web_is_production_like_environment, web_use_dev_inline_auth_resolver,
};

pub use correlation::{resolved_trace_id, with_problem_correlation, WebProblemCorrelation};
pub use machine_credential::MachineCredentialResolverDecorator;
pub use tenant_isolation::WebServerTenantIsolationPolicy;

const PRODUCTION_AUTH_UNAVAILABLE: &str = "production Web auth requires IAM PostgreSQL database";
const SHARED_ENVIRONMENT_KEY: &str = "SDKWORK_ENVIRONMENT";
const WEB_ENVIRONMENT_KEY: &str = "SDKWORK_WEBSERVER_ENVIRONMENT";
const SHARED_CORS_ALLOWED_ORIGINS_KEY: &str = "SDKWORK_CORS_ALLOWED_ORIGINS";

fn canonical_lifecycle_environment(value: &str) -> Result<&'static str, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "dev" | "development" | "local" => Ok("development"),
        "test" | "testing" => Ok("test"),
        "stage" | "staging" => Ok("staging"),
        "prod" | "production" | "live" => Ok("production"),
        _ => Err(format!(
            "unsupported SDKWork lifecycle environment: {value}"
        )),
    }
}

fn configured_environment(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn validate_environment_projection() {
    let Some(shared) = configured_environment(SHARED_ENVIRONMENT_KEY) else {
        return;
    };
    let Some(application) = configured_environment(WEB_ENVIRONMENT_KEY) else {
        return;
    };
    let shared = canonical_lifecycle_environment(&shared)
        .unwrap_or_else(|error| panic!("{SHARED_ENVIRONMENT_KEY} is invalid: {error}"));
    let application = canonical_lifecycle_environment(&application)
        .unwrap_or_else(|error| panic!("{WEB_ENVIRONMENT_KEY} is invalid: {error}"));
    assert_eq!(
        shared, application,
        "{SHARED_ENVIRONMENT_KEY} and {WEB_ENVIRONMENT_KEY} must select the same lifecycle environment"
    );
}

fn is_exact_http_origin(origin: &str) -> bool {
    let Ok(uri) = origin.parse::<Uri>() else {
        return false;
    };
    let Some(scheme) = uri.scheme_str() else {
        return false;
    };
    let Some(authority) = uri.authority() else {
        return false;
    };
    matches!(scheme, "http" | "https")
        && !authority.as_str().contains('@')
        && !origin.contains('*')
        && origin == format!("{scheme}://{authority}")
}

/// Desktop shells and embedded WebViews may use registered custom URL schemes
/// (for example `app://dsh`). Entries must be exact `scheme://authority` origins:
/// no wildcard, path, query, fragment, or userinfo.
fn is_exact_custom_runtime_origin(origin: &str) -> bool {
    let Ok(uri) = origin.parse::<Uri>() else {
        return false;
    };
    let Some(scheme) = uri.scheme_str() else {
        return false;
    };
    if !matches!(scheme, "app" | "tauri") {
        return false;
    }
    let Some(authority) = uri.authority() else {
        return false;
    };
    let authority = authority.as_str();
    if authority.is_empty() || authority.contains('@') || origin.contains('*') {
        return false;
    }
    if uri.query().is_some() {
        return false;
    }
    let path = uri.path();
    if !path.is_empty() && path != "/" {
        return false;
    }
    origin == format!("{scheme}://{authority}")
}

fn is_exact_allowed_origin(origin: &str) -> bool {
    is_exact_http_origin(origin) || is_exact_custom_runtime_origin(origin)
}

fn web_security_policy(
    environment: &WebEnvironment,
    configured_origins: Vec<String>,
) -> Result<SecurityPolicy, String> {
    if let Some(origin) = configured_origins
        .iter()
        .find(|origin| !is_exact_allowed_origin(origin))
    {
        return Err(format!(
            "{SHARED_CORS_ALLOWED_ORIGINS_KEY} contains an invalid exact origin: {origin}"
        ));
    }
    if matches!(environment, WebEnvironment::Prod) && configured_origins.is_empty() {
        return Err(format!(
            "production-like Web HTTP runtime requires {SHARED_CORS_ALLOWED_ORIGINS_KEY}"
        ));
    }

    // Production-like environments start from the production SecurityPolicy
    // (rate limiting, HSTS, strict CORS) so every mounted Web API surface
    // passes the framework production-assembly validation; operator-configured
    // origins are merged in below. Development/test environments keep the
    // framework defaults.
    let mut policy = if matches!(environment, WebEnvironment::Prod) {
        sdkwork_web_core::security::SecurityPolicy::production()
    } else {
        sdkwork_web_bootstrap::security_policy_for_environment(
            environment,
            configured_origins.clone(),
        )
    };
    for origin in configured_origins {
        if !policy.cors.allowed_origins.contains(&origin) {
            policy.cors.allowed_origins.push(origin);
        }
    }
    if matches!(environment, WebEnvironment::Prod) {
        policy
            .cors
            .validate_for_production()
            .map_err(|error| format!("invalid production-like Web CORS configuration: {error}"))?;
    }
    Ok(policy)
}

/// Resolves one framework environment and CORS policy for every mounted Web API surface.
pub fn web_framework_runtime_policy_from_env() -> (WebEnvironment, SecurityPolicy) {
    validate_environment_projection();
    let environment = sdkwork_web_bootstrap::web_environment_from_env(&[
        SHARED_ENVIRONMENT_KEY,
        WEB_ENVIRONMENT_KEY,
    ]);
    let configured_origins =
        sdkwork_web_bootstrap::cors_allowed_origins_from_env(&[SHARED_CORS_ALLOWED_ORIGINS_KEY]);
    let policy = web_security_policy(&environment, configured_origins)
        .unwrap_or_else(|error| panic!("Web Framework security configuration is invalid: {error}"));
    (environment, policy)
}

#[expect(
    clippy::large_enum_variant,
    reason = "public route-integration enum; boxing the resolver requires coordinated API review"
)]
pub enum WebAuthMode {
    DevInline,
    IamDatabase(IamWebRequestContextResolver),
    ProductionFailClosed,
}

pub async fn web_auth_mode_from_env() -> WebAuthMode {
    if web_use_dev_inline_auth_resolver() {
        return WebAuthMode::DevInline;
    }

    let iam_database_explicitly_configured = std::env::var("SDKWORK_DATABASE_URL")
        .or_else(|_| std::env::var("SDKWORK_DATABASE_ENGINE"))
        .is_ok();

    if web_is_production_like_environment() && !iam_database_explicitly_configured {
        return WebAuthMode::ProductionFailClosed;
    }

    WebAuthMode::IamDatabase(
        sdkwork_iam_web_adapter::iam_web_request_context_resolver_from_env().await,
    )
}

#[derive(Clone, Default)]
pub struct ProductionFailClosedResolver;

#[async_trait]
impl WebRequestContextResolver for ProductionFailClosedResolver {
    async fn resolve_api_key(
        &self,
        _raw_api_key: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        Err(WebFrameworkError::invalid_credentials(
            PRODUCTION_AUTH_UNAVAILABLE,
        ))
    }

    async fn resolve_dual_token(
        &self,
        _raw_auth_token: &str,
        _raw_access_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        Err(WebFrameworkError::invalid_credentials(
            PRODUCTION_AUTH_UNAVAILABLE,
        ))
    }

    async fn resolve_access_token(
        &self,
        _raw_access_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        Err(WebFrameworkError::invalid_credentials(
            PRODUCTION_AUTH_UNAVAILABLE,
        ))
    }

    async fn resolve_oauth_bearer(
        &self,
        _raw_bearer_token: &str,
    ) -> Result<WebRequestPrincipal, WebFrameworkError> {
        Err(WebFrameworkError::invalid_credentials(
            PRODUCTION_AUTH_UNAVAILABLE,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::web_security_policy;
    use sdkwork_web_core::WebEnvironment;

    #[test]
    fn development_policy_accepts_local_browser_preflight_origins() {
        let policy = web_security_policy(&WebEnvironment::Dev, Vec::new()).expect("dev policy");
        policy
            .cors
            .validate_origin_value("http://127.0.0.1:5182")
            .expect("PC development origin");
        policy
            .cors
            .validate_origin_value("https://evil.example.com")
            .expect_err("public origins remain denied in development");
    }

    #[test]
    fn production_policy_accepts_only_configured_exact_origins() {
        let policy = web_security_policy(
            &WebEnvironment::Prod,
            vec!["https://server.sdkwork.com".to_owned()],
        )
        .expect("production policy");
        policy
            .cors
            .validate_origin_value("https://server.sdkwork.com")
            .expect("configured production origin");
        policy
            .cors
            .validate_origin_value("https://evil.example.com")
            .expect_err("unconfigured production origin");
    }

    #[test]
    fn production_policy_fails_closed_without_an_exact_origin() {
        assert!(web_security_policy(&WebEnvironment::Prod, Vec::new())
            .expect_err("empty production allowlist")
            .contains("requires SDKWORK_CORS_ALLOWED_ORIGINS"));
        assert!(web_security_policy(
            &WebEnvironment::Prod,
            vec!["https://*.sdkwork.com".to_owned()],
        )
        .expect_err("wildcard production origin")
        .contains("invalid exact origin"));
        assert!(web_security_policy(
            &WebEnvironment::Prod,
            vec!["https://server.sdkwork.com/path".to_owned()],
        )
        .expect_err("origin with a path")
        .contains("invalid exact origin"));
    }

    #[test]
    fn development_policy_accepts_desktop_custom_scheme_origin() {
        let policy = web_security_policy(&WebEnvironment::Dev, vec!["app://dsh".to_owned()])
            .expect("dev desktop origin policy");
        policy
            .cors
            .validate_origin_value("app://dsh")
            .expect("configured desktop origin");
        policy
            .cors
            .validate_origin_value("app://other")
            .expect_err("unconfigured desktop origin");
    }

    #[test]
    fn production_policy_accepts_configured_desktop_custom_scheme_origin() {
        let policy = web_security_policy(
            &WebEnvironment::Prod,
            vec!["https://api.sdkwork.com".to_owned(), "app://dsh".to_owned()],
        )
        .expect("production desktop origin policy");
        policy
            .cors
            .validate_origin_value("app://dsh")
            .expect("configured desktop origin");
        policy
            .cors
            .validate_origin_value("javascript:alert(1)")
            .expect_err("unregistered custom scheme");
    }
}
