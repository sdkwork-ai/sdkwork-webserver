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
pub const CLUSTER_INSTANCE_ROLES: [&str; 5] =
    ["GATEWAY", "MANAGEMENT", "DATA_PLANE", "WORKER", "OTHER"];

/// wire enum: deployment environment for a cluster instance.
pub const CLUSTER_ENVIRONMENTS: [&str; 4] = ["development", "test", "staging", "production"];

/// wire enum: instance health state (`HEALTHY`, `DEGRADED`, `UNHEALTHY`,
/// `UNKNOWN`).
pub const CLUSTER_HEALTH_STATES: [&str; 4] = ["HEALTHY", "DEGRADED", "UNHEALTHY", "UNKNOWN"];

/// wire enum: cluster event severity (`INFO`, `WARNING`, `ERROR`).
pub const CLUSTER_EVENT_SEVERITIES: [&str; 3] = ["INFO", "WARNING", "ERROR"];

/// wire enum: how a host joins the cluster (`LAN` = same-subnet direct
/// connectivity with shared-database or direct-API access; `TUNNEL` = FRP-style
/// reverse tunnel through the public gateway with API-only cluster access and
/// no database reachability).
pub const CLUSTER_JOIN_MODES: [&str; 2] = ["LAN", "TUNNEL"];

/// wire enum: configuration/application sync state for one instance and
/// sync kind (`UNKNOWN`, `IN_SYNC`, `PENDING`, `FAILED`).
pub const CLUSTER_SYNC_STATUSES: [&str; 4] = ["UNKNOWN", "IN_SYNC", "PENDING", "FAILED"];

/// wire enum: data-sync payload kind (`config` = webserver runtime
/// configuration, `applications` = application management manifest).
pub const CLUSTER_SYNC_KINDS: [&str; 2] = ["config", "applications"];

/// wire enum: cluster request-routing strategy (`round_robin` DEFAULT,
/// `weighted_round_robin`, `least_connections`, `random`,
/// `random_two_choices`, `ip_hash`, `consistent_hash`).
pub const CLUSTER_LB_STRATEGIES: [&str; 7] = [
    "round_robin",
    "weighted_round_robin",
    "least_connections",
    "random",
    "random_two_choices",
    "ip_hash",
    "consistent_hash",
];

/// Numerical host join mode stored in the registry (`0 = LAN`, `1 = TUNNEL`).
pub const CLUSTER_JOIN_MODE_LAN: i32 = 0;
pub const CLUSTER_JOIN_MODE_TUNNEL: i32 = 1;

/// Numerical instance sync status stored in the registry.
pub const CLUSTER_SYNC_STATUS_UNKNOWN: i32 = 0;
pub const CLUSTER_SYNC_STATUS_IN_SYNC: i32 = 1;
pub const CLUSTER_SYNC_STATUS_PENDING: i32 = 2;
pub const CLUSTER_SYNC_STATUS_FAILED: i32 = 3;

/// Numerical sync revision kind stored in the registry.
pub const CLUSTER_SYNC_KIND_CONFIG: i32 = 0;
pub const CLUSTER_SYNC_KIND_APPLICATIONS: i32 = 1;

/// Maps a wire join-mode label onto its registry integer; `None` when the
/// label is not a valid join mode.
pub fn cluster_join_mode_value(label: &str) -> Option<i32> {
    match label {
        "LAN" => Some(CLUSTER_JOIN_MODE_LAN),
        "TUNNEL" => Some(CLUSTER_JOIN_MODE_TUNNEL),
        _ => None,
    }
}

/// Maps a registry join-mode integer back onto the wire label.
pub fn cluster_join_mode_label(value: i32) -> &'static str {
    if value == CLUSTER_JOIN_MODE_TUNNEL {
        "TUNNEL"
    } else {
        "LAN"
    }
}

/// Maps a registry sync-status integer onto the wire label.
pub fn cluster_sync_status_label(value: i32) -> &'static str {
    match value {
        CLUSTER_SYNC_STATUS_IN_SYNC => "IN_SYNC",
        CLUSTER_SYNC_STATUS_PENDING => "PENDING",
        CLUSTER_SYNC_STATUS_FAILED => "FAILED",
        _ => "UNKNOWN",
    }
}

