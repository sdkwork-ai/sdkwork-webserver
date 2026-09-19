use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct EnqueueClusterPeerMessagesRequest {
    #[serde(rename = "clusterId")]
    pub cluster_id: String,

    /// Target instance; omitted broadcasts to every online member.
    #[serde(rename = "toInstanceId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_instance_id: Option<String>,

    /// Sender instance; omitted for control-plane-originated messages.
    #[serde(rename = "fromInstanceId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_instance_id: Option<String>,

    #[serde(rename = "messageType")]
    pub message_type: String,

    /// Message payload JSON; bounded to 16 KiB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<std::collections::HashMap<String, serde_json::Value>>,

    #[serde(rename = "expiresInSeconds")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in_seconds: Option<i64>,
}
