use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreateRootDomainRequest {
    pub hostname: String,

    /// Cloud account whose DNS automation this root domain is bound to. Omitted leaves the Zone resolving its account per operation, which is the state every root domain reconciled from the edge's own configuration is in.
    #[serde(rename = "cloudAccountId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_account_id: Option<String>,
}
