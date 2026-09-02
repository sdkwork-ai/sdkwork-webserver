use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct FieldError {
    pub field: String,

    pub message: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<i64>,
}
