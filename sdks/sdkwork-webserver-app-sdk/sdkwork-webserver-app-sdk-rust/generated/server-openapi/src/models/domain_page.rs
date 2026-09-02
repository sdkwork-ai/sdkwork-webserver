use serde::{Deserialize, Serialize};

use crate::models::{DomainResponse};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DomainPage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<DomainResponse>>,

    /// Total item count as a string to avoid JavaScript precision loss.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<String>,
}
