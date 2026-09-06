use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WebserverConfigEntry {
    /// Stable, content-independent catalog id (SHA-256 over the catalog identity).
    pub id: String,

    pub kind: String,

    pub name: String,

    /// Posix path relative to the owning group root.
    pub path: String,

    /// Editor language id for the file content.
    pub language: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,

    /// Last modification time, Unix seconds.
    #[serde(rename = "updatedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,

    /// Module sidecar configs are read-only by ownership contract.
    pub writable: bool,
}
