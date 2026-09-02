use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct UpdateDomainApplicationBindingRequest {
    #[serde(rename = "applicationId")]
    pub application_id: String,

    #[serde(rename = "isPrimary")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_primary: Option<bool>,
}
