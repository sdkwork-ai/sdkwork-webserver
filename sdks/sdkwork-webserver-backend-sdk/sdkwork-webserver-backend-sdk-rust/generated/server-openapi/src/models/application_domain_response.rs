use serde::{Deserialize, Serialize};

use crate::models::DomainDeploymentResponse;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ApplicationDomainResponse {
    pub id: String,

    pub hostname: String,

    #[serde(rename = "rootDomainId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_domain_id: Option<String>,

    #[serde(rename = "recordName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_name: Option<String>,

    #[serde(rename = "applicationId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_id: Option<String>,

    #[serde(rename = "applicationName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_name: Option<String>,

    #[serde(rename = "certificateCount")]
    pub certificate_count: String,

    #[serde(rename = "isPrimary")]
    pub is_primary: bool,

    #[serde(rename = "isVerified")]
    pub is_verified: bool,

    #[serde(rename = "sslEnabled")]
    pub ssl_enabled: bool,

    #[serde(rename = "sslProvider")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_provider: Option<String>,

    pub status: i64,

    #[serde(rename = "latestDeployment")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_deployment: Option<DomainDeploymentResponse>,

    #[serde(rename = "createdAt")]
    pub created_at: String,

    #[serde(rename = "updatedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
