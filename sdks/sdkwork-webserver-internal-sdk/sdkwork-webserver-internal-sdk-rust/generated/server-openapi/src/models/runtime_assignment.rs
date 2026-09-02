use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RuntimeAssignment {
    #[serde(rename = "assignmentUuid")]
    pub assignment_uuid: String,

    #[serde(rename = "nodeUuid")]
    pub node_uuid: String,

    pub environment: String,

    pub generation: String,

    #[serde(rename = "snapshotUuid")]
    pub snapshot_uuid: String,

    #[serde(rename = "snapshotSha256")]
    pub snapshot_sha256: String,

    #[serde(rename = "assignedAt")]
    pub assigned_at: String,
}
