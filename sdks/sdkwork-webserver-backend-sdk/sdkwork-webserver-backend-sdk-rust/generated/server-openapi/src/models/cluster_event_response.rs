use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterEventResponse {
    pub id: String,

    #[serde(rename = "clusterId")]
    pub cluster_id: String,

    #[serde(rename = "hostId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_id: Option<String>,

    #[serde(rename = "instanceId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,

    #[serde(rename = "eventType")]
    pub event_type: String,

    pub severity: String,

    pub message: String,

    pub detail: std::collections::HashMap<String, serde_json::Value>,

    #[serde(rename = "occurredAt")]
    pub occurred_at: String,

    #[serde(rename = "createdAt")]
    pub created_at: String,
}
