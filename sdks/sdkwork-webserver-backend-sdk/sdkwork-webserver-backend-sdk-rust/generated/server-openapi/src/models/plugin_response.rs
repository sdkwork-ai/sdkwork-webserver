use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PluginResponse {
    pub id: String,

    #[serde(rename = "pluginKey")]
    pub plugin_key: String,

    #[serde(rename = "displayName")]
    pub display_name: String,

    pub summary: String,

    pub version: String,

    #[serde(rename = "categoryId")]
    pub category_id: String,

    #[serde(rename = "supportedHostTools")]
    pub supported_host_tools: Vec<String>,

    #[serde(rename = "contributedCapabilities")]
    pub contributed_capabilities: Vec<String>,

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

    pub status: String,

    #[serde(rename = "createdAt")]
    pub created_at: String,

    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}
