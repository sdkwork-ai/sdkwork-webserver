//! Route health probes (FRP `healthCheck` parity, PRD §17).
//!
//! The probe runs on the **agent**, against the agent's own local target: the
//! gateway cannot reach the service except through the tunnel, and probing
//! through the tunnel would consume the very budget the route serves with. The
//! agent reports the outcome over the control plane, and the gateway uses it
//! to steer a load-balancer group away from a replica that cannot serve.

use serde::{Deserialize, Serialize};

use crate::error::{Result, TunnelError, ValidationField};

/// Default probe cadence in seconds (FRP default).
pub const DEFAULT_HEALTH_INTERVAL_SECS: u64 = 10;
/// Default probe timeout in seconds.
pub const DEFAULT_HEALTH_TIMEOUT_SECS: u64 = 3;
/// Default consecutive failures before a route is considered down.
pub const DEFAULT_HEALTH_MAX_FAILED: u32 = 1;

/// How the agent probes its local target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HealthCheckKind {
    /// Open a TCP connection to the target.
    #[default]
    Tcp,
    /// Open a connection and require a 2xx/3xx response to `GET path`.
    Http,
}

impl HealthCheckKind {
    /// The configuration/wire label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Http => "http",
        }
    }

    /// Parses the configuration/wire label.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "tcp" => Some(Self::Tcp),
            "http" => Some(Self::Http),
            _ => None,
        }
    }
}

/// One route's health probe configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct HealthCheckConfig {
    /// Probe kind.
    pub kind: HealthCheckKind,
    /// Seconds between probes.
    pub interval_seconds: u64,
    /// Per-probe timeout in seconds.
    pub timeout_seconds: u64,
    /// Consecutive failures that mark the route down. Recovery is immediate:
    /// one success marks it up again, which is what an operator expects from
    /// a probe that is now passing.
    pub max_failed: u32,
    /// Request path for [`HealthCheckKind::Http`]; ignored for `tcp`.
    pub path: Option<String>,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            kind: HealthCheckKind::Tcp,
            interval_seconds: DEFAULT_HEALTH_INTERVAL_SECS,
            timeout_seconds: DEFAULT_HEALTH_TIMEOUT_SECS,
            max_failed: DEFAULT_HEALTH_MAX_FAILED,
            path: None,
        }
    }
}

impl HealthCheckConfig {
    /// Validates the probe, including the fields the kind does not use.
    ///
    /// A `path` on a TCP probe is refused rather than ignored: an operator who
    /// wrote a path believes an HTTP probe is running, and silently probing a
    /// socket instead would report "healthy" for a service that cannot answer
    /// HTTP at all.
    pub fn validate(&self) -> Result<()> {
        if self.interval_seconds == 0 {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: "health check intervalSeconds must be at least 1".to_owned(),
            });
        }
        if self.timeout_seconds == 0 {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: "health check timeoutSeconds must be at least 1".to_owned(),
            });
        }
        if self.timeout_seconds > self.interval_seconds {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: format!(
                    "health check timeoutSeconds {} must not exceed intervalSeconds {}",
                    self.timeout_seconds, self.interval_seconds
                ),
            });
        }
        if self.max_failed == 0 {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: "health check maxFailed must be at least 1".to_owned(),
            });
        }
        match (self.kind, self.path.as_deref()) {
            (HealthCheckKind::Http, Some(path)) => {
                if !path.starts_with('/') {
                    return Err(TunnelError::Validation {
                        field: ValidationField::Config,
                        reason: format!("health check path `{path}` must start with `/`"),
                    });
                }
            }
            (HealthCheckKind::Http, None) => {
                return Err(TunnelError::Validation {
                    field: ValidationField::Config,
                    reason: "an http health check requires a path".to_owned(),
                });
            }
            (HealthCheckKind::Tcp, Some(_)) => {
                return Err(TunnelError::Validation {
                    field: ValidationField::Config,
                    reason: "a tcp health check cannot carry a path".to_owned(),
                });
            }
            (HealthCheckKind::Tcp, None) => {}
        }
        Ok(())
    }

    /// The probe path, defaulted for an HTTP probe.
    pub fn request_path(&self) -> &str {
        self.path.as_deref().unwrap_or("/")
    }
}

/// Validates a load-balancer group name: the identity that makes several
/// routes replicas of one service.
pub fn validate_load_balancer_group(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(TunnelError::Validation {
            field: ValidationField::Config,
            reason: "load balancer group must not be empty".to_owned(),
        });
    }
    if trimmed.len() > 64 {
        return Err(TunnelError::Validation {
            field: ValidationField::Config,
            reason: "load balancer group must be at most 64 characters".to_owned(),
        });
    }
    if trimmed.contains(char::is_whitespace) {
        return Err(TunnelError::Validation {
            field: ValidationField::Config,
            reason: format!("load balancer group `{raw}` must not contain whitespace"),
        });
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_labels_round_trip_and_reject_unknown_values() {
        for kind in [HealthCheckKind::Tcp, HealthCheckKind::Http] {
            assert_eq!(HealthCheckKind::parse(kind.label()), Some(kind));
        }
        assert_eq!(HealthCheckKind::parse("ICMP"), None);
    }

    #[test]
    fn defaults_match_frp() {
        let config = HealthCheckConfig::default();
        assert_eq!(config.kind, HealthCheckKind::Tcp);
        assert_eq!(config.interval_seconds, 10);
        assert_eq!(config.timeout_seconds, 3);
        assert_eq!(config.max_failed, 1);
        config.validate().expect("defaults validate");
    }

    #[test]
    fn an_http_probe_requires_a_rooted_path_and_a_tcp_probe_refuses_one() {
        let http = HealthCheckConfig {
            kind: HealthCheckKind::Http,
            path: Some("/health".to_owned()),
            ..HealthCheckConfig::default()
        };
        http.validate().expect("a rooted path validates");
        assert_eq!(http.request_path(), "/health");

        let unrooted = HealthCheckConfig {
            path: Some("health".to_owned()),
            ..http.clone()
        };
        assert!(unrooted.validate().is_err());

        let missing = HealthCheckConfig {
            kind: HealthCheckKind::Http,
            path: None,
            ..HealthCheckConfig::default()
        };
        assert!(missing.validate().is_err());

        // A path on a TCP probe means the operator believes an HTTP probe is
        // running; refuse instead of silently checking the socket.
        let tcp_with_path = HealthCheckConfig {
            path: Some("/health".to_owned()),
            ..HealthCheckConfig::default()
        };
        assert!(tcp_with_path.validate().is_err());
    }

    #[test]
    fn probe_timing_is_bounded() {
        for broken in [
            HealthCheckConfig {
                interval_seconds: 0,
                ..HealthCheckConfig::default()
            },
            HealthCheckConfig {
                timeout_seconds: 0,
                ..HealthCheckConfig::default()
            },
            HealthCheckConfig {
                interval_seconds: 2,
                timeout_seconds: 5,
                ..HealthCheckConfig::default()
            },
            HealthCheckConfig {
                max_failed: 0,
                ..HealthCheckConfig::default()
            },
        ] {
            assert!(broken.validate().is_err(), "{broken:?} must not validate");
        }
    }

    #[test]
    fn group_names_are_trimmed_and_bounded() {
        assert_eq!(
            validate_load_balancer_group(" web ").expect("trims"),
            "web".to_owned()
        );
        for bad in ["", "   ", "two words", &"x".repeat(65)] {
            assert!(
                validate_load_balancer_group(bad).is_err(),
                "`{bad}` must not be a group name"
            );
        }
    }
}
