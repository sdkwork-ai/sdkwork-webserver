//! Bounded, truthful execution of project operations.
//!
//! Two execution models exist, and every operation maps to exactly one:
//!
//! - **Foreground** (`run_foreground`): build/package-style commands run to
//!   completion under a hard timeout. On expiry the process tree is killed
//!   and reaped, so an operation can never pin the handler forever.
//!   Captured output is byte-capped so a chatty build cannot balloon the
//!   response.
//! - **Managed** (`start_managed` / `stop_managed`): long-running commands
//!   (dev servers, service processes) are spawned detached from the request
//!   with their identity persisted in a per-project pid file. Stop and
//!   restart address that exact pid-file record and kill the whole process
//!   tree (process group on Unix, `taskkill /T` on Windows) instead of
//!   guessing with shell interpolation such as `kill $(pgrep ...)`, which
//!   never executes because operations are argv-only by contract.
//!
//! Every spawned subprocess is argv-only, bounded by a timeout, and its
//! output is bounded by a byte cap. There is no shell interpretation anywhere
//! in this module.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

/// Hard wall-clock budget for one foreground operation (build/package).
pub const FOREGROUND_OPERATION_TIMEOUT: Duration = Duration::from_secs(600);

/// Maximum stdout/stderr bytes captured per stream for a foreground run.
pub const MAXIMUM_CAPTURED_OUTPUT_BYTES: usize = 128 * 1024;

/// Budget for stopping one managed process tree.
pub const MANAGED_STOP_TIMEOUT: Duration = Duration::from_secs(10);

/// Directory (inside the project root) holding managed-process bookkeeping.
pub const RUN_DIRECTORY_NAME: &str = ".sdkwork-run";

/// Failure modes of operation execution. `Io` keeps only a message so OS
/// detail stays under caller control.
#[derive(Debug, thiserror::Error)]
pub enum OperationRunError {
    #[error("operation timed out after {timeout_secs}s: {program}")]
    Timeout { program: String, timeout_secs: u64 },
    #[error("failed to start {program}: {detail}")]
    Spawn { program: String, detail: String },
    #[error("managed process is already running (pid {pid})")]
    ManagedAlreadyRunning { pid: u32 },
    #[error("managed process bookkeeping is unreadable: {0}")]
    ManagedState(String),
    #[error("managed process could not be stopped: {0}")]
    ManagedStop(String),
    #[error("operation I/O failed: {detail}")]
    Io { detail: String },
}

/// Result of one bounded foreground run.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForegroundRunOutcome {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

/// Result of starting one managed background process.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedStartOutcome {
    pub pid: u32,
    pub pid_file: String,
    pub log_file: String,
}

/// Result of stopping one managed background process.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedStopOutcome {
    pub stopped: bool,
    /// The pid that was stopped, when a live record existed.
    pub pid: Option<u32>,
}

/// Persisted identity of one managed process.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManagedProcessRecord {
    pid: u32,
    program: String,
    started_at_unix: u64,
}

/// Run `program` with `args` in `cwd` to completion under a hard timeout,
/// killing the process tree on expiry. Output capture is capped per stream.
pub async fn run_foreground(
    program: &str,
    args: &[String],
    cwd: &Path,
) -> Result<ForegroundRunOutcome, OperationRunError> {
    run_foreground_with_limits(program, args, cwd, FOREGROUND_OPERATION_TIMEOUT).await
}

