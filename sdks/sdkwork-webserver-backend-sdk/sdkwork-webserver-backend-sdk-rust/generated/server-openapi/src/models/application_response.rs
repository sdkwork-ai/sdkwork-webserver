use serde::{Deserialize, Serialize};

use crate::models::{ApplicationStoreListing};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ApplicationResponse {
    pub id: String,

    pub name: String,

    pub slug: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(rename = "appKind")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_kind: Option<String>,

    #[serde(rename = "siteType")]
    pub site_type: i64,

    pub status: i64,

    /// Whether the application already has a source version. The application
    /// list and detail projections derive it in a single `EXISTS` query over
    /// `webserver_source_version`, so callers never probe
    /// `applications/{applicationId}/source_versions` row by row.
    #[serde(rename = "hasSourceVersion")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_source_version: Option<bool>,

    #[serde(rename = "runtimeConfig")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_config: Option<std::collections::HashMap<String, serde_json::Value>>,

    #[serde(rename = "storeListing")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_listing: Option<ApplicationStoreListing>,

    #[serde(rename = "createdAt")]
    pub created_at: String,

    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}
