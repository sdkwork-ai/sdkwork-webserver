use serde::{Deserialize, Serialize};

use crate::models::{ServerEntry};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ServerDirectoryListing {
    #[serde(rename = "nodeId")]
    pub node_id: String,

    pub path: String,

    #[serde(rename = "parentPath")]
    pub parent_path: String,

    pub entries: Vec<ServerEntry>,
}
