//! nginx `ngx_http_access_module` allow/deny evaluation.

use std::net::IpAddr;

use ipnet::IpNet;

use super::model::{AccessAction, AccessRuleConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessDecision {
    Allow,
    Deny,
}

/// nginx `allow`/`deny` semantics as executed by the **stream** access phase:
/// rules are checked in order and the first match wins; a peer that matches no
/// rule is allowed (nginx's handler returns NGX_DECLINED and the access phase
/// passes). The HTTP plane keeps [`evaluate_access`]'s default-deny behavior,
/// which the differential slices pin for the declared rule sets.
pub fn evaluate_access_first_match_allow(
    client_ip: IpAddr,
    rules: &[AccessRuleConfig],
) -> AccessDecision {
    for rule in rules {
        if access_rule_matches(client_ip, rule) {
            return match rule.action {
                AccessAction::Allow => AccessDecision::Allow,
                AccessAction::Deny => AccessDecision::Deny,
            };
        }
    }
    AccessDecision::Allow
}

/// Evaluate ordered allow/deny rules (nginx HTTP access module).
///
/// Rules are checked in sequence until the first match. When any rule is
/// present and none match, access is denied (the runtime's fail-closed HTTP
/// default). An empty rule list means the access module is inactive (allow).
pub fn evaluate_access(client_ip: IpAddr, rules: &[AccessRuleConfig]) -> AccessDecision {
    if rules.is_empty() {
        return AccessDecision::Allow;
    }
    for rule in rules {
        if access_rule_matches(client_ip, rule) {
            return match rule.action {
                AccessAction::Allow => AccessDecision::Allow,
                AccessAction::Deny => AccessDecision::Deny,
            };
        }
    }
    AccessDecision::Deny
}

fn access_rule_matches(client_ip: IpAddr, rule: &AccessRuleConfig) -> bool {
    network_matches(client_ip, &rule.network)
}

fn network_matches(client_ip: IpAddr, network: &str) -> bool {
    let trimmed = network.trim();
    if trimmed.eq_ignore_ascii_case("all") {
        return true;
    }
    if let Ok(ip) = trimmed.parse::<IpAddr>() {
        return client_ip == ip;
    }
    if let Ok(cidr) = trimmed.parse::<IpNet>() {
        return cidr.contains(&client_ip);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{evaluate_access, AccessDecision};
    use crate::config::model::{AccessAction, AccessRuleConfig};
    use std::net::{IpAddr, Ipv4Addr};

    fn allow(network: &str) -> AccessRuleConfig {
        AccessRuleConfig {
            action: AccessAction::Allow,
            network: network.to_owned(),
        }
    }

    fn deny(network: &str) -> AccessRuleConfig {
        AccessRuleConfig {
            action: AccessAction::Deny,
            network: network.to_owned(),
        }
    }

    #[test]
    fn empty_rules_allow() {
        let ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        assert_eq!(evaluate_access(ip, &[]), AccessDecision::Allow);
    }

    #[test]
    fn allow_then_deny_all() {
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5));
        let outside = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));
        let rules = vec![allow("10.0.0.0/8"), deny("all")];
        assert_eq!(evaluate_access(ip, &rules), AccessDecision::Allow);
        assert_eq!(evaluate_access(outside, &rules), AccessDecision::Deny);
    }

    #[test]
    fn unmatched_with_rules_denies() {
        let ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        let rules = vec![allow("10.0.0.0/8")];
        assert_eq!(evaluate_access(ip, &rules), AccessDecision::Deny);
    }

    #[test]
    fn stream_first_match_wins_and_unmatched_allows() {
        let inside = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let outside = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5));
        // First match wins: `deny 127.0.0.1` before `allow all` rejects loopback.
        let rules = vec![deny("127.0.0.1"), allow("all")];
        assert_eq!(
            super::evaluate_access_first_match_allow(inside, &rules),
            AccessDecision::Deny
        );
        assert_eq!(
            super::evaluate_access_first_match_allow(outside, &rules),
            AccessDecision::Allow
        );
        // A peer matching no rule is allowed (nginx access phase declines).
        let only = vec![allow("127.0.0.1")];
        assert_eq!(
            super::evaluate_access_first_match_allow(outside, &only),
            AccessDecision::Allow
        );
        assert_eq!(
            super::evaluate_access_first_match_allow(inside, &only),
            AccessDecision::Allow
        );
    }
}
