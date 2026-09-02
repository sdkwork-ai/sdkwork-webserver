use serde::{Deserialize, Serialize};

use crate::models::{CertificateIdentifierResponse};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ListenerCertificateSummaryResponse {
    #[serde(rename = "certName")]
    pub cert_name: String,

    pub identifiers: Vec<CertificateIdentifierResponse>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,

    #[serde(rename = "notAfter")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_after: Option<String>,

    pub status: String,
}
