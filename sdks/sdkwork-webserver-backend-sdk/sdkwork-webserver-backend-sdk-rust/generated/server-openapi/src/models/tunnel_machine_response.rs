use serde::{Deserialize, Serialize};

use crate::models::{TunnelMachineRouteResponse};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TunnelMachineResponse {
    #[serde(rename = "deviceId")]
    pub device_id: String,

    #[serde(rename = "deviceName")]
    pub device_name: String,

    pub platform: String,

    pub online: bool,

    pub enabled: bool,

    #[serde(rename = "sessionId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    #[serde(rename = "firstSeenAt")]
    pub first_seen_at: String,

    #[serde(rename = "lastSeenAt")]
    pub last_seen_at: String,

    #[serde(rename = "connectedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected_at: Option<String>,

    #[serde(rename = "disconnectedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disconnected_at: Option<String>,

    #[serde(rename = "remoteAddr")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_addr: Option<String>,

    #[serde(rename = "bytesIn")]
    pub bytes_in: String,

    #[serde(rename = "bytesOut")]
    pub bytes_out: String,

    #[serde(rename = "sessionsTotal")]
    pub sessions_total: String,

    #[serde(rename = "routeCount")]
    pub route_count: String,

    #[serde(rename = "healthyRouteCount")]
    pub healthy_route_count: String,

    #[serde(rename = "unhealthyRouteIds")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unhealthy_route_ids: Option<Vec<String>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routes: Option<Vec<TunnelMachineRouteResponse>>,
}
