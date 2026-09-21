//! Auto discovery: the seam between the registry (database-backed) and the
//! routing runtime (in-memory snapshots).
//!
//! The data plane depends only on the [`DiscoverySource`] trait; the
//! database-backed implementation lives in the management assembly, keeping
//! the layering clean (PRD: 高内聚低耦合).

use std::sync::Arc;

use async_trait::async_trait;

use super::strategy::LoadBalancingStrategy;
use super::topology::ServiceMembership;

/// Everything the topology builder needs for one cluster snapshot.
#[derive(Debug, Clone)]
pub struct DiscoveredCluster {
    /// Cluster code.
    pub cluster_code: String,
    /// Cluster-configured default strategy (from `webserver_cluster`).
    pub strategy: LoadBalancingStrategy,
    /// Per-instance membership (instances × served domains).
    pub members: Vec<ServiceMembership>,
}

/// Fetches the live cluster topology for auto discovery.
#[async_trait]
pub trait DiscoverySource: Send + Sync {
    /// Returns the discovered cluster, or `None` when the cluster code is
    /// unknown / disabled. Implementations must be bounded (timeouts +
    /// limits live in the SQL).
    async fn discover(&self, cluster_code: &str) -> Option<DiscoveredCluster>;
}

/// A discovery source backed by a fixed membership set — tests and static
/// deployments.
#[derive(Clone)]
pub struct StaticDiscoverySource {
    cluster: Option<DiscoveredCluster>,
}

impl StaticDiscoverySource {
    /// Builds a source always returning `cluster`.
    pub fn always(cluster: DiscoveredCluster) -> Self {
        Self {
            cluster: Some(cluster),
        }
    }

    /// Builds a source that discovers nothing (auto-routing disabled).
    pub fn never() -> Self {
        Self { cluster: None }
    }
}

#[async_trait]
impl DiscoverySource for StaticDiscoverySource {
    async fn discover(&self, cluster_code: &str) -> Option<DiscoveredCluster> {
        self.cluster
            .as_ref()
            .filter(|cluster| cluster.cluster_code == cluster_code)
            .cloned()
    }
}

/// Type-erased shared handle for composition sites.
pub type SharedDiscoverySource = Arc<dyn DiscoverySource>;
