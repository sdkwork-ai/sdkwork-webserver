use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterSyncAckRequest {
    pub kind: String,

    pub revision: String,

    pub status: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}
