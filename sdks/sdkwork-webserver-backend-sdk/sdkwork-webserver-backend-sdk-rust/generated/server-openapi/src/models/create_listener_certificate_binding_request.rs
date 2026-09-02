use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreateListenerCertificateBindingRequest {
    #[serde(rename = "certificateId")]
    pub certificate_id: String,

    /// Immutable certificate version. Omit to bind the certificate's current active version.
    #[serde(rename = "certificateVersionId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate_version_id: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,

    #[serde(rename = "isDefault")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
}
