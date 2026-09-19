use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterInstanceDescriptor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    pub role: String,

    pub environment: String,

    #[serde(rename = "processPid")]
    pub process_pid: i64,

    #[serde(rename = "processStartedAt")]
    pub process_started_at: String,

    #[serde(rename = "bindHost")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind_host: Option<String>,

    #[serde(rename = "bindPort")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind_port: Option<i64>,

    /// Advertised endpoint peers use to reach this instance.
    #[serde(rename = "publicEndpoint")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_endpoint: Option<String>,

    #[serde(rename = "buildVersion")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_version: Option<String>,
}
