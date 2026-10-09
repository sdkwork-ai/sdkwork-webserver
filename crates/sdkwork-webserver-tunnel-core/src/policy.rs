//! Route access policy (PRD §17, §44, §46).
//!
//! Policy is declarative domain data; enforcement lives in the gateway
//! security module. A route admits a visitor through one of three
//! independent channels, evaluated in this order:
//!
//! 1. **Network allow-list** (`allowed_ips`): a visitor whose address falls
//!    inside an explicitly configured network is admitted on its own — this
//!    is the only channel available to the raw TCP and UDP planes, which
//!    cannot carry a bearer header.
//! 2. **Public surface** (`allow_public`): any visitor whose address passes
//!    the network allow-list (unrestricted when empty) is admitted.
//! 3. **Visitor token** (`auth = bearer_token`): the visitor must present one
//!    of `visitor_tokens`.
//!
//! When neither a network allow-list nor a public surface nor a usable auth
//! policy is configured, the route denies every visitor (fail closed).

use std::net::IpAddr;

use ipnet::IpNet;
use serde::{Deserialize, Serialize};

use crate::error::{Result, TunnelError, ValidationField};
use crate::route::TunnelProtocolKind;

/// Parses CIDR entries into a route network allow-list.
///
/// Shared by every surface that turns operator or wire input into a
/// [`RoutePolicy`] so an unparsable entry produces one canonical validation
/// error instead of per-caller ad-hoc handling.
pub fn parse_allowed_ips(entries: &[String]) -> Result<Vec<IpNet>> {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            entry.parse().map_err(|_| TunnelError::Validation {
                field: ValidationField::Config,
                reason: format!("allowed_ips[{index}] `{entry}` is not a valid CIDR network"),
            })
        })
        .collect()
}

/// Authentication demanded before a visitor may use a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthPolicy {
    /// No visitor authentication (public preview surface).
    None,
    /// Visitors must present one of the route's shared bearer tokens.
    BearerToken,
}

impl AuthPolicy {
    /// True when this auth policy can be satisfied on the given relay plane.
    ///
    /// Only the HTTP plane carries a visitor `Authorization` header, so a
    /// bearer-token route is meaningless on the raw TCP and UDP planes: such
    /// a route would reject every visitor forever. Rejecting the combination
    /// at construction time turns that silent unreachability into an
    /// operator-visible error (FRP STCP/SUDP parity requires a visitor
    /// client, which V1 does not implement).
    pub const fn is_satisfiable_on(self, protocol: TunnelProtocolKind) -> bool {
        match self {
            Self::None => true,
            Self::BearerToken => matches!(protocol, TunnelProtocolKind::Http),
        }
    }

    /// The `snake_case` label this policy travels under in STP/1 messages.
    ///
    /// One canonical mapping: the agent's registration builder and the
    /// gateway's registration validator must never disagree about what
    /// `"bearer_token"` means, or a protected route would silently register
    /// unprotected (or unreachable).
    pub const fn wire_label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::BearerToken => "bearer_token",
        }
    }

    /// Parses the STP/1 label produced by [`Self::wire_label`].
    pub fn from_wire_label(raw: &str) -> Result<Self> {
        match raw {
            "none" => Ok(Self::None),
            "bearer_token" => Ok(Self::BearerToken),
            other => Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: format!("unknown route auth policy `{other}` (expected none|bearer_token)"),
            }),
        }
    }
}

/// Per-route access policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RoutePolicy {
    /// Serve anonymous Internet traffic from any address that passes
    /// [`Self::allowed_ips`]. `false` restricts admission to the network
    /// allow-list or to a visitor token.
    pub allow_public: bool,
    /// Visitor IP allow-list. Empty means the network is not restricted;
    /// non-empty admits matching visitors on its own.
    pub allowed_ips: Vec<IpNet>,
    /// Visitor authentication demanded when neither [`Self::allow_public`]
    /// nor [`Self::allowed_ips`] admits the visitor.
    pub auth: AuthPolicy,
    /// Shared visitor tokens for [`AuthPolicy::BearerToken`] routes.
    pub visitor_tokens: Vec<String>,
}

impl RoutePolicy {
    /// The V1 default: a private route with no network restriction and no
    /// visitor tokens. Nothing can be admitted, so the route fails closed
    /// until an admission channel is configured.
    pub fn private() -> Self {
        Self {
            allow_public: false,
            allowed_ips: Vec::new(),
            auth: AuthPolicy::None,
            visitor_tokens: Vec::new(),
        }
    }

    /// Overlays a control-plane-declared visitor-authentication policy onto
    /// this policy.
    ///
    /// `None` keeps the local value, so a gateway that predates the fields
    /// changes nothing; `Some` replaces it, which is how a declaration can
    /// protect (or unprotect) a route whose target still comes from the
    /// agent's own configuration.
    #[must_use]
    pub fn with_declared_auth(
        mut self,
        auth: Option<AuthPolicy>,
        visitor_tokens: Option<Vec<String>>,
    ) -> Self {
        if let Some(auth) = auth {
            self.auth = auth;
        }
        if let Some(tokens) = visitor_tokens {
            self.visitor_tokens = tokens;
        }
        self
    }

    /// True when this policy demands a visitor bearer token that no visitor
    /// could ever present, because the token list is empty.
    pub fn bearer_without_tokens(&self) -> bool {
        self.auth == AuthPolicy::BearerToken && self.visitor_tokens.is_empty()
    }

