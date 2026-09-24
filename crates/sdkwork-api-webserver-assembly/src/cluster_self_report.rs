//! In-process cluster self-report: the gateway registers its own host + process
//! into the distributed cluster registry and heartbeats liveness, health, and
//! resource metrics on a timer, then runs the bounded liveness sweep.
//!
//! Owner: the standalone gateway process. The task is spawned for the process
//! lifetime by [`spawn_cluster_self_report_task`] and never stopped mid-flight;
//! a failed tick is retried with bounded backoff on the next loop iteration.
//! The remote-instance path (other hosts) uses the machine-only internal HTTP
//! surface with `wreg_`/`winst_` credentials instead; this module is the
//! trusted in-process shortcut that needs no loopback HTTP.
//!
//! Host identity comes from the environment first, then from the platform
//! (`/proc`, `/sys`, `/etc`), so it works in containers and on Windows with
//! graceful degradation. Operators override anything with
//! `SDKWORK_WEBSERVER_HOST_*` variables.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use sdkwork_intelligence_webserver_service::WebService;
use sdkwork_utils_rust::crypto::sha256_hash;
use sdkwork_webserver_contract::{
    ClusterHeartbeatRequest, ClusterInstanceOpsDirectives, ClusterRegistrationRequest,
    CLUSTER_ENVIRONMENTS,
};
use tokio::task::JoinHandle;

const DEFAULT_HEARTBEAT_INTERVAL_SECS: u64 = 15;
const MAX_HEARTBEAT_INTERVAL_SECS: u64 = 600;
const MIN_HEARTBEAT_INTERVAL_SECS: u64 = 5;
const MAX_BACKOFF_MULTIPLIER: u32 = 8;
const MACHINE_CODE_MIN_CHARS: usize = 8;

/// Health is measured, not asserted: the reported state is derived from the
/// process RSS against the deployment's configured memory budget
/// (`SDKWORK_WEBSERVER_MEMORY_LIMIT`, the same governance limit compose and
/// Kubernetes apply). At 75% of budget the instance reports `DEGRADED`, at
/// 90% `UNHEALTHY`; without both a reading and a budget the instance reports
/// `HEALTHY` because liveness is owned by the heartbeat itself and silence is
/// judged by the registry sweep — an unmeasurable process must not invent a
/// reading it does not have.
const MEMORY_DEGRADED_RATIO_PERCENT: i64 = 75;
const MEMORY_UNHEALTHY_RATIO_PERCENT: i64 = 90;

/// Immutable self-report configuration resolved from the environment.
#[derive(Clone, Debug)]
pub struct ClusterSelfReportConfig {
    pub cluster_code: Option<String>,
    pub instance_name: Option<String>,
    pub role: String,
    pub environment: String,
    pub public_endpoint: Option<String>,
    pub bind_host: Option<String>,
    pub bind_port: Option<i32>,
    pub heartbeat_interval_secs: u64,
    pub enabled: bool,
}

impl ClusterSelfReportConfig {
    pub fn from_env() -> Result<Self, String> {
        let enabled = std::env::var("SDKWORK_WEBSERVER_CLUSTER_SELF_REPORT_ENABLED")
            .map(|value| {
                !matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "0" | "false" | "off"
                )
            })
            .unwrap_or(true);
        let role = std::env::var("SDKWORK_WEBSERVER_CLUSTER_ROLE")
            .unwrap_or_else(|_| "GATEWAY".to_string())
            .trim()
            .to_uppercase();
        let environment = sdkwork_webserver_contract::web_environment_name();
        if !CLUSTER_ENVIRONMENTS.contains(&environment.as_str()) {
            return Err(format!(
                "webserver environment {environment} is not a supported cluster environment"
            ));
        }
        let heartbeat_interval_secs = parse_bounded_interval(
            "SDKWORK_WEBSERVER_CLUSTER_HEARTBEAT_INTERVAL_SECS",
            MIN_HEARTBEAT_INTERVAL_SECS,
            MAX_HEARTBEAT_INTERVAL_SECS,
            DEFAULT_HEARTBEAT_INTERVAL_SECS,
        )?;
        let (bind_host, bind_port) = ingress_bind_from_env();
        Ok(Self {
            cluster_code: non_empty_env("SDKWORK_WEBSERVER_CLUSTER_CODE"),
            instance_name: non_empty_env("SDKWORK_WEBSERVER_INSTANCE_NAME"),
            role,
            environment,
            public_endpoint: non_empty_env("SDKWORK_WEBSERVER_INSTANCE_PUBLIC_ENDPOINT"),
            bind_host,
            bind_port,
            heartbeat_interval_secs,
            enabled,
        })
    }
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_bounded_interval(
    key: &str,
    minimum: u64,
    maximum: u64,
    default: u64,
) -> Result<u64, String> {
    match std::env::var(key) {
        Ok(value) => {
            let parsed: u64 = value
                .trim()
                .parse()
                .map_err(|error| format!("invalid {key}: {error}"))?;
            if !(minimum..=maximum).contains(&parsed) {
                return Err(format!(
                    "{key} must be between {minimum} and {maximum} seconds"
                ));
            }
            Ok(parsed)
        }
        Err(_) => Ok(default),
    }
}

