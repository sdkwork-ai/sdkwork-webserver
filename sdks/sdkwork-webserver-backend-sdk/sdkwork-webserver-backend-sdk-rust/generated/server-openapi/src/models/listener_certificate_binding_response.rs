use serde::{Deserialize, Serialize};

use crate::models::ListenerCertificateSummaryResponse;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ListenerCertificateBindingResponse {
    pub id: String,

    #[serde(rename = "siteId")]
    pub site_id: String,

    #[serde(rename = "domainId")]
    pub domain_id: String,

    #[serde(rename = "certificateId")]
    pub certificate_id: String,

    #[serde(rename = "desiredCertificateVersionId")]
    pub desired_certificate_version_id: String,

    #[serde(rename = "currentCertificateVersionId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_certificate_version_id: Option<String>,

    #[serde(rename = "desiredCertificate")]
    pub desired_certificate: ListenerCertificateSummaryResponse,

    #[serde(rename = "currentCertificate")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_certificate: Option<ListenerCertificateSummaryResponse>,

    #[serde(rename = "keyAlgorithm")]
    pub key_algorithm: String,

    pub priority: i64,

    #[serde(rename = "isDefault")]
    pub is_default: bool,

    pub status: String,

    #[serde(rename = "activatedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activated_at: Option<String>,

    #[serde(rename = "createdAt")]
    pub created_at: String,

    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}
