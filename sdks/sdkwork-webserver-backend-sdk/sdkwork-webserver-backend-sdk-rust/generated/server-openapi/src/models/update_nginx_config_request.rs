use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct UpdateNginxConfigRequest {
    #[serde(rename = "configContent")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_content: Option<String>,

    #[serde(rename = "configName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_name: Option<String>,
}
