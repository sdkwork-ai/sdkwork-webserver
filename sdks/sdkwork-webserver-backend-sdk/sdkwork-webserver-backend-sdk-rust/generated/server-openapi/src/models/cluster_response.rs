use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterResponse {
    pub id: String,

    pub name: String,

    pub code: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// 0=inactive, 1=active
    pub status: i64,

    #[serde(rename = "heartbeatIntervalSeconds")]
    pub heartbeat_interval_seconds: i64,

    #[serde(rename = "offlineThresholdSeconds")]
    pub offline_threshold_seconds: i64,

    #[serde(rename = "hostCount")]
    pub host_count: String,

    #[serde(rename = "instanceCount")]
    pub instance_count: String,

    #[serde(rename = "onlineInstanceCount")]
    pub online_instance_count: String,

    #[serde(rename = "createdAt")]
    pub created_at: String,

    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}
