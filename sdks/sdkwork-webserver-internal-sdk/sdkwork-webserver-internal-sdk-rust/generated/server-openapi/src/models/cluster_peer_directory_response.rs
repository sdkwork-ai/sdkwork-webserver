use serde::{Deserialize, Serialize};

use crate::models::{ClusterPeer};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterPeerDirectoryResponse {
    #[serde(rename = "instanceId")]
    pub instance_id: String,

    pub peers: Vec<ClusterPeer>,
}
