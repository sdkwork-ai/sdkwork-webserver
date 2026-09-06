use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WebserverConfigFile {
    pub id: String,

    pub kind: String,

    pub name: String,

    pub path: String,

    pub language: String,

    pub writable: bool,

    /// Decoded text content, bounded by the read size limit.
    pub content: String,

    pub size: String,

    /// SHA-256 hex digest of the on-disk bytes.
    pub sha256: String,

    /// Last modification time, Unix seconds.
    #[serde(rename = "updatedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
