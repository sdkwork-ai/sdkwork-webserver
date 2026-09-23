use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PublishClusterSyncRequest {
    /// Desired-state track to publish.
    pub kind: String,

    /// Track-specific desired-state payload; opaque to the transport.
    pub payload: std::collections::HashMap<String, serde_json::Value>,
}
