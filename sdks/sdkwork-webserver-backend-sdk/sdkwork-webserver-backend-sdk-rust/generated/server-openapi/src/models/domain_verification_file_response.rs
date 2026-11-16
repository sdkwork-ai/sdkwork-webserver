use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DomainVerificationFileResponse {
    pub id: String,

    #[serde(rename = "rootDomainId")]
    pub root_domain_id: String,

    /// Issuing platform. Grouping/audit only; the edge matches by file name.
    pub platform: String,

    /// File name issued by the platform (for example MP_verify_xxx.txt, google123.html); served by exact name.
    #[serde(rename = "fileName")]
    pub file_name: String,

    /// Verbatim file content, stored and returned byte for byte.
    pub content: String,

    /// Time the file was first registered.
    #[serde(rename = "createdAt")]
    pub created_at: String,

    /// Time of the most recent upload or replacement.
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}
