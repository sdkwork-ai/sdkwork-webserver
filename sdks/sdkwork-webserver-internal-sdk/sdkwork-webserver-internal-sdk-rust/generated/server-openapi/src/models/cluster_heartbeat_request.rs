use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterHeartbeatRequest {
    pub status: i64,

    #[serde(rename = "healthState")]
    pub health_state: String,

    #[serde(rename = "uptimeSeconds")]
    pub uptime_seconds: String,

    #[serde(rename = "buildVersion")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_version: Option<String>,

    /// Resource metrics snapshot (CPU/memory/connections); bounded to 16 KiB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<std::collections::HashMap<String, serde_json::Value>>,
}
