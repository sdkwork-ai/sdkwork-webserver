use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct UpdateClusterInstanceRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<i64>,

    #[serde(rename = "publicEndpoint")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_endpoint: Option<String>,

    /// Cordon switch: `false` removes the instance from the routing pool while it keeps serving.
    #[serde(rename = "routingEnabled")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_enabled: Option<bool>,

    /// Graceful drain start/clear. Starting a drain also cordons routing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draining: Option<bool>,

    /// Active-probe target override.
    #[serde(rename = "probeUrl")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe_url: Option<String>,

    /// Operator labels; replaces the whole map when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<std::collections::HashMap<String, String>>,

    /// Per-instance load balancing weight override.
    #[serde(rename = "routingWeight")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_weight: Option<i64>,

    /// Operator maintenance reason/context.
    #[serde(rename = "maintenanceNote")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintenance_note: Option<String>,
}
