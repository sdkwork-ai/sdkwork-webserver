//! Distributed Web Server cluster management wire contracts.
//!
//! The cluster plane models the webserver deployment itself: one
//! [`ClusterResponse`] groups many [`ClusterHostResponse`] machines, and every
//! machine runs one or more [`ClusterInstanceResponse`] webserver processes.
//! Hosts carry the system/network identity (system basics, remote IP, local
//! IPs, machine code, MAC addresses) reported at registration; instances carry
//! the process identity (PID, bind address, build version) and heartbeat
//! state. Cluster data is platform infrastructure and lives under tenant 0.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// wire enum: instance process role (`GATEWAY`, `MANAGEMENT`, `DATA_PLANE`,
/// `WORKER`, `OTHER`).
pub const CLUSTER_INSTANCE_ROLES: [&str; 5] = [
    "GATEWAY",
    "MANAGEMENT",
    "DATA_PLANE",
    "WORKER",
    "OTHER",
];

/// wire enum: deployment environment for a cluster instance.
pub const CLUSTER_ENVIRONMENTS: [&str; 4] = ["development", "test", "staging", "production"];

/// wire enum: instance health state (`HEALTHY`, `DEGRADED`, `UNHEALTHY`,
/// `UNKNOWN`).
pub const CLUSTER_HEALTH_STATES: [&str; 4] = ["HEALTHY", "DEGRADED", "UNHEALTHY", "UNKNOWN"];

/// wire enum: cluster event severity (`INFO`, `WARNING`, `ERROR`).
pub const CLUSTER_EVENT_SEVERITIES: [&str; 3] = ["INFO", "WARNING", "ERROR"];

