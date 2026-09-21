//! Discovered instance model: the unit of cluster request routing.

use serde::{Deserialize, Serialize};

/// Registry status of one cluster instance (mirrors the cluster plane wire
/// enum: `0=offline, 1=online, 2=starting, 3=stopping, 4=error,
/// 5=maintenance`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InstanceStatus {
    #[default]
    Offline,
    Online,
    Starting,
    Stopping,
    Error,
    Maintenance,
}

impl InstanceStatus {
    /// Parses the registry integer.
    pub fn from_registry(value: i32) -> Self {
        match value {
            1 => Self::Online,
            2 => Self::Starting,
            3 => Self::Stopping,
            4 => Self::Error,
            5 => Self::Maintenance,
            _ => Self::Offline,
        }
    }

    /// True when the instance may accept routed traffic.
    pub fn is_routeable_status(self) -> bool {
        matches!(self, Self::Online)
    }
}

/// Liveness health state (`0=unknown, 1=healthy, 2=degraded, 3=unhealthy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InstanceHealth {
    #[default]
    Unknown,
    Healthy,
    Degraded,
    Unhealthy,
}

impl InstanceHealth {
    /// Parses the registry integer.
    pub fn from_registry(value: i32) -> Self {
        match value {
            1 => Self::Healthy,
            2 => Self::Degraded,
            3 => Self::Unhealthy,
            _ => Self::Unknown,
        }
    }

    /// True when routed traffic is acceptable (degraded still serves).
    pub fn is_routeable_health(self) -> bool {
        matches!(self, Self::Healthy | Self::Degraded)
    }
}

/// One discovered cluster instance eligible (or not) for request routing.
/// Instances are shared by `Arc` (the atomics are the shared strategy
/// state); the value itself is not `Clone` by design.
#[derive(Debug)]
pub struct DiscoveredInstance {
    /// Registry instance uuid.
    pub id: String,
    /// Public endpoint (`https://host:port`) requests are routed to.
    pub endpoint: String,
    /// Relative routing weight (>= 1; used by weighted strategies).
    pub weight: u32,
    pub status: InstanceStatus,
    pub health: InstanceHealth,
    /// Node service quality score 0..=100 (`None` when never reported).
    pub quality_score: Option<i32>,
    /// The host's join mode label (`LAN` / `TUNNEL`) for diagnostics.
    pub join_mode: String,
    /// Cordon/drain/eject gate: `false` removes the instance from the
    /// routing pool even when online (registry-driven ops state).
    pub routing_enabled: bool,
    /// In-flight routed requests (maintained by [`crate::balancer`]).
    pub(crate) inflight: std::sync::atomic::AtomicU32,
    /// Smooth weighted round-robin current weight (strategy scratch).
    pub(crate) current_weight: std::sync::atomic::AtomicI64,
}

impl DiscoveredInstance {
    /// Builds a routeable instance with the default weight.
    pub fn new(id: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            endpoint: endpoint.into(),
            weight: 1,
            status: InstanceStatus::Online,
            health: InstanceHealth::Healthy,
            quality_score: None,
            join_mode: "LAN".to_owned(),
            routing_enabled: true,
            inflight: std::sync::atomic::AtomicU32::new(0),
            current_weight: std::sync::atomic::AtomicI64::new(0),
        }
    }

    /// Sets the routing weight (saturating at 1..=10_000).
    #[must_use]
    pub fn with_weight(mut self, weight: u32) -> Self {
        self.weight = weight.clamp(1, 10_000);
        self
    }

    /// Sets registry status and health.
    #[must_use]
    pub fn with_state(mut self, status: InstanceStatus, health: InstanceHealth) -> Self {
        self.status = status;
        self.health = health;
        self
    }

    /// Sets the reported quality score.
    #[must_use]
    pub fn with_quality_score(mut self, score: Option<i32>) -> Self {
        self.quality_score = score;
        self
    }

    /// Sets the join mode label.
    #[must_use]
    pub fn with_join_mode(mut self, join_mode: impl Into<String>) -> Self {
        self.join_mode = join_mode.into();
        self
    }

    /// True when the instance may receive newly routed requests: online and
    /// not unhealthy. Degraded instances keep serving (graceful degradation,
    /// nginx `max_fails` semantics are handled by the registry sweep).
    pub fn is_routeable(&self) -> bool {
        self.routing_enabled
            && self.status.is_routeable_status()
            && self.health.is_routeable_health()
    }

    /// The current in-flight routed-request count.
    pub fn inflight(&self) -> u32 {
        self.inflight.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Wire projection of a discovered instance (admin/diagnostics surface).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredInstanceView {
    /// Registry instance uuid.
    pub id: String,
    /// Public endpoint.
    pub endpoint: String,
    /// Routing weight.
    pub weight: u32,
    /// Routeable right now.
    pub routeable: bool,
    /// Status label.
    pub status: String,
    /// Health label.
    pub health: String,
    /// Quality score.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_score: Option<i32>,
    /// Join mode.
    pub join_mode: String,
    /// In-flight routed requests.
    pub inflight: u32,
}
