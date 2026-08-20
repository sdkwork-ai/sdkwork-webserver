use serde::{Deserialize, Serialize};

use crate::models::{ApplicationStoreListing};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreateApplicationRequest {
    pub name: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(rename = "appKind")]
    pub app_kind: String,

    #[serde(rename = "runtimeConfig")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_config: Option<std::collections::HashMap<String, serde_json::Value>>,

    #[serde(rename = "storeListing")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_listing: Option<ApplicationStoreListing>,
}
