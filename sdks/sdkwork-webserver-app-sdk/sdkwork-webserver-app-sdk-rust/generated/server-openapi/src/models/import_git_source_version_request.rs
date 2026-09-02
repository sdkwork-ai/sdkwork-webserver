use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ImportGitSourceVersionRequest {
    #[serde(rename = "versionTag")]
    pub version_tag: String,

    #[serde(rename = "repositoryUrl")]
    pub repository_url: String,

    #[serde(rename = "gitRef")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
}
