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

    /// The `deploy_app.id` of the sdkwork-deployments application this
    /// carrier record carries.
    ///
    /// Required. Deploy is the single application identity and publishing
    /// authority; a webserver application exists only to carry a site,
    /// domain, certificate, and reverse-proxy configuration for a Deploy
    /// application, and the link is 1:1 (unique per live carrier row). The
    /// service rejects a missing value with a validation error that names
    /// this field - the webserver never mints an application identity of
    /// its own.
    #[serde(rename = "deployApplicationId")]
    pub deploy_application_id: String,

    #[serde(rename = "runtimeConfig")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_config: Option<std::collections::HashMap<String, serde_json::Value>>,

    #[serde(rename = "storeListing")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_listing: Option<ApplicationStoreListing>,
}
