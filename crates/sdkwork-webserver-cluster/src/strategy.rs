//! Load balancing strategy configuration (PRD: 支持多种负载均衡策略，默认
//! 行业最通用策略，灵活配置).

use serde::{Deserialize, Serialize};

/// Request routing strategy over discovered cluster instances.
///
/// Defaults to [`LoadBalancingStrategy::RoundRobin`] — the industry's most
/// common default (nginx upstream default): stateless, perfectly fair for
/// homogeneous fleets, and cache-friendly per client when combined with
/// `ip_hash`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LoadBalancingStrategy {
    /// Round robin over healthy instances (DEFAULT). Weights interleave
    /// proportionally.
    #[default]
    RoundRobin,
    /// Smooth weighted round robin (nginx `weight=` semantics): proportional
    /// interleaving without burst clustering.
    WeightedRoundRobin,
    /// Route to the instance with the fewest in-flight routed requests
    /// (nginx `least_conn`; best for uneven request durations).
    LeastConnections,
    /// Uniform random choice (lock-free xorshift).
    Random,
    /// Power of two choices: sample two candidates, prefer the less loaded
    /// (industry best practice at high concurrency; near-least-conn without
    /// global state).
    RandomTwoChoices,
    /// Sticky routing by client IP hash (nginx `ip_hash`; session affinity).
    IpHash,
    /// Consistent hashing over an arbitrary request key (cache-friendly;
    /// minimal remapping on membership changes).
    ConsistentHash,
}

impl LoadBalancingStrategy {
    /// The configuration default.
    pub const DEFAULT_NAME: &'static str = "round_robin";

    /// All supported strategy names (for diagnostics and validation).
    pub const NAMES: [&'static str; 7] = [
        "round_robin",
        "weighted_round_robin",
        "least_connections",
        "random",
        "random_two_choices",
        "ip_hash",
        "consistent_hash",
    ];

    /// Parses a strategy from configuration. Accepts the canonical snake_case
    /// names plus common aliases (`rr`, `least_conn`, `two_choices`,
    /// `ip_hash`, `consistent`, `weighted`). Unknown names fall back to the
    /// default with `false` returned so callers can log the fallback.
    pub fn parse(raw: &str) -> (Self, bool) {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "round_robin" | "rr" => (Self::RoundRobin, true),
            "weighted_round_robin" | "weighted" | "smooth_weighted" => {
                (Self::WeightedRoundRobin, true)
            }
            "least_connections" | "least_conn" | "leastconn" => (Self::LeastConnections, true),
            "random" => (Self::Random, true),
            "random_two_choices" | "two_choices" | "power_of_two" => (Self::RandomTwoChoices, true),
            "ip_hash" | "iphash" | "source_ip" => (Self::IpHash, true),
            "consistent_hash" | "consistent" | "ketama" => (Self::ConsistentHash, true),
            _ => (Self::RoundRobin, false),
        }
    }

    /// The canonical configuration name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RoundRobin => "round_robin",
            Self::WeightedRoundRobin => "weighted_round_robin",
            Self::LeastConnections => "least_connections",
            Self::Random => "random",
            Self::RandomTwoChoices => "random_two_choices",
            Self::IpHash => "ip_hash",
            Self::ConsistentHash => "consistent_hash",
        }
    }

    /// True when the strategy is sticky (same key → same instance while the
    /// membership is stable).
    pub fn is_sticky(self) -> bool {
        matches!(self, Self::IpHash | Self::ConsistentHash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_round_robin() {
        assert_eq!(
            LoadBalancingStrategy::default(),
            LoadBalancingStrategy::RoundRobin
        );
        assert_eq!(
            LoadBalancingStrategy::parse("round_robin"),
            (LoadBalancingStrategy::RoundRobin, true)
        );
    }

    #[test]
    fn parses_aliases_and_unknown_falls_back() {
        assert_eq!(
            LoadBalancingStrategy::parse("Least_Conn"),
            (LoadBalancingStrategy::LeastConnections, true)
        );
        assert_eq!(
            LoadBalancingStrategy::parse("ketama"),
            (LoadBalancingStrategy::ConsistentHash, true)
        );
        let (strategy, known) = LoadBalancingStrategy::parse("buzzword");
        assert!(!known);
        assert_eq!(strategy, LoadBalancingStrategy::RoundRobin);
    }

    #[test]
    fn sticky_strategies_are_marked() {
        assert!(LoadBalancingStrategy::IpHash.is_sticky());
        assert!(LoadBalancingStrategy::ConsistentHash.is_sticky());
        assert!(!LoadBalancingStrategy::RoundRobin.is_sticky());
    }
}
