use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterInstanceResponse {
    pub id: String,

    #[serde(rename = "clusterId")]
    pub cluster_id: String,

    #[serde(rename = "hostId")]
    pub host_id: String,

    #[serde(rename = "hostName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_name: Option<String>,

    pub name: String,

    pub role: String,

    pub environment: String,

    #[serde(rename = "processPid")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_pid: Option<i64>,

    #[serde(rename = "processStartedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_started_at: Option<String>,

    #[serde(rename = "bindHost")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind_host: Option<String>,

    #[serde(rename = "bindPort")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind_port: Option<i64>,

    #[serde(rename = "publicEndpoint")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_endpoint: Option<String>,

    #[serde(rename = "buildVersion")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_version: Option<String>,

    /// 0=offline, 1=online, 2=starting, 3=stopping, 4=error, 5=maintenance
    pub status: i64,

    #[serde(rename = "healthState")]
    pub health_state: String,

    #[serde(rename = "lastHeartbeatAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<String>,

    #[serde(rename = "lastOnlineAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_online_at: Option<String>,

    #[serde(rename = "uptimeSeconds")]
    pub uptime_seconds: String,

    /// Latest resource metrics snapshot (CPU/memory/connections).
    pub metrics: std::collections::HashMap<String, serde_json::Value>,

    #[serde(rename = "createdAt")]
    pub created_at: String,

    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}
