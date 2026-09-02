use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CertificateIdentifierResponse {
    #[serde(rename = "domainId")]
    pub domain_id: String,

    pub hostname: String,

    #[serde(rename = "identifierType")]
    pub identifier_type: String,

    pub position: i64,
}