/// Maps a registry sync-kind integer onto the wire label.
pub fn cluster_sync_kind_label(value: i32) -> &'static str {
    if value == CLUSTER_SYNC_KIND_APPLICATIONS {
        "applications"
    } else {
        "config"
    }
}

/// Computes the node service-quality score (0..=100) from the latest
/// heartbeat quality sample (PRD: node service quality). The score weights
/// resource saturation and transport latency: CPU pressure, memory
/// pressure, connection pressure, round-trip latency, and the request error
/// rate each subtract from a perfect 100. `None` metrics are neutral.
pub fn cluster_quality_score(sample: &ClusterQualityMetrics) -> i32 {
    let mut score = 100_i32;
    if let Some(cpu) = sample.cpu_percent {
        if cpu > 90.0 {
            score -= 30;
        } else if cpu > 75.0 {
            score -= 15;
        } else if cpu > 60.0 {
            score -= 5;
        }
    }
    if let Some(memory) = sample.memory_percent {
        if memory > 90.0 {
            score -= 30;
        } else if memory > 80.0 {
            score -= 15;
        } else if memory > 70.0 {
            score -= 5;
        }
    }
    if let Some(connections) = sample.open_connections {
        if connections > 8_000 {
            score -= 15;
        } else if connections > 4_000 {
            score -= 5;
        }
    }
    if let Some(rtt) = sample.rtt_millis {
        if rtt > 1_000.0 {
            score -= 20;
        } else if rtt > 400.0 {
            score -= 10;
        } else if rtt > 150.0 {
            score -= 5;
        }
    }
    if let Some(rate) = sample.error_rate_percent {
        if rate > 10.0 {
            score -= 25;
        } else if rate > 5.0 {
            score -= 10;
        } else if rate > 1.0 {
            score -= 5;
        }
    }
    score.clamp(0, 100)
}

/// Quality-of-service sample carried inside the heartbeat `metrics` JSON
/// under the `"quality"` key (PRD: node service quality and monitoring).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClusterQualityMetrics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_connections: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtt_millis: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_rate_percent: Option<f64>,
}

/// One data-sync track (config or applications) for an instance: the
/// desired revision the cluster wants, the revision the instance applied,
/// and the resulting state (PRD: complete data synchronization).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClusterSyncState {
    pub kind: String,
    #[serde(
        rename = "desiredRevision",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub desired_revision: Option<String>,
    #[serde(
        rename = "appliedRevision",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub applied_revision: Option<String>,
    pub status: String,
    #[serde(rename = "updatedAt", default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Desired-state manifest served to cluster nodes over the machine-only
/// API surface (config or application management payload; PRD: cluster
/// data synchronization for application and configuration management).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClusterSyncManifest {
    #[serde(rename = "clusterId")]
    pub cluster_id: String,
    pub kind: String,
    pub revision: String,
    #[serde(rename = "sha256")]
    pub sha256: String,
    pub payload: Value,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

/// Node acknowledgment after applying (or failing to apply) one sync
/// revision; persisted on the instance row for drift detection.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClusterSyncAckRequest {
    pub kind: String,
    pub revision: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

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
    /// Request routing strategy across the cluster's instances
    /// (`round_robin` default; see CLUSTER_LB_STRATEGIES).
    #[serde(
        rename = "lbStrategy",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub lb_strategy: Option<String>,
    /// Service domains auto-routed to cluster instances.
    #[serde(
        rename = "servedDomains",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub served_domains: Option<Vec<String>>,
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
    #[serde(
        rename = "lbStrategy",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub lb_strategy: Option<String>,
    #[serde(
        rename = "servedDomains",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub served_domains: Option<Vec<String>>,
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
    #[serde(
        rename = "lbStrategy",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub lb_strategy: Option<String>,
    #[serde(
        rename = "servedDomains",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub served_domains: Option<Vec<String>>,
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
    #[serde(rename = "osVersion", default, skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    #[serde(
        rename = "kernelVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub kernel_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(rename = "cpuModel", default, skip_serializing_if = "Option::is_none")]
    pub cpu_model: Option<String>,
    #[serde(rename = "cpuCores", default, skip_serializing_if = "Option::is_none")]
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
    /// Join mode: `LAN` (same-subnet, direct API / shared database) or
    /// `TUNNEL` (API-only through the reverse tunnel).
    #[serde(rename = "joinMode", default, skip_serializing_if = "Option::is_none")]
    pub join_mode: Option<String>,
    /// Tunnel route domain for `TUNNEL` hosts (None on LAN hosts).
    #[serde(
        rename = "tunnelRouteDomain",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub tunnel_route_domain: Option<String>,
}

