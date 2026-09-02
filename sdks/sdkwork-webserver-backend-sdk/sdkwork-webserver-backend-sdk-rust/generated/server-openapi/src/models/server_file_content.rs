use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ServerFileContent {
    #[serde(rename = "nodeId")]
    pub node_id: String,

    pub path: String,

    /// Decoded text content, bounded by the node read size limit.
    pub content: String,

    pub size: String,
}
