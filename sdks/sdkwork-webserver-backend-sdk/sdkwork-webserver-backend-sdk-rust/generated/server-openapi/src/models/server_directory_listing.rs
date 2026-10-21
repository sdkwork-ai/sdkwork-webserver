use serde::{Deserialize, Serialize};

use crate::models::{PageInfo, ServerEntry};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ServerDirectoryListing {
    #[serde(rename = "nodeId")]
    pub node_id: String,

    pub path: String,

    #[serde(rename = "parentPath")]
    pub parent_path: String,

    pub entries: Vec<ServerEntry>,

    /// True when the directory has more children than the deployment's maximumEntries cap; the listing is truncated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,

    /// Present only when page/pageSize were requested; omitted for the legacy whole-directory response.
    #[serde(rename = "pageInfo")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_info: Option<PageInfo>,
}
