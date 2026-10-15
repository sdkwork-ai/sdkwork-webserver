//! Tunnel route: the core domain object (PRD §14–§17, §29).
//!
//! A route binds a public matcher (domain or gateway TCP port) to a target
//! that lives behind an agent, guarded by a policy. Domain models are not
//! wire messages: the protocol crate carries its own transfer types.

use std::collections::BTreeMap;
use std::net::{SocketAddr, SocketAddrV4, SocketAddrV6};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Result, TunnelError, ValidationField};
use crate::headers::RouteHeaderRewrite;
use crate::health::{validate_load_balancer_group, HealthCheckConfig};
use crate::ids::{RouteId, SessionId};
use crate::policy::RoutePolicy;
use crate::proxy_protocol::ProxyProtocolVersion;

/// Traffic kind a route relays (PRD §8 P0: HTTP and TCP only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TunnelProtocolKind {
    /// Host-based HTTP(S) relay; the gateway terminates visitor TLS.
    #[default]
    Http,
    /// Raw byte relay bound to a dedicated gateway TCP listener port.
    Tcp,
    /// Datagram relay bound to a dedicated gateway UDP listener port
    /// (FRP UDP proxy parity).
    Udp,
    /// Secret TCP (FRP `stcp`): **no public port is opened at all**. A visitor
    /// client authenticates with a shared secret key, asks for the route by
    /// name, and the gateway bridges that stream to the provider's local
    /// target. This is how a private service — SSH above all — is shared with
    /// selected people instead of the whole Internet.
    Stcp,
    /// Secret UDP (FRP `sudp`): the datagram form of [`Self::Stcp`].
    Sudp,
}

impl TunnelProtocolKind {
    /// The configuration/wire label.
    pub const fn wire_label(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Tcp => "tcp",
            Self::Udp => "udp",
            Self::Stcp => "stcp",
            Self::Sudp => "sudp",
        }
    }

    /// Parses the configuration/wire label.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "http" => Some(Self::Http),
            "tcp" => Some(Self::Tcp),
            "udp" => Some(Self::Udp),
            "stcp" => Some(Self::Stcp),
            "sudp" => Some(Self::Sudp),
            _ => None,
        }
    }

    /// True for the secret kinds, whose matcher is a name plus a shared key
    /// rather than a public surface.
    pub const fn is_secret(self) -> bool {
        matches!(self, Self::Stcp | Self::Sudp)
    }

    /// True for datagram kinds (no per-visitor selection, no PROXY protocol).
    pub const fn is_datagram(self) -> bool {
        matches!(self, Self::Udp | Self::Sudp)
    }
}

/// Where an agent delivers relayed traffic on its own network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelTarget {
    /// Dial this IPv4/IPv6 socket address on the agent host.
    Tcp(SocketAddr),
    /// Dial this UDP socket address on the agent host (datagram relay).
    Udp(SocketAddr),
    /// Dial this Unix domain socket path (POSIX agents only).
    UnixSocket(PathBuf),
}

impl TunnelTarget {
    /// Parses a `host:port` target string (PRD §36 request shape).
    pub fn parse_tcp(raw: &str) -> Result<Self> {
        Ok(Self::Tcp(Self::parse_address(raw)?))
    }

    /// Parses a UDP `host:port` target string.
    pub fn parse_udp(raw: &str) -> Result<Self> {
        Ok(Self::Udp(Self::parse_address(raw)?))
    }

    /// Parses a `unix:/absolute/path` target string (POSIX agents only).
    ///
    /// A Unix domain socket is a stream target, so only `http` and `tcp`
    /// routes may use it: a datagram route has no socket type to connect to,
    /// and a relative path would resolve against the agent's process working
    /// directory, which is not something the operator can predict.
    pub fn parse_unix_socket(raw: &str) -> Result<Self> {
        let path = PathBuf::from(raw);
        if raw.is_empty() {
            return Err(TunnelError::Validation {
                field: ValidationField::Target,
                reason: "unix socket target requires a path".to_owned(),
            });
        }
        if !Self::unix_socket_path_is_usable(&path) {
            return Err(TunnelError::Validation {
                field: ValidationField::Target,
                reason: format!("unix socket target `{raw}` must be a rooted absolute path"),
            });
        }
        Ok(Self::UnixSocket(path))
    }

    /// True when a Unix socket target path is usable: rooted (`/run/app.sock`),
    /// so it can never resolve against the agent's process working directory.
    ///
    /// Rooted rather than `Path::is_absolute` on purpose: a POSIX target must
    /// validate identically on a Windows gateway (which only relays the
    /// registration) and on the POSIX agent that dials it, and Windows does
    /// not call `/run/app.sock` absolute.
    pub fn unix_socket_path_is_usable(path: &std::path::Path) -> bool {
        !path.as_os_str().is_empty() && path.has_root()
    }

    /// Parses a target string for the relay protocol that will use it: UDP
    /// routes dial a datagram socket, every other kind dials a stream. Single
    /// entry point so agent templates and gateway registrations can never
    /// disagree about a route's target kind.
    ///
    /// The `unix:` prefix selects a Unix domain socket target on the stream
    /// planes; without a producer for it the variant was an unreachable
    /// capability (a template could never name one).
    pub fn parse_for_protocol(protocol: TunnelProtocolKind, raw: &str) -> Result<Self> {
        if let Some(path) = raw.strip_prefix("unix:") {
            if protocol == TunnelProtocolKind::Udp {
                return Err(TunnelError::Validation {
                    field: ValidationField::Target,
                    reason: "udp routes cannot target a unix domain socket".to_owned(),
                });
            }
            return Self::parse_unix_socket(path);
        }
        match protocol {
            TunnelProtocolKind::Udp | TunnelProtocolKind::Sudp => Self::parse_udp(raw),
            TunnelProtocolKind::Http | TunnelProtocolKind::Tcp | TunnelProtocolKind::Stcp => {
                Self::parse_tcp(raw)
            }
        }
    }

    fn parse_address(raw: &str) -> Result<SocketAddr> {
        raw.parse().map_err(|_| TunnelError::Validation {
            field: ValidationField::Target,
            reason: format!("`{raw}` is not an IP:port socket address"),
        })
    }
}

impl std::fmt::Display for TunnelTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tcp(addr) => write!(f, "{addr}"),
            Self::Udp(addr) => write!(f, "udp:{addr}"),
            Self::UnixSocket(path) => write!(f, "unix:{}", path.display()),
        }
    }
}

/// How a public request selects this route (PRD §16).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteMatcher {
    /// Exact host match, case-insensitive, no port suffix.
    Domain(String),
    /// Gateway TCP listener port match.
    Port(u16),
    /// Secret-route name (FRP `stcp`/`sudp` `serverName`): the name a visitor
    /// client asks for, together with the route's secret key. Nothing is
    /// reachable without knowing both.
    Secret(String),
}