fn ingress_bind_from_env() -> (Option<String>, Option<i32>) {
    let Some(bind) = non_empty_env("SDKWORK_WEBSERVER_APPLICATION_PUBLIC_INGRESS_BIND") else {
        return (None, None);
    };
    let Some((host, port)) = bind.rsplit_once(':') else {
        return (None, None);
    };
    match port.parse::<i32>() {
        Ok(port) => (Some(host.trim().to_string()), Some(port)),
        Err(_) => (None, None),
    }
}

/// Host identity reported at registration (system basics, IPs, MACs, machine
/// code).
#[derive(Clone, Debug)]
pub struct HostIdentity {
    pub hostname: String,
    pub machine_code: String,
    pub os_name: String,
    pub os_version: Option<String>,
    pub kernel_version: Option<String>,
    pub arch: String,
    pub cpu_cores: Option<i32>,
    pub memory_total_mb: Option<i64>,
    pub local_ips: Vec<String>,
    pub mac_addresses: Vec<String>,
}

pub fn collect_host_identity() -> HostIdentity {
    let hostname = hostname();
    HostIdentity {
        machine_code: machine_code(&hostname),
        hostname,
        os_name: std::env::consts::OS.to_string(),
        os_version: os_version(),
        kernel_version: kernel_version(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_cores: std::thread::available_parallelism()
            .ok()
            .map(|cores| cores.get() as i32),
        memory_total_mb: memory_total_mb(),
        local_ips: local_ips(),
        mac_addresses: mac_addresses(),
    }
}

fn hostname() -> String {
    if let Some(name) = non_empty_env("SDKWORK_WEBSERVER_HOST_NAME") {
        return name;
    }
    if let Ok(contents) = std::fs::read_to_string("/etc/hostname") {
        let trimmed = contents.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if let Some(name) = non_empty_env("COMPUTERNAME") {
        return name;
    }
    "unknown-host".to_string()
}

/// Stable per-machine fingerprint: operator override, then the OS machine id
/// (`/etc/machine-id`), then a hash of the hostname (documented fallback for
/// VM clones — set `SDKWORK_WEBSERVER_HOST_MACHINE_CODE` when hostnames are
/// not unique).
fn machine_code(hostname: &str) -> String {
    if let Some(code) = non_empty_env("SDKWORK_WEBSERVER_HOST_MACHINE_CODE") {
        return code;
    }
    if let Ok(contents) = std::fs::read_to_string("/etc/machine-id") {
        let trimmed = contents.trim();
        if trimmed.len() >= MACHINE_CODE_MIN_CHARS {
            return sha256_hash(trimmed.as_bytes());
        }
    }
    sha256_hash(format!("sdkwork-webserver|{hostname}").as_bytes())
}

fn os_version() -> Option<String> {
    if let Ok(contents) = std::fs::read_to_string("/etc/os-release") {
        for line in contents.lines() {
            if let Some(value) = line.strip_prefix("PRETTY_NAME=") {
                return Some(value.trim_matches('"').to_string());
            }
        }
    }
    non_empty_env("SDKWORK_WEBSERVER_HOST_OS_VERSION")
}

fn kernel_version() -> Option<String> {
    if let Ok(contents) = std::fs::read_to_string("/proc/sys/kernel/osrelease") {
        let trimmed = contents.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    non_empty_env("SDKWORK_WEBSERVER_HOST_KERNEL_VERSION")
}

fn memory_total_mb() -> Option<i64> {
    let contents = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            let kib: i64 = rest.trim().trim_end_matches(" kB").trim().parse().ok()?;
            return Some(kib / 1024);
        }
    }
    None
}

/// Primary outbound local IP via a connection-less UDP "connect" (no packet is
/// sent); the full interface list is available through
/// `SDKWORK_WEBSERVER_HOST_LOCAL_IPS`.
fn local_ips() -> Vec<String> {
    if let Some(configured) = non_empty_env("SDKWORK_WEBSERVER_HOST_LOCAL_IPS") {
        return configured
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();
    }
    let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") else {
        return Vec::new();
    };
    socket
        .connect("8.8.8.8:80")
        .ok()
        .and_then(|_| socket.local_addr().ok())
        .map(|address| vec![address.ip().to_string()])
        .unwrap_or_default()
}

