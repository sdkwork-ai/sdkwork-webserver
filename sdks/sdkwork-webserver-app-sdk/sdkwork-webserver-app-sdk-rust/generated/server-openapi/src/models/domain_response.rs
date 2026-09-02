use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DomainResponse {
    pub id: String,

    pub hostname: String,

    #[serde(rename = "applicationId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_id: Option<String>,

    #[serde(rename = "applicationName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_name: Option<String>,

    #[serde(rename = "certificateCount")]
    pub certificate_count: String,

    #[serde(rename = "isPrimary")]
    pub is_primary: bool,

    #[serde(rename = "isVerified")]
    pub is_verified: bool,

    #[serde(rename = "sslEnabled")]
    pub ssl_enabled: bool,

    #[serde(rename = "sslProvider")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_provider: Option<String>,

    pub status: i64,

    #[serde(rename = "createdAt")]
    pub created_at: String,
}
