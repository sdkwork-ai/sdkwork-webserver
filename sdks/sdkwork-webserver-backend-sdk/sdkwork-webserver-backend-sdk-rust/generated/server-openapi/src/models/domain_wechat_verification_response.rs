use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DomainWechatVerificationResponse {
    #[serde(rename = "rootDomainId")]
    pub root_domain_id: String,

    /// File name issued by the platform (for example MP_verify_xxx.txt); served by exact name.
    #[serde(rename = "fileName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,

    /// Verbatim file content, stored and returned byte for byte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    /// Time of the most recent upload or replacement.
    #[serde(rename = "updatedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