/// Longest-prefix match of one request path against a route's locations
/// (FRP `locations` parity).
///
/// Matching is segment-aware: `/api` covers `/api` and `/api/v1` but never
/// `/apifoo`, which is the difference between a real path prefix and a string
/// prefix — a string prefix would hand `/apifoo` to the `/api` service.
/// An empty location list matches every path, so a route without locations
/// keeps the original "this host is mine" behaviour.
pub fn location_matches(locations: &[String], path: &str) -> Option<usize> {
    if locations.is_empty() {
        return Some(0);
    }
    let path = path.split(['?', '#']).next().unwrap_or(path);
    locations
        .iter()
        .filter(|location| {
            let location = location.trim_end_matches('/');
            if location.is_empty() {
                // A route that declares `/` owns the whole host.
                return true;
            }
            path == location
                || path
                    .strip_prefix(location)
                    .is_some_and(|rest| rest.starts_with('/'))
        })
        .map(String::len)
        .max()
}

/// Parses a bandwidth ceiling written the way operators write it — `"512KB"`,
/// `"2MB"`, `"1GB"`, or a plain integer meaning bytes per second — into
/// bytes per second.
///
/// Binary multipliers (1 KB = 1024 B) match what a tunnel operator expects
/// from a link ceiling, and the unit is mandatory in the string form: `"512"`
/// is 512 bytes per second, never a silently different magnitude.
pub fn parse_bandwidth_limit(raw: &str) -> Result<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(TunnelError::Validation {
            field: ValidationField::Config,
            reason: "bandwidth limit must not be empty".to_owned(),
        });
    }
    let upper = trimmed.to_ascii_uppercase();
    let (digits, multiplier) = [
        ("KIB", 1024_u64),
        ("MIB", 1024 * 1024),
        ("GIB", 1024 * 1024 * 1024),
        ("KB", 1024),
        ("MB", 1024 * 1024),
        ("GB", 1024 * 1024 * 1024),
        ("K", 1024),
        ("M", 1024 * 1024),
        ("G", 1024 * 1024 * 1024),
    ]
    .into_iter()
    .find_map(|(suffix, multiplier)| {
        upper
            .strip_suffix(suffix)
            .map(|digits| (digits.trim().to_owned(), multiplier))
    })
    .unwrap_or((upper.clone(), 1));
    let value: u64 = digits.parse().map_err(|_| TunnelError::Validation {
        field: ValidationField::Config,
        reason: format!("`{raw}` is not a bandwidth limit (expected e.g. 512KB, 2MB, or bytes/s)"),
    })?;
    let bytes = value
        .checked_mul(multiplier)
        .ok_or_else(|| TunnelError::Validation {
            field: ValidationField::Config,
            reason: format!("`{raw}` overflows a byte-per-second ceiling"),
        })?;
    if bytes == 0 {
        return Err(TunnelError::Validation {
            field: ValidationField::Config,
            reason: "bandwidth limit must be greater than zero (omit it for unlimited)".to_owned(),
        });
    }
    Ok(bytes)
}

/// Validates one location prefix: rooted, no scheme/host, no whitespace, and
/// no `..` segment (a traversal could otherwise reshape the match).
pub fn validate_location(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(TunnelError::Validation {
            field: ValidationField::Config,
            reason: "location must not be empty".to_owned(),
        });
    }
    if !trimmed.starts_with('/') {
        return Err(TunnelError::Validation {
            field: ValidationField::Config,
            reason: format!("location `{raw}` must start with `/`"),
        });
    }
    if trimmed.contains("//") || trimmed.contains(char::is_whitespace) {
        return Err(TunnelError::Validation {
            field: ValidationField::Config,
            reason: format!("location `{raw}` must not contain whitespace or `//`"),
        });
    }
    if trimmed.split('/').any(|segment| segment == "..") {
        return Err(TunnelError::Validation {
            field: ValidationField::Config,
            reason: format!("location `{raw}` must not contain `..`"),
        });
    }
    // One canonical spelling: no trailing slash except the root itself, so
    // `/api/` and `/api` can never be two different claims on the same host.
    let normalized = if trimmed == "/" {
        "/".to_owned()
    } else {
        trimmed.trim_end_matches('/').to_owned()
    };
    Ok(normalized)
}

impl RouteMatcher {
    /// Validates and builds a domain matcher: lowercased, no scheme, no
    /// port, no trailing dot, at least two DNS labels. The wildcard form
    /// `*.suffix` routes every subdomain of `suffix` (FRP subdomain-host
    /// parity); the bare `suffix` itself is not covered by a wildcard.
    pub fn domain(raw: &str) -> Result<Self> {
        let lowered = raw.trim().to_ascii_lowercase();
        let host = lowered
            .strip_suffix('.')
            .unwrap_or(lowered.as_str())
            .to_owned();
        if host.is_empty() || host.len() > 253 {
            return Err(TunnelError::Validation {
                field: ValidationField::Domain,
                reason: "domain length must be 1..=253".to_owned(),
            });
        }
        if host.contains("://") || host.contains('/') || host.contains(':') {
            return Err(TunnelError::Validation {
                field: ValidationField::Domain,
                reason: format!("`{raw}` must be a bare hostname without scheme or port"),
            });
        }
        // The wildcard label is syntax, not a DNS label; validate the rest.
        let hostname = host.strip_prefix("*.").unwrap_or(&host);
        let labels: Vec<&str> = hostname.split('.').collect();
        if labels.len() < 2
            || labels
                .iter()
                .any(|label| label.is_empty() || label.len() > 63 || *label == "*")
        {
            return Err(TunnelError::Validation {
                field: ValidationField::Domain,
                reason: format!("`{host}` is not a sequence of valid DNS labels"),
            });
        }
        Ok(Self::Domain(host))
    }

    /// The matched domain, when this is a domain matcher. Wildcard matchers
    /// keep their `*.suffix` form.
    pub fn as_domain(&self) -> Option<&str> {
        match self {
            Self::Domain(domain) => Some(domain),
            Self::Port(_) | Self::Secret(_) => None,
        }
    }

    /// True when this matcher is a wildcard domain (`*.suffix`).
    pub fn is_wildcard_domain(&self) -> bool {
        self.as_domain()
            .is_some_and(|domain| domain.starts_with("*."))
    }

    /// The suffix covered by a wildcard domain matcher (without `*.`).
    pub fn wildcard_suffix(&self) -> Option<&str> {
        self.as_domain()
            .and_then(|domain| domain.strip_prefix("*."))
    }

    /// The matched gateway port, when this is a port matcher.
    pub const fn as_port(&self) -> Option<u16> {
        match self {
            Self::Port(port) => Some(*port),
            Self::Domain(_) | Self::Secret(_) => None,
        }
    }

    /// The secret-route name, when this is a secret matcher.
    pub fn as_secret(&self) -> Option<&str> {
        match self {
            Self::Secret(name) => Some(name),
            Self::Domain(_) | Self::Port(_) => None,
        }
    }

    /// Validates and builds a secret-route name.
    ///
    /// The name is what a visitor asks for, so it is deliberately shaped like
    /// a route name (no whitespace, no path or port syntax) — a name that
    /// cannot be written down unambiguously cannot be requested reliably.
    pub fn secret(raw: &str) -> Result<Self> {
        let name = raw.trim();
        if name.is_empty() || name.len() > 128 {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: "secret route name must be 1..=128 characters".to_owned(),
            });
        }
        if name.contains(char::is_whitespace) || name.contains('/') || name.contains(':') {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: format!(
                    "secret route name `{raw}` must not contain whitespace, `/`, or `:`"
                ),
            });
        }
        Ok(Self::Secret(name.to_owned()))
    }
}

