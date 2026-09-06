use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WebserverConfigWriteRequest {
    /// New text content; NUL-free, within the write size limit, and syntax-validated for JSON/TOML files.
    pub content: String,

    /// When provided, the write is rejected unless the on-disk digest still matches (optimistic concurrency).
    #[serde(rename = "expectedSha256")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_sha256: Option<String>,
}
