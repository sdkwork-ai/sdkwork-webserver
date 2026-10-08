use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreateDomainDnsRecordRequest {
    #[serde(rename = "recordType")]
    pub record_type: String,

    /// Zone-relative 主机记录 (`@` for the apex, `www`, `api.eu`, `*`).
    pub host: String,

    #[serde(rename = "recordValue")]
    pub record_value: String,

    #[serde(rename = "ttlSeconds")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<i64>,

    /// MX priority; rejected on types that carry none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,

    /// Provider resolution line; omitted = the provider default.
    #[serde(rename = "recordLine")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_line: Option<String>,
}