// ---------------------------------------------------------------------------
// Admin surface: clusters
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClusterResponse {
    pub id: String,
    pub name: String,
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: i32,
    #[serde(rename = "heartbeatIntervalSeconds")]
    pub heartbeat_interval_seconds: i32,
    #[serde(rename = "offlineThresholdSeconds")]
    pub offline_threshold_seconds: i32,
    #[serde(rename = "hostCount", with = "sdkwork_utils_rust::serde_int64")]
    pub host_count: i64,
    #[serde(rename = "instanceCount", with = "sdkwork_utils_rust::serde_int64")]
    pub instance_count: i64,
    #[serde(
        rename = "onlineInstanceCount",
        with = "sdkwork_utils_rust::serde_int64"
    )]
    pub online_instance_count: i64,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClusterPage {
    pub items: Vec<ClusterResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateClusterRequest {
    pub name: String,
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(
        rename = "heartbeatIntervalSeconds",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub heartbeat_interval_seconds: Option<i32>,
    #[serde(
        rename = "offlineThresholdSeconds",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub offline_threshold_seconds: Option<i32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateClusterRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    #[serde(
        rename = "heartbeatIntervalSeconds",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub heartbeat_interval_seconds: Option<i32>,
    #[serde(
        rename = "offlineThresholdSeconds",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub offline_threshold_seconds: Option<i32>,
}

// ---------------------------------------------------------------------------
// Admin surface: hosts
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClusterHostResponse {
    pub id: String,
    #[serde(rename = "clusterId")]
    pub cluster_id: String,
    pub name: String,
    pub hostname: String,
    #[serde(rename = "machineCode")]
    pub machine_code: String,
    #[serde(rename = "osName", default, skip_serializing_if = "Option::is_none")]
    pub os_name: Option<String>,
    #[serde(
        rename = "osVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub os_version: Option<String>,
    #[serde(
        rename = "kernelVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub kernel_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(
        rename = "cpuModel",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub cpu_model: Option<String>,
    #[serde(
        rename = "cpuCores",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub cpu_cores: Option<i32>,
    #[serde(
        rename = "memoryTotalMb",
        with = "sdkwork_utils_rust::serde_int64::option",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub memory_total_mb: Option<i64>,
    #[serde(
        rename = "remoteIp",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub remote_ip: Option<String>,
    #[serde(rename = "localIps", default)]
    pub local_ips: Vec<String>,
    #[serde(rename = "macAddresses", default)]
    pub mac_addresses: Vec<String>,
    #[serde(
        rename = "daemonVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub daemon_version: Option<String>,
    pub status: i32,
    #[serde(
        rename = "lastHeartbeatAt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub last_heartbeat_at: Option<String>,
    #[serde(
        rename = "instanceCount",
        with = "sdkwork_utils_rust::serde_int64",
        default
    )]
    pub instance_count: i64,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClusterHostPage {
    pub items: Vec<ClusterHostResponse>,
    /// Offset-mode total; `0` in cursor mode (exact continuation comes from
    /// `hasMore`/`nextCursor`).
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateClusterHostRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "clusterId", default, skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Admin surface: instances
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClusterInstanceResponse {
    pub id: String,
    #[serde(rename = "clusterId")]
    pub cluster_id: String,
    #[serde(rename = "hostId")]
    pub host_id: String,
    #[serde(
        rename = "hostName",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub host_name: Option<String>,
    pub name: String,
    pub role: String,
    pub environment: String,
    #[serde(
        rename = "processPid",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub process_pid: Option<i32>,
    #[serde(
        rename = "processStartedAt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub process_started_at: Option<String>,
    #[serde(
        rename = "bindHost",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub bind_host: Option<String>,
    #[serde(
        rename = "bindPort",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub bind_port: Option<i32>,
    #[serde(
        rename = "publicEndpoint",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub public_endpoint: Option<String>,
    #[serde(
        rename = "buildVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub build_version: Option<String>,
    pub status: i32,
    #[serde(rename = "healthState")]
    pub health_state: String,
    #[serde(
        rename = "lastHeartbeatAt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub last_heartbeat_at: Option<String>,
    #[serde(
        rename = "lastOnlineAt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub last_online_at: Option<String>,
    #[serde(
        rename = "uptimeSeconds",
        with = "sdkwork_utils_rust::serde_int64"
    )]
    pub uptime_seconds: i64,
    #[serde(default)]
    pub metrics: Value,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClusterInstancePage {
    pub items: Vec<ClusterInstanceResponse>,
    /// Offset-mode total; `0` in cursor mode.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateClusterInstanceRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    #[serde(
        rename = "publicEndpoint",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub public_endpoint: Option<String>,
}

// ---------------------------------------------------------------------------
// Admin surface: events and overview
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClusterEventResponse {
    pub id: String,
    #[serde(rename = "clusterId")]
    pub cluster_id: String,
    #[serde(rename = "hostId", default, skip_serializing_if = "Option::is_none")]
    pub host_id: Option<String>,
    #[serde(
        rename = "instanceId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub instance_id: Option<String>,
    #[serde(rename = "eventType")]
    pub event_type: String,
    pub severity: String,
    pub message: String,
    #[serde(default)]
    pub detail: Value,
    #[serde(rename = "occurredAt")]
    pub occurred_at: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClusterEventPage {
    pub items: Vec<ClusterEventResponse>,
    /// Offset-mode total; `0` in cursor mode.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClusterOverviewResponse {
    #[serde(rename = "totalHosts", with = "sdkwork_utils_rust::serde_int64")]
    pub total_hosts: i64,
    #[serde(rename = "onlineHosts", with = "sdkwork_utils_rust::serde_int64")]
    pub online_hosts: i64,
    #[serde(
        rename = "totalInstances",
        with = "sdkwork_utils_rust::serde_int64"
    )]
    pub total_instances: i64,
    #[serde(
        rename = "onlineInstances",
        with = "sdkwork_utils_rust::serde_int64"
    )]
    pub online_instances: i64,
    #[serde(
        rename = "unhealthyInstances",
        with = "sdkwork_utils_rust::serde_int64"
    )]
    pub unhealthy_instances: i64,
    #[serde(
        rename = "pendingPeerMessages",
        with = "sdkwork_utils_rust::serde_int64"
    )]
    pub pending_peer_messages: i64,
    #[serde(rename = "generatedAt")]
    pub generated_at: String,
}

