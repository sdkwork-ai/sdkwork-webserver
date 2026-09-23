use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TrafficUsageDailyPoint {
    #[serde(rename = "usageDate")]
    pub usage_date: String,

    pub dimension: String,

    pub quantity: String,
}
