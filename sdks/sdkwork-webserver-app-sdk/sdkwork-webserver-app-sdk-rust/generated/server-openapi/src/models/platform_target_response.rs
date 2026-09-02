use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PlatformTargetResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(rename = "appId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,

    #[serde(rename = "targetKey")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_key: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,

    #[serde(rename = "techStack")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tech_stack: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub architectures: Option<Vec<String>>,

    #[serde(rename = "bundleId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,

    #[serde(rename = "packageName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_name: Option<String>,

    #[serde(rename = "appIdValue")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id_value: Option<String>,

    #[serde(rename = "bundleName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_name: Option<String>,

    #[serde(rename = "targetStatus")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_status: Option<String>,

    #[serde(rename = "createdAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,

    #[serde(rename = "updatedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