/// A tunnel route (PRD §14): identity, traffic kind, matcher, agent-local
/// target, and policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelRoute {
    /// Route identity.
    pub id: RouteId,
    /// Operator-facing name.
    pub name: String,
    /// Relayed traffic kind.
    pub protocol: TunnelProtocolKind,
    /// Public matcher.
    pub matcher: RouteMatcher,
    /// Agent-local delivery target.
    pub target: TunnelTarget,
    /// Access policy.
    pub policy: RoutePolicy,
    /// PROXY protocol version the agent prepends toward its local target
    /// (FRP `transport.proxyProtocolVersion` parity). `None` keeps the raw
    /// stream byte-for-byte, which is what every service expects by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_protocol: Option<ProxyProtocolVersion>,
    /// Path prefixes this route answers on its host (FRP `locations` parity).
    /// Empty means the route owns every path of the host.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<String>,
    /// Relay ceiling in bytes per second, applied to each direction of this
    /// route (FRP `transport.bandwidthLimit` parity). `None` is unlimited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth_limit_bytes_per_second: Option<u64>,
    /// Load-balancer group this route belongs to (FRP `loadBalancer.group`
    /// parity): routes sharing one group and one matcher are replicas of one
    /// service, and traffic is spread across the healthy ones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_balancer_group: Option<String>,
    /// Health probe the agent runs against its own target (FRP `healthCheck`
    /// parity). `None` means the route is always considered healthy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_check: Option<HealthCheckConfig>,
    /// HTTP header rewriting this route applies at the gateway (FRP
    /// `hostHeaderRewrite` + `requestHeaders.set` / `responseHeaders.set`).
    ///
    /// Empty for every route that rewrites nothing — which is also what an
    /// agent predating this capability registers, so the field is additive on
    /// the wire.
    #[serde(default, skip_serializing_if = "RouteHeaderRewrite::is_empty")]
    pub headers: RouteHeaderRewrite,
    /// Shared secret a visitor must present for a [`TunnelProtocolKind::Stcp`]
    /// or `Sudp` route (FRP `sk` parity).
    ///
    /// Never serialized: the key is verified by the gateway and must not leak
    /// through a route listing, a status endpoint, or a log line. Agents carry
    /// it in the registration message, which is a different type.
    #[serde(skip)]
    pub secret_key: Option<String>,
    /// Owning session (gateway-assigned; agents never set this).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// User this route belongs to (gateway-assigned from the authenticated
    /// token, FRP `user` parity).
    ///
    /// Gateway-assigned rather than agent-claimed: an agent's `user` is only a
    /// request to be treated as that user, and the token decides. Keeping it on
    /// the route is what lets a listing show which team owns a public surface
    /// and lets per-user quotas be counted without a second index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Registration instant (gateway-assigned).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    /// Free-form operator metadata (FRP `annotations`).
    ///
    /// Carried, not interpreted: the gateway stores and lists it so a console
    /// can group or filter by whatever the fleet agrees on (owner, ticket,
    /// environment) without a schema change per label. Bounded in size, and
    /// never used for routing or authorization.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, String>,
}

/// Maximum number of annotations one route may carry.
pub const MAX_ANNOTATIONS: usize = 16;
/// Maximum length of an annotation key.
pub const MAX_ANNOTATION_KEY_LEN: usize = 64;
/// Maximum length of an annotation value.
pub const MAX_ANNOTATION_VALUE_LEN: usize = 256;

/// Validates an annotation map.
///
/// Bounded, because these are stored per route and echoed in listings: an
/// unbounded map is a memory and payload amplifier an agent could pull for
/// free once authenticated.
pub fn validate_annotations(annotations: &BTreeMap<String, String>) -> Result<()> {
    if annotations.len() > MAX_ANNOTATIONS {
        return Err(TunnelError::Validation {
            field: ValidationField::Config,
            reason: format!("a route may carry at most {MAX_ANNOTATIONS} annotations"),
        });
    }
    for (key, value) in annotations {
        let key = key.trim();
        if key.is_empty() || key.len() > MAX_ANNOTATION_KEY_LEN {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: format!("annotation keys must be 1..={MAX_ANNOTATION_KEY_LEN} characters"),
            });
        }
        if !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
        {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: format!(
                    "annotation key `{key}` may only contain letters, digits, `-`, `_`, `.`, and `/`"
                ),
            });
        }
        if value.len() > MAX_ANNOTATION_VALUE_LEN {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: format!(
                    "annotation `{key}` must be at most {MAX_ANNOTATION_VALUE_LEN} characters"
                ),
            });
        }
    }
    Ok(())
}

