//! Tunnel configuration (PRD §38, §39, §40, §51, §106).
//!
//! One serde model shared by three consumers: the webserver app config
//! (`[tunnel]` server.toml section), the standalone gateway process, and the
//! tunnel CLI. Secrets are never stored here: `token` fields hold
//! environment-variable *names* resolved at runtime (`SDKWORK_TUNNEL_TOKEN`
//! and friends), so real credentials never reach configuration files or git.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Result, TunnelError, ValidationField};
use crate::route::{RouteMatcher, TunnelProtocolKind, TunnelRoute, TunnelTarget};
use crate::{ids::RouteId, policy::RoutePolicy};

/// Default heartbeat cadence in seconds (PRD §28).
pub const DEFAULT_HEARTBEAT_INTERVAL_SECS: u64 = 10;
/// Default QUIC tunnel listener port.
pub const DEFAULT_TUNNEL_PORT: u16 = 8443;
/// Default address per-route `tcp`/`udp` listeners bind (FRP `proxyBindAddr`).
pub const DEFAULT_PROXY_BIND_ADDR: &str = "0.0.0.0";
/// Default port for a configured DNS server (FRP `dnsServer`).
pub const DEFAULT_DNS_PORT: u16 = 53;

/// Parses a configured DNS server: `ip` or `ip:port`.
///
/// A hostname is refused rather than resolved: resolving the resolver is a
/// dependency loop, and it would put the system resolver back into the path the
/// operator removed by configuring this at all.
pub fn parse_dns_server(raw: &str) -> std::result::Result<std::net::SocketAddr, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("dnsServer must not be empty".to_owned());
    }
    if let Ok(ip) = raw.parse::<std::net::IpAddr>() {
        return Ok(std::net::SocketAddr::new(ip, DEFAULT_DNS_PORT));
    }
    raw.parse::<std::net::SocketAddr>()
        .map_err(|_| format!("dnsServer `{raw}` must be an IP address, optionally with a port"))
}

/// The `[tunnel.gateway]` section's own accessors.
impl TunnelGatewayConfig {
    /// The address per-route listeners bind, parsed.
    ///
    /// A literal IP only: a hostname would have to be resolved once per
    /// registration (and could resolve differently between two routes on one
    /// gateway), and a `host:port` form would fight the route's own port.
    pub fn proxy_bind_addr(&self) -> std::result::Result<std::net::IpAddr, String> {
        let raw = self.proxy_bind_addr.trim();
        if raw.is_empty() {
            return Err("tunnel gateway proxyBindAddr must not be empty".to_owned());
        }
        if raw.contains(':') && raw.parse::<std::net::IpAddr>().is_err() {
            // `1.2.3.4:80` or `host:port`: the port would be ignored, so say so
            // instead of binding somewhere the operator did not ask for.
            return Err(format!(
                "tunnel gateway proxyBindAddr `{raw}` must be an IP address without a port"
            ));
        }
        raw.parse::<std::net::IpAddr>().map_err(|_| {
            format!("tunnel gateway proxyBindAddr `{raw}` must be a literal IP address")
        })
    }
}

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
        let gateway = self.gateway_or_default();
        // The proxy bind address must be a literal IP: a hostname here would be
        // resolved per registration, and a port would be overwritten by the
        // route's own port, so both are refused rather than silently ignored.
        gateway
            .proxy_bind_addr()
            .map_err(|error| crate::TunnelError::Validation {
                field: ValidationField::Config,
                reason: error,
            })?;
        if gateway.route_port_min == 0 || gateway.route_port_min > gateway.route_port_max {
            return Err(crate::TunnelError::Validation {
                field: crate::ValidationField::Config,
                reason: format!(
                    "gateway routePortMin {} must be non-zero and not exceed routePortMax {}",
                    gateway.route_port_min, gateway.route_port_max
                ),
            });
        }
        for template in self.routes_or_empty() {
            template.validate()?;
        }
        // Every gateway address is checked when the section is present: a
        // fallback that cannot be dialled is worse than no fallback, because
        // the operator believes failover is configured.
        if let Some(agent) = self.agent.as_ref() {
            agent.validate_endpoints()?;
        }
        // Network tunables are validated as a set, because their relationships
        // are what an operator gets wrong (a heartbeat timeout below the
        // heartbeat interval expires the session before it can be refreshed).
        self.network_or_default().validate_network()?;
        // Users: each entry valid, and names unique. Two entries sharing one
        // name would make "which quota applies" ambiguous, and the first match
        // would silently win.
        let mut seen_users: Vec<&str> = Vec::with_capacity(gateway.users.len());
        for user in &gateway.users {
            user.validate()?;
            let name = user.name.trim();
            if seen_users.contains(&name) {
                return Err(crate::TunnelError::Validation {
                    field: crate::ValidationField::Config,
                    reason: format!("tunnel gateway user `{name}` is declared twice"),
                });
            }
            seen_users.push(name);
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
    /// Inclusive lower bound of the public port a `tcp`/`udp` route may
    /// register. Ports below the bound are gateway-reserved (the edge's own
    /// listeners and well-known services); an authenticated agent must not be
    /// able to pre-empt them. Widen deliberately via configuration only.
    pub route_port_min: u16,
    /// Inclusive upper bound of the registrable public port range; see
    /// [`Self::route_port_min`].
    pub route_port_max: u16,
    /// Optional single multiplexed ingress port speaking HTTP `CONNECT`
    /// (FRP `tcpmux` parity, expressed as a public ingress).
    ///
    /// One port then reaches **every** tunneled route: the request's
    /// `CONNECT host:port` target is resolved against the domain registry
    /// first and the port registry second, so a network that only permits one
    /// outbound port no longer forces one public port per route. `None`
    /// (the default) binds nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcpmux_port: Option<u16>,
    /// Places an agent must re-present its token, beyond the handshake (FRP
    /// `auth.additionalScopes`).
    ///
    /// The gateway advertises the effective list in its `AuthResult`, so an
    /// agent attaches the token exactly where it is required and the two sides
    /// cannot drift; an agent that predates the advertisement is refused only
    /// when an operator opted in, which is why the default is empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auth_scopes: Vec<crate::policy::AuthScope>,
    /// Address the per-route `tcp`/`udp` listeners bind (FRP `proxyBindAddr`).
    ///
    /// Default `0.0.0.0`: every interface. A host with several addresses
    /// (public plus a private management network) sets this so that
    /// agent-registered ports are reachable exactly where the operator
    /// intends, instead of everywhere the machine happens to be attached.
    pub proxy_bind_addr: String,
    /// Per-user credentials and quotas (FRP `user`, made enforceable).
    ///
    /// FRP's `user` is a label: any client may claim any name, so it groups
    /// proxies in a dashboard but cannot bound what one team consumes on a
    /// shared gateway. Here a user owns a token and its own ceilings, which is
    /// what makes both the isolation and the quota real: an agent that
    /// authenticates with team A's token **is** team A, and cannot claim to be
    /// team B.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub users: Vec<TunnelUserConfig>,
}

