use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreateDomainRequest {
    pub hostname: String,

    #[serde(rename = "isPrimary")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_primary: Option<bool>,

    #[serde(rename = "sslEnabled")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_enabled: Option<bool>,

    #[serde(rename = "sslProvider")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_provider: Option<String>,
}
