use serde::{Deserialize, Serialize};

use crate::models::{CertificateIdentifierResponse};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CertificateResponse {
    pub id: String,

    #[serde(rename = "certName")]
    pub cert_name: String,

    pub identifiers: Vec<CertificateIdentifierResponse>,

    #[serde(rename = "certType")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cert_type: Option<i64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,

    #[serde(rename = "keyAlgorithm")]
    pub key_algorithm: String,

    #[serde(rename = "notBefore")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_before: Option<String>,

    #[serde(rename = "notAfter")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_after: Option<String>,

    #[serde(rename = "autoRenew")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_renew: Option<bool>,

    #[serde(rename = "renewalStatus")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renewal_status: Option<String>,

    pub status: String,

    #[serde(rename = "createdAt")]
    pub created_at: String,
}
