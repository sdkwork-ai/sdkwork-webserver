use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterDrainCompleteResponse {
    #[serde(rename = "acknowledgedAt")]
    pub acknowledged_at: String,
}
