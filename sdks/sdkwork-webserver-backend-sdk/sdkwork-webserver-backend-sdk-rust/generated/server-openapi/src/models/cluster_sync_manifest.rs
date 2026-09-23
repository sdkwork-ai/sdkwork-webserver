use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterSyncManifest {
    #[serde(rename = "clusterId")]
    pub cluster_id: String,

    pub kind: String,

    pub revision: String,

    /// Canonical payload digest the node verifies before applying.
    pub sha256: String,

    /// Track-specific desired-state payload; opaque to the transport.
    pub payload: std::collections::HashMap<String, serde_json::Value>,

    #[serde(rename = "createdAt")]
    pub created_at: String,
}
