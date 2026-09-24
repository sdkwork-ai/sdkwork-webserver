use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MetricsSeriesPoint {
    /// Calendar day in UTC.
    pub date: String,

    pub quantity: String,
}
