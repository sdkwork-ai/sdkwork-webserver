use serde::{Deserialize, Serialize};

use crate::models::{ClusterHostDescriptor, ClusterInstanceDescriptor};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterRegistrationRequest {
    /// Target cluster code; omitted registers into the auto-provisioned default cluster.
    #[serde(rename = "clusterCode")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_code: Option<String>,

    pub host: ClusterHostDescriptor,

    pub instance: ClusterInstanceDescriptor,
}
