use serde::{Deserialize, Serialize};

/// The window the per-day series was cut against, as the server resolved it. Reported because the request's bounds are optional: a surface that labelled the series from its own guess at the default would name a period the points do not cover. Deliberately separate from the card windows, which answer a different question and may not coincide with this one.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MetricsSeriesWindow {
    /// Inclusive UTC day.
    #[serde(rename = "dateFrom")]
    pub date_from: String,

    /// **Exclusive** UTC day.
    #[serde(rename = "dateTo")]
    pub date_to: String,
}
