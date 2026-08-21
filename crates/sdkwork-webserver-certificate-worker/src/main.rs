//! Durable certificate operation worker.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sdkwork_intelligence_webserver_repository_sqlx::bootstrap_web_runtime_from_env;
use tracing::{info, warn};

const DEFAULT_OPERATION_POLL_INTERVAL_SECS: u64 = 5;
const MIN_OPERATION_POLL_INTERVAL_SECS: u64 = 1;
const MAX_OPERATION_POLL_INTERVAL_SECS: u64 = 60;
const DEFAULT_RENEWAL_SCHEDULE_INTERVAL_SECS: u64 = 3_600;
const MIN_RENEWAL_SCHEDULE_INTERVAL_SECS: u64 = 60;
const MAX_RENEWAL_SCHEDULE_INTERVAL_SECS: u64 = 86_400;
/// Watchdog ceiling for one certificate operation cycle. A cycle that hangs
/// (for example a stalled ACME or database call without its own deadline)
/// must not block renewal scheduling or graceful shutdown forever.
const MIN_CYCLE_TIMEOUT_SECS: u64 = 60;
const MAX_CYCLE_TIMEOUT_SECS: u64 = 600;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    // Installed deployments load the typed runtime configuration
    // (/etc/sdkwork/webserver/config.toml) and materialize it into
    // the process environment before any runtime component reads it.
    sdkwork_webserver_core::runtime_config::load_runtime_toml_config()
        .map_err(|error| anyhow::anyhow!("runtime TOML configuration is invalid: {error}"))?;
    sdkwork_database_sqlx::enable_process_shared_database_pool();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let poll_interval_secs = parse_bounded_interval(
        "SDKWORK_WEBSERVER_CERT_OPERATION_POLL_INTERVAL_SECS",
        std::env::var("SDKWORK_WEBSERVER_CERT_OPERATION_POLL_INTERVAL_SECS")
            .ok()
            .as_deref(),
        DEFAULT_OPERATION_POLL_INTERVAL_SECS,
        MIN_OPERATION_POLL_INTERVAL_SECS,
        MAX_OPERATION_POLL_INTERVAL_SECS,
    )?;
    let renewal_schedule_interval_secs = parse_bounded_interval(
        "SDKWORK_WEBSERVER_CERT_RENEW_SCAN_INTERVAL_SECS",
        std::env::var("SDKWORK_WEBSERVER_CERT_RENEW_SCAN_INTERVAL_SECS")
            .ok()
            .as_deref(),
        DEFAULT_RENEWAL_SCHEDULE_INTERVAL_SECS,
        MIN_RENEWAL_SCHEDULE_INTERVAL_SECS,
        MAX_RENEWAL_SCHEDULE_INTERVAL_SECS,
    )?;
    let worker_id = resolve_worker_id(std::env::var("SDKWORK_WEBSERVER_CERT_WORKER_ID").ok())?;

    info!(
        worker_id,
        poll_interval_secs,
        renewal_schedule_interval_secs,
        "sdkwork-webserver-certificate-worker started"
    );

    let runtime = bootstrap_web_runtime_from_env()
        .await
        .map_err(|error| anyhow::anyhow!(error))?;
    let mut shutdown_task = tokio::spawn(shutdown_signal());
    let mut next_renewal_schedule = Instant::now();
    // Reconcile the node TLS runtime snapshot on startup: after a worker or
    // data-plane restart the snapshot must converge to the database state
    // without waiting for the next certificate operation.
    if let Err(error) = runtime.service.publish_node_tls_material().await {
        warn!(
            error = %error,
            "startup node TLS material reconciliation failed"
        );
    }
    // Randomize the startup phase so multiple worker replicas do not claim
    // operations in lockstep.
    tokio::time::sleep(Duration::from_millis(jitter_millis(
        poll_interval_secs * 1_000,
    )))
    .await;

    let mut consecutive_failures: u32 = 0;
    // Watchdog: a cycle that outlives this bound is cancelled so renewal
    // scheduling and graceful shutdown always make progress.
    let cycle_timeout_secs = (poll_interval_secs.saturating_mul(12))
        .clamp(MIN_CYCLE_TIMEOUT_SECS, MAX_CYCLE_TIMEOUT_SECS);
    loop {
        let schedule_renewals = Instant::now() >= next_renewal_schedule;
        let cycle_report = tokio::select! {
            result = runtime
                .service
                .run_certificate_operation_cycle(&worker_id, schedule_renewals) => {
                match result {
                    Ok(report) => Some(report),
                    Err(error) => {
                        consecutive_failures = consecutive_failures.saturating_add(1);
                        warn!(error = %error, "certificate operation cycle failed");
                        None
                    }
                }
            }
            result = &mut shutdown_task => {
                result
                    .map_err(|error| anyhow::anyhow!("certificate worker shutdown task failed: {error}"))?
                    .map_err(|error| anyhow::anyhow!("certificate worker shutdown listener failed: {error}"))?;
                info!("certificate operation worker stopped after completing the active cycle");
                break;
            }
            () = tokio::time::sleep(Duration::from_secs(cycle_timeout_secs)) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                warn!(
                    cycle_timeout_secs,
                    "certificate operation cycle exceeded its watchdog timeout"
                );
                None
            }
        };
        if let Some(report) = cycle_report {
            consecutive_failures = 0;
            if schedule_renewals {
                next_renewal_schedule =
                    Instant::now() + Duration::from_secs(renewal_schedule_interval_secs);
            }
            if report.scheduled > 0 || report.claimed > 0 {
                info!(
                    scheduled = report.scheduled,
                    claimed = report.claimed,
                    succeeded = report.succeeded,
                    retried = report.retried,
                    failed = report.failed,
                    "certificate operation cycle completed"
                );
            }
        }
        // Exponential backoff on failure (bounded) plus per-cycle jitter so
        // replica workers spread their SKIP LOCKED claim scans.
        let base_secs = if consecutive_failures > 0 {
            (2_u64 << consecutive_failures.min(5)).min(poll_interval_secs.max(2))
        } else {
            poll_interval_secs
        };
        let delay =
            Duration::from_secs(base_secs) + Duration::from_millis(jitter_millis(base_secs * 250));
        tokio::select! {
            result = &mut shutdown_task => {
                result
                    .map_err(|error| anyhow::anyhow!("certificate worker shutdown task failed: {error}"))?
                    .map_err(|error| anyhow::anyhow!("certificate worker shutdown listener failed: {error}"))?;
                info!("certificate operation worker stopped after completing the active cycle");
                break;
            }
            () = tokio::time::sleep(delay) => {}
        }
    }

    Ok(())
}

