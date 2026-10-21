use serde::{Deserialize, Serialize};

use crate::models::{PageInfo, WebserverConfigEntry};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WebserverConfigCatalog {
    /// Web Server runtime configuration root the catalog was enumerated from.
    #[serde(rename = "configRoot")]
    pub config_root: String,

    pub items: Vec<WebserverConfigEntry>,

    /// True when the deployment has more catalog entries than the server's maximumEntries cap; items is the capped prefix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,

    /// Present only when page/pageSize were requested; omitted for the legacy whole-catalog response.
    #[serde(rename = "pageInfo")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_info: Option<PageInfo>,
}
