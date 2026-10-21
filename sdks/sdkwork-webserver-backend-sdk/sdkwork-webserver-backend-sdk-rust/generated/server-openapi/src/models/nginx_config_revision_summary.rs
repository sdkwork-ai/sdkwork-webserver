use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct NginxConfigRevisionSummary {
    #[serde(rename = "versionNo")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_no: Option<i64>,

    #[serde(rename = "configName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_name: Option<String>,

    #[serde(rename = "configHash")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_hash: Option<String>,

    #[serde(rename = "createdBy")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,

    #[serde(rename = "createdAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}
