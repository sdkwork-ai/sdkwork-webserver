use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClusterInstanceResponse {
    pub id: String,

    #[serde(rename = "clusterId")]
    pub cluster_id: String,

    #[serde(rename = "hostId")]
    pub host_id: String,

    #[serde(rename = "hostName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_name: Option<String>,

    pub name: String,

    pub role: String,

    pub environment: String,

    #[serde(rename = "processPid")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_pid: Option<i64>,

    #[serde(rename = "processStartedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_started_at: Option<String>,

    #[serde(rename = "bindHost")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind_host: Option<String>,

    #[serde(rename = "bindPort")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind_port: Option<i64>,

    #[serde(rename = "publicEndpoint")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_endpoint: Option<String>,

    #[serde(rename = "buildVersion")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_version: Option<String>,

    /// 0=offline, 1=online, 2=starting, 3=stopping, 4=error, 5=maintenance
    pub status: i64,

    #[serde(rename = "healthState")]
    pub health_state: String,

    #[serde(rename = "lastHeartbeatAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<String>,

    #[serde(rename = "lastOnlineAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_online_at: Option<String>,

    #[serde(rename = "uptimeSeconds")]
    pub uptime_seconds: String,

    /// Latest resource metrics snapshot (CPU/memory/connections).
    pub metrics: std::collections::HashMap<String, serde_json::Value>,

    /// `LAN` = same-subnet member; `TUNNEL` = API-only member reached through the reverse tunnel.
    #[serde(rename = "joinMode")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join_mode: Option<String>,

    /// Service quality 0-100 derived from the latest heartbeat sample.
    #[serde(rename = "qualityScore")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_score: Option<i64>,

    /// Desired configuration revision; absent until the cluster publishes one.
    #[serde(rename = "desiredConfigRevision")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desired_config_revision: Option<String>,

    /// Configuration revision this instance last acknowledged as applied.
    #[serde(rename = "appliedConfigRevision")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_config_revision: Option<String>,

    /// Desired applications-manifest revision; absent until the cluster publishes one.
    #[serde(rename = "desiredApplicationsRevision")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desired_applications_revision: Option<String>,

    /// Applications-manifest revision this instance last acknowledged as applied.
    #[serde(rename = "appliedApplicationsRevision")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_applications_revision: Option<String>,

    /// Aggregate desired-vs-applied sync status for this instance.
    #[serde(rename = "syncStatus")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_status: Option<String>,

    /// Cordon switch: `false` keeps the instance serving but removes it from the routing pool.
    #[serde(rename = "routingEnabled")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_enabled: Option<bool>,

    /// Graceful drain in progress.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draining: Option<bool>,

    /// Taken out of the routing pool by the active prober after consecutive failures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ejected: Option<bool>,

    /// Process restarts observed for this instance slot (auto-recovery evidence).
    #[serde(rename = "restartCount")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart_count: Option<i64>,

    /// Operator labels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<std::collections::HashMap<String, String>>,

    /// Per-instance load balancing weight.
    #[serde(rename = "routingWeight")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_weight: Option<i64>,

    /// Operator maintenance reason/context.
    #[serde(rename = "maintenanceNote")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintenance_note: Option<String>,

    /// Consecutive active-probe failures; reset on success.
    #[serde(rename = "probeFailures")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe_failures: Option<i64>,

    /// Active-probe target override.
    #[serde(rename = "probeUrl")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe_url: Option<String>,

    #[serde(rename = "createdAt")]
    pub created_at: String,

    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}
