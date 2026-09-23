use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreateClusterRequest {
    pub name: String,

    pub code: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(rename = "heartbeatIntervalSeconds")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat_interval_seconds: Option<i64>,

    #[serde(rename = "offlineThresholdSeconds")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offline_threshold_seconds: Option<i64>,

    /// Request routing strategy; defaults to `round_robin` when omitted.
    #[serde(rename = "lbStrategy")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lb_strategy: Option<String>,

    /// Service domains auto-routed to this cluster's instances.
    #[serde(rename = "servedDomains")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub served_domains: Option<Vec<String>>,
}
