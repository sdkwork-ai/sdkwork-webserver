use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterHeartbeatSampleResponse {
    pub id: String,

    pub status: i64,

    #[serde(rename = "latencyMs")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<i64>,

    /// Resource metrics snapshot captured at heartbeat time.
    pub metrics: std::collections::HashMap<String, serde_json::Value>,

    #[serde(rename = "reportedAt")]
    pub reported_at: String,
}
