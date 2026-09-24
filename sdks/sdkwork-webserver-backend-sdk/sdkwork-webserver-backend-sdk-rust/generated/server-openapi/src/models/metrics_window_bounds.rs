use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MetricsWindowBounds {
    pub window: String,

    /// Inclusive UTC day. Absent for the lifetime window, whose lower bound is wherever the figures begin rather than a day this contract could invent.
    #[serde(rename = "dateFrom")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_from: Option<String>,

    #[serde(rename = "dateTo")]
    pub date_to: String,
}
