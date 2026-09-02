use serde::{Deserialize, Serialize};

use crate::models::{DeploymentResponse};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DeploymentPage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<DeploymentResponse>>,

    /// Total item count as a string to avoid JavaScript precision loss.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<String>,
}
