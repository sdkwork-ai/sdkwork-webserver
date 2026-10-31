use serde::{Deserialize, Serialize};

use crate::models::AgentCertificateObservation;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AgentHeartbeatRequest {
    #[serde(rename = "agentVersion")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_version: Option<String>,

    #[serde(rename = "nginxEnabled")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nginx_enabled: Option<bool>,

    /// Number of active nginx configs reported by the agent as a string.
    #[serde(rename = "activeConfigs")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_configs: Option<String>,

    /// Last successfully applied syncVersion reported by the edge agent.
    #[serde(rename = "lastSyncVersion")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_version: Option<String>,

    #[serde(rename = "certificateObservations")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate_observations: Option<Vec<AgentCertificateObservation>>,
}
