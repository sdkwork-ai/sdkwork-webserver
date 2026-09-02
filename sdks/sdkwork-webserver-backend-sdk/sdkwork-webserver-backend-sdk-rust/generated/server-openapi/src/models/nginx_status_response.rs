use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct NginxStatusResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<i64>,

    #[serde(rename = "activeConnections")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_connections: Option<i64>,

    #[serde(rename = "configPath")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime: Option<String>,
}
