use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DomainDnsRecordResponse {
    pub id: String,

    /// Absolute record owner inside the zone, e.g. `www.example.com`.
    #[serde(rename = "recordName")]
    pub record_name: String,

    /// Record type as the provider spells it (A, AAAA, CNAME, TXT, MX).
    #[serde(rename = "recordType")]
    pub record_type: String,

    /// Record content — the resolution IP for A/AAAA, the target for CNAME/MX.
    #[serde(rename = "recordValue")]
    pub record_value: String,

    #[serde(rename = "ttlSeconds")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<i64>,

    /// Priority for the record types that carry one (MX, SRV).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,

    /// Provider resolution line, when the provider splits one owner per line.
    #[serde(rename = "recordLine")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_line: Option<String>,

    /// The registered subdomain this record resolves; absent when its owner matches none.
    #[serde(rename = "domainId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_id: Option<String>,

    /// Provider family the snapshot was read from.
    #[serde(rename = "dnsProvider")]
    pub dns_provider: String,

    /// The cloud account the snapshot was read through.
    #[serde(rename = "cloudAccountId")]
    pub cloud_account_id: String,

    /// Provider-assigned record identity, when the provider returned one.
    #[serde(rename = "providerRecordRef")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_record_ref: Option<String>,

    /// When this row was read from the provider.
    #[serde(rename = "syncedAt")]
    pub synced_at: String,
}
