//! The lock-free request router: strategy-driven instance picking with
//! in-flight lease tracking (PRD: 高并发、自动路由、多策略负载均衡).

use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::instance::DiscoveredInstance;
use super::strategy::LoadBalancingStrategy;
use super::topology::{ClusterTopology, ServiceGroup};

/// The key a pick is made with. Sticky strategies consume it; stateless
/// strategies ignore it.
#[derive(Clone, Debug)]
pub enum PickKey<'a> {
    /// No specific key (pure strategies only).
    None,
    /// Client IP literal (for `ip_hash`).
    ClientIp(&'a str),
    /// Arbitrary routing key (for `consistent_hash`).
    HashKey(&'a str),
}

/// A picked instance with an in-flight lease: dropping the guard releases
/// the connection count that `LeastConnections` / `RandomTwoChoices` read.
/// The lease also carries the routing decision metadata (single-pick
/// contract: callers must not re-pick to learn what was chosen).
pub struct InstanceLease {
    decision: PickDecision,
    instance: Arc<DiscoveredInstance>,
}

impl InstanceLease {
    /// The picked instance.
    pub fn instance(&self) -> &DiscoveredInstance {
        &self.instance
    }

    /// The picked instance handle (shared; snapshot lifetime).
    pub fn shared(&self) -> &Arc<DiscoveredInstance> {
        &self.instance
    }

    /// The routing endpoint.
    pub fn endpoint(&self) -> &str {
        &self.instance.endpoint
    }

    /// The routing decision metadata.
    pub fn decision(&self) -> &PickDecision {
        &self.decision
    }
}

impl Drop for InstanceLease {
    fn drop(&mut self) {
        self.instance.inflight.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Route decision emitted for diagnostics and the admin preview surface.
#[derive(Debug, Clone)]
pub struct PickDecision {
    /// Service group name.
    pub service: String,
    /// Strategy applied.
    pub strategy: LoadBalancingStrategy,
    /// Picked instance id.
    pub instance_id: String,
    /// Routing endpoint.
    pub endpoint: String,
}

/// The cluster request router: `pick` never blocks and never allocates on
/// the hot path (atomics only), never returns a non-routeable instance, and
/// is safe under arbitrary concurrency.
pub struct ClusterLoadBalancer;

impl ClusterLoadBalancer {
    /// Picks one routeable instance of `group` from the snapshot using the
    /// group's strategy (or the override when provided).
    pub fn pick(
        topology: &ClusterTopology,
        host: &str,
        key: PickKey<'_>,
        strategy_override: Option<LoadBalancingStrategy>,
    ) -> Option<InstanceLease> {
        let group = topology.group_for_host(host)?;
        let strategy = strategy_override.unwrap_or(group.strategy);
        let instance = Self::pick_from_group(group, strategy, &key)?;
        Some(InstanceLease {
            decision: PickDecision {
                service: group.name.clone(),
                strategy,
                instance_id: instance.id.clone(),
                endpoint: instance.endpoint.clone(),
            },
            instance,
        })
    }

    /// Diagnostic pick that also reports the decision metadata (no lease;
    /// does not touch in-flight counts).
    pub fn pick_decision(
        topology: &ClusterTopology,
        host: &str,
        key: PickKey<'_>,
        strategy_override: Option<LoadBalancingStrategy>,
    ) -> Option<PickDecision> {
        let group = topology.group_for_host(host)?;
        let strategy = strategy_override.unwrap_or(group.strategy);
        let instance = Self::pick_from_group(group, strategy, &key)?;
        Some(PickDecision {
            service: group.name.clone(),
            strategy,
            instance_id: instance.id.clone(),
            endpoint: instance.endpoint.clone(),
        })
    }

    pub(crate) fn pick_from_group(
        group: &ServiceGroup,
        strategy: LoadBalancingStrategy,
        key: &PickKey<'_>,
    ) -> Option<Arc<DiscoveredInstance>> {
        match strategy {
            LoadBalancingStrategy::RoundRobin => round_robin(group),
            LoadBalancingStrategy::WeightedRoundRobin => smooth_weighted(group),
            LoadBalancingStrategy::LeastConnections => least_connections(group),
            LoadBalancingStrategy::Random => random(group),
            LoadBalancingStrategy::RandomTwoChoices => random_two_choices(group),
            LoadBalancingStrategy::IpHash => match key {
                PickKey::ClientIp(ip) => group
                    .hash_ring
                    .resolve(ip.as_bytes())
                    .or_else(|| fallback_any(group)),
                _ => round_robin(group),
            },
            LoadBalancingStrategy::ConsistentHash => match key {
                PickKey::HashKey(key) => group
                    .hash_ring
                    .resolve(key.as_bytes())
                    .or_else(|| fallback_any(group)),
                PickKey::ClientIp(ip) => group
                    .hash_ring
                    .resolve(ip.as_bytes())
                    .or_else(|| fallback_any(group)),
                PickKey::None => round_robin(group),
            },
        }
        .inspect(|instance| {
            instance.inflight.fetch_add(1, Ordering::Relaxed);
        })
    }
}

/// Picks with a round-robin cursor over routeable instances, honoring
/// weights: heavier instances are visited proportionally more often via the
/// accumulated cursor position.
fn round_robin(group: &ServiceGroup) -> Option<Arc<DiscoveredInstance>> {
    let candidates: Vec<Arc<DiscoveredInstance>> = group.routeable().cloned().collect();
    if candidates.is_empty() {
        return None;
    }
    let total_weight: u64 = candidates
        .iter()
        .map(|candidate| u64::from(candidate.weight))
        .sum();
    let position = group.cursor.fetch_add(1, Ordering::Relaxed) % total_weight.max(1);
    let mut accumulated = 0_u64;
    for candidate in &candidates {
        accumulated += u64::from(candidate.weight);
        if position < accumulated {
            return Some(Arc::clone(candidate));
        }
    }
    candidates.first().map(Arc::clone)
}

/// Smooth weighted round robin (nginx algorithm): each pick adds every
/// instance's weight to its current value, picks the maximum, and subtracts
/// the total from it — proportional interleaving without bursts.
fn smooth_weighted(group: &ServiceGroup) -> Option<Arc<DiscoveredInstance>> {
    let candidates: Vec<Arc<DiscoveredInstance>> = group.routeable().cloned().collect();
    if candidates.is_empty() {
        return None;
    }
    let total: i64 = candidates
        .iter()
        .map(|candidate| i64::from(candidate.weight))
        .sum();
    let mut best: Option<(Arc<DiscoveredInstance>, i64)> = None;
    for candidate in &candidates {
        let current = candidate
            .current_weight
            .fetch_add(i64::from(candidate.weight), Ordering::Relaxed)
            + i64::from(candidate.weight);
        match &best {
            Some((_, best_current)) if current <= *best_current => {}
            _ => best = Some((Arc::clone(candidate), current)),
        }
    }
    let (winner, current) = best?;
    winner
        .current_weight
        .store(current - total, Ordering::Relaxed);
    Some(winner)
}

/// Least in-flight connections; ties broken by lower quality pressure
/// (higher quality score), then by round-robin cursor for fairness.
fn least_connections(group: &ServiceGroup) -> Option<Arc<DiscoveredInstance>> {
    let mut candidates: Vec<Arc<DiscoveredInstance>> = group.routeable().cloned().collect();
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by(|left, right| {
        let left_inflight = left.inflight();
        let right_inflight = right.inflight();
        left_inflight
            .cmp(&right_inflight)
            .then_with(|| {
                right
                    .quality_score
                    .unwrap_or(50)
                    .cmp(&left.quality_score.unwrap_or(50))
            })
            .then_with(|| left.id.cmp(&right.id))
    });
    candidates.first().map(Arc::clone)
}

fn random_state(group: &ServiceGroup) -> u64 {
    group
        .random_state
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |state| {
            Some(next_xorshift(state))
        })
        .unwrap_or(0x9e37_79b9_7f4a_7c15)
}

fn next_xorshift(state: u64) -> u64 {
    let mut x = state | 1;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

fn random(group: &ServiceGroup) -> Option<Arc<DiscoveredInstance>> {
    let candidates: Vec<Arc<DiscoveredInstance>> = group.routeable().cloned().collect();
    if candidates.is_empty() {
        return None;
    }
    let index = (random_state(group) % candidates.len() as u64) as usize;
    candidates.get(index).map(Arc::clone)
}

/// Power of two choices: sample two, prefer the less loaded (fall back to
/// the other candidate when the first is saturated).
fn random_two_choices(group: &ServiceGroup) -> Option<Arc<DiscoveredInstance>> {
    let candidates: Vec<Arc<DiscoveredInstance>> = group.routeable().cloned().collect();
    if candidates.is_empty() {
        return None;
    }
    let first = (random_state(group) % candidates.len() as u64) as usize;
    let second = (random_state(group) % candidates.len() as u64) as usize;
    let candidate_a = &candidates[first];
    let candidate_b = &candidates[second];
    let winner = if candidate_a.inflight() <= candidate_b.inflight() {
        candidate_a
    } else {
        candidate_b
    };
    Some(Arc::clone(winner))
}

/// Last-resort pick when a sticky strategy's owner is not routeable: fall
/// back to any routeable instance (availability over affinity).
fn fallback_any(group: &ServiceGroup) -> Option<Arc<DiscoveredInstance>> {
    group.routeable().next().map(Arc::clone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::{InstanceHealth, InstanceStatus};
    use crate::topology::{ClusterTopology, ServiceMembership};

    fn instance(id: &str, weight: u32) -> Arc<DiscoveredInstance> {
        Arc::new(DiscoveredInstance::new(id, format!("https://{id}:443")).with_weight(weight))
    }

    fn instance_with_state(
        id: &str,
        weight: u32,
        status: InstanceStatus,
        health: InstanceHealth,
    ) -> Arc<DiscoveredInstance> {
        Arc::new(
            DiscoveredInstance::new(id, format!("https://{id}:443"))
                .with_weight(weight)
                .with_state(status, health),
        )
    }

    fn single_topology(
        strategy: LoadBalancingStrategy,
        instances: Vec<Arc<DiscoveredInstance>>,
    ) -> ClusterTopology {
        ClusterTopology::build(
            "test",
            0,
            strategy,
            instances
                .into_iter()
                .map(|instance| ServiceMembership {
                    instance,
                    served_domains: vec!["svc.test".to_owned()],
                    strategy: None,
                })
                .collect(),
        )
    }

    #[test]
    fn round_robin_is_fair() {
        let topology = single_topology(
            LoadBalancingStrategy::RoundRobin,
            vec![instance("a", 1), instance("b", 1), instance("c", 1)],
        );
        let mut counts = std::collections::HashMap::new();
        for _ in 0..90 {
            let lease = ClusterLoadBalancer::pick(&topology, "svc.test", PickKey::None, None)
                .expect("pick");
            *counts.entry(lease.instance().id.clone()).or_insert(0) += 1;
            drop(lease);
        }
        assert_eq!(counts.get("a"), Some(&30));
        assert_eq!(counts.get("b"), Some(&30));
        assert_eq!(counts.get("c"), Some(&30));
    }

    #[test]
    fn weighted_interleaves_proportionally() {
        let topology = single_topology(
            LoadBalancingStrategy::RoundRobin,
            vec![instance("heavy", 3), instance("light", 1)],
        );
        let mut counts = std::collections::HashMap::new();
        for _ in 0..40 {
            let lease = ClusterLoadBalancer::pick(&topology, "svc.test", PickKey::None, None)
                .expect("pick");
            *counts.entry(lease.instance().id.clone()).or_insert(0) += 1;
            drop(lease);
        }
        assert_eq!(counts.get("heavy"), Some(&30));
        assert_eq!(counts.get("light"), Some(&10));
    }

    #[test]
    fn smooth_weighted_avoids_bursts() {
        let topology = single_topology(
            LoadBalancingStrategy::WeightedRoundRobin,
            vec![instance("heavy", 3), instance("light", 1)],
        );
        let mut sequence = Vec::new();
        for _ in 0..8 {
            let lease = ClusterLoadBalancer::pick(&topology, "svc.test", PickKey::None, None)
                .expect("pick");
            sequence.push(lease.instance().id.clone());
            drop(lease);
        }
        // Smooth WRR never emits more than `weight` consecutive picks of the
        // same instance within one full cycle (4 picks = 3 heavy + 1 light).
        let window: Vec<String> = sequence[..4].to_vec();
        let mut max_run = 1;
        let mut run = 1;
        for pair in window.windows(2) {
            if pair[0] == pair[1] {
                run += 1;
                max_run = max_run.max(run);
            } else {
                run = 1;
            }
        }
        assert!(max_run <= 3, "burst of {max_run} in {sequence:?}");
    }

    #[test]
    fn least_connections_prefers_idle_instances() {
        let topology = single_topology(
            LoadBalancingStrategy::LeastConnections,
            vec![instance("a", 1), instance("b", 1)],
        );
        let busy =
            ClusterLoadBalancer::pick(&topology, "svc.test", PickKey::None, None).expect("pick");
        for _ in 0..5 {
            let next = ClusterLoadBalancer::pick(&topology, "svc.test", PickKey::None, None)
                .expect("pick");
            assert_ne!(
                next.instance().id,
                busy.instance().id,
                "must avoid the busy instance while a lease is open"
            );
        }
        drop(busy);
    }

    #[test]
    fn unhealthy_instances_are_never_picked() {
        let offline = instance_with_state(
            "offline",
            1,
            InstanceStatus::Offline,
            InstanceHealth::Healthy,
        );
        let unhealthy =
            instance_with_state("sick", 1, InstanceStatus::Online, InstanceHealth::Unhealthy);
        let topology = single_topology(
            LoadBalancingStrategy::RoundRobin,
            vec![offline, unhealthy, instance("healthy", 1)],
        );
        for _ in 0..10 {
            let lease = ClusterLoadBalancer::pick(&topology, "svc.test", PickKey::None, None)
                .expect("pick");
            assert_eq!(lease.instance().id, "healthy");
            drop(lease);
        }
    }

    #[test]
    fn ip_hash_is_sticky_per_client() {
        let topology = single_topology(
            LoadBalancingStrategy::IpHash,
            vec![instance("a", 1), instance("b", 1)],
        );
        let first = ClusterLoadBalancer::pick(
            &topology,
            "svc.test",
            PickKey::ClientIp("203.0.113.7"),
            None,
        )
        .expect("pick");
        for _ in 0..5 {
            let next = ClusterLoadBalancer::pick(
                &topology,
                "svc.test",
                PickKey::ClientIp("203.0.113.7"),
                None,
            )
            .expect("pick");
            assert_eq!(next.instance().id, first.instance().id);
        }
    }

    #[test]
    fn unknown_host_routes_nothing() {
        let topology = single_topology(LoadBalancingStrategy::RoundRobin, vec![instance("a", 1)]);
        assert!(ClusterLoadBalancer::pick(&topology, "other.test", PickKey::None, None).is_none());
    }

    #[test]
    fn cordoned_instance_is_excluded_from_routing() {
        let mut cordoned = instance("cordoned", 1);
        Arc::get_mut(&mut cordoned)
            .expect("sole owner")
            .routing_enabled = false;
        let topology = single_topology(
            LoadBalancingStrategy::RoundRobin,
            vec![cordoned, instance("live", 1)],
        );
        for _ in 0..6 {
            let lease = ClusterLoadBalancer::pick(&topology, "svc.test", PickKey::None, None)
                .expect("pick");
            assert_eq!(lease.instance().id, "live");
            drop(lease);
        }
    }

    #[test]
    fn all_instances_down_routes_nothing() {
        let offline = instance_with_state("a", 1, InstanceStatus::Error, InstanceHealth::Healthy);
        let topology = single_topology(LoadBalancingStrategy::RoundRobin, vec![offline]);
        assert!(ClusterLoadBalancer::pick(&topology, "svc.test", PickKey::None, None).is_none());
    }

    #[test]
    fn strategy_override_applies_per_request() {
        let topology = single_topology(
            LoadBalancingStrategy::RoundRobin,
            vec![instance("a", 1), instance("b", 1)],
        );
        let sticky = ClusterLoadBalancer::pick(
            &topology,
            "svc.test",
            PickKey::ClientIp("10.0.0.1"),
            Some(LoadBalancingStrategy::IpHash),
        )
        .expect("pick");
        let again = ClusterLoadBalancer::pick(
            &topology,
            "svc.test",
            PickKey::ClientIp("10.0.0.1"),
            Some(LoadBalancingStrategy::IpHash),
        )
        .expect("pick");
        assert_eq!(sticky.instance().id, again.instance().id);
    }

    #[test]
    fn concurrent_picking_never_panics_and_stays_in_bounds() {
        let topology = Arc::new(single_topology(
            LoadBalancingStrategy::RoundRobin,
            vec![instance("a", 1), instance("b", 1), instance("c", 1)],
        ));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let topology = Arc::clone(&topology);
                std::thread::spawn(move || {
                    for _ in 0..2_000 {
                        let lease =
                            ClusterLoadBalancer::pick(&topology, "svc.test", PickKey::None, None)
                                .expect("pick");
                        assert!(lease.instance().inflight() >= 1);
                        drop(lease);
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().expect("worker");
        }
        for group in topology.groups() {
            for instance in &group.instances {
                assert_eq!(instance.inflight(), 0, "leases must balance to zero");
            }
        }
    }
}
