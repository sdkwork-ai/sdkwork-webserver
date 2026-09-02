use serde::{Deserialize, Serialize};

use crate::models::{WebsiteRuntimeSetSnapshot};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PublishRuntimeAssignmentRequest {
    #[serde(rename = "runtimeSet")]
    pub runtime_set: WebsiteRuntimeSetSnapshot,
}
