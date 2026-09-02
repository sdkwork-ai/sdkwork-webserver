use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DomainVerifyResponse {
    pub verified: bool,

    pub status: String,

    pub method: String,

    #[serde(rename = "recordName")]
    pub record_name: String,

    #[serde(rename = "recordValue")]
    pub record_value: String,

    #[serde(rename = "attemptCount")]
    pub attempt_count: i64,

    #[serde(rename = "expiresAt")]
    pub expires_at: String,

    #[serde(rename = "nextAttemptAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_attempt_at: Option<String>,

    #[serde(rename = "checkedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<String>,

    #[serde(rename = "failureCode")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
}
