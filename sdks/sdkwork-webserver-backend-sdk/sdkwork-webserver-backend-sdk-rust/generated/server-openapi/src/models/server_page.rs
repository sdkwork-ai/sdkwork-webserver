use serde::{Deserialize, Serialize};

use crate::models::ServerResponse;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ServerPage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<ServerResponse>>,

    /// Total item count as a string to avoid JavaScript precision loss.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<String>,
}
