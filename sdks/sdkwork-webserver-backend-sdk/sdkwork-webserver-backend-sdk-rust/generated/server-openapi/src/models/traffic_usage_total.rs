use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TrafficUsageTotal {
    pub dimension: String,

    pub quantity: String,

    pub unit: String,
}
