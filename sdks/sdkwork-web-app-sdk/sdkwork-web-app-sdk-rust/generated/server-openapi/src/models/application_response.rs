use serde::{Deserialize, Serialize};

use crate::models::{ApplicationStoreListing};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ApplicationResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// The application's backing site id (internal carrier)
    #[serde(rename = "siteId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_id: Option<String>,

    #[serde(rename = "applicationType")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_type: Option<String>,

    #[serde(rename = "siteType")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_type: Option<i64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<i64>,

    #[serde(rename = "runtimeConfig")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_config: Option<std::collections::HashMap<String, serde_json::Value>>,

    #[serde(rename = "storeListing")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_listing: Option<ApplicationStoreListing>,

    #[serde(rename = "createdAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,

    #[serde(rename = "updatedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
