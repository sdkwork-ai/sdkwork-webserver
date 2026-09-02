use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct NginxDeployResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,

    #[serde(rename = "configId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_id: Option<String>,

    #[serde(rename = "deployedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployed_at: Option<String>,

    #[serde(rename = "reloadResult")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reload_result: Option<serde_json::Value>,
}
