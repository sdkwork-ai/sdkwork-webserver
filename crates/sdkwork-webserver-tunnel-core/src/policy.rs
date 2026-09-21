//! Route access policy (PRD §17, §44, §46).
//!
//! Policy is declarative domain data; enforcement lives in the gateway
//! security module. `allow_public = false` requires every visitor identity to
//! pass the configured auth policy before traffic is relayed.

use std::net::IpAddr;

use ipnet::IpNet;
use serde::{Deserialize, Serialize};

/// Authentication demanded before a visitor may use a route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthPolicy {
    /// No visitor authentication (public preview surface).
    None,
    /// Visitors must present one of the route's shared bearer tokens.
    BearerToken,
}

/// Per-route access policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RoutePolicy {
    /// Serve anonymous Internet traffic. `false` denies every visitor that
    /// does not satisfy [`Self::auth`].
    pub allow_public: bool,
    /// Visitor IP allow-list; empty means the network is not restricted.
    pub allowed_ips: Vec<IpNet>,
    /// Visitor authentication demanded when `allow_public` is `false`.
    pub auth: AuthPolicy,
    /// Shared visitor tokens for [`AuthPolicy::BearerToken`] routes.
    pub visitor_tokens: Vec<String>,
}

impl RoutePolicy {
    /// The V1 default: a private route with no network restriction and no
    /// visitor tokens (auth fails closed until tokens are configured).
    pub fn private() -> Self {
        Self {
            allow_public: false,
            allowed_ips: Vec::new(),
            auth: AuthPolicy::None,
            visitor_tokens: Vec::new(),
        }
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

    /// Decides whether a visitor may pass: network allow-list first, then
    /// the public/auth decision. Visitor tokens are verified by the caller
    /// (HTTP header extraction is a webserver concern, not domain logic).
    pub fn visitor_admitted(&self, client_ip: IpAddr, bearer: Option<&str>) -> bool {
        if !self.ip_allowed(client_ip) {
            return false;
        }
        if self.allow_public {
            return true;
        }
        match self.auth {
            AuthPolicy::None => false,
            AuthPolicy::BearerToken => match bearer {
                Some(token) => self
                    .visitor_tokens
                    .iter()
                    .any(|candidate| constant_time_eq(candidate, token)),
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

/// Length-independent equality so token comparison does not leak prefix
/// match length through timing.
fn constant_time_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |acc, (a, b)| acc | (a ^ b))
        == 0
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
    fn ip_allow_list_gates_everything() {
        let policy = RoutePolicy {
            allow_public: true,
            allowed_ips: vec!["10.0.0.0/8".parse().expect("valid cidr")],
            ..RoutePolicy::private()
        };
        assert!(policy.visitor_admitted(ip(9), None));
        assert!(!policy.visitor_admitted(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), None));
    }
}
