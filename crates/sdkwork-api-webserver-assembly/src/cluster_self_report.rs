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
    ClusterHeartbeatRequest, ClusterRegistrationRequest, CLUSTER_ENVIRONMENTS,
};
use tokio::task::JoinHandle;

const DEFAULT_HEARTBEAT_INTERVAL_SECS: u64 = 15;
const MAX_HEARTBEAT_INTERVAL_SECS: u64 = 600;
const MIN_HEARTBEAT_INTERVAL_SECS: u64 = 5;
const MAX_BACKOFF_MULTIPLIER: u32 = 8;
const MACHINE_CODE_MIN_CHARS: usize = 8;

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
                !matches!(value.trim().to_ascii_lowercase().as_str(), "0" | "false" | "off")
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
                return Err(format!("{key} must be between {minimum} and {maximum} seconds"));
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
            if let Some(rss_mb) = resident_memory_mb() {
                metrics.insert("rssMb".to_string(), serde_json::json!(rss_mb));
            }
            let request = ClusterHeartbeatRequest {
                status: 1,
                health_state: "HEALTHY".to_string(),
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
