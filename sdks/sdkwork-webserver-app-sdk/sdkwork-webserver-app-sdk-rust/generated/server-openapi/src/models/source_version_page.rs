use serde::{Deserialize, Serialize};

use crate::models::{SourceVersionResponse};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SourceVersionPage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<SourceVersionResponse>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<String>,
}