impl TunnelRoute {
    /// Validates and builds a route without ownership metadata.
    pub fn new(
        id: RouteId,
        name: impl Into<String>,
        protocol: TunnelProtocolKind,
        matcher: RouteMatcher,
        target: TunnelTarget,
        policy: RoutePolicy,
    ) -> Result<Self> {
        let name = name.into();
        if name.is_empty() || name.len() > 128 {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: "route name must be 1..=128 characters".to_owned(),
            });
        }
        if let TunnelTarget::Tcp(addr) = &target {
            if addr.port() == 0 {
                return Err(TunnelError::Validation {
                    field: ValidationField::Target,
                    reason: "target port 0 is not routable".to_owned(),
                });
            }
        }
        // A route built directly (not through `parse_for_protocol`) must obey
        // the same target rules, otherwise a hand-built route could carry a
        // Unix socket target on a datagram route and fail only at relay time.
        if let TunnelTarget::UnixSocket(path) = &target {
            if protocol == TunnelProtocolKind::Udp {
                return Err(TunnelError::Validation {
                    field: ValidationField::Target,
                    reason: "udp routes cannot target a unix domain socket".to_owned(),
                });
            }
            if !TunnelTarget::unix_socket_path_is_usable(path) {
                return Err(TunnelError::Validation {
                    field: ValidationField::Target,
                    reason: format!(
                        "unix socket target `{}` must be a rooted absolute path",
                        path.display()
                    ),
                });
            }
        }
        if protocol == TunnelProtocolKind::Tcp && matcher.as_port().is_none() {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: "tcp routes must match a gateway port".to_owned(),
            });
        }
        if protocol == TunnelProtocolKind::Http && matcher.as_domain().is_none() {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: "http routes must match a domain".to_owned(),
            });
        }
        if protocol == TunnelProtocolKind::Udp && matcher.as_port().is_none() {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: "udp routes must match a gateway port".to_owned(),
            });
        }
        // Secret routes are reached by name, never by a public surface; the
        // two matcher families must not be mixed, or a stcp route would sit
        // unreachable behind a host/port nothing ever asks for.
        if protocol.is_secret() && matcher.as_secret().is_none() {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: format!("{} routes must match a secret name", protocol.wire_label()),
            });
        }
        if !protocol.is_secret() && matcher.as_secret().is_some() {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: format!(
                    "{} routes cannot match a secret name; use stcp or sudp",
                    protocol.wire_label()
                ),
            });
        }
        // A stream secret route cannot target a datagram socket and vice
        // versa: the target kind follows the protocol everywhere else, and a
        // mismatch here would only fail at relay time.
        match (protocol, &target) {
            (TunnelProtocolKind::Stcp, TunnelTarget::Udp(_)) => {
                return Err(TunnelError::Validation {
                    field: ValidationField::Target,
                    reason: "an stcp route must target a stream socket".to_owned(),
                })
            }
            (TunnelProtocolKind::Sudp, TunnelTarget::Tcp(_))
            | (TunnelProtocolKind::Sudp, TunnelTarget::UnixSocket(_)) => {
                return Err(TunnelError::Validation {
                    field: ValidationField::Target,
                    reason: "a sudp route must target a udp socket".to_owned(),
                })
            }
            _ => {}
        }
        // A bearer token can only travel in an HTTP `Authorization` header.
        // Rejecting the combination here keeps a TCP/UDP route from being
        // registered into permanent unreachability.
        if !policy.auth.is_satisfiable_on(protocol) {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: format!(
                    "{protocol:?} routes cannot require bearer visitor authentication; \
                     use allowed_ips to restrict access"
                ),
            });
        }
        // A bearer route with no tokens is unreachable for the same reason:
        // every visitor would be rejected, and the operator would see a
        // registered route instead of a configuration error.
        if policy.bearer_without_tokens() {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: "a bearer_token route requires at least one visitor token".to_owned(),
            });
        }
        Ok(Self {
            id,
            name,
            protocol,
            matcher,
            target,
            policy,
            proxy_protocol: None,
            locations: Vec::new(),
            bandwidth_limit_bytes_per_second: None,
            load_balancer_group: None,
            health_check: None,
            headers: RouteHeaderRewrite::default(),
            secret_key: None,
            session_id: None,
            user: None,
            created_at: None,
            annotations: BTreeMap::new(),
        })
    }

    /// Attaches the path prefixes this route answers on its host, normalizing
    /// and de-duplicating them.
    ///
    /// Locations are only meaningful on a domain matcher: a raw TCP/UDP route
    /// has no request path to match, so declaring one there is a
    /// configuration error rather than a silently ignored field.
    pub fn with_locations(mut self, locations: Vec<String>) -> Result<Self> {
        if locations.is_empty() {
            return Ok(self);
        }
        if self.matcher.as_domain().is_none() {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: "locations require a domain route (a port route has no request path)"
                    .to_owned(),
            });
        }
        let mut normalized = Vec::with_capacity(locations.len());
        for raw in &locations {
            let location = validate_location(raw)?;
            if !normalized.contains(&location) {
                normalized.push(location);
            }
        }
        self.locations = normalized;
        Ok(self)
    }

    /// The longest location prefix this route declares that covers `path`, or
    /// `None` when the route does not serve that path.
    pub fn matches_path(&self, path: &str) -> Option<usize> {
        location_matches(&self.locations, path)
    }

    /// Attaches the shared secret a visitor must present for a secret route
    /// (FRP `sk` parity).
    ///
    /// The key is refused on a public route rather than ignored: an operator
    /// who wrote one there believes access is restricted, and silently
    /// publishing the service instead is the worst possible outcome.
    pub fn with_secret_key(mut self, secret_key: Option<String>) -> Result<Self> {
        match (self.protocol.is_secret(), secret_key) {
            (true, Some(key)) => {
                let key = key.trim().to_owned();
                if key.len() < 8 || key.len() > 128 {
                    return Err(TunnelError::Validation {
                        field: ValidationField::Config,
                        reason: "a secret route key must be 8..=128 characters".to_owned(),
                    });
                }
                self.secret_key = Some(key);
            }
            (true, None) => {
                return Err(TunnelError::Validation {
                    field: ValidationField::Config,
                    reason: format!(
                        "{} routes require a secret key (FRP `sk`)",
                        self.protocol.wire_label()
                    ),
                })
            }
            (false, Some(_)) => {
                return Err(TunnelError::Validation {
                    field: ValidationField::Config,
                    reason: format!(
                        "{} routes are public surfaces and cannot carry a secret key",
                        self.protocol.wire_label()
                    ),
                })
            }
            (false, None) => {}
        }
        Ok(self)
    }

    /// Attaches the HTTP header rewriting this route applies at the gateway
    /// (FRP `hostHeaderRewrite` + `requestHeaders.set` / `responseHeaders.set`).
    ///
    /// Refused on a plane that carries no headers, and validated pair by pair:
    /// a policy that only fails once a request arrives would turn a
    /// configuration mistake into a runtime failure on live traffic.
    pub fn with_headers(mut self, headers: RouteHeaderRewrite) -> Result<Self> {
        headers.validate()?;
        headers.validate_for_protocol(self.protocol)?;
        self.headers = headers;
        Ok(self)
    }

    /// True when `candidate` is this route's secret key.
    ///
    /// Constant-time: a visitor client can retry with guessed keys, and a
    /// byte-wise early exit would leak the key one byte at a time.
    pub fn secret_key_matches(&self, candidate: &str) -> bool {
        match self.secret_key.as_deref() {
            Some(key) => sdkwork_utils_rust::crypto::secure_compare(key, candidate),
            None => false,
        }
    }

    /// Attaches a per-direction bandwidth ceiling.
    ///
    /// A zero ceiling is refused rather than treated as "unlimited": the two
    /// readings are exact opposites, so a `0` that silently meant "no traffic"
    /// (or "no limit") would be the kind of ambiguity that shows up as an
    /// outage. Omit the field for unlimited.
    pub fn with_bandwidth_limit(mut self, bytes_per_second: Option<u64>) -> Result<Self> {
        if bytes_per_second == Some(0) {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: "bandwidth limit must be greater than zero (omit it for unlimited)"
                    .to_owned(),
            });
        }
        self.bandwidth_limit_bytes_per_second = bytes_per_second;
        Ok(self)
    }

    /// Attaches the load-balancer group and health probe (FRP
    /// `loadBalancer`/`healthCheck` parity).
    ///
    /// A health probe is refused on a datagram route: FRP probes TCP and HTTP
    /// only, and a UDP "probe" would have to invent a request the local
    /// service never agreed to answer.
    pub fn with_load_balancing(
        mut self,
        group: Option<String>,
        health_check: Option<HealthCheckConfig>,
    ) -> Result<Self> {
        if let Some(group) = group.as_deref() {
            if self.protocol == TunnelProtocolKind::Udp {
                return Err(TunnelError::Validation {
                    field: ValidationField::Config,
                    reason: format!(
                        "udp routes cannot join load balancer group `{group}`: a datagram port has no per-visitor selection"
                    ),
                });
            }
            self.load_balancer_group = Some(validate_load_balancer_group(group)?);
        }
        if let Some(health_check) = &health_check {
            health_check.validate()?;
            if self.protocol == TunnelProtocolKind::Udp {
                return Err(TunnelError::Validation {
                    field: ValidationField::Config,
                    reason: "udp routes cannot carry a health check (FRP probes tcp and http)"
                        .to_owned(),
                });
            }
        }
        self.health_check = health_check;
        Ok(self)
    }

    /// Attaches a PROXY-protocol declaration, validating that the route can
    /// actually carry one.
    ///
    /// A datagram route has no connection to prefix, and a Unix socket target
    /// has no address pair to describe, so both are refused here — the
    /// operator sees a configuration error instead of a service that receives
    /// a header it never asked for, or silence where it expected one.
    pub fn with_proxy_protocol(mut self, version: Option<ProxyProtocolVersion>) -> Result<Self> {
        if let Some(version) = version {
            if self.protocol == TunnelProtocolKind::Udp {
                return Err(TunnelError::Validation {
                    field: ValidationField::Config,
                    reason: format!(
                        "udp routes cannot emit a {} PROXY protocol header (datagrams have no connection)",
                        version.label()
                    ),
                });
            }
            if matches!(self.target, TunnelTarget::UnixSocket(_)) {
                return Err(TunnelError::Validation {
                    field: ValidationField::Config,
                    reason: "a unix socket target cannot carry a PROXY protocol header".to_owned(),
                });
            }
        }
        self.proxy_protocol = version;
        Ok(self)
    }

    /// Attaches gateway-assigned ownership metadata.
    pub fn with_session(mut self, session_id: SessionId, now: DateTime<Utc>) -> Self {
        self.session_id = Some(session_id);
        self.created_at = Some(now);
        self
    }

    /// Attaches operator metadata (FRP `annotations`).
    ///
    /// Validated here rather than at the gateway: an oversized or malformed map
    /// is the agent's configuration error, and failing at construction keeps
    /// the wire from ever carrying it.
    pub fn with_annotations(mut self, annotations: BTreeMap<String, String>) -> Result<Self> {
        validate_annotations(&annotations)?;
        self.annotations = annotations;
        Ok(self)
    }

    /// Attaches the user this route belongs to.
    ///
    /// Gateway-assigned: the value comes from the token the session
    /// authenticated with, never from the registration itself, so an agent
    /// cannot relabel its routes into someone else's quota.
    #[must_use]
    pub fn with_user(mut self, user: Option<String>) -> Self {
        self.user = user
            .map(|user| user.trim().to_owned())
            .filter(|user| !user.is_empty());
        self
    }
}

