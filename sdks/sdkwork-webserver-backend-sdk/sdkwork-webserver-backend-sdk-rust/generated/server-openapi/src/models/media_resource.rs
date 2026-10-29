use serde::{Deserialize, Serialize};

use crate::models::MediaChecksum;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MediaResource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    pub kind: String,

    pub source: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    #[serde(rename = "publicUrl")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_url: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,

    #[serde(rename = "objectBlobId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_blob_id: Option<String>,

    #[serde(rename = "fileName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,

    #[serde(rename = "mimeType")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,

    #[serde(rename = "sizeBytes")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<MediaChecksum>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,

    #[serde(rename = "durationSeconds")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<f64>,

    #[serde(rename = "altText")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt_text: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<std::collections::HashMap<String, serde_json::Value>>,
}
