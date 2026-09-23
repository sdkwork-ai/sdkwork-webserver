use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct UpdateClusterRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<i64>,

    #[serde(rename = "heartbeatIntervalSeconds")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat_interval_seconds: Option<i64>,

    #[serde(rename = "offlineThresholdSeconds")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offline_threshold_seconds: Option<i64>,

    /// Request routing strategy across the cluster's instances.
    #[serde(rename = "lbStrategy")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lb_strategy: Option<String>,

    /// Service domains auto-routed to this cluster's instances; replaces the whole list when present.
    #[serde(rename = "servedDomains")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub served_domains: Option<Vec<String>>,
}