async fn run_foreground_with_limits(
    program: &str,
    args: &[String],
    cwd: &Path,
    timeout: Duration,
) -> Result<ForegroundRunOutcome, OperationRunError> {
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        // Own process group so a timeout kill can take the whole tree
        // (cargo/npm/pnpm spawn their own children).
        command.process_group(0);
    }

    let mut child = command.spawn().map_err(|error| OperationRunError::Spawn {
        program: program.to_owned(),
        detail: error.to_string(),
    })?;
    let mut stdout_pipe = child.stdout.take().ok_or_else(|| OperationRunError::Io {
        detail: "stdout pipe unavailable".to_owned(),
    })?;
    let mut stderr_pipe = child.stderr.take().ok_or_else(|| OperationRunError::Io {
        detail: "stderr pipe unavailable".to_owned(),
    })?;

    // Read both streams concurrently. The reads complete when the child
    // exits or closes its pipes; the timeout covers the pathological child
    // that keeps its pipes open forever.
    let capture = async {
        let (stdout, stdout_truncated) =
            read_capped(&mut stdout_pipe, MAXIMUM_CAPTURED_OUTPUT_BYTES).await;
        let (stderr, stderr_truncated) =
            read_capped(&mut stderr_pipe, MAXIMUM_CAPTURED_OUTPUT_BYTES).await;
        let status = child.wait().await;
        (stdout, stdout_truncated, stderr, stderr_truncated, status)
    };
    match tokio::time::timeout(timeout, capture).await {
        Ok((stdout, stdout_truncated, stderr, stderr_truncated, status)) => {
            let exit_code = status.ok().and_then(|status| status.code());
            Ok(ForegroundRunOutcome {
                exit_code,
                timed_out: false,
                stdout,
                stderr,
                stdout_truncated,
                stderr_truncated,
            })
        }
        Err(_elapsed) => {
            // Kill the whole tree so a timed-out build cannot leak
            // descendants that keep running after the request failed.
            if let Some(pid) = child.id() {
                let _ = kill_pid_tree(pid).await;
            }
            Err(OperationRunError::Timeout {
                program: program.to_owned(),
                timeout_secs: timeout.as_secs(),
            })
        }
    }
}

async fn read_capped(
    reader: &mut (impl tokio::io::AsyncRead + Unpin),
    limit: usize,
) -> (String, bool) {
    // Read one extra byte so truncation is detectable without overshooting
    // the cap by an unbounded amount.
    let mut buffer = Vec::with_capacity(limit.min(64 * 1024));
    let mut limited = reader.take(limit as u64 + 1);
    if let Err(error) = limited.read_to_end(&mut buffer).await {
        tracing::warn!(%error, "operation output capture ended early");
    }
    let truncated = buffer.len() > limit;
    buffer.truncate(limit);
    (String::from_utf8_lossy(&buffer).into_owned(), truncated)
}

/// Spawn `program` detached in `cwd`, recording its identity under the
/// project run directory. A live record for the same operation slot refuses
/// a duplicate start. Output is appended to the run log file.
pub async fn start_managed(
    operation_id: &str,
    program: &str,
    args: &[String],
    cwd: &Path,
) -> Result<ManagedStartOutcome, OperationRunError> {
    let run_dir = cwd.join(RUN_DIRECTORY_NAME);
    let pid_file = run_dir.join(format!("{operation_id}.pid"));
    let log_file = run_dir.join(format!("{operation_id}.log"));

    tokio::fs::create_dir_all(&run_dir)
        .await
        .map_err(|error| OperationRunError::Io {
            detail: format!("create run directory: {error}"),
        })?;
    if let Some(record) = read_managed_record(&pid_file).await? {
        if managed_process_alive(record.pid).await {
            return Err(OperationRunError::ManagedAlreadyRunning { pid: record.pid });
        }
        // Stale record: the previous process is gone.
        let _ = tokio::fs::remove_file(&pid_file).await;
    }

    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
        .map_err(|error| OperationRunError::Io {
            detail: format!("open run log: {error}"),
        })?;
    let error_log = log.try_clone().map_err(|error| OperationRunError::Io {
        detail: format!("share run log: {error}"),
    })?;

    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(error_log))
        .kill_on_drop(false);
    #[cfg(unix)]
    {
        command.process_group(0);
    }

    let child = command.spawn().map_err(|error| OperationRunError::Spawn {
        program: program.to_owned(),
        detail: error.to_string(),
    })?;
    let pid = child.id().ok_or_else(|| OperationRunError::Spawn {
        program: program.to_owned(),
        detail: "process exited before its pid could be recorded".to_owned(),
    })?;

    let record = ManagedProcessRecord {
        pid,
        program: program.to_owned(),
        started_at_unix: unix_now(),
    };
    write_managed_record(&pid_file, &record).await?;
    tracing::info!(
        operation_id = %operation_id,
        pid = pid,
        "managed project operation started"
    );

    Ok(ManagedStartOutcome {
        pid,
        pid_file: pid_file.to_string_lossy().into_owned(),
        log_file: log_file.to_string_lossy().into_owned(),
    })
}

