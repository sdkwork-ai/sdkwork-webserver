use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ServerOperationResult {
    #[serde(rename = "operationId")]
    pub operation_id: String,

    #[serde(rename = "exitCode")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}
