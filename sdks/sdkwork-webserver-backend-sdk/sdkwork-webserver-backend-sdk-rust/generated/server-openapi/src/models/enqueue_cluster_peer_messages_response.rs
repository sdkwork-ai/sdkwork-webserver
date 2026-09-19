use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct EnqueueClusterPeerMessagesResponse {
    pub enqueued: String,
}
