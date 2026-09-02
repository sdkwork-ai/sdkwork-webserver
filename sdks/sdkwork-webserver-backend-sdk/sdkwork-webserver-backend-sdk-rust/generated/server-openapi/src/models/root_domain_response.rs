use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RootDomainResponse {
    pub id: String,

    pub hostname: String,

    pub status: i64,

    #[serde(rename = "subdomainCount")]
    pub subdomain_count: String,

    #[serde(rename = "boundSubdomainCount")]
    pub bound_subdomain_count: String,

    #[serde(rename = "verifiedSubdomainCount")]
    pub verified_subdomain_count: String,

    #[serde(rename = "httpsSubdomainCount")]
    pub https_subdomain_count: String,

    #[serde(rename = "activeDeploymentCount")]
    pub active_deployment_count: String,

    #[serde(rename = "createdAt")]
    pub created_at: String,

    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}