/// One user of a shared gateway: its credential source and its ceilings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct TunnelUserConfig {
    /// User name an agent claims and that appears in listings.
    pub name: String,
    /// Environment variable holding this user's token, resolved at startup.
    ///
    /// A *name*, never the token itself: credentials stay in the environment,
    /// and the gateway never reads one from a configuration file.
    pub token_env: String,
    /// Maximum live routes this user may register; absent falls back to the
    /// global `limits.maxRoutes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_routes: Option<u32>,
    /// Maximum public TCP/UDP ports this user may occupy at once; absent falls
    /// back to "as many as the global limits allow".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_ports: Option<u32>,
    /// Maximum concurrent sessions (machines) for this user; absent falls back
    /// to the global `limits.maxDevices`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_devices: Option<u32>,
}

impl TunnelUserConfig {
    /// Validates one user entry.
    pub fn validate(&self) -> Result<()> {
        let name = self.name.trim();
        // One name validator for the whole workspace: the agent's claim and the
        // gateway's table must not disagree about what a user name may be.
        crate::device::validate_user_name(name)?;
        if self.token_env.trim().is_empty() {
            return Err(crate::TunnelError::Validation {
                field: crate::ValidationField::Config,
                reason: format!(
                    "user `{name}` must name the environment variable holding its token"
                ),
            });
        }
        for (label, value) in [
            ("maxRoutes", self.max_routes),
            ("maxPorts", self.max_ports),
            ("maxDevices", self.max_devices),
        ] {
            if value == Some(0) {
                // Zero is not "no quota": it is a user that can do nothing,
                // which an operator means as "disabled" — say so explicitly by
                // omitting the user, or set a real ceiling.
                return Err(crate::TunnelError::Validation {
                    field: crate::ValidationField::Config,
                    reason: format!("user `{name}`: {label} must be at least 1 when set"),
                });
            }
        }
        Ok(())
    }
}

impl Default for TunnelGatewayConfig {
    fn default() -> Self {
        Self {
            listen: format!("0.0.0.0:{DEFAULT_TUNNEL_PORT}"),
            domain_suffixes: Vec::new(),
            agent_token_env: vec!["SDKWORK_TUNNEL_GATEWAY_TOKEN".to_owned()],
            tls_cert_pem_env: None,
            tls_key_pem_env: None,
            // Privileged and edge-reserved ports stay out of agent reach by
            // default, matching ngrok/rathole remote-port policy.
            route_port_min: 1024,
            route_port_max: u16::MAX,
            tcpmux_port: None,
            auth_scopes: Vec::new(),
            proxy_bind_addr: DEFAULT_PROXY_BIND_ADDR.to_owned(),
            users: Vec::new(),
        }
    }
}

/// Agent (inside network) tunnel settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct TunnelAgentConfig {
    /// Gateway endpoint advertised to agents, `host:port` (QUIC/UDP).
    pub endpoint: String,
    /// Additional gateway addresses, tried in order after `endpoint` whenever
    /// the agent cannot reach the current one (FRP's ordered `serverAddr`
    /// list).
    ///
    /// The list is what makes a gateway rollout survivable: an agent whose
    /// primary is being restarted moves to the next address instead of
    /// retrying one dead port until the operator intervenes. Entries are
    /// validated at configuration load, so a typo is a startup error rather
    /// than a fallback that never happens.
    pub endpoints: Vec<String>,
    /// Environment variable holding the agent bearer token
    /// (default `SDKWORK_TUNNEL_TOKEN`, PRD §40).
    pub token_env: String,
    /// Environment variable overriding the generated device id.
    pub device_id_env: String,
    /// Device name reported in `Hello`; empty means the hostname.
    pub device_name: String,
    /// Subdomain suffix used to expand auto-generated expose domains.
    pub domain_suffix: String,
    /// User this machine serves (FRP `user`).
    ///
    /// A claim: the gateway resolves the effective user from the token and
    /// refuses a contradictory one, so this only ever *names* the team whose
    /// quotas and listings the machine wants to appear in.
    pub user: String,
    /// Stop instead of retrying when the gateway rejects our credentials.
    ///
    /// FRP's `loginFailExit` defaults to **true**; here it defaults to false,
    /// because this agent is a service whose contract is "keep working through
    /// a gateway outage" (PRD §27) and a fleet that silently exits on one bad
    /// rollout is worse than one that retries with backoff and reports the
    /// reason. Setting it to true gives FRP's behaviour: a rejected credential
    /// is an operator error, not an outage, so the process stops with the
    /// reason in its log rather than hammering a gateway that will never say
    /// yes.
    ///
    /// Only a *credential rejection* ends the process; a transport failure, a
    /// timeout, or an unreachable gateway keep retrying.
    pub login_fail_exit: bool,
    /// Server TLS verification policy for the agent side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<TunnelAgentTlsConfig>,
}

