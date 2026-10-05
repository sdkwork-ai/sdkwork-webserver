use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterProbeRunResponse {
    /// Probe reached the instance and got a healthy answer.
    pub healthy: bool,

    /// Round-trip latency of the probe in milliseconds.
    #[serde(rename = "latencyMs")]
    pub latency_ms: String,

    /// Consecutive probe failures after this run; 0 when healthy.
    pub failures: i64,

    /// Auto-eject transition happened on this run.
    pub ejected: bool,

    /// Auto-recovery transition happened on this run.
    pub recovered: bool,
}
