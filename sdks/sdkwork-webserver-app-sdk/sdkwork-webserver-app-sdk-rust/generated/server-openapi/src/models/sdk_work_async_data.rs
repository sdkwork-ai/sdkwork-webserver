use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SdkWorkAsyncData {
    pub accepted: bool,

    #[serde(rename = "operationId")]
    pub operation_id: String,

    pub status: String,

    #[serde(rename = "pollUrl")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_url: Option<String>,
}
