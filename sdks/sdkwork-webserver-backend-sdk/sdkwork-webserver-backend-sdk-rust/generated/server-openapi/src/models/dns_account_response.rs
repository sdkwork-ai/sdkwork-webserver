use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DnsAccountResponse {
    /// Stable operator-facing reference, and the value `providerAccountId` on an issue request names.
    #[serde(rename = "accountId")]
    pub account_id: String,

    /// Provider family whose API this account presents challenges through.
    pub provider: String,

    /// Hosted zone apex the account can publish into. An identifier is covered when it equals this apex or lives beneath it.
    #[serde(rename = "zoneApex")]
    pub zone_apex: String,
}
