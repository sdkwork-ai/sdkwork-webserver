use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterHostResponse {
    pub id: String,

    #[serde(rename = "clusterId")]
    pub cluster_id: String,

    pub name: String,

    pub hostname: String,

    #[serde(rename = "machineCode")]
    pub machine_code: String,

    #[serde(rename = "osName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_name: Option<String>,

    #[serde(rename = "osVersion")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,

    #[serde(rename = "kernelVersion")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel_version: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,

    #[serde(rename = "cpuModel")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_model: Option<String>,

    #[serde(rename = "cpuCores")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_cores: Option<i64>,

    #[serde(rename = "memoryTotalMb")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_total_mb: Option<String>,

    #[serde(rename = "remoteIp")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_ip: Option<String>,

    #[serde(rename = "localIps")]
    pub local_ips: Vec<String>,

    #[serde(rename = "macAddresses")]
    pub mac_addresses: Vec<String>,

    #[serde(rename = "daemonVersion")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_version: Option<String>,

    /// 0=offline, 1=online, 2=deploying, 3=error, 4=maintenance
    pub status: i64,

    #[serde(rename = "lastHeartbeatAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<String>,

    #[serde(rename = "instanceCount")]
    pub instance_count: String,

    /// `LAN` = same-subnet host with shared-database or direct-API reachability; `TUNNEL` = API-only host reached through the reverse tunnel.
    #[serde(rename = "joinMode")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join_mode: Option<String>,

    /// Tunnel route domain for `TUNNEL` hosts; absent on `LAN` hosts.
    #[serde(rename = "tunnelRouteDomain")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel_route_domain: Option<String>,

    #[serde(rename = "createdAt")]
    pub created_at: String,

    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}
