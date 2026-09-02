use serde::{Deserialize, Serialize};

use crate::models::{RuntimeAssignment, WebsiteRuntimeSetSnapshot};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RuntimeAssignmentDelivery {
    pub unchanged: bool,

    pub assignment: RuntimeAssignment,

    #[serde(rename = "latestObservationState")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_observation_state: Option<String>,

    #[serde(rename = "runtimeSet")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_set: Option<WebsiteRuntimeSetSnapshot>,
}