/// (Cluster host responses carry the per-host join mode: `LAN`
/// (same-subnet) or `TUNNEL` (API-only through the reverse tunnel).)
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    #[serde(rename = "hostName", default, skip_serializing_if = "Option::is_none")]
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
    #[serde(rename = "bindHost", default, skip_serializing_if = "Option::is_none")]
    pub bind_host: Option<String>,
    #[serde(rename = "bindPort", default, skip_serializing_if = "Option::is_none")]
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
    #[serde(rename = "uptimeSeconds", with = "sdkwork_utils_rust::serde_int64")]
    pub uptime_seconds: i64,
    #[serde(default)]
    pub metrics: Value,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    /// Join mode of the instance: `LAN` (same-subnet) or `TUNNEL`
    /// (API-only through the reverse tunnel, no database reachability).
    #[serde(rename = "joinMode", default, skip_serializing_if = "Option::is_none")]
    pub join_mode: Option<String>,
    /// Service quality score 0..=100 derived from the latest heartbeat
    /// quality sample (None when no sample has been reported).
    #[serde(
        rename = "qualityScore",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub quality_score: Option<i32>,
    /// Config sync: desired vs applied revision.
    #[serde(
        rename = "desiredConfigRevision",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub desired_config_revision: Option<String>,
    #[serde(
        rename = "appliedConfigRevision",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub applied_config_revision: Option<String>,
    /// Applications sync: desired vs applied revision.
    #[serde(
        rename = "desiredApplicationsRevision",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub desired_applications_revision: Option<String>,
    #[serde(
        rename = "appliedApplicationsRevision",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub applied_applications_revision: Option<String>,
    /// Aggregate sync status: `UNKNOWN`, `IN_SYNC`, `PENDING`, `FAILED`.
    #[serde(
        rename = "syncStatus",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub sync_status: Option<String>,
    /// Cordon switch (routed traffic admitted).
    #[serde(
        rename = "routingEnabled",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub routing_enabled: Option<bool>,
    /// Graceful drain in progress.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draining: Option<bool>,
    /// Auto-probe ejection (instance temporarily out of the routing pool).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ejected: Option<bool>,
    /// Observed process restarts (auto-recovery evidence).
    ///
    /// `i32`, not `i64`: the column is `INTEGER` (migration
    /// `0016_webserver_cluster_instance_ops`), and decoding an INT4 column into
    /// `Option<i64>` fails at read time with `mismatched types; Rust type
    /// Option<i64> (as SQL type INT8) is not compatible with SQL type INT4`.
    /// It matches the sibling `probe_failures`, which is the same width.
    #[serde(
        rename = "restartCount",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub restart_count: Option<i32>,
    /// Operator labels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<std::collections::BTreeMap<String, String>>,
    /// Per-instance load balancing weight (1..=10000).
    #[serde(
        rename = "routingWeight",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub routing_weight: Option<i32>,
    /// Operator maintenance reason/context.
    #[serde(
        rename = "maintenanceNote",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub maintenance_note: Option<String>,
    /// Consecutive active-probe failures (reset on success).
    #[serde(
        rename = "probeFailures",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub probe_failures: Option<i32>,
    /// Active-probe target override.
    #[serde(rename = "probeUrl", default, skip_serializing_if = "Option::is_none")]
    pub probe_url: Option<String>,
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
    /// Cordon switch: `false` removes the instance from the routing pool
    /// while it keeps serving (industry cordon/uncordon).
    #[serde(
        rename = "routingEnabled",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub routing_enabled: Option<bool>,
    /// Graceful drain start/clear. Starting a drain also cordons routing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draining: Option<bool>,
    /// Active-probe target override.
    #[serde(rename = "probeUrl", default, skip_serializing_if = "Option::is_none")]
    pub probe_url: Option<String>,
    /// Operator labels (replaces the whole map when present).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<std::collections::BTreeMap<String, String>>,
    /// Per-instance load balancing weight override (1..=10000).
    #[serde(
        rename = "routingWeight",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub routing_weight: Option<i32>,
    /// Operator maintenance reason/context.
    #[serde(
        rename = "maintenanceNote",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub maintenance_note: Option<String>,
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
    #[serde(rename = "totalInstances", with = "sdkwork_utils_rust::serde_int64")]
    pub total_instances: i64,
    #[serde(rename = "onlineInstances", with = "sdkwork_utils_rust::serde_int64")]
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
    #[serde(rename = "osVersion", default, skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    #[serde(
        rename = "kernelVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub kernel_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(rename = "cpuModel", default, skip_serializing_if = "Option::is_none")]
    pub cpu_model: Option<String>,
    #[serde(rename = "cpuCores", default, skip_serializing_if = "Option::is_none")]
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
    /// How this host joins the cluster: `LAN` (same-subnet, direct API or
    /// shared database) or `TUNNEL` (FRP-style reverse tunnel, API-only,
    /// no database reachability). Defaults to `LAN`.
    #[serde(rename = "joinMode", default, skip_serializing_if = "Option::is_none")]
    pub join_mode: Option<String>,
    /// Tunnel identity for `TUNNEL` hosts: the tunnel route domain that
    /// reaches this host's services through the public gateway.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<ClusterTunnelDescriptor>,
}

