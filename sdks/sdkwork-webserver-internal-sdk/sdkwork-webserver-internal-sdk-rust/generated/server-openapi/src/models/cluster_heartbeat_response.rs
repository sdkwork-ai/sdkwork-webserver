use serde::{Deserialize, Serialize};

use crate::models::{ClusterPeer, ClusterPeerMessage};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterHeartbeatResponse {
    #[serde(rename = "instanceId")]
    pub instance_id: String,

    pub status: i64,

    #[serde(rename = "acknowledgedAt")]
    pub acknowledged_at: String,

    #[serde(rename = "heartbeatIntervalSeconds")]
    pub heartbeat_interval_seconds: i64,

    #[serde(rename = "offlineThresholdSeconds")]
    pub offline_threshold_seconds: i64,

    pub peers: Vec<ClusterPeer>,

    pub messages: Vec<ClusterPeerMessage>,
}
