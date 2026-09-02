use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct HealthCheckResponse {
    pub id: String,

    #[serde(rename = "checkType")]
    pub check_type: i64,

    #[serde(rename = "checkUrl")]
    pub check_url: String,

    #[serde(rename = "checkInterval")]
    pub check_interval: i64,

    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i64,

    #[serde(rename = "retryCount")]
    pub retry_count: i64,

    pub status: i64,

    #[serde(rename = "createdAt")]
    pub created_at: String,
}