impl Default for TunnelAgentConfig {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            endpoints: Vec::new(),
            token_env: "SDKWORK_TUNNEL_TOKEN".to_owned(),
            device_id_env: "SDKWORK_TUNNEL_DEVICE_ID".to_owned(),
            device_name: String::new(),
            domain_suffix: String::new(),
            user: String::new(),
            // FRP ships `loginFailExit = true`; see the field's documentation
            // for why this agent defaults to the opposite.
            login_fail_exit: false,
            tls: None,
        }
    }
}

impl TunnelAgentConfig {
    /// The gateway addresses to try, in preference order, deduplicated.
    ///
    /// `endpoint` stays first so a configuration that sets both keys keeps its
    /// primary where the operator put it, and an empty list reproduces the
    /// single-address behaviour exactly.
    #[must_use]
    pub fn gateway_endpoints(&self) -> Vec<String> {
        let mut ordered: Vec<String> = Vec::with_capacity(self.endpoints.len() + 1);
        for candidate in std::iter::once(&self.endpoint).chain(self.endpoints.iter()) {
            let candidate = candidate.trim();
            if candidate.is_empty() || ordered.iter().any(|seen| seen == candidate) {
                continue;
            }
            ordered.push(candidate.to_owned());
        }
        ordered
    }

    /// Validates every configured address as a `host:port` authority.
    pub fn validate_endpoints(&self) -> Result<()> {
        for candidate in self.gateway_endpoints() {
            validate_endpoint_authority(&candidate)?;
        }
        Ok(())
    }
}

/// Validates one `host:port` gateway address.
///
/// Deliberately stricter than "does it parse": an address with a scheme, a
/// path, or a port of 0 is a configuration mistake that would otherwise
/// surface as an unreachable gateway — the same failure shape as an outage,
/// which is exactly what an operator cannot debug from a log line.
pub fn validate_endpoint_authority(raw: &str) -> Result<()> {
    let candidate = raw.trim();
    if candidate.is_empty() {
        return Err(crate::TunnelError::Validation {
            field: crate::ValidationField::Config,
            reason: "a gateway endpoint must not be empty".to_owned(),
        });
    }
    if candidate.contains("://") || candidate.contains('/') || candidate.contains('?') {
        return Err(crate::TunnelError::Validation {
            field: crate::ValidationField::Config,
            reason: format!("gateway endpoint `{candidate}` must be a host:port authority"),
        });
    }
    if candidate.chars().any(char::is_whitespace) {
        return Err(crate::TunnelError::Validation {
            field: crate::ValidationField::Config,
            reason: format!("gateway endpoint `{candidate}` must not contain whitespace"),
        });
    }
    let (host, port) =
        candidate
            .rsplit_once(':')
            .ok_or_else(|| crate::TunnelError::Validation {
                field: crate::ValidationField::Config,
                reason: format!("gateway endpoint `{candidate}` must be host:port"),
            })?;
    if host.is_empty() {
        return Err(crate::TunnelError::Validation {
            field: crate::ValidationField::Config,
            reason: format!("gateway endpoint `{candidate}` has an empty host"),
        });
    }
    let port = port
        .parse::<u16>()
        .map_err(|_| crate::TunnelError::Validation {
            field: crate::ValidationField::Config,
            reason: format!("gateway endpoint `{candidate}` has an invalid port"),
        })?;
    if port == 0 {
        return Err(crate::TunnelError::Validation {
            field: crate::ValidationField::Config,
            reason: format!("gateway endpoint `{candidate}` must not use port 0"),
        });
    }
    Ok(())
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
    /// QUIC keep-alive probing interval in seconds (FRP
    /// `transport.tcpMuxKeepaliveInterval` / `quic.keepalivePeriod` parity).
    ///
    /// `None` keeps the derived default (half the heartbeat cadence); `Some(0)`
    /// disables probing, which is only sane where an idle connection is
    /// expected and the peer's idle timeout is long enough to match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_alive_interval_secs: Option<u64>,
    /// How long a session may go without a heartbeat before the gateway
    /// considers it stale (FRP `transport.heartbeatTimeout` parity).
    ///
    /// `None` keeps the derived rule `min(idle, heartbeat x 3)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat_timeout_secs: Option<u64>,
    /// Largest UDP payload the relay frames, in bytes (FRP
    /// `transport.udpPacketSize` parity).
    ///
    /// `None` keeps the protocol ceiling (65,507 bytes, the largest payload an
    /// IPv4 datagram can carry). Lowering it is how an operator avoids
    /// fragmentation on a constrained path; a datagram above it is dropped and
    /// counted, never truncated, because a truncated datagram is a corrupt
    /// datagram.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub udp_packet_size: Option<u32>,
    /// Maximum concurrent bidirectional streams per QUIC connection (FRP
    /// `quic.maxIncomingStreams` parity).
    ///
    /// `None` keeps each role's fallback (the gateway's
    /// `limits.maxStreamsPerSession`, the agent's protocol default), so the two
    /// sides agree unless an operator deliberately separates them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent_streams: Option<u32>,
    /// Source address the agent binds before dialing a route's local target
    /// (FRP `transport.dialServerLocalIP`).
    ///
    /// A multi-homed machine reaches the same service on different networks,
    /// and the service (or the ACL in front of it) decides by source address:
    /// with this unset the kernel picks an egress address, which is usually
    /// right and occasionally exactly wrong. Only target dials are affected —
    /// the gateway connection keeps its own transport.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dial_local_addr: Option<std::net::IpAddr>,
    /// DNS server the agent resolves the gateway host through (FRP
    /// `dnsServer`), as `ip` or `ip:port`.
    ///
    /// Unset means the system resolver, which is what every deployment has
    /// today. Set means *only* this server: a split-horizon or offline network
    /// publishes the gateway's name in its own resolver, and silently falling
    /// back to the system one would resolve through a resolver the operator
    /// explicitly did not choose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_server: Option<String>,
}

