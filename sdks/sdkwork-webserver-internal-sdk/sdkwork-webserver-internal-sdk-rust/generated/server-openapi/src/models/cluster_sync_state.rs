use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterSyncState {
    pub kind: String,

    #[serde(rename = "desiredRevision")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desired_revision: Option<String>,

    #[serde(rename = "appliedRevision")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_revision: Option<String>,

    pub status: String,

    #[serde(rename = "updatedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
