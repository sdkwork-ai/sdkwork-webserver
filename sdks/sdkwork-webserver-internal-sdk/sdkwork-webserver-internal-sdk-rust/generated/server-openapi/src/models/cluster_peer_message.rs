use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterPeerMessage {
    pub id: String,

    #[serde(rename = "messageType")]
    pub message_type: String,

    #[serde(rename = "fromInstanceId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_instance_id: Option<String>,

    #[serde(rename = "toInstanceId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_instance_id: Option<String>,

    pub payload: std::collections::HashMap<String, serde_json::Value>,

    #[serde(rename = "createdAt")]
    pub created_at: String,
}