impl Default for TunnelNetworkConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_secs: DEFAULT_HEARTBEAT_INTERVAL_SECS,
            reconnect: true,
            initial_backoff_secs: 1,
            max_backoff_secs: 60,
            keep_alive_interval_secs: None,
            heartbeat_timeout_secs: None,
            udp_packet_size: None,
            max_concurrent_streams: None,
            dial_local_addr: None,
            dns_server: None,
        }
    }
}

/// Smallest UDP payload ceiling an operator may configure.
///
/// Below a DNS-sized datagram the relay would be useless, and a value this low
/// is far more likely to be a unit mistake (bytes vs. kilobytes) than a real
/// intent.
pub const MIN_UDP_PACKET_SIZE: u32 = 512;
/// Largest UDP payload ceiling: the maximum IPv4 UDP payload.
///
/// Mirrors `sdkwork_webserver_tunnel_protocol::packet_frame::MAX_PACKET_PAYLOAD`
/// (the protocol crate depends on this one, so the number is restated here
/// rather than imported — and a mismatch would be caught by the relay refusing
/// to frame a datagram the configuration allowed).
pub const MAX_UDP_PACKET_SIZE: u32 = 65_507;

impl TunnelNetworkConfig {
    /// The keep-alive interval to apply, given the heartbeat cadence.
    ///
    /// One rule for both roles: an explicit value wins (with `0` meaning "do
    /// not probe"), otherwise the connection is probed at half the heartbeat
    /// cadence so a missed heartbeat is noticed before the session expires.
    #[must_use]
    pub fn keep_alive_interval(&self) -> Option<Duration> {
        match self.keep_alive_interval_secs {
            Some(0) => None,
            Some(seconds) => Some(Duration::from_secs(seconds)),
            None => Some(Duration::from_millis(
                (self.heartbeat_interval_secs.max(1) * 1000) / 2,
            )),
        }
    }

    /// How long a session may stay silent before it is considered stale.
    ///
    /// An explicit `heartbeatTimeout` wins; otherwise the rule is
    /// `min(idle, heartbeat x 3)`: three missed heartbeats, capped by the
    /// transport's own idle timeout so the two never disagree about which side
    /// gives up first.
    #[must_use]
    pub fn stale_after(&self, idle: Duration) -> Duration {
        let derived = idle.min(Duration::from_secs(self.heartbeat_interval_secs.max(1) * 3));
        match self.heartbeat_timeout_secs {
            Some(seconds) => Duration::from_secs(seconds.max(1)),
            None => derived,
        }
    }

    /// The UDP payload ceiling in bytes.
    #[must_use]
    pub fn udp_packet_size_bytes(&self) -> usize {
        match self.udp_packet_size {
            Some(size) => size.clamp(MIN_UDP_PACKET_SIZE, MAX_UDP_PACKET_SIZE) as usize,
            None => MAX_UDP_PACKET_SIZE as usize,
        }
    }

    /// The streams-per-connection ceiling, given the role's fallback.
    ///
    /// The floor of 16 matches the transport: a connection needs room for the
    /// control stream plus concurrent data streams, and a smaller value would
    /// make the tunnel unusable while looking like a network problem.
    #[must_use]
    pub fn max_concurrent_streams(&self, fallback: u32) -> u32 {
        self.max_concurrent_streams
            .unwrap_or(fallback)
            .clamp(16, 65_536)
    }

