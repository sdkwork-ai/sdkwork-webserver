use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterHostDescriptor {
    pub hostname: String,

    /// Stable hardware/machine fingerprint of the host.
    #[serde(rename = "machineCode")]
    pub machine_code: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_ips: Option<Vec<String>>,

    #[serde(rename = "macAddresses")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac_addresses: Option<Vec<String>>,

    #[serde(rename = "daemonVersion")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_version: Option<String>,
}
