use serde::{Deserialize, Serialize};

/// One app's aggregate over the window. A row carrying neither `appUuid` nor `appSlug` is the **unattributed** bucket: traffic served for a hostname the edge could not resolve to an app. It is reported rather than dropped so the per-app rows keep summing back to the total; a surface must render it as its own row, or the breakdown appears to lose traffic.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TrafficUsageAppTotal {
    #[serde(rename = "appUuid")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_uuid: Option<String>,

    #[serde(rename = "appSlug")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_slug: Option<String>,

    pub dimension: String,

    pub quantity: String,

    pub unit: String,
}
