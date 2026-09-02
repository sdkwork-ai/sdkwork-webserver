use serde::{Deserialize, Serialize};

use crate::models::{ApplicationResponse};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ApplicationPage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<ApplicationResponse>>,

    /// Total item count as a string to avoid JavaScript precision loss.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<i64>,

    #[serde(rename = "pageSize")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_size: Option<i64>,
}
