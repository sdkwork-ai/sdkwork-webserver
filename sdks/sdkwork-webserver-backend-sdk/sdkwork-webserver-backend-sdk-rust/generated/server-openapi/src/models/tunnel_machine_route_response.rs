use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TunnelMachineRouteResponse {
    #[serde(rename = "routeId")]
    pub route_id: String,

    pub name: String,

    pub protocol: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<i64>,

    #[serde(rename = "secretName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<String>>,

    #[serde(rename = "loadBalancerGroup")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_balancer_group: Option<String>,

    #[serde(rename = "bandwidthLimitBytesPerSecond")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth_limit_bytes_per_second: Option<String>,

    #[serde(rename = "proxyProtocol")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_protocol: Option<String>,

    #[serde(rename = "allowPublic")]
    pub allow_public: bool,

    pub auth: String,

    #[serde(rename = "allowedIps")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_ips: Option<Vec<String>>,

    pub healthy: bool,
}