/// Non-cryptographic millisecond jitter in `[0, max_ms)` derived from the clock
/// and process identity; used only to de-synchronize worker replicas.
fn jitter_millis(max_ms: u64) -> u64 {
    if max_ms == 0 {
        return 0;
    }
    let mut state = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
        ^ u64::from(std::process::id()) << 32;
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    state % max_ms
}

fn parse_bounded_interval(
    key: &str,
    value: Option<&str>,
    default: u64,
    minimum: u64,
    maximum: u64,
) -> anyhow::Result<u64> {
    let interval = match value {
        None => default,
        Some(value) => value.parse::<u64>().map_err(|_| {
            anyhow::anyhow!("{key} must be an integer between {minimum} and {maximum}")
        })?,
    };
    if !(minimum..=maximum).contains(&interval) {
        anyhow::bail!("{key} must be between {minimum} and {maximum}");
    }
    Ok(interval)
}

fn resolve_worker_id(configured: Option<String>) -> anyhow::Result<String> {
    let worker_id = configured.unwrap_or_else(|| {
        let epoch_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("certificate-worker:{}:{epoch_nanos}", std::process::id())
    });
    if worker_id.is_empty()
        || worker_id.len() > 128
        || !worker_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        anyhow::bail!(
            "SDKWORK_WEBSERVER_CERT_WORKER_ID must contain 1..128 ASCII letters, digits, '-', '_', '.', or ':'"
        );
    }
    Ok(worker_id)
}

async fn shutdown_signal() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut interrupt = signal(SignalKind::interrupt())?;
        let mut terminate = signal(SignalKind::terminate())?;
        tokio::select! {
            _ = interrupt.recv() => Ok(()),
            _ = terminate.recv() => Ok(()),
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_bounded_interval, resolve_worker_id, DEFAULT_OPERATION_POLL_INTERVAL_SECS};

    #[test]
    fn operation_interval_defaults_and_accepts_bounded_values() {
        assert_eq!(
            parse_bounded_interval("TEST", None, DEFAULT_OPERATION_POLL_INTERVAL_SECS, 1, 60)
                .expect("default interval"),
            DEFAULT_OPERATION_POLL_INTERVAL_SECS
        );
        assert_eq!(
            parse_bounded_interval("TEST", Some("1"), 5, 1, 60).unwrap(),
            1
        );
        assert_eq!(
            parse_bounded_interval("TEST", Some("60"), 5, 1, 60).unwrap(),
            60
        );
    }

    #[test]
    fn bounded_interval_rejects_hot_loops_overflow_and_invalid_text() {
        for value in ["0", "61", "18446744073709551616", "five"] {
            assert!(parse_bounded_interval("TEST", Some(value), 5, 1, 60).is_err());
        }
    }

    #[test]
    fn worker_id_is_unique_by_default_and_rejects_unsafe_values() {
        assert!(resolve_worker_id(None)
            .unwrap()
            .starts_with("certificate-worker:"));
        assert_eq!(
            resolve_worker_id(Some("worker-1:primary".to_string())).unwrap(),
            "worker-1:primary"
        );
        assert!(resolve_worker_id(Some("worker/1".to_string())).is_err());
    }
}
