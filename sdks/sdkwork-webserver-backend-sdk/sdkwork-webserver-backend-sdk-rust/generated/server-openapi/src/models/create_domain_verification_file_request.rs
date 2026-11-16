use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreateDomainVerificationFileRequest {
    /// Issuing platform.
    pub platform: String,

    /// Single-segment ASCII file name ending with .txt, .html or .xml.
    #[serde(rename = "fileName")]
    pub file_name: String,

    /// Verbatim file content, stored and returned byte for byte.
    pub content: String,
}
