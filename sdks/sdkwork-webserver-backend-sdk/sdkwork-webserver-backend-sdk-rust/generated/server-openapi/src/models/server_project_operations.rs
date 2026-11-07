use serde::{Deserialize, Serialize};

use crate::models::{ServerProjectOperation};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ServerProjectOperations {
    #[serde(rename = "nodeId")]
    pub node_id: String,

    pub path: String,

    #[serde(rename = "projectType")]
    pub project_type: String,

    pub operations: Vec<ServerProjectOperation>,
}
