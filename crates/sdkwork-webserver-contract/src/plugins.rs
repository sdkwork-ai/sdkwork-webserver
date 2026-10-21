//! Per-owner plugin catalog contracts.
//!
//! The catalog is server-owned: each record belongs to one IAM user inside a
//! tenant, so an administrator can review another user's plugins. The wire
//! shape mirrors the console model (`plugin.*` keys, draft/active status,
//! git/archive sources) without exposing storage details.

use serde::{Deserialize, Serialize};

use crate::dto::{default_page, default_page_size};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginResponse {
    /// Server UUID; the wire id is a string, never a JSON number.
    pub id: String,
    pub plugin_key: String,
    pub display_name: String,
    pub summary: String,
    /// Plugin semantic version (the row's optimistic lock is separate).
    pub version: String,
    pub category_id: String,
    pub supported_host_tools: Vec<String>,
    pub contributed_capabilities: Vec<String>,
    pub source_kind: String,
    pub git_repository: Option<String>,
    pub git_ref: Option<String>,
    pub artifact_ref: Option<String>,
    pub checksum_sha256: Option<String>,
    pub archive_file_name: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreatePluginRequest {
    pub plugin_key: String,
    pub display_name: String,
    #[serde(default)]
    pub summary: String,
    pub version: String,
    pub category_id: String,
    #[serde(default)]
    pub supported_host_tools: Vec<String>,
    #[serde(default)]
    pub contributed_capabilities: Vec<String>,
    pub source_kind: String,
    #[serde(default)]
    pub git_repository: Option<String>,
    #[serde(default)]
    pub git_ref: Option<String>,
    #[serde(default)]
    pub artifact_ref: Option<String>,
    #[serde(default)]
    pub checksum_sha256: Option<String>,
    #[serde(default)]
    pub archive_file_name: Option<String>,
    /// Defaults to `draft`: publishing is an explicit transition.
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdatePluginRequest {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub supported_host_tools: Option<Vec<String>>,
    #[serde(default)]
    pub contributed_capabilities: Option<Vec<String>>,
    #[serde(default)]
    pub git_repository: Option<String>,
    #[serde(default)]
    pub git_ref: Option<String>,
    #[serde(default)]
    pub artifact_ref: Option<String>,
    #[serde(default)]
    pub checksum_sha256: Option<String>,
    #[serde(default)]
    pub archive_file_name: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPage {
    pub items: Vec<PluginResponse>,
    /// Exact total in offset mode; cursor mode is not offered for this
    /// per-owner collection because the owner scope is itself the partition.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPluginsQuery {
    #[serde(default = "default_page")]
    pub page: i32,
    #[serde(default = "default_page_size")]
    pub page_size: i32,
    /// `draft` or `active`; omitted means every status.
    #[serde(default)]
    pub status: Option<String>,
    /// Free-text search on display name or plugin key.
    #[serde(default)]
    pub q: Option<String>,
    /// Platform-operator review: read another owner's catalog. Only the
    /// platform operator tenant may set it; for a tenant-bound caller the
    /// owner is always the authenticated operator.
    #[serde(
        default,
        deserialize_with = "sdkwork_utils_rust::serde_int64::option_query::deserialize"
    )]
    pub owner_id: Option<i64>,
}
