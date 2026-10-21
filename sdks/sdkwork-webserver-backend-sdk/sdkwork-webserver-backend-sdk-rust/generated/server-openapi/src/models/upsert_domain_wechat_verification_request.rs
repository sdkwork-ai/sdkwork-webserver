use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct UpsertDomainWechatVerificationRequest {
    /// File name issued by the platform; must end with .txt.
    #[serde(rename = "fileName")]
    pub file_name: String,

    /// Verbatim file content, stored and returned byte for byte.
    pub content: String,
}
