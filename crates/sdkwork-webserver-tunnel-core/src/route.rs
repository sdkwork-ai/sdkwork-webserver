//! Tunnel route: the core domain object (PRD §14–§17, §29).
//!
//! A route binds a public matcher (domain or gateway TCP port) to a target
//! that lives behind an agent, guarded by a policy. Domain models are not
//! wire messages: the protocol crate carries its own transfer types.

use std::net::{SocketAddr, SocketAddrV4, SocketAddrV6};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Result, TunnelError, ValidationField};
use crate::ids::{RouteId, SessionId};
use crate::policy::RoutePolicy;

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
            Self::Port(_) => None,
        }
    }

    /// True when this matcher is a wildcard domain (`*.suffix`).
    pub fn is_wildcard_domain(&self) -> bool {
        self.as_domain().is_some_and(|domain| domain.starts_with("*."))
    }

    /// The suffix covered by a wildcard domain matcher (without `*.`).
    pub fn wildcard_suffix(&self) -> Option<&str> {
        self.as_domain().and_then(|domain| domain.strip_prefix("*."))
    }

    /// The matched gateway port, when this is a port matcher.
    pub const fn as_port(&self) -> Option<u16> {
        match self {
            Self::Port(port) => Some(*port),
            Self::Domain(_) => None,
        }
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
    /// Owning session (gateway-assigned; agents never set this).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Registration instant (gateway-assigned).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
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
        Ok(Self {
            id,
            name,
            protocol,
            matcher,
            target,
            policy,
            session_id: None,
            created_at: None,
        })
    }

    /// Attaches gateway-assigned ownership metadata.
    pub fn with_session(mut self, session_id: SessionId, now: DateTime<Utc>) -> Self {
        self.session_id = Some(session_id);
        self.created_at = Some(now);
        self
    }
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

/// Convenience constructor for IPv6 loopback targets.
#[allow(dead_code)]
/// Convenience constructor for a local UDP target (datagram relay).
pub fn local_udp_target(port: u16) -> TunnelTarget {
    TunnelTarget::Udp(SocketAddr::V4(SocketAddrV4::new(
        std::net::Ipv4Addr::LOCALHOST,
        port,
    )))
}

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
}
