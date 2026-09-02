use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WebsiteRuntimeSetSnapshot {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,

    pub kind: String,

    #[serde(rename = "snapshotUuid")]
    pub snapshot_uuid: String,

    #[serde(rename = "nodeUuid")]
    pub node_uuid: String,

    pub environment: serde_json::Value,

    pub generation: i64,

    #[serde(rename = "generatedAt")]
    pub generated_at: String,

    #[serde(rename = "compilerVersion")]
    pub compiler_version: String,

    #[serde(rename = "snapshotSha256")]
    pub snapshot_sha256: String,

    #[serde(rename = "maximumSites")]
    pub maximum_sites: i64,

    pub descriptors: Vec<std::collections::HashMap<String, serde_json::Value>>,
}
