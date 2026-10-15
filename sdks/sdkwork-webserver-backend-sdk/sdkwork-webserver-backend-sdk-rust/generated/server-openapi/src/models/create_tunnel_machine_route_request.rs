use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CreateTunnelMachineRouteRequest {
    pub name: String,

    pub protocol: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<i64>,

    #[serde(rename = "allowPublic")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_public: Option<bool>,

    #[serde(rename = "allowedIps")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_ips: Option<Vec<String>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,

    #[serde(rename = "visitorTokens")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visitor_tokens: Option<Vec<String>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<String>>,

    #[serde(rename = "bandwidthLimitBytesPerSecond")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth_limit_bytes_per_second: Option<String>,

    #[serde(rename = "proxyProtocol")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_protocol: Option<String>,

    #[serde(rename = "loadBalancerGroup")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_balancer_group: Option<String>,
}
