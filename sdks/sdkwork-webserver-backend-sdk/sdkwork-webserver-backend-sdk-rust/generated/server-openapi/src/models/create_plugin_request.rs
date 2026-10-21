use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreatePluginRequest {
    #[serde(rename = "pluginKey")]
    pub plugin_key: String,

    #[serde(rename = "displayName")]
    pub display_name: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    pub version: String,

    #[serde(rename = "categoryId")]
    pub category_id: String,

    #[serde(rename = "supportedHostTools")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supported_host_tools: Option<Vec<String>>,

    #[serde(rename = "contributedCapabilities")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contributed_capabilities: Option<Vec<String>>,

    #[serde(rename = "sourceKind")]
    pub source_kind: String,

    #[serde(rename = "gitRepository")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_repository: Option<String>,

    #[serde(rename = "gitRef")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,

    #[serde(rename = "artifactRef")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_ref: Option<String>,

    #[serde(rename = "checksumSha256")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum_sha256: Option<String>,

    #[serde(rename = "archiveFileName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_file_name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}
