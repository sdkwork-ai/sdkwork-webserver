use serde::{Deserialize, Serialize};

use crate::models::{AgentCertificateBundle, AgentNginxConfigBundle};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AgentSyncResponse {
    #[serde(rename = "serverId")]
    pub server_id: String,

    /// Stable SHA-256 fingerprint of active nginx configs and certificates for the tenant.
    #[serde(rename = "syncVersion")]
    pub sync_version: String,

    /// True when ifSyncVersion matched syncVersion; bundles are omitted to save bandwidth.
    pub unchanged: bool,

    #[serde(rename = "nginxConfigs")]
    pub nginx_configs: Vec<AgentNginxConfigBundle>,

    pub certificates: Vec<AgentCertificateBundle>,
}
