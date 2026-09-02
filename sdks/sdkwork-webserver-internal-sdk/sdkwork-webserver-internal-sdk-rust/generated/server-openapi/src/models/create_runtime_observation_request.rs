use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreateRuntimeObservationRequest {
    pub generation: String,

    #[serde(rename = "snapshotSha256")]
    pub snapshot_sha256: String,

    pub state: String,

    #[serde(rename = "nodeVersion")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_version: Option<String>,

    #[serde(rename = "reasonCode")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}
