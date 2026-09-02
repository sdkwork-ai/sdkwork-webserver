use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreateNginxConfigRequest {
    #[serde(rename = "configType")]
    pub config_type: i64,

    #[serde(rename = "configName")]
    pub config_name: String,

    #[serde(rename = "configContent")]
    pub config_content: String,

    #[serde(rename = "siteId")]
    pub site_id: String,
}
