use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CertificateOperationResponse {
    pub id: String,

    #[serde(rename = "certificateId")]
    pub certificate_id: String,

    #[serde(rename = "operationType")]
    pub operation_type: String,

    pub status: String,

    #[serde(rename = "attemptCount")]
    pub attempt_count: i64,

    #[serde(rename = "maxAttempts")]
    pub max_attempts: i64,

    #[serde(rename = "nextAttemptAt")]
    pub next_attempt_at: String,

    #[serde(rename = "failureCode")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,

    /// What the failure said, as the named provider worded it, redacted and bounded. The code classifies a failure; this is the part an operator acts on.
    #[serde(rename = "failureDetail")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_detail: Option<String>,

    #[serde(rename = "createdAt")]
    pub created_at: String,

    #[serde(rename = "updatedAt")]
    pub updated_at: String,

    #[serde(rename = "completedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}
