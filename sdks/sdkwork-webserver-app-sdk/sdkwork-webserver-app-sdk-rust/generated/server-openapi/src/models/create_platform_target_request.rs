use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreatePlatformTargetRequest {
    #[serde(rename = "targetKey")]
    pub target_key: String,

    pub platform: String,

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

    /// Platform application id (WeChat / Douyin mini program)
    #[serde(rename = "appId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,

    #[serde(rename = "bundleName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_name: Option<String>,

    #[serde(rename = "allowedChannels")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_channels: Option<Vec<String>>,
}
