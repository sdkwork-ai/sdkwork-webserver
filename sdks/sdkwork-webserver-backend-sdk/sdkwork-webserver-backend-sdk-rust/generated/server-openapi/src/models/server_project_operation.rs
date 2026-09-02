use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ServerProjectOperation {
    pub id: String,

    pub kind: String,

    pub label: String,

    /// IAM permission required to invoke the operation.
    pub permission: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dangerous: Option<bool>,
}