    /// Validates the tunables, including the relationships between them.
    pub fn validate_network(&self) -> Result<()> {
        if let Some(keep_alive) = self.keep_alive_interval_secs {
            if keep_alive > 3_600 {
                return Err(crate::TunnelError::Validation {
                    field: crate::ValidationField::Config,
                    reason: format!(
                        "keepAliveIntervalSecs {keep_alive} must be 0 (disabled) or at most 3600"
                    ),
                });
            }
        }
        if let Some(timeout) = self.heartbeat_timeout_secs {
            let heartbeat = self.heartbeat_interval_secs.max(1);
            if timeout < heartbeat {
                // Without this the session would expire before its first
                // heartbeat could arrive, which looks exactly like a broken
                // network and is really a unit mistake.
                return Err(crate::TunnelError::Validation {
                    field: crate::ValidationField::Config,
                    reason: format!(
                        "heartbeatTimeoutSecs {timeout} must be at least the heartbeat interval {heartbeat}"
                    ),
                });
            }
        }
        if let Some(size) = self.udp_packet_size {
            if !(MIN_UDP_PACKET_SIZE..=MAX_UDP_PACKET_SIZE).contains(&size) {
                return Err(crate::TunnelError::Validation {
                    field: crate::ValidationField::Config,
                    reason: format!(
                        "udpPacketSize {size} must be between {MIN_UDP_PACKET_SIZE} and {MAX_UDP_PACKET_SIZE} bytes"
                    ),
                });
            }
        }
        if let Some(streams) = self.max_concurrent_streams {
            if !(16..=65_536).contains(&streams) {
                return Err(crate::TunnelError::Validation {
                    field: crate::ValidationField::Config,
                    reason: format!("maxConcurrentStreams {streams} must be between 16 and 65536"),
                });
            }
        }
        if let Some(server) = &self.dns_server {
            // Parsed here, not at the first reconnect: a hostname or a typo must
            // be a startup error, because at reconnect time it would look like
            // "the gateway is unreachable".
            parse_dns_server(server).map_err(|reason| crate::TunnelError::Validation {
                field: crate::ValidationField::Config,
                reason,
            })?;
        }
        Ok(())
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
    /// Idle session timeout, measured on **heartbeat freshness only**.
    ///
    /// The sweep expires a session whose last heartbeat is older than
    /// `min(idle, heartbeat × 3)`, which is why an idle *data stream* does not
    /// extend a session's life: the heartbeat rides the same QUIC connection
    /// as the data streams, so a live stream implies live heartbeats and a
    /// stale heartbeat means the whole connection is gone. It also bounds how
    /// long a dead agent keeps its routes and gateway ports reserved.
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
    /// Maximum concurrent visitor/control connections across every listener.
    /// This is the gateway-side global admission ceiling; saturation closes
    /// new connections immediately instead of queueing unbounded tasks.
    pub max_connections: u32,
}

impl Default for TunnelLimitsConfig {
    fn default() -> Self {
        Self {
            max_devices: 10_000,
            max_sessions: 10_000,
            max_streams_per_session: 256,
            max_routes: 10_000,
            max_control_message_bytes: 64 * 1024,
            max_connections: 8_192,
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
    /// `http`, `tcp`, or `udp`.
    pub protocol: TunnelProtocolKind,
    /// Public domain (HTTP routes).
    pub domain: Option<String>,
    /// Public gateway port (TCP and UDP routes).
    pub port: Option<u16>,
    /// Agent-local `ip:port` target.
    pub target: String,
    /// Access policy overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<RoutePolicy>,
    /// PROXY protocol version prepended toward the local target (FRP
    /// `transport.proxyProtocolVersion` parity). Absent keeps the stream raw.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_protocol: Option<crate::proxy_protocol::ProxyProtocolVersion>,
    /// Path prefixes this route answers on its host (FRP `locations` parity).
    /// Empty (the default) means the route owns every path of the host.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<String>,
    /// Relay ceiling in bytes per second, each direction (FRP
    /// `transport.bandwidthLimit` parity). Absent means unlimited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth_limit_bytes_per_second: Option<u64>,
    /// Load-balancer group (FRP `loadBalancer.group` parity): routes sharing a
    /// group and a matcher are replicas of one service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_balancer_group: Option<String>,
    /// Agent-side health probe for this route (FRP `healthCheck` parity).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_check: Option<crate::health::HealthCheckConfig>,
    /// Name of the environment variable holding the shared secret a visitor
    /// must present for an `stcp`/`sudp` route (FRP `sk` parity).
    ///
    /// A *name*, never the key itself: credentials stay in the environment
    /// (component invariant "secrets live in environment variables, never in
    /// configuration files"), are resolved by the agent at startup, and reach
    /// the gateway only inside the encrypted registration message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_key_env: Option<String>,
    /// Replace the `Host` header before the request reaches the agent (FRP
    /// `hostHeaderRewrite`). HTTP routes only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_header_rewrite: Option<String>,
    /// Headers set on the request forwarded to the agent (FRP
    /// `requestHeaders.set`). HTTP routes only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_headers: Option<crate::headers::HeaderSetConfig>,
    /// Headers set on the response returned to the visitor (FRP
    /// `responseHeaders.set`). HTTP routes only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_headers: Option<crate::headers::HeaderSetConfig>,
    /// Free-form operator metadata carried on the route (FRP `annotations`).
    ///
    /// Declared as a table (`[tunnel.routes.annotations] owner = "team-a"`),
    /// bounded and validated by the route model, and never used for routing or
    /// authorization — it exists so a console can group and filter by whatever
    /// the fleet agrees on.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, String>,
}

impl TunnelRouteTemplate {
    /// Validates the template shape.
    ///
    /// Delegates to [`Self::to_route`] so a template can never validate into
    /// a route that [`TunnelRoute::new`] would then reject.
    pub fn validate(&self) -> Result<()> {
        let (route_id, _) = self.to_route_id_and_matcher()?;
        self.to_route(route_id).map(|_| ())
    }

    /// Resolves the matcher declared by this template.
    pub fn matcher(&self) -> Result<RouteMatcher> {
        self.to_route_id_and_matcher().map(|(_, matcher)| matcher)
    }

    /// Parses the target declared by this template; the protocol decides the
    /// target kind so UDP routes resolve to [`TunnelTarget::Udp`].
    pub fn target(&self) -> Result<TunnelTarget> {
        TunnelTarget::parse_for_protocol(self.protocol, &self.target)
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
            TunnelProtocolKind::Tcp | TunnelProtocolKind::Udp => {
                let protocol_label = if self.protocol == TunnelProtocolKind::Udp {
                    "udp"
                } else {
                    "tcp"
                };
                let port = self.port.ok_or_else(|| TunnelError::Validation {
                    field: ValidationField::Config,
                    reason: format!("{protocol_label} route `{}` requires a port", self.name),
                })?;
                RouteMatcher::Port(port)
            }
            // Secret kinds open no public surface: the route name *is* the
            // name a visitor asks for, and a domain or port on the template
            // would promise a surface the gateway never binds.
            TunnelProtocolKind::Stcp | TunnelProtocolKind::Sudp => {
                if self.domain.is_some() || self.port.is_some() {
                    return Err(TunnelError::Validation {
                        field: ValidationField::Config,
                        reason: format!(
                            "secret route `{}` must not declare a domain or port (visitors ask for it by name)",
                            self.name
                        ),
                    });
                }
                // The key's *source* is configuration; the key itself is not.
                // Without one the agent cannot authenticate a single visitor,
                // so the omission is a configuration error, not a default.
                if self
                    .secret_key_env
                    .as_deref()
                    .map(str::trim)
                    .is_none_or(str::is_empty)
                {
                    return Err(TunnelError::Validation {
                        field: ValidationField::Config,
                        reason: format!(
                            "secret route `{}` requires secretKeyEnv (the environment variable holding the shared key)",
                            self.name
                        ),
                    });
                }
                RouteMatcher::secret(&self.name)?
            }
        };
        Ok((RouteId::parse(format!("route_{}", self.name))?, matcher))
    }

    /// Derives this template's route id inside a user's namespace.
    ///
    /// Route ids and secret names are gateway-global, so two teams could not
    /// both serve a template called `web`: the second registration collided
    /// with the first team's id and was refused. Namespacing the derived
    /// identity is what makes FRP's `user` an actual namespace rather than a
    /// label — `team-a`'s `web` and `team-b`'s `web` become two routes.
    ///
    /// The same derivation is what the gateway checks a registration against,
    /// so a machine cannot register inside a namespace its token does not
    /// speak for.
    pub fn route_id_for_user(&self, user: Option<&str>) -> Result<RouteId> {
        crate::route::derived_route_id(user, &self.name)
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
        )?
        .with_proxy_protocol(self.proxy_protocol)?
        .with_locations(self.locations.clone())?
        .with_bandwidth_limit(self.bandwidth_limit_bytes_per_second)?
        .with_load_balancing(self.load_balancer_group.clone(), self.health_check.clone())?
        .with_headers(self.header_rewrite()?)?
        .with_annotations(self.annotations.clone())
    }

