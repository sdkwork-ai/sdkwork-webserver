use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ServerFilesNode {
    pub id: String,

    pub name: String,

    pub host: String,

    #[serde(rename = "sshPort")]
    pub ssh_port: i64,

    pub status: String,

    /// Authorized filesystem root the node may browse (e.g. /opt/deploy).
    #[serde(rename = "filesystemRoot")]
    pub filesystem_root: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}
