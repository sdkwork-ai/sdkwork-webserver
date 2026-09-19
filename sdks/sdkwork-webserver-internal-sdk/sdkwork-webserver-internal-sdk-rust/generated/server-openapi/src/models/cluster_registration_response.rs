use serde::{Deserialize, Serialize};

use crate::models::{ClusterHostRef, ClusterInstanceRef, ClusterPeer, ClusterRef};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterRegistrationResponse {
    pub cluster: ClusterRef,

    pub host: ClusterHostRef,

    pub instance: ClusterInstanceRef,

    /// Secret heartbeat token for this instance (winst_ prefix); store securely.
    #[serde(rename = "instanceToken")]
    pub instance_token: String,

    #[serde(rename = "heartbeatIntervalSeconds")]
    pub heartbeat_interval_seconds: i64,

    #[serde(rename = "offlineThresholdSeconds")]
    pub offline_threshold_seconds: i64,

    pub peers: Vec<ClusterPeer>,
}
