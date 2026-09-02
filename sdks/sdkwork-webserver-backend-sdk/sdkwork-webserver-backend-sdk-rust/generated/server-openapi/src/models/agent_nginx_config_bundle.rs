use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AgentNginxConfigBundle {
    #[serde(rename = "configId")]
    pub config_id: String,

    pub domain: String,

    #[serde(rename = "configContent")]
    pub config_content: String,

    /// SHA-256 hex digest of configContent.
    pub fingerprint: String,

    /// Config revision number as a string to avoid JavaScript precision loss.
    pub version: String,
}