    /// True when the client IP satisfies the route network allow-list. An
    /// empty list means the network is not restricted.
    pub fn ip_allowed(&self, client_ip: IpAddr) -> bool {
        self.allowed_ips.is_empty()
            || self
                .allowed_ips
                .iter()
                .any(|network| network.contains(&client_ip))
    }

    /// Decides whether a visitor may pass, in admission-channel order:
    /// network allow-list, public surface, then visitor token. Visitor tokens
    /// are verified by the caller (HTTP header extraction is a webserver
    /// concern, not domain logic).
    ///
    /// An explicitly configured network allow-list admits on its own: the raw
    /// TCP and UDP planes can present no bearer header, so the allow-list is
    /// their only reachable admission channel.
    pub fn visitor_admitted(&self, client_ip: IpAddr, bearer: Option<&str>) -> bool {
        if !self.ip_allowed(client_ip) {
            return false;
        }
        if self.allow_public {
            return true;
        }
        if !self.allowed_ips.is_empty() {
            return true;
        }
        match self.auth {
            AuthPolicy::None => false,
            AuthPolicy::BearerToken => match bearer {
                Some(token) => self
                    .visitor_tokens
                    .iter()
                    .any(|candidate| sdkwork_utils_rust::crypto::secure_compare(candidate, token)),
                None => false,
            },
        }
    }
}

impl Default for RoutePolicy {
    fn default() -> Self {
        Self::private()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip(value: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, value))
    }

    #[test]
    fn public_policy_admits_any_allowed_ip() {
        let policy = RoutePolicy {
            allow_public: true,
            ..RoutePolicy::private()
        };
        assert!(policy.visitor_admitted(ip(1), None));
    }

    #[test]
    fn private_policy_denies_anonymous_visitors() {
        let policy = RoutePolicy::private();
        assert!(!policy.visitor_admitted(ip(1), None));
    }

    #[test]
    fn bearer_policy_verifies_tokens() {
        let policy = RoutePolicy {
            auth: AuthPolicy::BearerToken,
            visitor_tokens: vec!["s3cret".to_owned()],
            ..RoutePolicy::private()
        };
        assert!(policy.visitor_admitted(ip(1), Some("s3cret")));
        assert!(!policy.visitor_admitted(ip(1), Some("wrong")));
        assert!(!policy.visitor_admitted(ip(1), None));
    }

    #[test]
    fn auth_policy_wire_labels_round_trip() {
        // One canonical label mapping: the agent writes it, the gateway reads
        // it. A drift here silently changes whether a route is protected.
        for policy in [AuthPolicy::None, AuthPolicy::BearerToken] {
            assert_eq!(
                AuthPolicy::from_wire_label(policy.wire_label()).expect("label parses"),
                policy
            );
        }
        assert!(AuthPolicy::from_wire_label("basic").is_err());
        assert_eq!(AuthPolicy::None.wire_label(), "none");
        assert_eq!(AuthPolicy::BearerToken.wire_label(), "bearer_token");
    }

    #[test]
    fn declared_auth_overlays_only_the_fields_the_control_plane_sent() {
        let local = RoutePolicy {
            auth: AuthPolicy::BearerToken,
            visitor_tokens: vec!["local".to_owned()],
            ..RoutePolicy::private()
        };
        // A declaration that says nothing about auth keeps the local policy.
        assert_eq!(local.clone().with_declared_auth(None, None), local);
        // An explicit declaration replaces it.
        let relaxed = local
            .clone()
            .with_declared_auth(Some(AuthPolicy::None), Some(Vec::new()));
        assert_eq!(relaxed.auth, AuthPolicy::None);
        assert!(relaxed.visitor_tokens.is_empty());
        assert!(!relaxed.bearer_without_tokens());
        // Bearer with an empty token list is the unreachable shape.
        let unreachable = RoutePolicy {
            auth: AuthPolicy::BearerToken,
            ..RoutePolicy::private()
        };
        assert!(unreachable.bearer_without_tokens());
    }

    #[test]
    fn ip_allow_list_gates_everything() {
        let policy = RoutePolicy {
            allow_public: true,
            allowed_ips: vec!["10.0.0.0/8".parse().expect("valid cidr")],
            ..RoutePolicy::private()
        };
        assert!(policy.visitor_admitted(ip(9), None));
        assert!(!policy.visitor_admitted(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), None));
    }

    #[test]
    fn network_allow_list_is_an_independent_admission_channel() {
        // The raw TCP and UDP planes can present no bearer header, so an
        // explicitly configured allow-list must admit on its own.
        let policy = RoutePolicy {
            allowed_ips: vec!["10.0.0.0/8".parse().expect("valid cidr")],
            ..RoutePolicy::private()
        };
        assert!(policy.visitor_admitted(ip(9), None));
        assert!(!policy.visitor_admitted(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), None));
    }

    #[test]
    fn bearer_auth_is_only_satisfiable_on_the_http_plane() {
        use crate::route::TunnelProtocolKind;
        assert!(AuthPolicy::None.is_satisfiable_on(TunnelProtocolKind::Http));
        assert!(AuthPolicy::None.is_satisfiable_on(TunnelProtocolKind::Tcp));
        assert!(AuthPolicy::None.is_satisfiable_on(TunnelProtocolKind::Udp));
        assert!(AuthPolicy::BearerToken.is_satisfiable_on(TunnelProtocolKind::Http));
        assert!(!AuthPolicy::BearerToken.is_satisfiable_on(TunnelProtocolKind::Tcp));
        assert!(!AuthPolicy::BearerToken.is_satisfiable_on(TunnelProtocolKind::Udp));
    }
}
