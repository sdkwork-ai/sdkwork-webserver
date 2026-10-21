use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DomainWechatVerificationCheckResponse {
    /// Whether any protocol fetched a 2xx response.
    pub reachable: bool,

    /// Whether the fetched content matches the stored file byte for byte.
    pub matched: bool,

    /// HTTP status code of the matched attempt.
    #[serde(rename = "statusCode")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_code: Option<i64>,

    /// Matched scheme (https or http).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,

    /// One-line fact when the check did not pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,

    /// Time of the check.
    #[serde(rename = "checkedAt")]
    pub checked_at: String,
}
