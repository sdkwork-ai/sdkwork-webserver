use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreateRootDomainHostnameRequest {
    /// Relative hostname such as @, www, or api.internal. The apex marker `@` resolves to the root domain's own hostname. The single value `*` declares the hostname in wildcard form (`*.example.com`), which is the only shape stored as a `WILDCARD` hostname and therefore the only hostname a certificate with `certificateScope` `WILDCARD` can cover.
    #[serde(rename = "recordName")]
    pub record_name: String,

    #[serde(rename = "applicationId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_id: Option<String>,

    #[serde(rename = "isPrimary")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_primary: Option<bool>,

    #[serde(rename = "sslEnabled")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_enabled: Option<bool>,

    #[serde(rename = "sslProvider")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_provider: Option<String>,
}
