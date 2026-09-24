use serde::{Deserialize, Serialize};

/// Partial edit; an omitted member leaves the stored value unchanged. The apex hostname is not editable.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct UpdateRootDomainRequest {
    #[serde(rename = "displayName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,

    #[serde(rename = "dnsProvider")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_provider: Option<String>,

    #[serde(rename = "providerZoneRef")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_zone_ref: Option<String>,

    /// 0=pending, 1=active, 2=disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<i64>,
}