fn mac_addresses() -> Vec<String> {
    if let Some(configured) = non_empty_env("SDKWORK_WEBSERVER_HOST_MAC_ADDRESSES") {
        return configured
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();
    }
    // Linux: the kernel exposes per-interface addresses under /sys.
    let mut macs = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            let Ok(contents) = std::fs::read_to_string(entry.path().join("address")) else {
                continue;
            };
            let mac = contents.trim();
            if mac.is_empty() || mac == "00:00:00:00:00:00" {
                continue;
            }
            if !macs.iter().any(|existing| existing == mac) {
                macs.push(mac.to_string());
            }
        }
    }
    macs
}

/// Current resident set size in MiB when the platform exposes it.
fn resident_memory_mb() -> Option<i64> {
    let contents = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kib: i64 = rest.trim().trim_end_matches(" kB").trim().parse().ok()?;
            return Some(kib / 1024);
        }
    }
    None
}

/// Parses a byte-size budget (`4g`, `512m`, `1073741824`) into MiB. Accepts
/// the same values the compose/Kubernetes resource limits take so the
/// self-report reads the deployment's actual governance limit.
fn memory_limit_mib() -> Option<i64> {
    parse_memory_limit_mib(
        std::env::var("SDKWORK_WEBSERVER_MEMORY_LIMIT")
            .ok()
            .as_deref(),
    )
}

fn parse_memory_limit_mib(raw: Option<&str>) -> Option<i64> {
    let raw = raw?.trim().to_ascii_lowercase();
    let (digits, multiplier_mib): (String, i64) = if let Some(value) = raw.strip_suffix("g") {
        (value.trim().to_owned(), 1024)
    } else if let Some(value) = raw.strip_suffix("m") {
        (value.trim().to_owned(), 1)
    } else if let Some(value) = raw.strip_suffix("k") {
        // A kiB-granular budget is below the MiB resolution of the reading;
        // treat it as 0 MiB rather than rounding it up to a false success.
        (value.trim().to_owned(), 0)
    } else if raw.chars().all(|c| c.is_ascii_digit()) && !raw.is_empty() {
        (raw, 0)
    } else {
        return None;
    };
    let value: i64 = digits.parse().ok()?;
    Some(value.checked_mul(multiplier_mib)?)
}

/// One heartbeat's measured health: the state string the contract expects
/// (`HEALTHY`/`DEGRADED`/`UNHEALTHY`) plus the memory utilization percent
/// when both a reading and a budget exist.
fn measured_health_state(
    rss_mb: Option<i64>,
    limit_mb: Option<i64>,
) -> (&'static str, Option<i64>) {
    let Some(rss) = rss_mb else {
        return ("HEALTHY", None);
    };
    let Some(limit) = limit_mb else {
        return ("HEALTHY", None);
    };
    if limit <= 0 {
        return ("HEALTHY", None);
    }
    let percent = rss.saturating_mul(100) / limit;
    let state = if percent >= MEMORY_UNHEALTHY_RATIO_PERCENT {
        "UNHEALTHY"
    } else if percent >= MEMORY_DEGRADED_RATIO_PERCENT {
        "DEGRADED"
    } else {
        "HEALTHY"
    };
    (state, Some(percent))
}

