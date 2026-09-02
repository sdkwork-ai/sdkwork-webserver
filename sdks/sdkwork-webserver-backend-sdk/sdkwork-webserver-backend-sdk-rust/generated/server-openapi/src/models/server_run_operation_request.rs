use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ServerRunOperationRequest {
    pub path: String,

    #[serde(rename = "operationId")]
    pub operation_id: String,
}
