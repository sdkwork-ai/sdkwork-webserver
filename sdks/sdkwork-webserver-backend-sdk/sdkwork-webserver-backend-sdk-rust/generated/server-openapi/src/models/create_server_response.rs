use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreateServerResponse {
    pub id: String,

    pub name: String,

    pub host: String,

    #[serde(rename = "tenantScopeHash")]
    pub tenant_scope_hash: String,

    #[serde(rename = "sshPort")]
    pub ssh_port: i64,

    /// 0=offline, 1=online
    pub status: i64,

    #[serde(rename = "lastHeartbeatAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<String>,

    #[serde(rename = "createdAt")]
    pub created_at: String,

    /// Bootstrap agent credential; returned once at registration.
    #[serde(rename = "agentToken")]
    pub agent_token: String,
}
