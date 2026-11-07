use serde::{Deserialize, Serialize};

use crate::models::{NginxConfigResponse};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct NginxConfigPage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<NginxConfigResponse>>,

    /// Total item count as a string to avoid JavaScript precision loss.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<String>,
}
