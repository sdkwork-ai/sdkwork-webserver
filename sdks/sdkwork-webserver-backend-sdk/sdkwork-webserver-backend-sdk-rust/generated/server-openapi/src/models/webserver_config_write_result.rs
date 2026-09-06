use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WebserverConfigWriteResult {
    pub id: String,

    pub path: String,

    pub size: String,

    /// SHA-256 hex digest of the written content.
    pub sha256: String,

    /// Backup file holding the previous content, when an existing file was overwritten.
    #[serde(rename = "backupPath")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,

    /// Modification time of the written file, Unix seconds.
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}