/// Tunnel transport identity of a `TUNNEL`-mode host (PRD: FRP-mode remote
/// cluster participation through the public gateway).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClusterTunnelDescriptor {
    /// Tunnel route domain that routes through the gateway to this host
    /// (e.g. `node-5.cluster.example.com`).
    #[serde(rename = "routeDomain")]
    pub route_domain: String,
    /// Advertised tunnel endpoint the host dials (gateway `host:port`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
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
    #[serde(rename = "bindHost", default, skip_serializing_if = "Option::is_none")]
    pub bind_host: Option<String>,
    #[serde(rename = "bindPort", default, skip_serializing_if = "Option::is_none")]
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
    /// Per-instance join mode override: `LAN` (same-subnet) or `TUNNEL`
    /// (API-only through the reverse tunnel). Defaults to the host mode.
    #[serde(rename = "joinMode", default, skip_serializing_if = "Option::is_none")]
    pub join_mode: Option<String>,
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
    #[serde(rename = "joinMode", default, skip_serializing_if = "Option::is_none")]
    pub join_mode: Option<String>,
    #[serde(
        rename = "tunnelRouteDomain",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub tunnel_route_domain: Option<String>,
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
    /// Desired-vs-applied sync state per track (config, applications);
    /// `None` when the cluster has no sync plane configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync: Option<Vec<ClusterSyncState>>,
    /// Operations directives the node must obey (routing participation and
    /// drain completion are registry-driven).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ops: Option<ClusterInstanceOpsDirectives>,
}

/// Node drain-completion acknowledgment response.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClusterDrainCompleteResponse {
    #[serde(rename = "acknowledgedAt")]
    pub acknowledged_at: String,
}

/// Result of an on-demand connectivity probe (admin "Test connection").
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClusterProbeRunResponse {
    /// Probe reached the instance and got a healthy answer.
    pub healthy: bool,
    /// Round-trip latency of the probe.
    #[serde(rename = "latencyMs")]
    pub latency_ms: u64,
    /// Consecutive probe failures after this run (0 when healthy).
    pub failures: i32,
    /// Auto-eject transition happened on this run.
    pub ejected: bool,
    /// Auto-recovery transition happened on this run.
    pub recovered: bool,
}

/// Registry-driven operations directives delivered on every heartbeat.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClusterInstanceOpsDirectives {
    /// The instance participates in cluster request routing.
    #[serde(rename = "routingEnabled")]
    pub routing_enabled: bool,
    /// The instance must finish in-flight work and stop (graceful drain).
    #[serde(rename = "drainRequested")]
    pub drain_requested: bool,
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

#[cfg(test)]
mod cluster_v2_tests {
    use super::*;

