use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RuntimeObservation {
    #[serde(rename = "observationUuid")]
    pub observation_uuid: String,

    #[serde(rename = "assignmentUuid")]
    pub assignment_uuid: String,

    #[serde(rename = "tenantId")]
    pub tenant_id: String,

    #[serde(rename = "nodeUuid")]
    pub node_uuid: String,

    pub environment: String,

    pub generation: String,

    #[serde(rename = "snapshotUuid")]
    pub snapshot_uuid: String,

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

    #[serde(rename = "observedAt")]
    pub observed_at: String,
}