/// Stop the managed process recorded for `operation_id` in `cwd`'s run
/// directory, killing the whole process tree. A missing record answers
/// `stopped: false` — stopping an already-stopped project is not an error.
pub async fn stop_managed(
    operation_id: &str,
    cwd: &Path,
) -> Result<ManagedStopOutcome, OperationRunError> {
    let pid_file = cwd
        .join(RUN_DIRECTORY_NAME)
        .join(format!("{operation_id}.pid"));
    let Some(record) = read_managed_record(&pid_file).await? else {
        return Ok(ManagedStopOutcome {
            stopped: false,
            pid: None,
        });
    };
    let pid = record.pid;
    if managed_process_alive(pid).await {
        kill_pid_tree(pid)
            .await
            .map_err(OperationRunError::ManagedStop)?;
    }
    tokio::fs::remove_file(&pid_file)
        .await
        .map_err(|error| OperationRunError::ManagedState(format!("remove pid file: {error}")))?;
    tracing::info!(
        operation_id = %operation_id,
        pid = pid,
        "managed project operation stopped"
    );
    Ok(ManagedStopOutcome {
        stopped: true,
        pid: Some(pid),
    })
}

async fn read_managed_record(
    pid_file: &Path,
) -> Result<Option<ManagedProcessRecord>, OperationRunError> {
    match tokio::fs::read(pid_file).await {
        Ok(bytes) => {
            // The record file is small and self-written; still bound the
            // parse input so a corrupted file cannot balloon.
            let bytes = if bytes.len() > 4096 {
                &bytes[..4096]
            } else {
                &bytes[..]
            };
            serde_json::from_slice(bytes).map(Some).map_err(|error| {
                OperationRunError::ManagedState(format!("parse pid file: {error}"))
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(OperationRunError::ManagedState(format!(
            "read pid file: {error}"
        ))),
    }
}

async fn write_managed_record(
    pid_file: &Path,
    record: &ManagedProcessRecord,
) -> Result<(), OperationRunError> {
    let body = serde_json::to_vec(record)
        .map_err(|error| OperationRunError::ManagedState(format!("encode pid file: {error}")))?;
    let temp = pid_file.with_extension("pid.tmp");
    tokio::fs::write(&temp, body)
        .await
        .map_err(|error| OperationRunError::ManagedState(format!("write pid file: {error}")))?;
    tokio::fs::rename(&temp, pid_file)
        .await
        .map_err(|error| OperationRunError::ManagedState(format!("publish pid file: {error}")))?;
    Ok(())
}

/// Best-effort liveness check for a recorded pid. A check that cannot run
/// answers `true` (conservative: refuse duplicate starts rather than allow
/// them).
async fn managed_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let outcome = tokio::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
        matches!(outcome, Ok(status) if status.success())
    }
    #[cfg(windows)]
    {
        let outcome = tokio::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .stdin(Stdio::null())
            .output()
            .await;
        match outcome {
            Ok(output) => {
                let text = String::from_utf8_lossy(&output.stdout);
                text.contains(&format!(" {pid} "))
            }
            Err(_) => true,
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        true
    }
}

/// Kill a pid and every descendant. Unix signals the process group (spawned
/// children run in their own group); Windows walks the tree with
/// `taskkill /T`.
async fn kill_pid_tree(pid: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        // Negative pid targets the whole process group.
        let group = format!("-{pid}");
        let signal = |flag: &str| {
            tokio::process::Command::new("kill")
                .args([flag, "--", &group])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
        };
        let _ = signal("-TERM").await;
        if wait_for_exit(pid, MANAGED_STOP_TIMEOUT).await {
            return Ok(());
        }
        let _ = signal("-KILL").await;
        if wait_for_exit(pid, Duration::from_secs(2)).await {
            Ok(())
        } else {
            Err(format!("process group {pid} ignored both TERM and KILL"))
        }
    }
    #[cfg(windows)]
    {
        let outcome = tokio::time::timeout(
            MANAGED_STOP_TIMEOUT,
            tokio::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdin(Stdio::null())
                .output(),
        )
        .await;
        match outcome {
            Ok(Ok(output)) if output.status.success() => Ok(()),
            Ok(Ok(output)) => Err(format!(
                "taskkill exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )),
            Ok(Err(error)) => Err(error.to_string()),
            Err(_) => Err("taskkill did not finish within the stop budget".to_owned()),
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err("process-tree stop is not supported on this platform".to_owned())
    }
}

#[cfg(unix)]
async fn wait_for_exit(pid: u32, budget: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + budget;
    loop {
        if !managed_process_alive(pid).await {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

// Every test in this module drives real OS processes (`sh`, process groups);
// they only compile where those primitives exist.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sdkwork-operation-runtime-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn foreground_run_captures_output_and_exit_code() {
        let dir = scratch_dir("foreground");
        let outcome = run_foreground_with_limits(
            "sh",
            &["-c".to_owned(), "echo out; echo err 1>&2".to_owned()],
            &dir,
            Duration::from_secs(10),
        )
        .await
        .expect("bounded run succeeds");
        assert_eq!(outcome.exit_code, Some(0));
        assert!(outcome.stdout.contains("out"), "stdout: {}", outcome.stdout);
        assert!(outcome.stderr.contains("err"), "stderr: {}", outcome.stderr);
        assert!(!outcome.timed_out);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn foreground_run_kills_on_timeout() {
        let dir = scratch_dir("timeout");
        let started = std::time::Instant::now();
        let error = run_foreground_with_limits(
            "sh",
            &["-c".to_owned(), "sleep 60".to_owned()],
            &dir,
            Duration::from_millis(300),
        )
        .await
        .expect_err("the sleeper must hit the timeout");
        assert!(matches!(error, OperationRunError::Timeout { .. }));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "kill must be prompt"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn foreground_run_caps_output() {
        let dir = scratch_dir("cap");
        let outcome = run_foreground_with_limits(
            "sh",
            &["-c".to_owned(), "yes hello".to_owned()],
            &dir,
            Duration::from_secs(10),
        )
        .await
        .expect("bounded run succeeds");
        assert!(outcome.stdout_truncated, "the yes stream must be capped");
        assert_eq!(outcome.stdout.len(), MAXIMUM_CAPTURED_OUTPUT_BYTES);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn managed_start_stop_round_trip() {
        let dir = scratch_dir("managed");
        let started = start_managed(
            "start",
            "sh",
            &["-c".to_owned(), "sleep 60".to_owned()],
            &dir,
        )
        .await
        .expect("managed start succeeds");
        assert!(started.pid > 0);

        let duplicate = start_managed(
            "start",
            "sh",
            &["-c".to_owned(), "sleep 60".to_owned()],
            &dir,
        )
        .await
        .expect_err("duplicate start must be refused");
        assert!(matches!(
            duplicate,
            OperationRunError::ManagedAlreadyRunning { .. }
        ));

        let stopped = stop_managed("start", &dir)
            .await
            .expect("managed stop succeeds");
        assert_eq!(stopped.pid, Some(started.pid));
        assert!(stopped.stopped);

        let again = stop_managed("start", &dir).await.expect("idempotent stop");
        assert!(!again.stopped, "a second stop finds no record");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
