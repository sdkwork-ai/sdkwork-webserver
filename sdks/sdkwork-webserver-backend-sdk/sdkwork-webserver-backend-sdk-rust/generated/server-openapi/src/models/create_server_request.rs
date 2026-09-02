use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreateServerRequest {
    pub name: String,

    pub host: String,

    /// Irreversible tenant scope bound to runtime-set delivery for this node.
    #[serde(rename = "tenantScopeHash")]
    pub tenant_scope_hash: String,

    #[serde(rename = "sshPort")]
    pub ssh_port: i64,
}