/// The user-namespaced form of a route name.
///
/// One spelling everywhere, because a team's route id must not be able to
/// disagree with itself: `team-a` + `web` is `team-a_web` in the id the agent
/// builds, the id the gateway checks, and the id the data plane carries.
///
/// `_` rather than a dot: route ids accept only `[A-Za-z0-9_-]`, and widening
/// that charset to admit a separator would put a new character into every id
/// the registry, the wire, and the admin API already handle.
#[must_use]
pub fn namespaced_name(user: Option<&str>, name: &str) -> String {
    match user.map(str::trim).filter(|user| !user.is_empty()) {
        Some(user) => format!("{user}_{name}"),
        None => name.to_owned(),
    }
}

/// Derives the route id a template gets inside a user's namespace.
///
/// Shared by the agent (which builds the registration) and the gateway (which
/// checks it): a registration whose id was not derived this way is refused, so
/// a machine cannot claim another team's id space.
pub fn derived_route_id(user: Option<&str>, name: &str) -> Result<RouteId> {
    RouteId::parse(format!("route_{}", namespaced_name(user, name)))
}

/// Renders an `https://` URL for a domain route, the public preview surface
/// (PRD §34, §85).
pub fn public_url(matcher: &RouteMatcher) -> Option<String> {
    matcher
        .as_domain()
        .map(|domain| format!("https://{domain}"))
}

/// Convenience constructor for IPv4 loopback targets used in tests and local
/// development exposure.
pub fn local_target(port: u16) -> TunnelTarget {
    TunnelTarget::Tcp(SocketAddr::V4(SocketAddrV4::new(
        std::net::Ipv4Addr::LOCALHOST,
        port,
    )))
}

/// Convenience constructor for a local UDP target (datagram relay).
pub fn local_udp_target(port: u16) -> TunnelTarget {
    TunnelTarget::Udp(SocketAddr::V4(SocketAddrV4::new(
        std::net::Ipv4Addr::LOCALHOST,
        port,
    )))
}

