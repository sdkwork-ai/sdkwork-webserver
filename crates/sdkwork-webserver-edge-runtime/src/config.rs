use std::net::SocketAddr;
use std::path::PathBuf;

use crate::EdgeRuntimeResult;

#[derive(Clone, Debug)]
pub struct EdgeRuntimeConfig {
    pub nginx_enabled: bool,
    pub nginx_binary: String,
    pub nginx_main_config: PathBuf,
    pub nginx_sites_root: PathBuf,
    pub cert_live_root: PathBuf,
    pub site_family: String,
    pub nginx_command_timeout_ms: u64,
    pub tls_verify_address: SocketAddr,
    pub tls_verify_timeout_ms: u64,
}

impl EdgeRuntimeConfig {
    pub fn from_env() -> EdgeRuntimeResult<Self> {
        let nginx_enabled = match std::env::var("SDKWORK_WEBSERVER_NGINX_ENABLED") {
            Ok(value) => parse_enabled(&value)?,
            Err(std::env::VarError::NotPresent) => true,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(crate::EdgeRuntimeError::Config(
                    "SDKWORK_WEBSERVER_NGINX_ENABLED must be valid Unicode".to_string(),
                ));
            }
        };

        let nginx_binary =
            std::env::var("SDKWORK_WEBSERVER_NGINX_BINARY").unwrap_or_else(|_| "nginx".to_string());

        let nginx_main_config = PathBuf::from(
            std::env::var("SDKWORK_WEBSERVER_NGINX_MAIN_CONF")
                .unwrap_or_else(|_| "/etc/nginx/nginx.conf".to_string()),
        );

        // Site family is the path segment between `sites-enabled` and the
        // domain file (`/etc/nginx/sites-enabled/<family>/<domain>.conf` per
        // `NGINX_SPEC.md`). It must be a safe single path segment so it can
        // never escape the sites root.
        let site_family = std::env::var("SDKWORK_WEBSERVER_NGINX_SITE_FAMILY")
            .unwrap_or_else(|_| "sdkwork".to_string());
        validate_site_family(&site_family)?;

        let nginx_sites_root = PathBuf::from(
            std::env::var("SDKWORK_WEBSERVER_NGINX_SITES_ROOT")
                .unwrap_or_else(|_| format!("/etc/nginx/sites-enabled/{site_family}")),
        );

        let cert_live_root = PathBuf::from(
            std::env::var("SDKWORK_WEBSERVER_CERT_LIVE_ROOT")
                .unwrap_or_else(|_| "/etc/sdkwork/certs/letsencrypt".to_string()),
        );

        let nginx_command_timeout_ms = std::env::var("SDKWORK_WEBSERVER_NGINX_COMMAND_TIMEOUT_MS")
            .ok()
            .map(|value| {
                value.parse::<u64>().map_err(|error| {
                    crate::EdgeRuntimeError::Config(format!(
                        "invalid SDKWORK_WEBSERVER_NGINX_COMMAND_TIMEOUT_MS: {error}"
                    ))
                })
            })
            .transpose()?
            .unwrap_or(10_000);
        if !(100..=60_000).contains(&nginx_command_timeout_ms) {
            return Err(crate::EdgeRuntimeError::Config(
                "SDKWORK_WEBSERVER_NGINX_COMMAND_TIMEOUT_MS must be between 100 and 60000"
                    .to_string(),
            ));
        }
        let tls_verify_address = std::env::var("SDKWORK_WEBSERVER_TLS_VERIFY_ADDRESS")
            .unwrap_or_else(|_| "127.0.0.1:443".to_string())
            .parse::<SocketAddr>()
            .map_err(|error| {
                crate::EdgeRuntimeError::Config(format!(
                    "invalid SDKWORK_WEBSERVER_TLS_VERIFY_ADDRESS: {error}"
                ))
            })?;
        if !tls_verify_address.ip().is_loopback() {
            return Err(crate::EdgeRuntimeError::Config(
                "SDKWORK_WEBSERVER_TLS_VERIFY_ADDRESS must be a loopback socket address"
                    .to_string(),
            ));
        }
        let tls_verify_timeout_ms = std::env::var("SDKWORK_WEBSERVER_TLS_VERIFY_TIMEOUT_MS")
            .ok()
            .map(|value| {
                value.parse::<u64>().map_err(|error| {
                    crate::EdgeRuntimeError::Config(format!(
                        "invalid SDKWORK_WEBSERVER_TLS_VERIFY_TIMEOUT_MS: {error}"
                    ))
                })
            })
            .transpose()?
            .unwrap_or(5_000);
        if !(100..=30_000).contains(&tls_verify_timeout_ms) {
            return Err(crate::EdgeRuntimeError::Config(
                "SDKWORK_WEBSERVER_TLS_VERIFY_TIMEOUT_MS must be between 100 and 30000".to_string(),
            ));
        }