/// What an `ops` directive asks this node to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpsAction {
    /// Keep reporting; the directive only describes how the registry treats
    /// this instance.
    Continue,
    /// Report drain completion, then stop.
    Drain,
}

/// The node's reading of an `ops` directive.
///
/// Cordon (`routingEnabled = false`) changes what the *registry* routes here,
/// never what the node does: a cordoned instance keeps serving and keeps
/// reporting. Only an explicit drain request ends this instance's service —
/// treating cordon as a stop would take an instance the operator only wanted
/// out of the balancing pool offline.
const fn ops_action(ops: &ClusterInstanceOpsDirectives) -> OpsAction {
    if ops.drain_requested {
        OpsAction::Drain
    } else {
        OpsAction::Continue
    }
}

/// Obeys the registry's per-instance operations directives
/// (`docs/architecture/tech/TECH-cluster-management.md`:183) and reports
/// whether they end this instance's service.
///
/// `drainRequested` is the node half of the graceful-drain loop: the operator
/// marked the instance draining, the registry already stopped routing new work
/// to it, and the node must close the loop. The completion report is written
/// **before** the stop is requested, so a process tearing down mid-drain can
/// never leave the operator's drain open. Unreported completion is safe:
/// the registry keeps answering `drainRequested` on every heartbeat until it
/// sees the report, and `record_cluster_drain_complete` is idempotent.
async fn obey_ops_directives(
    service: &Arc<WebService>,
    instance_uuid: &str,
    ops: ClusterInstanceOpsDirectives,
) -> bool {
    if !ops.routing_enabled {
        tracing::info!(
            instance = %instance_uuid,
            draining = ops.drain_requested,
            "cluster routing is disabled for this instance (cordoned or draining)"
        );
    }
    if ops_action(&ops) == OpsAction::Continue {
        return false;
    }
    if let Err(error) = service.cluster_drain_complete(instance_uuid).await {
        tracing::warn!(
            instance = %instance_uuid,
            error = ?error,
            "cluster drain-complete report failed; staying registered to retry"
        );
        return false;
    }
    tracing::info!(
        instance = %instance_uuid,
        "cluster drain complete reported; stopping this instance"
    );
    // Stop through the same trigger an operator signal uses, so the data plane
    // retires in-flight work within `drainTimeoutMs` instead of cutting it.
    crate::runtime_shutdown::request_shutdown(
        crate::runtime_shutdown::ShutdownReason::ClusterDrain,
    );
    true
}

/// Long-running self-report task handle. Detached by design: the owner is the
/// gateway process, and the loop retries with bounded backoff.
pub fn spawn_cluster_self_report_task(
    service: Arc<WebService>,
    config: ClusterSelfReportConfig,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        run_self_report_loop(service, config).await;
    })
}