    #[test]
    fn join_mode_mapping_round_trips() {
        assert_eq!(cluster_join_mode_value("LAN"), Some(CLUSTER_JOIN_MODE_LAN));
        assert_eq!(
            cluster_join_mode_value("TUNNEL"),
            Some(CLUSTER_JOIN_MODE_TUNNEL)
        );
        assert_eq!(cluster_join_mode_value("mesh"), None);
        assert_eq!(cluster_join_mode_label(CLUSTER_JOIN_MODE_TUNNEL), "TUNNEL");
        assert_eq!(cluster_join_mode_label(CLUSTER_JOIN_MODE_LAN), "LAN");
    }

    #[test]
    fn quality_score_penalizes_saturation_and_latency() {
        let perfect = ClusterQualityMetrics::default();
        assert_eq!(cluster_quality_score(&perfect), 100);
        let saturated = ClusterQualityMetrics {
            cpu_percent: Some(95.0),
            memory_percent: Some(95.0),
            rtt_millis: Some(1_500.0),
            error_rate_percent: Some(20.0),
            open_connections: Some(10_000),
        };
        assert_eq!(cluster_quality_score(&saturated), 0);
        let moderate = ClusterQualityMetrics {
            cpu_percent: Some(80.0),
            memory_percent: Some(50.0),
            ..ClusterQualityMetrics::default()
        };
        assert_eq!(cluster_quality_score(&moderate), 85);
    }

    #[test]
    fn sync_state_round_trips_camel_case() {
        let state = ClusterSyncState {
            kind: "config".to_owned(),
            desired_revision: Some("rev-9".to_owned()),
            applied_revision: None,
            status: "PENDING".to_owned(),
            updated_at: None,
        };
        let json = serde_json::to_string(&state).expect("serialize");
        assert!(json.contains("desiredRevision"));
        let decoded: ClusterSyncState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.kind, state.kind);
        assert_eq!(decoded.desired_revision, state.desired_revision);
        assert_eq!(decoded.status, state.status);
    }

    #[test]
    fn instance_detail_ops_fields_round_trip() {
        let json = serde_json::json!({
            "id": "instance-1",
            "clusterId": "cluster-1",
            "hostId": "host-1",
            "name": "gateway",
            "role": "GATEWAY",
            "environment": "production",
            "status": 1,
            "healthState": "HEALTHY",
            "createdAt": "2026-09-21T00:00:00Z",
            "updatedAt": "2026-09-21T00:00:00Z",
            "uptimeSeconds": "0",
            "metrics": {},
            "joinMode": "TUNNEL",
            "routingEnabled": false,
            "draining": true,
            "ejected": false,
            "restartCount": 3,
            "labels": {"tier": "edge"},
            "routingWeight": 200,
            "maintenanceNote": "kernel upgrade",
            "probeFailures": 1,
            "probeUrl": "http://127.0.0.1:3800/healthz"
        });
        let parsed: ClusterInstanceResponse = serde_json::from_value(json).expect("parses");
        assert_eq!(parsed.join_mode.as_deref(), Some("TUNNEL"));
        assert_eq!(parsed.routing_enabled, Some(false));
        assert_eq!(parsed.draining, Some(true));
        assert_eq!(parsed.restart_count, Some(3));
        assert_eq!(parsed.routing_weight, Some(200));
        assert_eq!(parsed.maintenance_note.as_deref(), Some("kernel upgrade"));
        assert_eq!(parsed.probe_failures, Some(1));
    }

    #[test]
    fn host_descriptor_rejects_unknown_fields() {
        let raw = r#"{"hostname":"a","machineCode":"mc","joinMode":"TUNNEL","bogus":1}"#;
        let parsed: Result<ClusterHostDescriptor, _> = serde_json::from_str(raw);
        assert!(parsed.is_err());
        let valid = r#"{"hostname":"a","machineCode":"mc","joinMode":"TUNNEL","tunnel":{"routeDomain":"node-5.cluster.test"}}"#;
        let parsed: ClusterHostDescriptor = serde_json::from_str(valid).expect("parses");
        assert_eq!(parsed.join_mode.as_deref(), Some("TUNNEL"));
        assert_eq!(
            parsed.tunnel.expect("tunnel").route_domain,
            "node-5.cluster.test"
        );
    }
}