        Ok(Self {
            nginx_enabled,
            nginx_binary,
            nginx_main_config,
            nginx_sites_root,
            cert_live_root,
            site_family,
            nginx_command_timeout_ms,
            tls_verify_address,
            tls_verify_timeout_ms,
        })
    }
}

fn parse_enabled(value: &str) -> EdgeRuntimeResult<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(crate::EdgeRuntimeError::Config(
            "SDKWORK_WEBSERVER_NGINX_ENABLED must be true, false, 1, or 0".to_string(),
        )),
    }
}

/// The site family is a single filesystem path segment: lowercase ASCII
/// letters, digits, and hyphens only, never empty and never dot segments.
fn validate_site_family(family: &str) -> EdgeRuntimeResult<()> {
    if family.is_empty()
        || family == "."
        || family == ".."
        || !family
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(crate::EdgeRuntimeError::Config(format!(
            "SDKWORK_WEBSERVER_NGINX_SITE_FAMILY must be a safe lowercase path segment: {family:?}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_enabled, validate_site_family};
    use std::path::PathBuf;
    use std::sync::Mutex;

    static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

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
    fn nginx_enabled_tokens_are_strict() {
        assert!(parse_enabled("true").unwrap());
        assert!(parse_enabled("1").unwrap());
        assert!(!parse_enabled("false").unwrap());
        assert!(!parse_enabled("0").unwrap());
        assert!(parse_enabled("yes").is_err());
        assert!(parse_enabled("").is_err());
    }

    #[test]
    fn site_family_is_a_safe_lowercase_path_segment() {
        assert!(validate_site_family("sdkwork").is_ok());
        assert!(validate_site_family("web-edge-2").is_ok());
        for unsafe_family in [
            "", ".", "..", "../web", "Web", "web/edge", "web edge", "web\x00",
        ] {
            assert!(
                validate_site_family(unsafe_family).is_err(),
                "family {unsafe_family:?} must be rejected"
            );
        }
    }

    #[test]
    fn default_sites_root_uses_the_site_family_segment() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        with_env("SDKWORK_WEBSERVER_NGINX_SITES_ROOT", None, || {
            with_env("SDKWORK_WEBSERVER_NGINX_SITE_FAMILY", None, || {
                let config = super::EdgeRuntimeConfig::from_env().unwrap();
                assert_eq!(
                    config.nginx_sites_root,
                    PathBuf::from("/etc/nginx/sites-enabled/sdkwork")
                );
                assert_eq!(config.site_family, "sdkwork");
            });
            with_env(
                "SDKWORK_WEBSERVER_NGINX_SITE_FAMILY",
                Some("web-edge"),
                || {
                    let config = super::EdgeRuntimeConfig::from_env().unwrap();
                    assert_eq!(
                        config.nginx_sites_root,
                        PathBuf::from("/etc/nginx/sites-enabled/web-edge")
                    );
                },
            );
        });
    }

    #[test]
    fn explicit_sites_root_overrides_the_family_default() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        with_env(
            "SDKWORK_WEBSERVER_NGINX_SITE_FAMILY",
            Some("web-edge"),
            || {
                with_env(
                    "SDKWORK_WEBSERVER_NGINX_SITES_ROOT",
                    Some("/custom/sites"),
                    || {
                        let config = super::EdgeRuntimeConfig::from_env().unwrap();
                        assert_eq!(config.nginx_sites_root, PathBuf::from("/custom/sites"));
                    },
                );
            },
        );
    }

    #[test]
    fn unsafe_site_family_fails_closed() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        with_env(
            "SDKWORK_WEBSERVER_NGINX_SITE_FAMILY",
            Some("../web"),
            || {
                assert!(super::EdgeRuntimeConfig::from_env().is_err());
            },
        );
    }
}
