use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterOverviewResponse {
    #[serde(rename = "totalHosts")]
    pub total_hosts: String,

    #[serde(rename = "onlineHosts")]
    pub online_hosts: String,

    #[serde(rename = "totalInstances")]
    pub total_instances: String,

    #[serde(rename = "onlineInstances")]
    pub online_instances: String,

    #[serde(rename = "unhealthyInstances")]
    pub unhealthy_instances: String,

    #[serde(rename = "pendingPeerMessages")]
    pub pending_peer_messages: String,

    #[serde(rename = "generatedAt")]
    pub generated_at: String,
}
