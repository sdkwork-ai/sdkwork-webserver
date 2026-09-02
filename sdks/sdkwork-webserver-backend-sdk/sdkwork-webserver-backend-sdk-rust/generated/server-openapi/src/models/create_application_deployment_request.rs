use serde::{Deserialize, Serialize};

/// Deployment source command. Git deployments (deployType 2) require an HTTPS sourceRef and may omit artifact fields. Other deployment types require artifactDriveUri, artifactSize, and artifactHash together.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreateApplicationDeploymentRequest {
    /// Ready, retained application source version selected for this release.
    #[serde(rename = "sourceVersionId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_version_id: Option<String>,

    /// 1 for a stored package, 2 for a Git repository, 3 for CI/CD, or 4 for API delivery.
    #[serde(rename = "deployType")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy_type: Option<i64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,

    #[serde(rename = "versionTag")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_tag: Option<String>,

    #[serde(rename = "commitHash")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,

    /// HTTPS Git repository URL when deployType is 2. Credentials, query parameters, and fragments are forbidden.
    #[serde(rename = "sourceRef")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,

    /// Stable Drive resource identity for package deployments. Signed delivery URLs are forbidden.
    #[serde(rename = "artifactDriveUri")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_drive_uri: Option<String>,

    #[serde(rename = "artifactSize")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_size: Option<String>,

    /// Lowercase SHA-256 hexadecimal digest of the uploaded package.
    #[serde(rename = "artifactHash")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_hash: Option<String>,
}