async fn run_self_report_loop(service: Arc<WebService>, config: ClusterSelfReportConfig) {
    if !config.enabled {
        tracing::info!("cluster self-report is disabled by configuration");
        return;
    }
    // Small startup jitter so scaled-out instances do not register in lockstep.
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.subsec_nanos() as u64)
        .unwrap_or(0)
        % 2_000;
    tokio::time::sleep(Duration::from_millis(jitter)).await;

    let host = collect_host_identity();
    let process_pid = std::process::id() as i32;
    let started = tokio::time::Instant::now();
    // Wall-clock approximation of the process start; restarts with the same
    // PID take a new instant, which is what the registry keys on.
    let process_started_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    let build_version = env!("CARGO_PKG_VERSION").to_string();

    let mut instance_uuid: Option<String> = None;
    let mut heartbeat_interval = Duration::from_secs(config.heartbeat_interval_secs);
    let mut backoff_multiplier: u32 = 1;

    loop {
        let mut tick_healthy = true;
        if instance_uuid.is_none() {
            let request = ClusterRegistrationRequest {
                cluster_code: config.cluster_code.clone(),
                host: sdkwork_webserver_contract::ClusterHostDescriptor {
                    hostname: host.hostname.clone(),
                    machine_code: host.machine_code.clone(),
                    name: Some(host.hostname.clone()),
                    os_name: Some(host.os_name.clone()),
                    os_version: host.os_version.clone(),
                    kernel_version: host.kernel_version.clone(),
                    arch: Some(host.arch.clone()),
                    cpu_model: None,
                    cpu_cores: host.cpu_cores,
                    memory_total_mb: host.memory_total_mb,
                    remote_ip: None,
                    local_ips: host.local_ips.clone(),
                    mac_addresses: host.mac_addresses.clone(),
                    daemon_version: Some(build_version.clone()),
                    join_mode: Some("LAN".to_owned()),
                    tunnel: None,
                },
                instance: sdkwork_webserver_contract::ClusterInstanceDescriptor {
                    name: config.instance_name.clone(),
                    role: config.role.clone(),
                    environment: config.environment.clone(),
                    process_pid,
                    process_started_at: process_started_at.clone(),
                    bind_host: config.bind_host.clone(),
                    bind_port: config.bind_port,
                    public_endpoint: config.public_endpoint.clone(),
                    build_version: Some(build_version.clone()),
                    join_mode: Some("LAN".to_owned()),
                },
            };
            match service.cluster_register(None, &request).await {
                Ok(response) => {
                    tracing::info!(
                        instance = %response.instance.id,
                        cluster = %response.cluster.code,
                        host = %response.host.hostname,
                        "cluster self-registration accepted"
                    );
                    instance_uuid = Some(response.instance.id);
                    heartbeat_interval = Duration::from_secs(
                        (response.heartbeat_interval_seconds as u64)
                            .clamp(MIN_HEARTBEAT_INTERVAL_SECS, MAX_HEARTBEAT_INTERVAL_SECS),
                    );
                }
                Err(error) => {
                    tick_healthy = false;
                    tracing::warn!(error = ?error, "cluster self-registration failed; retrying");
                }
            }
        }
        if let Some(uuid) = &instance_uuid {
            let mut metrics = serde_json::Map::new();
            metrics.insert("pid".to_string(), serde_json::json!(process_pid));
            let rss_mb = resident_memory_mb();
            let limit_mb = memory_limit_mib();
            if let Some(rss_mb) = rss_mb {
                metrics.insert("rssMb".to_string(), serde_json::json!(rss_mb));
            }
            if let Some(limit_mb) = limit_mb {
                metrics.insert("memoryLimitMb".to_string(), serde_json::json!(limit_mb));
            }
            let (health_state, memory_used_percent) = measured_health_state(rss_mb, limit_mb);
            if let Some(memory_used_percent) = memory_used_percent {
                metrics.insert(
                    "memoryUsedPercent".to_string(),
                    serde_json::json!(memory_used_percent),
                );
            }
            // `status` reports the instance is online: the beat itself proves
            // the process is running. `health_state` is the measured reading,
            // never a constant claim.
            let request = ClusterHeartbeatRequest {
                status: 1,
                health_state: health_state.to_string(),
                uptime_seconds: started.elapsed().as_secs() as i64,
                build_version: Some(build_version.clone()),
                metrics: serde_json::Value::Object(metrics),
            };
            match service.cluster_heartbeat(uuid, &request).await {
                Ok(response) => {
                    heartbeat_interval = Duration::from_secs(
                        (response.heartbeat_interval_seconds as u64)
                            .clamp(MIN_HEARTBEAT_INTERVAL_SECS, MAX_HEARTBEAT_INTERVAL_SECS),
                    );
                    if !response.messages.is_empty() {
                        tracing::debug!(
                            count = response.messages.len(),
                            "delivered cluster peer messages"
                        );
                    }
                    if let Some(ops) = response.ops {
                        if obey_ops_directives(&service, uuid, ops).await {
                            // The instance is recorded as stopped, so the loop
                            // must not heartbeat again: every heartbeat carries
                            // `status: 1`, which would put a stopped instance
                            // back online and leave the operator's drain open
                            // forever. The process itself leaves through the
                            // shutdown path requested above, where the data
                            // plane retires in-flight work within
                            // `drainTimeoutMs`.
                            return;
                        }
                    }
                }
                Err(sdkwork_webserver_contract::WebServiceError::NotFound(_)) => {
                    // The registry row disappeared (operator removal or drift
                    // repair); re-register on the next tick.
                    tracing::warn!("cluster instance is no longer registered; re-registering");
                    instance_uuid = None;
                    tick_healthy = false;
                }
                Err(error) => {
                    tick_healthy = false;
                    tracing::warn!(error = ?error, "cluster heartbeat failed; retrying");
                }
            }
        }
        match service.run_cluster_liveness_sweep().await {
            Ok(report) if report.instances_expired + report.hosts_expired > 0 => {
                tracing::info!(
                    instances = report.instances_expired,
                    hosts = report.hosts_expired,
                    purged = report.heartbeats_purged,
                    "cluster liveness sweep expired members"
                );
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(error = ?error, "cluster liveness sweep failed; retrying");
            }
        }
        // Backoff is a FAILURE response only: a healthy tick reports at the
        // configured cadence so members are never swept for slow reporting.
        if tick_healthy {
            backoff_multiplier = 1;
        } else {
            backoff_multiplier = (backoff_multiplier * 2).min(MAX_BACKOFF_MULTIPLIER);
        }
        let sleep = heartbeat_interval * backoff_multiplier;
        tokio::time::sleep(sleep).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directives(routing_enabled: bool, drain_requested: bool) -> ClusterInstanceOpsDirectives {
        ClusterInstanceOpsDirectives {
            routing_enabled,
            drain_requested,
        }
    }

    /// The full directive truth table. The node's only stopping condition is an
    /// explicit drain request; the routing flag is the registry's business.
    #[test]
    fn only_a_drain_request_ends_the_instance() {
        assert_eq!(
            ops_action(&directives(true, false)),
            OpsAction::Continue,
            "a routing instance keeps serving"
        );
        assert_eq!(
            ops_action(&directives(false, false)),
            OpsAction::Continue,
            "cordon takes the instance out of the balancing pool, not out of service"
        );
        assert_eq!(
            ops_action(&directives(true, true)),
            OpsAction::Drain,
            "a drain is obeyed even while routing is still on"
        );
        assert_eq!(
            ops_action(&directives(false, true)),
            OpsAction::Drain,
            "the registry's own drain posture is obeyed"
        );
    }

    #[test]
    fn memory_budget_parses_deployment_limit_values() {
        assert_eq!(parse_memory_limit_mib(Some("4g")), Some(4 * 1024));
        assert_eq!(parse_memory_limit_mib(Some("512m")), Some(512));
        assert_eq!(parse_memory_limit_mib(Some(" 1G ")), Some(1024));
        assert_eq!(parse_memory_limit_mib(Some("512k")), Some(0));
        assert_eq!(parse_memory_limit_mib(None), None);
        assert_eq!(parse_memory_limit_mib(Some("bogus")), None);
    }

    #[test]
    fn health_state_is_measured_against_the_configured_budget() {
        // No reading or no budget: the instance must not invent a claim, but
        // liveness is owned by the beat itself, so the state stays HEALTHY.
        assert_eq!(measured_health_state(None, Some(4096)), ("HEALTHY", None));
        assert_eq!(measured_health_state(Some(1024), None), ("HEALTHY", None));
        // 50% of budget.
        assert_eq!(
            measured_health_state(Some(2048), Some(4096)),
            ("HEALTHY", Some(50))
        );
        // 75% is the DEGRADED threshold, 90% UNHEALTHY.
        assert_eq!(
            measured_health_state(Some(3072), Some(4096)),
            ("DEGRADED", Some(75))
        );
        assert_eq!(
            measured_health_state(Some(3700), Some(4096)),
            ("UNHEALTHY", Some(90))
        );
    }
}
