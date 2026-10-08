use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct UpdateDomainDnsRecordRequest {
    #[serde(rename = "recordType")]
    pub record_type: String,

    pub host: String,

    #[serde(rename = "recordValue")]
    pub record_value: String,

    #[serde(rename = "ttlSeconds")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<i64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,

    #[serde(rename = "recordLine")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_line: Option<String>,
}