/// Convenience constructor for IPv6 loopback targets.
pub fn local_target_v6(port: u16) -> TunnelTarget {
    TunnelTarget::Tcp(SocketAddr::V6(SocketAddrV6::new(
        std::net::Ipv6Addr::LOCALHOST,
        port,
        0,
        0,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::AuthPolicy;

    #[test]
    fn domain_matcher_normalizes_and_validates() {
        let matcher = RouteMatcher::domain("Demo.SDKWORK.link.").expect("valid domain");
        assert_eq!(matcher.as_domain(), Some("demo.sdkwork.link"));
        assert!(RouteMatcher::domain("https://demo.sdkwork.link").is_err());
        assert!(RouteMatcher::domain("demo:8443").is_err());
        assert!(RouteMatcher::domain("localhost").is_err());
    }

    #[test]
    fn wildcard_domain_matcher_normalizes_and_validates() {
        let matcher = RouteMatcher::domain("*.App.SDKWORK.link.").expect("valid wildcard");
        assert_eq!(matcher.as_domain(), Some("*.app.sdkwork.link"));
        assert!(matcher.is_wildcard_domain());
        assert_eq!(matcher.wildcard_suffix(), Some("app.sdkwork.link"));
        let exact = RouteMatcher::domain("app.sdkwork.link").expect("valid exact");
        assert!(!exact.is_wildcard_domain());
        assert_eq!(exact.wildcard_suffix(), None);
        assert!(RouteMatcher::domain("*").is_err());
        assert!(RouteMatcher::domain("*.").is_err());
        assert!(RouteMatcher::domain("*.localhost").is_err());
        assert!(RouteMatcher::domain("bad.*.sdkwork.link").is_err());
    }

    #[test]
    fn http_route_requires_domain_matcher() {
        let route = TunnelRoute::new(
            RouteId::parse("route_1").expect("valid id"),
            "web",
            TunnelProtocolKind::Http,
            RouteMatcher::Port(7000),
            local_target(3000),
            RoutePolicy::private(),
        );
        assert!(route.is_err());
    }

    #[test]
    fn tcp_route_requires_port_matcher() {
        let route = TunnelRoute::new(
            RouteId::parse("route_2").expect("valid id"),
            "ssh",
            TunnelProtocolKind::Tcp,
            RouteMatcher::domain("ssh.example.com").expect("valid domain"),
            local_target(22),
            RoutePolicy::private(),
        );
        assert!(route.is_err());
    }

    #[test]
    fn valid_routes_round_trip_through_json() {
        let route = TunnelRoute::new(
            RouteId::parse("route_3").expect("valid id"),
            "local-web",
            TunnelProtocolKind::Http,
            RouteMatcher::domain("demo.sdkwork.link").expect("valid domain"),
            local_target(3000),
            RoutePolicy {
                allow_public: true,
                ..RoutePolicy::private()
            },
        )
        .expect("valid route");
        let json = serde_json::to_string(&route).expect("serialize");
        let decoded: TunnelRoute = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, route);
    }

    #[test]
    fn public_url_renders_https() {
        let matcher = RouteMatcher::domain("a8f3k2.sdkwork.link").expect("valid domain");
        assert_eq!(
            public_url(&matcher).as_deref(),
            Some("https://a8f3k2.sdkwork.link")
        );
        assert!(public_url(&RouteMatcher::Port(7000)).is_none());
    }

    #[test]
    fn udp_route_requires_port_matcher() {
        // A `udp` route is addressed by a gateway datagram port; a domain
        // matcher would leave it permanently unreachable on that plane.
        let route = TunnelRoute::new(
            RouteId::parse("route_dns").expect("valid id"),
            "dns",
            TunnelProtocolKind::Udp,
            RouteMatcher::domain("dns.sdkwork.link").expect("valid domain"),
            local_udp_target(53),
            RoutePolicy::private(),
        );
        assert!(route.is_err(), "udp routes must match a gateway port");
    }

    #[test]
    fn bearer_visitor_auth_is_rejected_on_tcp_and_udp_routes() {
        // docs/tunnel.md § security: a bearer token can only travel in an
        // HTTP `Authorization` header, so a TCP/UDP route demanding one would
        // reject every visitor forever. The combination must fail loudly at
        // construction time instead of registering into silent unreachability.
        let policy = RoutePolicy {
            auth: AuthPolicy::BearerToken,
            visitor_tokens: vec!["tok".to_owned()],
            ..RoutePolicy::private()
        };

        let tcp = TunnelRoute::new(
            RouteId::parse("route_ssh").expect("valid id"),
            "ssh",
            TunnelProtocolKind::Tcp,
            RouteMatcher::Port(7022),
            local_target(22),
            policy.clone(),
        );
        assert!(
            matches!(tcp, Err(TunnelError::Validation { .. })),
            "tcp + bearer must be rejected, got {tcp:?}"
        );

        let udp = TunnelRoute::new(
            RouteId::parse("route_dns").expect("valid id"),
            "dns",
            TunnelProtocolKind::Udp,
            RouteMatcher::Port(7053),
            local_udp_target(53),
            policy.clone(),
        );
        assert!(
            matches!(udp, Err(TunnelError::Validation { .. })),
            "udp + bearer must be rejected, got {udp:?}"
        );

        // The identical policy is legitimate on the HTTP plane.
        assert!(TunnelRoute::new(
            RouteId::parse("route_web").expect("valid id"),
            "web",
            TunnelProtocolKind::Http,
            RouteMatcher::domain("demo.sdkwork.link").expect("valid domain"),
            local_target(3000),
            policy,
        )
        .is_ok());
    }

    #[test]
    fn zero_port_target_is_rejected() {
        let route = TunnelRoute::new(
            RouteId::parse("route_ssh").expect("valid id"),
            "ssh",
            TunnelProtocolKind::Tcp,
            RouteMatcher::Port(7022),
            TunnelTarget::Tcp("127.0.0.1:0".parse().expect("parses")),
            RoutePolicy::private(),
        );
        assert!(route.is_err(), "a target port of 0 is not routable");
    }

    #[test]
    fn bearer_route_without_visitor_tokens_is_rejected() {
        // `auth = bearer_token` with an empty token list rejects every
        // visitor, so the route would register into permanent
        // unreachability. Fail at construction instead: the operator sees a
        // configuration error, not a route that never answers.
        let route = TunnelRoute::new(
            RouteId::parse("route_web").expect("valid id"),
            "web",
            TunnelProtocolKind::Http,
            RouteMatcher::domain("demo.sdkwork.link").expect("valid domain"),
            local_target(3000),
            RoutePolicy {
                auth: AuthPolicy::BearerToken,
                ..RoutePolicy::private()
            },
        );
        assert!(
            matches!(route, Err(TunnelError::Validation { .. })),
            "bearer_token without tokens must be rejected, got {route:?}"
        );

        // The same policy with one token is the usable shape.
        assert!(TunnelRoute::new(
            RouteId::parse("route_web").expect("valid id"),
            "web",
            TunnelProtocolKind::Http,
            RouteMatcher::domain("demo.sdkwork.link").expect("valid domain"),
            local_target(3000),
            RoutePolicy {
                auth: AuthPolicy::BearerToken,
                visitor_tokens: vec!["t0ken".to_owned()],
                ..RoutePolicy::private()
            },
        )
        .is_ok());
    }

    #[test]
    fn unix_socket_targets_parse_only_for_stream_planes() {
        // `unix:` targets used to be an enum variant with no producer: no
        // template could name one and no test covered it. It is now a real
        // stream-plane target.
        let parsed =
            TunnelTarget::parse_for_protocol(TunnelProtocolKind::Tcp, "unix:/run/app.sock")
                .expect("tcp may target a unix socket");
        assert_eq!(
            parsed,
            TunnelTarget::UnixSocket(PathBuf::from("/run/app.sock"))
        );
        assert_eq!(parsed.to_string(), "unix:/run/app.sock");
        assert!(matches!(
            TunnelTarget::parse_for_protocol(TunnelProtocolKind::Http, "unix:/run/app.sock"),
            Ok(TunnelTarget::UnixSocket(_))
        ));

        // The datagram plane has no socket type to connect to.
        assert!(
            TunnelTarget::parse_for_protocol(TunnelProtocolKind::Udp, "unix:/run/app.sock")
                .is_err()
        );
        // A relative path would resolve against the agent's working directory.
        assert!(
            TunnelTarget::parse_for_protocol(TunnelProtocolKind::Tcp, "unix:run/app.sock").is_err()
        );
        assert!(TunnelTarget::parse_for_protocol(TunnelProtocolKind::Tcp, "unix:").is_err());
    }

    #[test]
    fn a_hand_built_udp_route_cannot_target_a_unix_socket() {
        let route = TunnelRoute::new(
            RouteId::parse("route_dns").expect("valid id"),
            "dns",
            TunnelProtocolKind::Udp,
            RouteMatcher::Port(7053),
            TunnelTarget::UnixSocket(PathBuf::from("/run/app.sock")),
            RoutePolicy::private(),
        );
        assert!(
            matches!(route, Err(TunnelError::Validation { .. })),
            "a datagram route must not carry a stream socket target, got {route:?}"
        );
    }

    #[test]
    fn a_template_can_declare_a_unix_socket_target() {
        // The agent template is the producer the variant was missing.
        use crate::TunnelRouteTemplate;
        let template = TunnelRouteTemplate {
            name: "docker".to_owned(),
            protocol: TunnelProtocolKind::Tcp,
            domain: None,
            port: Some(7237),
            target: "unix:/var/run/docker.sock".to_owned(),
            policy: None,
            proxy_protocol: None,
            locations: Vec::new(),
            ..TunnelRouteTemplate::default()
        };
        template.validate().expect("template validates");
        assert_eq!(
            template.target().expect("target resolves"),
            TunnelTarget::UnixSocket(PathBuf::from("/var/run/docker.sock"))
        );
        assert_eq!(
            template.matcher().expect("matcher resolves"),
            RouteMatcher::Port(7237)
        );
    }

    #[test]
    fn locations_match_segment_wise_and_longest_prefix_wins() {
        // Segment-aware: `/api` covers `/api` and `/api/v1`, never `/apifoo`.
        // A string prefix would hand `/apifoo` to the `/api` service.
        let locations = vec!["/api".to_owned(), "/api/v1".to_owned()];
        assert_eq!(location_matches(&locations, "/api"), Some(4));
        assert_eq!(location_matches(&locations, "/api/v1/users"), Some(7));
        assert_eq!(location_matches(&locations, "/apifoo"), None);
        assert_eq!(location_matches(&locations, "/"), None);
        // Query strings and fragments are not part of the path.
        assert_eq!(location_matches(&locations, "/api?x=1"), Some(4));

        // An empty set is the whole host.
        assert_eq!(location_matches(&[], "/anything"), Some(0));
        // `/` claims the whole host too, and outranks nothing but matches all.
        assert_eq!(location_matches(&["/".to_owned()], "/anything"), Some(1));
    }

    #[test]
    fn location_validation_normalizes_and_refuses_unusable_prefixes() {
        assert_eq!(validate_location("/api/").expect("normalizes"), "/api");
        assert_eq!(validate_location(" /api ").expect("trims"), "/api");
        assert_eq!(validate_location("/").expect("root stays root"), "/");
        for bad in ["", "api", "//api", "/a b", "/a/../b"] {
            assert!(
                validate_location(bad).is_err(),
                "`{bad}` must be refused as a location"
            );
        }
    }

    #[test]
    fn with_locations_normalizes_deduplicates_and_requires_a_domain() {
        let route = TunnelRoute::new(
            RouteId::parse("route_api").expect("valid id"),
            "api",
            TunnelProtocolKind::Http,
            RouteMatcher::domain("demo.sdkwork.link").expect("valid domain"),
            local_target(3000),
            RoutePolicy::private(),
        )
        .expect("valid route")
        .with_locations(vec![
            "/api/".to_owned(),
            "/api".to_owned(),
            "/admin".to_owned(),
        ])
        .expect("locations attach");
        assert_eq!(
            route.locations,
            vec!["/api".to_owned(), "/admin".to_owned()]
        );
        assert_eq!(route.matches_path("/api/v2"), Some(4));
        assert_eq!(route.matches_path("/other"), None);

        // A port route has no request path, so a location claim on it is a
        // configuration error rather than a silently ignored field.
        let tcp = TunnelRoute::new(
            RouteId::parse("route_ssh").expect("valid id"),
            "ssh",
            TunnelProtocolKind::Tcp,
            RouteMatcher::Port(7022),
            local_target(22),
            RoutePolicy::private(),
        )
        .expect("valid route");
        assert!(tcp.with_locations(vec!["/api".to_owned()]).is_err());
    }

    #[test]
    fn bandwidth_limits_parse_sizes_and_bare_byte_rates() {
        assert_eq!(parse_bandwidth_limit("512KB").expect("kb"), 512 * 1024);
        assert_eq!(parse_bandwidth_limit("2MB").expect("mb"), 2 * 1024 * 1024);
        assert_eq!(
            parse_bandwidth_limit("1GB").expect("gb"),
            1024 * 1024 * 1024
        );
        assert_eq!(parse_bandwidth_limit(" 8k ").expect("trimmed"), 8 * 1024);
        // A bare number is bytes per second, never a silently larger unit.
        assert_eq!(parse_bandwidth_limit("4096").expect("bytes"), 4096);
        for bad in ["", "0", "fast", "1XB", "-1"] {
            assert!(
                parse_bandwidth_limit(bad).is_err(),
                "`{bad}` must not parse as a bandwidth limit"
            );
        }
        // Overflow is refused instead of wrapping into a tiny ceiling.
        assert!(parse_bandwidth_limit("18446744073709551615GB").is_err());
    }

    #[test]
    fn a_zero_bandwidth_limit_is_refused_on_a_route() {
        let route = TunnelRoute::new(
            RouteId::parse("route_metered").expect("valid id"),
            "metered",
            TunnelProtocolKind::Tcp,
            RouteMatcher::Port(7022),
            local_target(22),
            RoutePolicy::private(),
        )
        .expect("valid route");
        assert!(route.clone().with_bandwidth_limit(Some(0)).is_err());
        let limited = route
            .with_bandwidth_limit(Some(64 * 1024))
            .expect("limit attaches");
        assert_eq!(limited.bandwidth_limit_bytes_per_second, Some(64 * 1024));
    }

    #[test]
    fn load_balancing_attaches_only_where_it_can_work() {
        let http = TunnelRoute::new(
            RouteId::parse("route_web").expect("valid id"),
            "web",
            TunnelProtocolKind::Http,
            RouteMatcher::domain("lb.sdkwork.link").expect("valid domain"),
            local_target(3000),
            RoutePolicy::private(),
        )
        .expect("valid route");

        let probe = crate::health::HealthCheckConfig {
            kind: crate::health::HealthCheckKind::Http,
            path: Some("/health".to_owned()),
            ..crate::health::HealthCheckConfig::default()
        };
        let grouped = http
            .clone()
            .with_load_balancing(Some("web".to_owned()), Some(probe.clone()))
            .expect("group and probe attach");
        assert_eq!(grouped.load_balancer_group.as_deref(), Some("web"));
        assert_eq!(grouped.health_check, Some(probe));

        // An unusable group name is refused at construction.
        assert!(http
            .clone()
            .with_load_balancing(Some("  ".to_owned()), None)
            .is_err());
        // An unusable probe is refused too.
        assert!(http
            .with_load_balancing(
                None,
                Some(crate::health::HealthCheckConfig {
                    kind: crate::health::HealthCheckKind::Tcp,
                    path: Some("/health".to_owned()),
                    ..crate::health::HealthCheckConfig::default()
                })
            )
            .is_err());

        // A datagram route has no per-visitor selection and no probeable
        // connection, so neither a group nor a probe may be attached.
        let udp = TunnelRoute::new(
            RouteId::parse("route_dns").expect("valid id"),
            "dns",
            TunnelProtocolKind::Udp,
            RouteMatcher::Port(7053),
            local_udp_target(53),
            RoutePolicy::private(),
        )
        .expect("valid route");
        assert!(udp
            .clone()
            .with_load_balancing(Some("dns".to_owned()), None)
            .is_err());
        assert!(udp
            .with_load_balancing(None, Some(crate::health::HealthCheckConfig::default()))
            .is_err());
    }

    #[test]
    fn a_secret_route_needs_a_name_and_a_key_and_no_public_surface() {
        let target = local_target(22);
        let build = |matcher: RouteMatcher| {
            TunnelRoute::new(
                RouteId::parse("route_private_ssh").expect("valid id"),
                "private-ssh",
                TunnelProtocolKind::Stcp,
                matcher,
                target.clone(),
                RoutePolicy::private(),
            )
        };

        // The happy path: a name matcher plus a key of usable length.
        let route = build(RouteMatcher::secret("private-ssh").expect("valid name"))
            .expect("secret route builds")
            .with_secret_key(Some("s3cret-key-123".to_owned()))
            .expect("key attaches");
        assert!(route.secret_key_matches("s3cret-key-123"));
        assert!(!route.secret_key_matches("s3cret-key-124"));
        assert!(!route.secret_key_matches(""));
        assert!(route.matcher.as_domain().is_none(), "no public host");
        assert!(route.matcher.as_port().is_none(), "no public port");

        // A secret route must not claim a host or a port: that would promise a
        // surface the gateway never binds.
        let as_domain = build(RouteMatcher::domain("ssh.tunnel.test").expect("domain"))
            .expect_err("stcp cannot match a domain");
        assert!(format!("{as_domain}").contains("secret name"));
        let as_port = build(RouteMatcher::Port(7022)).expect_err("stcp cannot match a port");
        assert!(format!("{as_port}").contains("secret name"));

        // A public route must not borrow the secret matcher either.
        let public_secret = TunnelRoute::new(
            RouteId::parse("route_web").expect("valid id"),
            "web",
            TunnelProtocolKind::Tcp,
            RouteMatcher::secret("private-ssh").expect("valid name"),
            target.clone(),
            RoutePolicy::private(),
        )
        .expect_err("a public route cannot match a secret name");
        // Either refusal is correct and both are specific: a `tcp` route needs
        // a port before the matcher family is even considered.
        let refusal = format!("{public_secret}");
        assert!(
            refusal.contains("use stcp or sudp") || refusal.contains("gateway port"),
            "unexpected refusal: {refusal}"
        );

        // Keys are validated where they are attached.
        let short = build(RouteMatcher::secret("private-ssh").expect("valid name"))
            .expect("builds")
            .with_secret_key(Some("short".to_owned()))
            .expect_err("a 5-character key is refused");
        assert!(format!("{short}").contains("8..=128"));
        let missing = build(RouteMatcher::secret("private-ssh").expect("valid name"))
            .expect("builds")
            .with_secret_key(None)
            .expect_err("a secret route without a key is unreachable");
        assert!(format!("{missing}").contains("require a secret key"));

        // And a public route may not carry one at all: the operator who wrote
        // it there believes access is restricted.
        let public_key = TunnelRoute::new(
            RouteId::parse("route_web").expect("valid id"),
            "web",
            TunnelProtocolKind::Http,
            RouteMatcher::domain("web.tunnel.test").expect("valid domain"),
            local_target(3000),
            RoutePolicy::private(),
        )
        .expect("builds")
        .with_secret_key(Some("s3cret-key-123".to_owned()))
        .expect_err("a public route must not carry a secret key");
        assert!(format!("{public_key}").contains("public surfaces"));

        // A stream secret route cannot target a datagram socket.
        let datagram = TunnelRoute::new(
            RouteId::parse("route_private_dns").expect("valid id"),
            "private-dns",
            TunnelProtocolKind::Stcp,
            RouteMatcher::secret("private-dns").expect("valid name"),
            TunnelTarget::Udp("127.0.0.1:53".parse().expect("addr")),
            RoutePolicy::private(),
        )
        .expect_err("stcp requires a stream target");
        assert!(format!("{datagram}").contains("stream socket"));
    }

    #[test]
    fn a_secret_name_must_be_unambiguous() {
        assert!(RouteMatcher::secret("ssh-box").is_ok());
        assert!(RouteMatcher::secret("  ssh-box  ").is_ok(), "trimmed");
        assert!(RouteMatcher::secret("").is_err());
        assert!(RouteMatcher::secret("has space").is_err());
        assert!(RouteMatcher::secret("has/slash").is_err());
        assert!(RouteMatcher::secret("has:colon").is_err());
        assert!(RouteMatcher::secret(&"x".repeat(129)).is_err());
    }

    #[test]
    fn route_name_length_is_bounded() {
        let build = |name: String| {
            TunnelRoute::new(
                RouteId::parse("route_ssh").expect("valid id"),
                name,
                TunnelProtocolKind::Tcp,
                RouteMatcher::Port(7022),
                local_target(22),
                RoutePolicy::private(),
            )
        };
        for name in [String::new(), "x".repeat(129)] {
            assert!(
                build(name.clone()).is_err(),
                "name of {} chars must be rejected",
                name.len()
            );
        }
        assert!(build("x".repeat(128)).is_ok(), "128 chars is the ceiling");
    }

    #[test]
    fn protocol_decides_the_target_kind_and_its_label() {
        // `parse_for_protocol` is the single entry point shared by agent
        // templates and gateway registrations, so the protocol must be what
        // decides stream vs datagram — never the caller.
        assert_eq!(
            TunnelTarget::parse_for_protocol(TunnelProtocolKind::Udp, "127.0.0.1:53")
                .expect("valid target"),
            TunnelTarget::Udp("127.0.0.1:53".parse().expect("parses"))
        );
        assert_eq!(
            TunnelTarget::parse_for_protocol(TunnelProtocolKind::Tcp, "127.0.0.1:53")
                .expect("valid target"),
            TunnelTarget::Tcp("127.0.0.1:53".parse().expect("parses"))
        );
        assert_eq!(
            TunnelTarget::parse_for_protocol(TunnelProtocolKind::Http, "10.0.0.7:8080")
                .expect("valid target")
                .to_string(),
            "10.0.0.7:8080"
        );
        assert_eq!(local_udp_target(53).to_string(), "udp:127.0.0.1:53");
        assert_eq!(local_target_v6(8443).to_string(), "[::1]:8443");
        assert!(TunnelTarget::parse_udp("not-an-address").is_err());
    }
}

#[cfg(test)]
mod annotation_tests {
    use super::*;

    fn map(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn annotations_are_carried_and_bounded() {
        let good = map(&[("owner", "team-a"), ("ticket/OPS-1", "rollout")]);
        validate_annotations(&good).expect("ordinary metadata is accepted");

        let route = TunnelRoute::new(
            RouteId::parse("route_annotated").expect("id"),
            "annotated",
            TunnelProtocolKind::Http,
            RouteMatcher::domain("annotated.sdkwork.link").expect("domain"),
            local_target(8080),
            RoutePolicy::default(),
        )
        .expect("route")
        .with_annotations(good.clone())
        .expect("annotations attach");
        assert_eq!(route.annotations, good);

        // Absent stays absent on the wire: an agent that sends none does not
        // pay for a field it never uses.
        let plain = TunnelRoute::new(
            RouteId::parse("route_plain").expect("id"),
            "plain",
            TunnelProtocolKind::Http,
            RouteMatcher::domain("plain.sdkwork.link").expect("domain"),
            local_target(8080),
            RoutePolicy::default(),
        )
        .expect("route");
        let json = serde_json::to_string(&plain).expect("serializes");
        assert!(!json.contains("annotations"), "{json}");
    }

    #[test]
    fn unusable_annotations_are_refused_at_construction() {
        let too_many: BTreeMap<String, String> = (0..=MAX_ANNOTATIONS)
            .map(|index| (format!("key-{index}"), "v".to_owned()))
            .collect();
        assert!(validate_annotations(&too_many).is_err(), "count is bounded");

        assert!(
            validate_annotations(&map(&[("", "value")])).is_err(),
            "an empty key is refused"
        );
        assert!(
            validate_annotations(&map(&[("has space", "value")])).is_err(),
            "a key with a space is refused"
        );
        let long_key = "k".repeat(MAX_ANNOTATION_KEY_LEN + 1);
        assert!(
            validate_annotations(&map(&[(long_key.as_str(), "value")])).is_err(),
            "an overlong key is refused"
        );
        let long_value = "v".repeat(MAX_ANNOTATION_VALUE_LEN + 1);
        assert!(
            validate_annotations(&map(&[("owner", long_value.as_str())])).is_err(),
            "an overlong value is refused"
        );
    }
}
