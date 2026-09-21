//! Visitor access-control decisions shared by the HTTP and TCP dispatchers
//! (PRD §44, §46).
//!
//! The domain [`sdkwork_webserver_tunnel_core::RoutePolicy`] states the
//! rules; this module owns the gateway-side enforcement inputs (peer
//! address, visitor bearer header) and the deny logging.

use std::net::IpAddr;

use sdkwork_webserver_tunnel_core::RoutePolicy;

/// Outcome of an access-control check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AclDecision {
    /// Traffic may be relayed.
    Admit,
    /// Traffic is denied by the network allow-list.
    DeniedByNetwork,
    /// Traffic is denied because the route is not public and no valid
    /// visitor token was presented.
    DeniedByAuth,
}

impl AclDecision {
    /// True when the visitor may pass.
    pub const fn is_admitted(self) -> bool {
        matches!(self, Self::Admit)
    }
}

/// Gateway access-control evaluator. Stateless in V1; a hook point for
/// future tenant-aware policy sources.
#[derive(Debug, Clone, Copy, Default)]
pub struct RouteAcl;

impl RouteAcl {
    /// Evaluates a visitor against a route policy.
    pub fn evaluate(
        &self,
        policy: &RoutePolicy,
        client_ip: IpAddr,
        bearer: Option<&str>,
    ) -> AclDecision {
        evaluate(policy, client_ip, bearer)
    }
}

/// Evaluates a visitor against a route policy.
pub fn evaluate(policy: &RoutePolicy, client_ip: IpAddr, bearer: Option<&str>) -> AclDecision {
    if !policy.ip_allowed(client_ip) {
        return AclDecision::DeniedByNetwork;
    }
    if policy.allow_public {
        return AclDecision::Admit;
    }
    if policy.visitor_admitted(client_ip, bearer) {
        return AclDecision::Admit;
    }
    AclDecision::DeniedByAuth
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_route_admits_anonymous() {
        let policy = RoutePolicy {
            allow_public: true,
            ..RoutePolicy::private()
        };
        let ip: IpAddr = "203.0.113.7".parse().expect("ip");
        assert!(evaluate(&policy, ip, None).is_admitted());
    }

    #[test]
    fn network_allow_list_denies_first() {
        let policy = RoutePolicy {
            allow_public: true,
            allowed_ips: vec!["203.0.113.0/24".parse().expect("cidr")],
            ..RoutePolicy::private()
        };
        let outside: IpAddr = "198.51.100.9".parse().expect("ip");
        let inside: IpAddr = "203.0.113.9".parse().expect("ip");
        assert_eq!(
            evaluate(&policy, outside, None),
            AclDecision::DeniedByNetwork
        );
        assert!(evaluate(&policy, inside, None).is_admitted());
    }

    #[test]
    fn private_route_requires_bearer() {
        let policy = RoutePolicy {
            auth: sdkwork_webserver_tunnel_core::AuthPolicy::BearerToken,
            visitor_tokens: vec!["tok".to_owned()],
            ..RoutePolicy::private()
        };
        let ip: IpAddr = "203.0.113.7".parse().expect("ip");
        assert_eq!(evaluate(&policy, ip, None), AclDecision::DeniedByAuth);
        assert_eq!(
            evaluate(&policy, ip, Some("wrong")),
            AclDecision::DeniedByAuth
        );
        assert!(evaluate(&policy, ip, Some("tok")).is_admitted());
    }
}
