//! Web runtime environment helpers shared by the Web Server's routers, its
//! API server, and the node daemon.

use sdkwork_utils_rust::parse_bool;

static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[doc(hidden)]
pub fn env_test_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn web_environment_name() -> String {
    std::env::var("SDKWORK_WEBSERVER_ENVIRONMENT")
        .or_else(|_| std::env::var("SDKWORK_WEBSERVER_CONFIG_PROFILE"))
        .unwrap_or_else(|_| "development".to_string())
        .to_ascii_lowercase()
}

pub fn web_is_production_like_environment() -> bool {
    matches!(
        web_environment_name().as_str(),
        "production" | "prod" | "staging" | "stage" | "test"
    )
}

/// The TLS runtime state file the node's data plane reads.
///
/// Shared because two processes read one setting: the data plane locates
/// its snapshot with it, and the node daemon derives the served-certificate
/// report's path from that same file, so the two cannot disagree about where
/// the handoff lives. Names that only one process reads stay with that
/// process.
pub const TLS_RUNTIME_SNAPSHOT_FILE_ENV: &str = "SDKWORK_WEBSERVER_TLS_RUNTIME_SNAPSHOT_FILE";

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .and_then(|value| parse_bool(&value))
        .unwrap_or(false)
}

pub fn web_dev_auth_bypass_enabled() -> bool {
    env_truthy("SDKWORK_WEBSERVER_DEV_AUTH_BYPASS")
}

pub fn web_use_dev_inline_auth_resolver() -> bool {
    !web_is_production_like_environment() && web_dev_auth_bypass_enabled()
}

/// Overrides the platform operator tenant id (see [`web_platform_operator_tenant_id`]).
pub const PLATFORM_OPERATOR_TENANT_ID_ENV: &str = "SDKWORK_WEBSERVER_PLATFORM_OPERATOR_TENANT_ID";

/// The tenant that owns host-scoped administration surfaces (cluster plane,
/// server files explorer, Web Server configuration), per PRD-FR-030.
///
/// This is the **single** definition of "the platform operator tenant". Before
/// it existed the value was spelled out independently in three places that
/// could disagree: [`require_platform_operator`] compared against a literal
/// `0`, while the IAM bootstrap
/// (`sdkwork_iam_web_adapter::resolve_deployment_bootstrap_access_token`) and
/// the credential-entry bootstrap both defaulted to `100001`. No tenant is ever
/// `0` in a standalone deployment, so the guard could never return `Ok` and
/// every host-scoped surface answered `40301` unconditionally.
///
/// The environment key is named "tenant id" rather than "code" because the
/// comparison happens on the id the IAM session carries; `iam_tenant.code`
/// (`SDKWORK`) is the human-facing label for the same row.
///
/// [`require_platform_operator`]: ../sdkwork_routes_webserver_backend_api/auth/fn.require_platform_operator.html
pub fn web_platform_operator_tenant_id() -> String {
    configured_platform_operator_tenant_id()
        .unwrap_or_else(|| DEFAULT_PLATFORM_OPERATOR_TENANT_ID.to_owned())
}

/// Default platform operator tenant id, aligned with the IAM bootstrap tenant
/// default so the guard and the bootstrap cannot drift apart again.
pub const DEFAULT_PLATFORM_OPERATOR_TENANT_ID: &str = "100001";

fn configured_platform_operator_tenant_id() -> Option<String> {
    std::env::var(PLATFORM_OPERATOR_TENANT_ID_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Whether `tenant_id` is the platform operator tenant (PRD-FR-030).
///
/// Captured as a function so every host-scoped surface asks the same question
/// instead of re-spelling the literal.
pub fn web_is_platform_operator_tenant(tenant_id: Option<&str>) -> bool {
    let Some(tenant_id) = tenant_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    tenant_id == web_platform_operator_tenant_id()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_env(key: &str, value: Option<&str>, test: impl FnOnce()) {
        let previous = std::env::var(key).ok();
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
        test();
        match previous {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn production_never_uses_dev_inline_auth() {
        let _guard = env_test_lock();
        with_env("SDKWORK_WEBSERVER_ENVIRONMENT", Some("production"), || {
            with_env("SDKWORK_WEBSERVER_DEV_AUTH_BYPASS", Some("true"), || {
                assert!(!web_use_dev_inline_auth_resolver());
            });
        });
    }

    /// PRD-FR-030 guard: the platform operator tenant defaults to the IAM
    /// bootstrap tenant, so the guard and the bootstrap agree without extra
    /// configuration. A regression to the old literal `0` fails here.
    #[test]
    fn platform_operator_tenant_defaults_to_iam_bootstrap_tenant() {
        let _guard = env_test_lock();
        with_env(PLATFORM_OPERATOR_TENANT_ID_ENV, None, || {
            assert_eq!(web_platform_operator_tenant_id(), "100001");
            assert_eq!(
                web_platform_operator_tenant_id(),
                DEFAULT_PLATFORM_OPERATOR_TENANT_ID
            );
            assert!(web_is_platform_operator_tenant(Some("100001")));
            // The historical literal must not slip back in.
            assert!(!web_is_platform_operator_tenant(Some("0")));
            assert!(!web_is_platform_operator_tenant(Some("42")));
            assert!(!web_is_platform_operator_tenant(None));
            assert!(!web_is_platform_operator_tenant(Some("")));
        });
    }

    /// A deployment that provisions its platform tenant under a different id
    /// must be able to say so without recompiling.
    #[test]
    fn platform_operator_tenant_honours_env_override() {
        let _guard = env_test_lock();
        with_env(PLATFORM_OPERATOR_TENANT_ID_ENV, Some("  900001  "), || {
            assert_eq!(web_platform_operator_tenant_id(), "900001");
            assert!(web_is_platform_operator_tenant(Some("900001")));
            assert!(!web_is_platform_operator_tenant(Some("100001")));
        });
        // A blank override is not an override.
        with_env(PLATFORM_OPERATOR_TENANT_ID_ENV, Some("   "), || {
            assert_eq!(
                web_platform_operator_tenant_id(),
                DEFAULT_PLATFORM_OPERATOR_TENANT_ID
            );
        });
    }
}
