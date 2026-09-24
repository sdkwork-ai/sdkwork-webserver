use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MetricsWindowValue {
    pub window: String,

    pub quantity: String,

    pub unit: String,
}
