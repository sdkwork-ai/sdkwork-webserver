//! Tunnel configuration (PRD §38, §39, §40, §51, §106).
//!
//! One serde model shared by three consumers: the webserver app config
//! (`[tunnel]` server.toml section), the standalone gateway process, and the
//! tunnel CLI. Secrets are never stored here: `token` fields hold
//! environment-variable *names* resolved at runtime (`SDKWORK_TUNNEL_TOKEN`
//! and friends), so real credentials never reach configuration files or git.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Result, TunnelError, ValidationField};
use crate::route::{RouteMatcher, TunnelProtocolKind, TunnelRoute, TunnelTarget};
use crate::{ids::RouteId, policy::RoutePolicy};

/// Default heartbeat cadence in seconds (PRD §28).
pub const DEFAULT_HEARTBEAT_INTERVAL_SECS: u64 = 10;
/// Default QUIC tunnel listener port.
pub const DEFAULT_TUNNEL_PORT: u16 = 8443;

/// The `[tunnel]` configuration section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct TunnelConfig {
    /// Master switch. `false` must leave the webserver byte-for-byte
    /// unchanged (PRD §77).
    pub enabled: bool,
    /// Gateway-side settings; unused by agent-only deployments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<TunnelGatewayConfig>,
    /// Agent-side settings; unused by gateway-only deployments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<TunnelAgentConfig>,
    /// Shared network behavior (heartbeat, reconnect).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<TunnelNetworkConfig>,
    /// Handshake and I/O timeouts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<TunnelTimeoutConfig>,
    /// Resource ceilings (PRD §106).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<TunnelLimitsConfig>,
    /// Agent-side route templates (PRD §39).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routes: Option<Vec<TunnelRouteTemplate>>,
}

impl TunnelConfig {
    /// Disabled-by-default configuration (PRD §77 backward compatibility).
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            gateway: None,
            agent: None,
            network: None,
            timeout: None,
            limits: None,
            routes: None,
        }
    }

    /// Gateway settings with defaults applied.
    pub fn gateway_or_default(&self) -> TunnelGatewayConfig {
        self.gateway.clone().unwrap_or_default()
    }

    /// Agent settings with defaults applied.
    pub fn agent_or_default(&self) -> TunnelAgentConfig {
        self.agent.clone().unwrap_or_default()
    }

    /// Network settings with defaults applied.
    pub fn network_or_default(&self) -> TunnelNetworkConfig {
        self.network.clone().unwrap_or_default()
    }

    /// Timeout settings with defaults applied.
    pub fn timeout_or_default(&self) -> TunnelTimeoutConfig {
        self.timeout.clone().unwrap_or_default()
    }

    /// Limit settings with defaults applied.
    pub fn limits_or_default(&self) -> TunnelLimitsConfig {
        self.limits.clone().unwrap_or_default()
    }

    /// Route templates, defaulting to an empty set.
    pub fn routes_or_empty(&self) -> &[TunnelRouteTemplate] {
        self.routes.as_deref().unwrap_or(&[])
    }

    /// Validates cross-field constraints beyond per-field serde checks.
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        for template in self.routes_or_empty() {
            template.validate()?;
        }
        Ok(())
    }
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self::disabled()
    }
}

/// Gateway (public edge) tunnel settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct TunnelGatewayConfig {
    /// QUIC listener bind, `host:port`.
    pub listen: String,
    /// Domain suffixes an agent-registered domain route must end with.
    /// Empty disables the suffix policy.
    pub domain_suffixes: Vec<String>,
    /// Names of environment variables holding accepted agent bearer tokens,
    /// evaluated at startup. The first variable that is set and non-empty
    /// contributes one token; tokens are never read from the config file.
    pub agent_token_env: Vec<String>,
    /// Environment variable holding the TLS certificate chain (PEM) for the
    /// QUIC listener. When unset, a self-signed certificate is minted for
    /// development and the agent must pin it or skip verification.
    pub tls_cert_pem_env: Option<String>,
    /// Environment variable holding the TLS private key (PEM).
    pub tls_key_pem_env: Option<String>,
}

impl Default for TunnelGatewayConfig {
    fn default() -> Self {
        Self {
            listen: format!("0.0.0.0:{DEFAULT_TUNNEL_PORT}"),
            domain_suffixes: Vec::new(),
            agent_token_env: vec!["SDKWORK_TUNNEL_GATEWAY_TOKEN".to_owned()],
            tls_cert_pem_env: None,
            tls_key_pem_env: None,
        }
    }
}

