use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CertificateDistributionResponse {
    #[serde(rename = "serverId")]
    pub server_id: String,

    #[serde(rename = "serverName")]
    pub server_name: String,

    pub host: String,

    #[serde(rename = "desiredSyncVersion")]
    pub desired_sync_version: String,

    #[serde(rename = "appliedSyncVersion")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_sync_version: Option<String>,

    pub status: String,

    #[serde(rename = "lastHeartbeatAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<String>,
}
