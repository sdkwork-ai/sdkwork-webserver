use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RootDomainResponse {
    pub id: String,

    pub hostname: String,

    /// Operator-facing label; the apex hostname remains the identity.
    #[serde(rename = "displayName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,

    /// DNS provider declaration, or the literal manual when records are published by hand.
    #[serde(rename = "dnsProvider")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_provider: Option<String>,

    /// Provider-side zone identifier.
    #[serde(rename = "providerZoneRef")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_zone_ref: Option<String>,

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
