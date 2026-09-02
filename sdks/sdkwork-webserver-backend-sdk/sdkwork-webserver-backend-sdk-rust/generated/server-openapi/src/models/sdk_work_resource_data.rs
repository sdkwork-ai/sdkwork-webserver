use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SdkWorkResourceData {
    /// Typed domain resource for the operation.
    pub item: std::collections::HashMap<String, serde_json::Value>,
}