/// Agent (inside network) tunnel settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct TunnelAgentConfig {
    /// Gateway endpoint advertised to agents, `host:port` (QUIC/UDP).
    pub endpoint: String,
    /// Environment variable holding the agent bearer token
    /// (default `SDKWORK_TUNNEL_TOKEN`, PRD §40).
    pub token_env: String,
    /// Environment variable overriding the generated device id.
    pub device_id_env: String,
    /// Device name reported in `Hello`; empty means the hostname.
    pub device_name: String,
    /// Subdomain suffix used to expand auto-generated expose domains.
    pub domain_suffix: String,
    /// Server TLS verification policy for the agent side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<TunnelAgentTlsConfig>,
}

impl Default for TunnelAgentConfig {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            token_env: "SDKWORK_TUNNEL_TOKEN".to_owned(),
            device_id_env: "SDKWORK_TUNNEL_DEVICE_ID".to_owned(),
            device_name: String::new(),
            domain_suffix: String::new(),
            tls: None,
        }
    }
}

/// Agent-side TLS verification options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
#[derive(Default)]
pub struct TunnelAgentTlsConfig {
    /// PEM file with the gateway CA or leaf certificate to trust.
    pub ca_pem_path: Option<String>,
    /// SHA-256 hex fingerprint of the expected gateway leaf certificate
    /// (DER). Ideal for self-signed development gateways: the connection is
    /// still authenticated, just by pinning instead of a CA chain.
    pub pinned_server_sha256: Option<String>,
    /// Danger: skip gateway certificate verification. Development only; the
    /// gateway runtime logs a prominent warning when this is set.
    #[serde(default)]
    pub insecure_skip_verify: bool,
}

/// Shared network behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct TunnelNetworkConfig {
    /// Heartbeat cadence in seconds (PRD §28).
    pub heartbeat_interval_secs: u64,
    /// Automatic reconnect after transport loss.
    pub reconnect: bool,
    /// First reconnect delay seconds; doubles up to `max_backoff_secs` with
    /// jitter (PRD §27).
    pub initial_backoff_secs: u64,
    /// Reconnect backoff ceiling seconds.
    pub max_backoff_secs: u64,
}

impl Default for TunnelNetworkConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_secs: DEFAULT_HEARTBEAT_INTERVAL_SECS,
            reconnect: true,
            initial_backoff_secs: 1,
            max_backoff_secs: 60,
        }
    }
}

/// Handshake and I/O timeouts in seconds (PRD §51).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct TunnelTimeoutConfig {
    /// Transport connect timeout.
    pub connect: u64,
    /// Handshake + authentication timeout.
    pub handshake: u64,
    /// Data-stream open timeout.
    pub stream_open: u64,
    /// Idle session timeout (no heartbeat and no streams).
    pub idle: u64,
}

impl Default for TunnelTimeoutConfig {
    fn default() -> Self {
        Self {
            connect: 10,
            handshake: 10,
            stream_open: 10,
            idle: 300,
        }
    }
}

impl TunnelTimeoutConfig {
    /// Connect timeout as a `Duration`.
    pub fn connect_duration(&self) -> Duration {
        Duration::from_secs(self.connect.max(1))
    }

    /// Handshake timeout as a `Duration`.
    pub fn handshake_duration(&self) -> Duration {
        Duration::from_secs(self.handshake.max(1))
    }

    /// Stream-open timeout as a `Duration`.
    pub fn stream_open_duration(&self) -> Duration {
        Duration::from_secs(self.stream_open.max(1))
    }

    /// Idle timeout as a `Duration`.
    pub fn idle_duration(&self) -> Duration {
        Duration::from_secs(self.idle.max(1))
    }
}

/// Resource ceilings (PRD §106).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct TunnelLimitsConfig {
    /// Maximum concurrently registered devices.
    pub max_devices: u32,
    /// Maximum concurrent live sessions.
    pub max_sessions: u32,
    /// Maximum concurrent data streams per session.
    pub max_streams_per_session: u32,
    /// Maximum registered routes.
    pub max_routes: u32,
    /// Maximum STP control-message size in bytes (PRD §108).
    pub max_control_message_bytes: u32,
}

impl Default for TunnelLimitsConfig {
    fn default() -> Self {
        Self {
            max_devices: 10_000,
            max_sessions: 10_000,
            max_streams_per_session: 256,
            max_routes: 10_000,
            max_control_message_bytes: 64 * 1024,
        }
    }
}

/// Agent-side route template (PRD §39): the declarative shape in
/// `[[tunnel.routes]]` before the agent assigns ids and registers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct TunnelRouteTemplate {
    /// Operator-facing name.
    pub name: String,
    /// `http` or `tcp`.
    pub protocol: TunnelProtocolKind,
    /// Public domain (HTTP routes).
    pub domain: Option<String>,
    /// Public gateway port (TCP routes).
    pub port: Option<u16>,
    /// Agent-local `ip:port` target.
    pub target: String,
    /// Access policy overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<RoutePolicy>,
}

