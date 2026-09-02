use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct NginxValidateResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<serde_json::Value>>,
}
