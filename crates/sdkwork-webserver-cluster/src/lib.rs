//! SDKWork Web Server cluster routing runtime.
//!
//! The discovery and routing half of the cluster plane (complementing the
//! registry in the service layer): an immutable, atomically swapped
//! [`topology::ClusterTopology`] snapshot feeds a lock-free
//! [`balancer::ClusterLoadBalancer`] that picks healthy instances with
//! industry-standard strategies.
//!
//! Design goals (aligned with NGINX upstream / Kubernetes Services practice):
//!
//! - **Auto discovery**: a [`refresher::TopologyRefresher`] pulls the live
//!   instance inventory from a [`discovery::DiscoverySource`] on a bounded
//!   interval and publishes a new snapshot via `ArcSwap` — readers never
//!   block and never see a torn view.
//! - **Auto routing**: `pick` resolves one healthy instance for a request
//!   key (client IP, arbitrary hash key, or none for pure strategies),
//!   honoring per-instance weight and health.
//! - **High concurrency**: the pick path uses atomics only (`AtomicU64`
//!   counters, `AtomicI64` smooth-WRR currents, `AtomicU32` in-flight
//!   leases); no mutexes, no allocation on the hot path.
//! - **Flexible configuration**: strategies are named strings in
//!   configuration (`round_robin` default, `least_connections`, `random`,
//!   `random_two_choices`, `ip_hash`, `consistent_hash`,
//!   `weighted_round_robin`) and can be overridden per request.

pub mod balancer;
pub mod discovery;
pub mod instance;
pub mod refresher;
pub mod ring;
pub mod strategy;
pub mod topology;

pub use balancer::{ClusterLoadBalancer, InstanceLease, PickDecision, PickKey};
pub use discovery::{DiscoverySource, StaticDiscoverySource};
pub use instance::{DiscoveredInstance, InstanceHealth, InstanceStatus};
pub use ring::ConsistentHashRing;
pub use strategy::LoadBalancingStrategy;
pub use topology::{ClusterTopology, ServiceGroup};

/// The default strategy for cluster request routing: plain round robin —
/// the industry's most widely deployed default (nginx upstream default),
/// predictable and perfectly fair for homogeneous instances.
pub const DEFAULT_LOAD_BALANCING_STRATEGY: LoadBalancingStrategy =
    LoadBalancingStrategy::RoundRobin;
