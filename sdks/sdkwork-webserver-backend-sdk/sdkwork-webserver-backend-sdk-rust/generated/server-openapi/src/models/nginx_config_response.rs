use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct NginxConfigResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(rename = "configType")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_type: Option<i64>,

    #[serde(rename = "configName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_name: Option<String>,

    #[serde(rename = "configContent")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_content: Option<String>,

    #[serde(rename = "configHash")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_hash: Option<String>,

    #[serde(rename = "isActive")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,

    #[serde(rename = "versionNo")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_no: Option<i64>,

    #[serde(rename = "deployedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployed_at: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<i64>,

    #[serde(rename = "createdAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,

    #[serde(rename = "updatedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
