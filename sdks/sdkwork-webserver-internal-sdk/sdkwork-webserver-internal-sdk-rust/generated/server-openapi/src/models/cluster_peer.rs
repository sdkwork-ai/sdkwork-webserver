use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterPeer {
    #[serde(rename = "instanceId")]
    pub instance_id: String,

    pub name: String,

    pub role: String,

    pub status: i64,

    #[serde(rename = "hostName")]
    pub host_name: String,

    #[serde(rename = "remoteIp")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_ip: Option<String>,

    #[serde(rename = "publicEndpoint")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_endpoint: Option<String>,

    pub environment: String,

    #[serde(rename = "buildVersion")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_version: Option<String>,

    #[serde(rename = "lastHeartbeatAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<String>,
}
