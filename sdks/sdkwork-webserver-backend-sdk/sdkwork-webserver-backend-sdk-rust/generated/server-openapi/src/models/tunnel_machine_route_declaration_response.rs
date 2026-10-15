use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TunnelMachineRouteDeclarationResponse {
    #[serde(rename = "routeId")]
    pub route_id: String,

    pub active: bool,

    #[serde(rename = "publicUrl")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_url: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}
