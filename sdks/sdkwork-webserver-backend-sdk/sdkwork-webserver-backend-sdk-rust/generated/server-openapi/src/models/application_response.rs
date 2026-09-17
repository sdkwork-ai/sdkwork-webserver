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

    /// 该应用是否已有源码版本。由 application 列表/详情投影用一次查询算出
（`EXISTS` over `web_source_version`），调用方无需再逐行探测
`applications/{applicationId}/source_versions`。
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