    /// Builds this template's HTTP header rewriting policy.
    ///
    /// Public because two callers need it without materializing a route: the
    /// offline preflight (`verify`) reports the policy, and the agent puts it
    /// on the registration wire.
    pub fn header_rewrite(&self) -> Result<crate::headers::RouteHeaderRewrite> {
        use crate::headers::{HeaderDirection, RouteHeaderRewrite};

        let request_set = match self.request_headers.as_ref() {
            Some(config) => config.to_pairs(HeaderDirection::Request)?,
            None => Vec::new(),
        };
        let response_set = match self.response_headers.as_ref() {
            Some(config) => config.to_pairs(HeaderDirection::Response)?,
            None => Vec::new(),
        };
        let host = self
            .host_header_rewrite
            .as_deref()
            .map(str::trim)
            .filter(|host| !host.is_empty())
            .map(str::to_owned);
        let rewrite = RouteHeaderRewrite {
            host,
            request_set,
            response_set,
        };
        rewrite.validate()?;
        rewrite.validate_for_protocol(self.protocol)?;
        Ok(rewrite)
    }

    /// Reads the shared secret from the environment variable this template
    /// names (FRP `sk` parity), or `None` for a public route.
    ///
    /// Separate from [`Self::to_route`] on purpose: materialization stays pure
    /// (the gateway validates declarations without ever holding a key), while
    /// the agent — the only role that must authenticate visitors — resolves it
    /// here at startup and fails loudly when it is missing or too short.
    pub fn secret_key_from_env(&self) -> Result<Option<String>> {
        if !self.protocol.is_secret() {
            return Ok(None);
        }
        let env_name = self
            .secret_key_env
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| TunnelError::Validation {
                field: ValidationField::Config,
                reason: format!(
                    "secret route `{}` requires secretKeyEnv (the environment variable holding the shared key)",
                    self.name
                ),
            })?;
        let key = std::env::var(env_name).map_err(|_| TunnelError::Validation {
            field: ValidationField::Config,
            reason: format!(
                "secret route `{}`: environment variable `{env_name}` is not set",
                self.name
            ),
        })?;
        let key = key.trim().to_owned();
        if key.len() < 8 || key.len() > 128 {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: format!(
                    "secret route `{}`: `{env_name}` must hold a key of 8..=128 characters",
                    self.name
                ),
            });
        }
        Ok(Some(key))
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
            proxy_protocol: None,
            locations: Vec::new(),
            ..TunnelRouteTemplate::default()
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
            proxy_protocol: None,
            locations: Vec::new(),
            ..TunnelRouteTemplate::default()
        };
        template.validate().expect("valid template");
        let route = template
            .to_route(RouteId::parse("route_web").expect("valid id"))
            .expect("route builds");
        assert_eq!(route.matcher.as_domain(), Some("demo.sdkwork.link"));
    }

    #[test]
    fn limits_defaults_match_prd_ceilings() {
        let limits = TunnelLimitsConfig::default();
        assert_eq!(limits.max_devices, 10_000);
        assert_eq!(limits.max_sessions, 10_000);
        assert_eq!(limits.max_streams_per_session, 256);
        assert_eq!(limits.max_routes, 10_000);
        assert_eq!(limits.max_control_message_bytes, 64 * 1024);
    }

    #[test]
    fn agent_defaults_hold_env_var_names_never_secrets() {
        // PRD §40: the configuration file carries the *name* of the variable
        // holding the credential; the credential itself is resolved at
        // runtime and never reaches a file or git.
        let agent = TunnelAgentConfig::default();
        assert_eq!(agent.token_env, "SDKWORK_TUNNEL_TOKEN");
        assert_eq!(agent.device_id_env, "SDKWORK_TUNNEL_DEVICE_ID");
        assert!(agent.endpoint.is_empty(), "no endpoint is baked in");
        assert!(
            agent.device_name.is_empty(),
            "empty means: use the hostname"
        );
        assert!(agent.tls.is_none(), "TLS policy is opt-in");

        let tls = TunnelAgentTlsConfig::default();
        assert!(tls.ca_pem_path.is_none());
        assert!(tls.pinned_server_sha256.is_none());
        assert!(
            !tls.insecure_skip_verify,
            "skip-verify is never the default"
        );
    }

    #[test]
    fn gateway_defaults_fail_closed_and_never_carry_a_token() {
        let gateway = TunnelGatewayConfig::default();
        assert!(
            gateway.domain_suffixes.is_empty(),
            "an empty suffix list disables the suffix policy"
        );
        assert_eq!(
            gateway.agent_token_env,
            vec!["SDKWORK_TUNNEL_GATEWAY_TOKEN"]
        );
        // The default names a *variable*; a literal token here would be the
        // bug this assertion exists to catch.
        assert!(gateway
            .agent_token_env
            .iter()
            .all(|name| name.chars().all(|c| c.is_ascii_uppercase() || c == '_')));
        assert!(gateway.tls_cert_pem_env.is_none());
        assert!(gateway.tls_key_pem_env.is_none());
    }

    /// FRP's ordered `serverAddr` list: the primary stays first, duplicates
    /// and blanks are dropped, and an empty list keeps the old behaviour.
    #[test]
    fn the_gateway_address_list_is_ordered_and_deduplicated() {
        let agent = TunnelAgentConfig {
            endpoint: "gw1.sdkwork.link:8443".to_owned(),
            endpoints: vec![
                "gw2.sdkwork.link:8443".to_owned(),
                "gw1.sdkwork.link:8443".to_owned(),
                "  ".to_owned(),
                "gw3.sdkwork.link:8443".to_owned(),
            ],
            ..TunnelAgentConfig::default()
        };
        assert_eq!(
            agent.gateway_endpoints(),
            vec![
                "gw1.sdkwork.link:8443".to_owned(),
                "gw2.sdkwork.link:8443".to_owned(),
                "gw3.sdkwork.link:8443".to_owned(),
            ]
        );
        agent.validate_endpoints().expect("all addresses are valid");

        // A configuration that sets only the list still works: the first entry
        // is the primary.
        let list_only = TunnelAgentConfig {
            endpoint: String::new(),
            endpoints: vec!["gw9.sdkwork.link:8443".to_owned()],
            ..TunnelAgentConfig::default()
        };
        assert_eq!(
            list_only.gateway_endpoints(),
            vec!["gw9.sdkwork.link:8443".to_owned()]
        );

        // No address at all is not an error here: an agent without a gateway
        // is a runtime decision (the CLI insists on one), not a config shape
        // the model has to reject.
        assert!(TunnelAgentConfig::default().gateway_endpoints().is_empty());
    }

    /// A fallback that cannot be dialled is worse than no fallback, so a
    /// malformed address is a configuration error.
    #[test]
    fn a_malformed_gateway_address_is_a_configuration_error() {
        for address in [
            "gw1.sdkwork.link",
            "quic://gw1.sdkwork.link:8443",
            "gw1.sdkwork.link:8443/tunnel",
            "gw1.sdkwork.link:0",
            "gw1.sdkwork.link:not-a-port",
            ":8443",
        ] {
            let agent = TunnelAgentConfig {
                endpoint: address.to_owned(),
                ..TunnelAgentConfig::default()
            };
            assert!(
                agent.validate_endpoints().is_err(),
                "`{address}` must be refused"
            );
        }
        // Enabled configuration: the tunnel section validates through it.
        let config = TunnelConfig {
            enabled: true,
            agent: Some(TunnelAgentConfig {
                endpoint: "gw1.sdkwork.link:8443".to_owned(),
                endpoints: vec!["gw2.sdkwork.link:8443".to_owned()],
                ..TunnelAgentConfig::default()
            }),
            ..TunnelConfig::disabled()
        };
        config.validate().expect("a valid list passes");

        // Disabled: nothing is validated, so a half-written section cannot
        // break a deployment that never uses the tunnel (PRD §77).
        let disabled = TunnelConfig {
            enabled: false,
            agent: Some(TunnelAgentConfig {
                endpoint: "not-an-address".to_owned(),
                ..TunnelAgentConfig::default()
            }),
            ..TunnelConfig::disabled()
        };
        disabled
            .validate()
            .expect("disabled config always validates");
    }

    /// The transport tunables: derived defaults, explicit overrides, and the
    /// relationships that make a configuration unusable.
    #[test]
    fn transport_tunables_are_derived_then_overridable() {
        use std::time::Duration;

        let derived = TunnelNetworkConfig::default();
        assert_eq!(derived.keep_alive_interval(), Some(Duration::from_secs(5)));
        assert_eq!(
            derived.stale_after(Duration::from_secs(300)),
            Duration::from_secs(30)
        );
        assert_eq!(derived.udp_packet_size_bytes(), 65_507);
        assert_eq!(derived.max_concurrent_streams(256), 256);

        let explicit = TunnelNetworkConfig {
            heartbeat_interval_secs: 10,
            keep_alive_interval_secs: Some(2),
            heartbeat_timeout_secs: Some(45),
            udp_packet_size: Some(1_200),
            max_concurrent_streams: Some(64),
            ..TunnelNetworkConfig::default()
        };
        explicit.validate_network().expect("valid tunables");
        assert_eq!(explicit.keep_alive_interval(), Some(Duration::from_secs(2)));
        // The explicit timeout wins over `min(idle, heartbeat x 3)`.
        assert_eq!(
            explicit.stale_after(Duration::from_secs(300)),
            Duration::from_secs(45)
        );
        assert_eq!(explicit.udp_packet_size_bytes(), 1_200);
        assert_eq!(explicit.max_concurrent_streams(256), 64);

        // `0` disables keep-alive probing rather than meaning "no interval".
        let no_probe = TunnelNetworkConfig {
            keep_alive_interval_secs: Some(0),
            ..TunnelNetworkConfig::default()
        };
        no_probe.validate_network().expect("0 is a valid choice");
        assert_eq!(no_probe.keep_alive_interval(), None);
    }

    #[test]
    fn unusable_transport_tunables_are_configuration_errors() {
        // A heartbeat timeout below the heartbeat cadence expires the session
        // before its first heartbeat can arrive.
        let too_tight = TunnelNetworkConfig {
            heartbeat_interval_secs: 10,
            heartbeat_timeout_secs: Some(5),
            ..TunnelNetworkConfig::default()
        };
        let error = too_tight
            .validate_network()
            .expect_err("a timeout under one heartbeat interval is refused");
        assert!(
            error.to_string().contains("at least the heartbeat"),
            "{error}"
        );

        // A UDP ceiling outside the representable range is a unit mistake.
        for size in [1_u32, 511, 65_508] {
            let bad = TunnelNetworkConfig {
                udp_packet_size: Some(size),
                ..TunnelNetworkConfig::default()
            };
            assert!(
                bad.validate_network().is_err(),
                "udpPacketSize {size} must be refused"
            );
        }

        // A stream ceiling below the transport's floor would leave no room for
        // the control stream.
        for streams in [1_u32, 15, 65_537] {
            let bad = TunnelNetworkConfig {
                max_concurrent_streams: Some(streams),
                ..TunnelNetworkConfig::default()
            };
            assert!(
                bad.validate_network().is_err(),
                "maxConcurrentStreams {streams} must be refused"
            );
        }

        // An absurd keep-alive interval is refused; the range is generous on
        // purpose (probe every hour is a choice, probe every 2 hours is a typo).
        let bad = TunnelNetworkConfig {
            keep_alive_interval_secs: Some(7_200),
            ..TunnelNetworkConfig::default()
        };
        assert!(bad.validate_network().is_err());

        // And the tunnel section validates through it.
        let config = TunnelConfig {
            enabled: true,
            network: Some(too_tight),
            ..TunnelConfig::disabled()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn udp_template_resolves_a_datagram_target_and_requires_a_port() {
        let template = TunnelRouteTemplate {
            name: "dns".to_owned(),
            protocol: TunnelProtocolKind::Udp,
            domain: None,
            port: Some(7053),
            target: "127.0.0.1:53".to_owned(),
            policy: None,
            proxy_protocol: None,
            locations: Vec::new(),
            ..TunnelRouteTemplate::default()
        };
        template.validate().expect("valid udp template");
        assert_eq!(
            template.target().expect("target"),
            TunnelTarget::Udp("127.0.0.1:53".parse().expect("parses"))
        );
        let route = template
            .to_route(RouteId::parse("route_dns").expect("valid id"))
            .expect("route builds");
        assert_eq!(route.matcher, RouteMatcher::Port(7053));
        assert_eq!(route.protocol, TunnelProtocolKind::Udp);

        let portless = TunnelRouteTemplate {
            port: None,
            ..template
        };
        assert!(
            portless.validate().is_err(),
            "a udp template without a port can never be reached"
        );
    }

    #[test]
    fn a_template_name_must_be_usable_as_a_route_id() {
        // `to_route_id_and_matcher` derives `route_{name}` from the template,
        // so a name that cannot form a valid `RouteId` would produce a route
        // the registry then refuses. Validating the id here is what keeps
        // `validate()` and `to_route()` from disagreeing.
        let build = |name: &str| TunnelRouteTemplate {
            name: name.to_owned(),
            protocol: TunnelProtocolKind::Http,
            domain: Some("demo.sdkwork.link".to_owned()),
            port: None,
            target: "127.0.0.1:3000".to_owned(),
            policy: None,
            proxy_protocol: None,
            locations: Vec::new(),
            ..TunnelRouteTemplate::default()
        };
        let valid = build("home-web_2");
        valid.validate().expect("a DNS-label-shaped name is fine");
        assert_eq!(
            valid
                .to_route(RouteId::parse("route_home-web_2").expect("valid id"))
                .expect("route builds")
                .id
                .to_string(),
            "route_home-web_2"
        );

        for name in ["with space", "中文名", ""] {
            assert!(
                build(name).validate().is_err(),
                "name `{name}` cannot form a route id"
            );
        }
    }

    #[test]
    fn route_templates_are_only_validated_when_the_feature_is_enabled() {
        // PRD §77: `enabled = false` must leave the webserver byte-for-byte
        // unchanged, so a disabled section never inspects its templates. The
        // same (invalid) template must fail the moment the feature is on.
        let routes = vec![TunnelRouteTemplate {
            name: String::new(),
            protocol: TunnelProtocolKind::Http,
            domain: None,
            port: None,
            target: "not-a-socket-address".to_owned(),
            policy: None,
            proxy_protocol: None,
            locations: Vec::new(),
            ..TunnelRouteTemplate::default()
        }];
        let disabled = TunnelConfig {
            enabled: false,
            routes: Some(routes.clone()),
            ..TunnelConfig::disabled()
        };
        disabled
            .validate()
            .expect("a disabled section never validates its templates");

        let enabled = TunnelConfig {
            enabled: true,
            ..disabled
        };
        assert!(enabled.validate().is_err());
    }

    #[test]
    fn gateway_and_agent_sections_serialize_camel_case_wire_names() {
        // The CLI, server.toml and the standalone gateway all deserialize the
        // same JSON/TOML section, so the key spelling is a cross-component
        // contract and not an implementation detail.
        let config = TunnelConfig {
            enabled: true,
            gateway: Some(TunnelGatewayConfig::default()),
            agent: Some(TunnelAgentConfig::default()),
            ..TunnelConfig::disabled()
        };
        let json = serde_json::to_string(&config).expect("serialize");
        for key in [
            "\"domainSuffixes\"",
            "\"agentTokenEnv\"",
            "\"tlsCertPemEnv\"",
            "\"tlsKeyPemEnv\"",
            "\"tokenEnv\"",
            "\"deviceIdEnv\"",
            "\"deviceName\"",
            "\"domainSuffix\"",
            "\"routePortMin\"",
            "\"routePortMax\"",
        ] {
            assert!(json.contains(key), "missing {key} in {json}");
        }
        // snake_case spellings must never appear on the wire.
        assert!(!json.contains("domain_suffixes"), "{json}");
        assert!(!json.contains("token_env"), "{json}");
        assert!(!json.contains("route_port_min"), "{json}");
    }
}
