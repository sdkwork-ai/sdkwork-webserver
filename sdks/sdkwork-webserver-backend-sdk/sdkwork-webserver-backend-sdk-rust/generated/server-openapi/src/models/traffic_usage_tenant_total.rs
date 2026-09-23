use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TrafficUsageTenantTotal {
    #[serde(rename = "tenantId")]
    pub tenant_id: String,

    pub dimension: String,

    pub quantity: String,

    pub unit: String,
}
