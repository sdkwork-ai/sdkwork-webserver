//! Immutable cluster topology snapshot published by the discovery loop.
//!
//! One snapshot covers one cluster: every discovered instance grouped by
//! the service domains it serves (auto-routing matches request hosts
//! against these), plus per-strategy scratch state that survives via
//! atomics on the instances themselves.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::instance::DiscoveredInstance;
use super::ring::ConsistentHashRing;
use super::strategy::LoadBalancingStrategy;

/// One service group: a routed service name (usually the served domain),
/// its instances, the effective strategy, and the prebuilt consistent-hash
/// ring for sticky strategies.
#[derive(Debug)]
pub struct ServiceGroup {
    /// Service name the group is addressed by (lowercased served domain).
    pub name: String,
    /// Every discovered instance of the service (including non-routeable,
    /// kept for admin visibility; picking only considers routeable ones).
    pub instances: Vec<Arc<DiscoveredInstance>>,
    /// Effective strategy for this group.
    pub strategy: LoadBalancingStrategy,
    /// Round-robin cursor (atomic; shared across readers).
    pub(crate) cursor: std::sync::atomic::AtomicU64,
    /// Random state (xorshift; lock-free).
    pub(crate) random_state: std::sync::atomic::AtomicU64,
    /// Consistent hash ring (built once per snapshot for sticky picking).
    pub(crate) hash_ring: ConsistentHashRing,
}

impl ServiceGroup {
    /// The routeable instances of this group.
    pub fn routeable(&self) -> impl Iterator<Item = &Arc<DiscoveredInstance>> {
        self.instances
            .iter()
            .filter(|instance| instance.is_routeable())
    }

    /// Wire projection for diagnostics.
    pub fn view(&self) -> ServiceGroupView {
        ServiceGroupView {
            name: self.name.clone(),
            strategy: self.strategy.as_str().to_owned(),
            instances: self
                .instances
                .iter()
                .map(|instance| crate::instance::DiscoveredInstanceView {
                    id: instance.id.clone(),
                    endpoint: instance.endpoint.clone(),
                    weight: instance.weight,
                    routeable: instance.is_routeable(),
                    status: status_label(instance.status).to_owned(),
                    health: health_label(instance.health).to_owned(),
                    quality_score: instance.quality_score,
                    join_mode: instance.join_mode.clone(),
                    inflight: instance.inflight(),
                })
                .collect(),
        }
    }
}

fn status_label(status: crate::instance::InstanceStatus) -> &'static str {
    match status {
        crate::instance::InstanceStatus::Offline => "offline",
        crate::instance::InstanceStatus::Online => "online",
        crate::instance::InstanceStatus::Starting => "starting",
        crate::instance::InstanceStatus::Stopping => "stopping",
        crate::instance::InstanceStatus::Error => "error",
        crate::instance::InstanceStatus::Maintenance => "maintenance",
    }
}

fn health_label(health: crate::instance::InstanceHealth) -> &'static str {
    match health {
        crate::instance::InstanceHealth::Unknown => "unknown",
        crate::instance::InstanceHealth::Healthy => "healthy",
        crate::instance::InstanceHealth::Degraded => "degraded",
        crate::instance::InstanceHealth::Unhealthy => "unhealthy",
    }
}

/// Wire projection of one service group.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceGroupView {
    pub name: String,
    pub strategy: String,
    pub instances: Vec<crate::instance::DiscoveredInstanceView>,
}

/// An atomically swapped snapshot of one cluster's discovered topology.
/// `Clone` shares instances (Arc) — strategy atomics stay shared by design
/// so late attach never forks pick state.
#[derive(Debug, Default, Clone)]
pub struct ClusterTopology {
    /// Cluster code the snapshot was built for.
    pub cluster_code: String,
    /// Snapshot build instant (millis since epoch) for staleness checks.
    pub built_at_millis: u64,
    /// Service groups indexed by lowercase served domain.
    pub groups: BTreeMap<String, Arc<ServiceGroup>>,
}

impl ClusterTopology {
    /// Builds a snapshot from discovery rows.
    ///
    /// `members` pairs each served domain with the instances serving it;
    /// `strategy` is the cluster-configured default (per-service overrides
    /// may arrive in `members` entries with explicit strategies).
    pub fn build(
        cluster_code: impl Into<String>,
        built_at_millis: u64,
        default_strategy: LoadBalancingStrategy,
        members: Vec<ServiceMembership>,
    ) -> Self {
        let mut grouped: BTreeMap<String, Vec<Arc<DiscoveredInstance>>> = BTreeMap::new();
        let mut strategies: BTreeMap<String, LoadBalancingStrategy> = BTreeMap::new();
        for member in members {
            for served in &member.served_domains {
                let domain = served.trim().to_ascii_lowercase();
                if domain.is_empty() {
                    continue;
                }
                let strategy = member.strategy.unwrap_or(default_strategy);
                let entry = grouped.entry(domain.clone()).or_default();
                if !entry
                    .iter()
                    .any(|existing| existing.id == member.instance.id)
                {
                    entry.push(Arc::clone(&member.instance));
                }
                strategies.entry(domain).or_insert(strategy);
            }
        }
        let groups = grouped
            .into_iter()
            .map(|(name, instances)| {
                let strategy = strategies.get(&name).copied().unwrap_or(default_strategy);
                let hash_ring = ConsistentHashRing::build(&instances);
                let seed = super::ring::fnv1a64(name.as_bytes()) | 1;
                (
                    name.clone(),
                    Arc::new(ServiceGroup {
                        name,
                        instances,
                        strategy,
                        cursor: std::sync::atomic::AtomicU64::new(0),
                        random_state: std::sync::atomic::AtomicU64::new(seed),
                        hash_ring,
                    }),
                )
            })
            .collect();
        Self {
            cluster_code: cluster_code.into(),
            built_at_millis,
            groups,
        }
    }

    /// The service group a request host routes to (`None` when this cluster
    /// does not serve the host).
    pub fn group_for_host(&self, host: &str) -> Option<&Arc<ServiceGroup>> {
        let lowered = host.trim().to_ascii_lowercase();
        let host = lowered.strip_suffix('.').unwrap_or(lowered.as_str());
        self.groups.get(host)
    }

    /// Every service group (sorted by name for stable diagnostics).
    pub fn groups(&self) -> impl Iterator<Item = &Arc<ServiceGroup>> {
        self.groups.values()
    }

    /// Total discovered instances across groups (deduplicated by id).
    pub fn instance_count(&self) -> usize {
        let mut ids: Vec<&str> = self
            .groups
            .values()
            .flat_map(|group| group.instances.iter().map(|instance| instance.id.as_str()))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    }
}

/// One instance's membership in the topology with the service domains it
/// serves and an optional per-instance strategy override.
#[derive(Clone, Debug)]
pub struct ServiceMembership {
    pub instance: Arc<DiscoveredInstance>,
    /// Lowercased served domains (request hosts) mapped to this instance.
    pub served_domains: Vec<String>,
    /// Per-service strategy override (flexible configuration seam).
    pub strategy: Option<LoadBalancingStrategy>,
}

/// Wire projection of the whole snapshot (admin diagnostics).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterTopologyView {
    pub cluster_code: String,
    pub built_at_millis: u64,
    pub groups: Vec<ServiceGroupView>,
}

impl ClusterTopology {
    /// The wire projection.
    pub fn view(&self) -> ClusterTopologyView {
        ClusterTopologyView {
            cluster_code: self.cluster_code.clone(),
            built_at_millis: self.built_at_millis,
            groups: self.groups.values().map(|group| group.view()).collect(),
        }
    }
}
