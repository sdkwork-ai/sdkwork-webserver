use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SdkWorkCommandData {
    pub accepted: bool,

    #[serde(rename = "resourceId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}
