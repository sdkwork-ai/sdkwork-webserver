use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ServerEntry {
    pub name: String,

    pub kind: String,

    pub path: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,

    #[serde(rename = "projectType")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_type: Option<String>,

    #[serde(rename = "isProjectRoot")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_project_root: Option<bool>,
}
