use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AgentCertificateObservation {
    #[serde(rename = "certificateId")]
    pub certificate_id: String,

    pub fingerprint: String,

    #[serde(rename = "syncVersion")]
    pub sync_version: String,

    pub state: String,

    #[serde(rename = "observedAt")]
    pub observed_at: String,

    #[serde(rename = "failureCode")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
}
