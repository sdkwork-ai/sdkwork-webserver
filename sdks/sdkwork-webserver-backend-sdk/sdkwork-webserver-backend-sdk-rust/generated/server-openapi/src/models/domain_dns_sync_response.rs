use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DomainDnsSyncResponse {
    #[serde(rename = "recordCount")]
    pub record_count: String,

    #[serde(rename = "syncedAt")]
    pub synced_at: String,

    /// The zone apex the provider inventory was read for.
    #[serde(rename = "zoneApex")]
    pub zone_apex: String,

    #[serde(rename = "dnsProvider")]
    pub dns_provider: String,

    #[serde(rename = "cloudAccountId")]
    pub cloud_account_id: String,
}
