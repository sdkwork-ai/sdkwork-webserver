use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ServerOperationResult {
    #[serde(rename = "operationId")]
    pub operation_id: String,

    /// Foreground runs only; absent when the run timed out.
    #[serde(rename = "exitCode")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,

    #[serde(rename = "timedOut")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timed_out: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,

    /// True when stdout exceeded the capture cap and was truncated.
    #[serde(rename = "stdoutTruncated")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout_truncated: Option<bool>,

    /// True when stderr exceeded the capture cap and was truncated.
    #[serde(rename = "stderrTruncated")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr_truncated: Option<bool>,

    /// Managed runs; the recorded (or restarted) process id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<i64>,

    /// Managed runs; path of the pid-file record.
    #[serde(rename = "pidFile")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid_file: Option<String>,

    /// Managed runs; path of the append-only run log.
    #[serde(rename = "logFile")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_file: Option<String>,

    /// Managed stops/restarts; false when no live process existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped: Option<bool>,

    /// Human-readable outcome for managed runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