impl TunnelRouteTemplate {
    /// Validates the template shape.
    pub fn validate(&self) -> Result<()> {
        self.to_route_id_and_matcher()?;
        TunnelTarget::parse_tcp(&self.target)?;
        Ok(())
    }

    /// Resolves the matcher declared by this template.
    pub fn matcher(&self) -> Result<RouteMatcher> {
        self.to_route_id_and_matcher().map(|(_, matcher)| matcher)
    }

    /// Parses the target declared by this template.
    pub fn target(&self) -> Result<TunnelTarget> {
        TunnelTarget::parse_tcp(&self.target)
    }

    /// Policy with defaults applied.
    pub fn policy_or_default(&self) -> RoutePolicy {
        self.policy.clone().unwrap_or_default()
    }

    fn to_route_id_and_matcher(&self) -> Result<(RouteId, RouteMatcher)> {
        if self.name.is_empty() {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: "route template requires a name".to_owned(),
            });
        }
        let matcher = match self.protocol {
            TunnelProtocolKind::Http => {
                let domain = self
                    .domain
                    .as_deref()
                    .ok_or_else(|| TunnelError::Validation {
                        field: ValidationField::Domain,
                        reason: format!("http route `{}` requires a domain", self.name),
                    })?;
                RouteMatcher::domain(domain)?
            }
            TunnelProtocolKind::Tcp => {
                let port = self.port.ok_or_else(|| TunnelError::Validation {
                    field: ValidationField::Config,
                    reason: format!("tcp route `{}` requires a port", self.name),
                })?;
                RouteMatcher::Port(port)
            }
        };
        Ok((RouteId::parse(format!("route_{}", self.name))?, matcher))
    }

    /// Materializes a concrete [`TunnelRoute`] with a caller-supplied id
    /// (the gateway-assigned id on the control plane; a derived id when the
    /// agent builds registration requests).
    pub fn to_route(&self, id: RouteId) -> Result<TunnelRoute> {
        let (_, matcher) = self.to_route_id_and_matcher()?;
        TunnelRoute::new(
            id,
            self.name.clone(),
            self.protocol,
            matcher,
            self.target()?,
            self.policy_or_default(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_disabled() {
        let config = TunnelConfig::default();
        assert!(!config.enabled);
        config.validate().expect("disabled config always validates");
    }

    #[test]
    fn network_defaults_match_prd() {
        let network = TunnelNetworkConfig::default();
        assert_eq!(network.heartbeat_interval_secs, 10);
        assert_eq!(network.max_backoff_secs, 60);
    }

    #[test]
    fn timeout_defaults_match_prd() {
        let timeouts = TunnelTimeoutConfig::default();
        assert_eq!(timeouts.connect, 10);
        assert_eq!(timeouts.idle, 300);
        assert_eq!(timeouts.stream_open_duration(), Duration::from_secs(10));
    }

    #[test]
    fn gateway_listen_defaults_to_8443() {
        let gateway = TunnelGatewayConfig::default();
        assert!(gateway.listen.ends_with(":8443"));
        assert_eq!(
            gateway.agent_token_env,
            vec!["SDKWORK_TUNNEL_GATEWAY_TOKEN".to_owned()]
        );
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let json = r#"{"enabled":true,"unknown":"x"}"#;
        assert!(serde_json::from_str::<TunnelConfig>(json).is_err());
    }

    #[test]
    fn config_round_trips_camel_case() {
        let config = TunnelConfig {
            enabled: true,
            network: Some(TunnelNetworkConfig {
                heartbeat_interval_secs: 5,
                ..TunnelNetworkConfig::default()
            }),
            ..TunnelConfig::disabled()
        };
        let json = serde_json::to_string(&config).expect("serialize");
        assert!(json.contains("heartbeatIntervalSecs"));
        let decoded: TunnelConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, config);
    }

    #[test]
    fn http_template_requires_domain() {
        let template = TunnelRouteTemplate {
            name: "web".to_owned(),
            protocol: TunnelProtocolKind::Http,
            domain: None,
            port: None,
            target: "127.0.0.1:3000".to_owned(),
            policy: None,
        };
        assert!(template.validate().is_err());
    }

    #[test]
    fn valid_template_builds_route() {
        let template = TunnelRouteTemplate {
            name: "web".to_owned(),
            protocol: TunnelProtocolKind::Http,
            domain: Some("demo.sdkwork.link".to_owned()),
            port: None,
            target: "127.0.0.1:3000".to_owned(),
            policy: None,
        };
        template.validate().expect("valid template");
        let route = template
            .to_route(RouteId::parse("route_web").expect("valid id"))
            .expect("route builds");
        assert_eq!(route.matcher.as_domain(), Some("demo.sdkwork.link"));
    }
}
