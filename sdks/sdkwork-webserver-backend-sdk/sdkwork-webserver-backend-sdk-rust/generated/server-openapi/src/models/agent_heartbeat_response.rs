use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AgentHeartbeatResponse {
    #[serde(rename = "serverId")]
    pub server_id: String,

    pub status: i64,

    #[serde(rename = "acknowledgedAt")]
    pub acknowledged_at: String,
}
