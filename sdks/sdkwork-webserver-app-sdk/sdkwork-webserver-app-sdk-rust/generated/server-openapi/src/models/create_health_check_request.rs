use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreateHealthCheckRequest {
    #[serde(rename = "checkType")]
    pub check_type: i64,

    #[serde(rename = "checkUrl")]
    pub check_url: String,

    #[serde(rename = "checkInterval")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_interval: Option<i64>,

    #[serde(rename = "timeoutMs")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<i64>,

    #[serde(rename = "retryCount")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<i64>,
}