// ---------------------------------------------------------------------------
// Admin surface: heartbeat samples and peer message enqueue
// ---------------------------------------------------------------------------

/// One stored heartbeat sample (bounded retention) shown on the instance
/// drill-down timeline.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClusterHeartbeatSampleResponse {
    pub id: String,
    pub status: i32,
    #[serde(rename = "latencyMs", default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<i32>,
    #[serde(default)]
    pub metrics: Value,
    #[serde(rename = "reportedAt")]
    pub reported_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClusterHeartbeatSamplePage {
    pub items: Vec<ClusterHeartbeatSampleResponse>,
    /// Offset-mode total; `0` in cursor mode.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnqueueClusterPeerMessagesRequest {
    #[serde(rename = "clusterId")]
    pub cluster_id: String,
    /// Target instance uuid; omitted broadcasts to every ONLINE member of the
    /// cluster at enqueue time.
    #[serde(
        rename = "toInstanceId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub to_instance_id: Option<String>,
    /// Sender instance uuid; omitted for control-plane-originated messages.
    #[serde(
        rename = "fromInstanceId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub from_instance_id: Option<String>,
    #[serde(rename = "messageType")]
    pub message_type: String,
    #[serde(default)]
    pub payload: Value,
    #[serde(
        rename = "expiresInSeconds",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub expires_in_seconds: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnqueueClusterPeerMessagesResponse {
    /// Rows materialized: `1` for a direct message, one per online member for
    /// a broadcast, `0` when no member is online.
    #[serde(rename = "enqueued", with = "sdkwork_utils_rust::serde_int64")]
    pub enqueued: i64,
}

// ---------------------------------------------------------------------------
// Machine surface: registration, heartbeat, peer directory, mailbox
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterRegistrationRequest {
    #[serde(
        rename = "clusterCode",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub cluster_code: Option<String>,
    pub host: ClusterHostDescriptor,
    pub instance: ClusterInstanceDescriptor,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterHostDescriptor {
    pub hostname: String,
    #[serde(rename = "machineCode")]
    pub machine_code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "osName", default, skip_serializing_if = "Option::is_none")]
    pub os_name: Option<String>,
    #[serde(
        rename = "osVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub os_version: Option<String>,
    #[serde(
        rename = "kernelVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub kernel_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(
        rename = "cpuModel",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub cpu_model: Option<String>,
    #[serde(
        rename = "cpuCores",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub cpu_cores: Option<i32>,
    #[serde(
        rename = "memoryTotalMb",
        with = "sdkwork_utils_rust::serde_int64::option",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub memory_total_mb: Option<i64>,
    #[serde(rename = "remoteIp", default, skip_serializing_if = "Option::is_none")]
    pub remote_ip: Option<String>,
    #[serde(rename = "localIps", default)]
    pub local_ips: Vec<String>,
    #[serde(rename = "macAddresses", default)]
    pub mac_addresses: Vec<String>,
    #[serde(
        rename = "daemonVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub daemon_version: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterInstanceDescriptor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub role: String,
    pub environment: String,
    #[serde(rename = "processPid")]
    pub process_pid: i32,
    #[serde(rename = "processStartedAt")]
    pub process_started_at: String,
    #[serde(
        rename = "bindHost",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub bind_host: Option<String>,
    #[serde(
        rename = "bindPort",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub bind_port: Option<i32>,
    #[serde(
        rename = "publicEndpoint",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub public_endpoint: Option<String>,
    #[serde(
        rename = "buildVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub build_version: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterRegistrationResponse {
    pub cluster: ClusterRef,
    pub host: ClusterHostRef,
    pub instance: ClusterInstanceRef,
    #[serde(rename = "instanceToken")]
    pub instance_token: String,
    #[serde(rename = "heartbeatIntervalSeconds")]
    pub heartbeat_interval_seconds: i32,
    #[serde(rename = "offlineThresholdSeconds")]
    pub offline_threshold_seconds: i32,
    #[serde(default)]
    pub peers: Vec<ClusterPeer>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterRef {
    pub id: String,
    pub name: String,
    pub code: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterHostRef {
    pub id: String,
    pub name: String,
    pub hostname: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterInstanceRef {
    pub id: String,
    pub name: String,
    pub role: String,
}

/// One reachable cluster member as seen by the control plane. Instances use
/// the directory for peer-to-peer reachability (`publicEndpoint`) and health.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterPeer {
    #[serde(rename = "instanceId")]
    pub instance_id: String,
    pub name: String,
    pub role: String,
    pub status: i32,
    #[serde(rename = "hostName")]
    pub host_name: String,
    #[serde(rename = "remoteIp", default, skip_serializing_if = "Option::is_none")]
    pub remote_ip: Option<String>,
    #[serde(
        rename = "publicEndpoint",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub public_endpoint: Option<String>,
    pub environment: String,
    #[serde(
        rename = "buildVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub build_version: Option<String>,
    #[serde(
        rename = "lastHeartbeatAt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub last_heartbeat_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterHeartbeatRequest {
    pub status: i32,
    #[serde(rename = "healthState")]
    pub health_state: String,
    #[serde(rename = "uptimeSeconds", with = "sdkwork_utils_rust::serde_int64")]
    pub uptime_seconds: i64,
    #[serde(
        rename = "buildVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub build_version: Option<String>,
    #[serde(default)]
    pub metrics: Value,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterPeerMessage {
    pub id: String,
    #[serde(rename = "messageType")]
    pub message_type: String,
    #[serde(
        rename = "fromInstanceId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub from_instance_id: Option<String>,
    #[serde(
        rename = "toInstanceId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub to_instance_id: Option<String>,
    #[serde(default)]
    pub payload: Value,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterHeartbeatResponse {
    #[serde(rename = "instanceId")]
    pub instance_id: String,
    pub status: i32,
    #[serde(rename = "acknowledgedAt")]
    pub acknowledged_at: String,
    #[serde(rename = "heartbeatIntervalSeconds")]
    pub heartbeat_interval_seconds: i32,
    #[serde(rename = "offlineThresholdSeconds")]
    pub offline_threshold_seconds: i32,
    #[serde(default)]
    pub peers: Vec<ClusterPeer>,
    #[serde(default)]
    pub messages: Vec<ClusterPeerMessage>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterPeerDirectoryResponse {
    #[serde(rename = "instanceId")]
    pub instance_id: String,
    #[serde(default)]
    pub peers: Vec<ClusterPeer>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int64_counts_serialize_as_strings() {
        let overview = ClusterOverviewResponse {
            total_hosts: 12,
            online_hosts: 10,
            total_instances: 3_000_000_000_123,
            online_instances: 8,
            unhealthy_instances: 1,
            pending_peer_messages: 0,
            generated_at: "2026-09-19T00:00:00Z".to_string(),
        };
        let json = serde_json::to_value(&overview).unwrap();
        assert_eq!(
            json["totalInstances"],
            serde_json::Value::String("3000000000123".to_string())
        );
    }

    #[test]
    fn heartbeat_request_rejects_unknown_fields() {
        let raw = r#"{"status":1,"healthState":"HEALTHY","uptimeSeconds":120,"bogus":1}"#;
        let parsed: Result<ClusterHeartbeatRequest, _> = serde_json::from_str(raw);
        assert!(parsed.is_err());
    }

    #[test]
    fn registration_request_parses_host_identity() {
        let raw = r#"{
            "host": {
                "hostname": "edge-1",
                "machineCode": "mc-123",
                "osName": "Linux",
                "localIps": ["10.0.0.2"],
                "macAddresses": ["aa:bb:cc:dd:ee:ff"]
            },
            "instance": {
                "role": "GATEWAY",
                "environment": "production",
                "processPid": 4242,
                "processStartedAt": "2026-09-19T00:00:00Z"
            }
        }"#;
        let parsed: ClusterRegistrationRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.host.hostname, "edge-1");
        assert_eq!(parsed.instance.process_pid, 4242);
        assert_eq!(parsed.host.local_ips.len(), 1);
    }
}
