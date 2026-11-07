use serde::{Deserialize, Serialize};

use crate::models::{ApplicationSourceVersionConfigSnapshot};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ApplicationSourceVersionResponse {
    pub id: String,

    #[serde(rename = "siteId")]
    pub site_id: String,

    #[serde(rename = "versionTag")]
    pub version_tag: String,

    #[serde(rename = "sourceType")]
    pub source_type: String,

    #[serde(rename = "sourceRef")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,

    #[serde(rename = "commitHash")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,

    #[serde(rename = "artifactDriveUri")]
    pub artifact_drive_uri: String,

    #[serde(rename = "artifactSize")]
    pub artifact_size: String,

    #[serde(rename = "artifactHash")]
    pub artifact_hash: String,

    #[serde(rename = "configSnapshot")]
    pub config_snapshot: ApplicationSourceVersionConfigSnapshot,

    pub status: i64,

    pub retained: bool,

    #[serde(rename = "createdAt")]
    pub created_at: String,
}
