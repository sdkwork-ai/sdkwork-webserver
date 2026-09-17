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
}
