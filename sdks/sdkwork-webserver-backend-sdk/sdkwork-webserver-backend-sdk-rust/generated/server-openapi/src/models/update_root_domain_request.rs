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

    /// The cloud account to bind, or `null` to unbind. The **only** member of this request that distinguishes "leave it alone" from "clear it": an omitted member keeps the stored account and an explicit `null` removes it. This is the one place the surface's "a blank value is an omission" reading does not apply, because the member names an association rather than describing the row.
    #[serde(rename = "cloudAccountId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_account_id: Option<String>,
}
