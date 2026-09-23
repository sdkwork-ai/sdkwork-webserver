use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ProbeClusterInstanceRequest {
    /// Probe path; defaults to `/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Probe timeout, clamped to 100..=10000 ms.
    #[serde(rename = "timeoutMs")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<i64>,
}
