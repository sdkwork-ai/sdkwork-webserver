use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::Builder;

use crate::config::EdgeRuntimeConfig;
use crate::paths::{nginx_site_path, validate_domain_file_name};
use crate::{EdgeRuntimeError, EdgeRuntimeResult};

const MAX_NGINX_CONFIG_BYTES: usize = 1_048_576;
const MAX_NGINX_DIAGNOSTIC_BYTES: u64 = 8_192;
/// Ceiling for `nginx -T` captured output (`verify_served_config`). The dump
/// expands every included site file, so the capture must be bounded to keep
/// subprocess output memory O(ceiling) instead of O(merged configuration).
const MAX_NGINX_CAPTURE_BYTES: u64 = 4 * 1024 * 1024;
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(10);
const NGINX_DEPLOYMENT_LOCK_FILE: &str = ".nginx-deployment.lock";

pub fn deploy_nginx_config(
    config: &EdgeRuntimeConfig,
    domain: &str,
    config_content: &str,
) -> EdgeRuntimeResult<()> {
    let staged = stage_nginx_config(config, domain, config_content)?;
    let _lock = acquire_nginx_deployment_lock(config)?;
    let target = nginx_site_path(config, domain);
    let parent = target.parent().ok_or_else(|| {
        EdgeRuntimeError::Filesystem("nginx site target has no parent directory".to_string())
    })?;
    staged.persist(&target).map_err(|error| {
        EdgeRuntimeError::Filesystem(format!("activate nginx config: {}", error.error))
    })?;
    sync_directory(parent)?;
    Ok(())
}

pub(crate) fn acquire_nginx_deployment_lock(config: &EdgeRuntimeConfig) -> EdgeRuntimeResult<File> {
    std::fs::create_dir_all(&config.nginx_sites_root).map_err(|error| {
        EdgeRuntimeError::Filesystem(format!("create nginx sites directory: {error}"))
    })?;
    let lock_path = config.nginx_sites_root.join(NGINX_DEPLOYMENT_LOCK_FILE);
    if let Ok(metadata) = std::fs::symlink_metadata(&lock_path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(EdgeRuntimeError::Filesystem(
                "Nginx deployment lock must be a real file".to_string(),
            ));
        }
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|error| {
            EdgeRuntimeError::Filesystem(format!("open Nginx deployment lock: {error}"))
        })?;
    lock.try_lock().map_err(|error| {
        EdgeRuntimeError::Filesystem(format!(
            "Nginx deployment capacity exhausted by another process: {error}"
        ))
    })?;
    Ok(lock)
}

pub(crate) fn stage_nginx_config(
    config: &EdgeRuntimeConfig,
    domain: &str,
    config_content: &str,
) -> EdgeRuntimeResult<tempfile::NamedTempFile> {
    require_nginx_enabled(config)?;
    validate_domain_file_name(domain)?;
    validate_candidate_content(config_content)?;

    std::fs::create_dir_all(&config.nginx_sites_root).map_err(|error| {
        EdgeRuntimeError::Filesystem(format!("create nginx sites directory: {error}"))
    })?;
    let mut staged = Builder::new()
        .prefix(".sdkwork-nginx-")
        .suffix(".conf")
        .tempfile_in(&config.nginx_sites_root)
        .map_err(|error| {
            EdgeRuntimeError::Filesystem(format!("create staged nginx config: {error}"))
        })?;
    write_and_sync(
        staged.as_file_mut(),
        config_content.as_bytes(),
        "staged nginx config",
    )?;
    validate_nginx_file(config, staged.path())?;
    Ok(staged)
}

pub fn validate_nginx_config(
    config: &EdgeRuntimeConfig,
    config_content: &str,
) -> EdgeRuntimeResult<()> {
    require_nginx_enabled(config)?;
    validate_candidate_content(config_content)?;

    let directory = Builder::new()
        .prefix("sdkwork-nginx-validate-")
        .tempdir()
        .map_err(|error| {
            EdgeRuntimeError::Filesystem(format!("create nginx validation directory: {error}"))
        })?;
    let candidate = directory.path().join("candidate.conf");
    let mut file = File::create(&candidate).map_err(|error| {
        EdgeRuntimeError::Filesystem(format!("create nginx validation candidate: {error}"))
    })?;
    write_and_sync(
        &mut file,
        config_content.as_bytes(),
        "nginx validation candidate",
    )?;
    validate_nginx_file(config, &candidate)
}

fn validate_nginx_file(config: &EdgeRuntimeConfig, candidate: &Path) -> EdgeRuntimeResult<()> {
    let runtime = Builder::new()
        .prefix("sdkwork-nginx-runtime-")
        .tempdir()
        .map_err(|error| {
            EdgeRuntimeError::Filesystem(format!("create nginx validation runtime: {error}"))
        })?;
    let directory = runtime.path();
    for runtime_directory in ["logs", "temp"] {
        std::fs::create_dir_all(directory.join(runtime_directory)).map_err(|error| {
            EdgeRuntimeError::Filesystem(format!(
                "create nginx validation {runtime_directory} directory: {error}"
            ))
        })?;
    }
    let main_config = directory.join("sdkwork-validation-nginx.conf");
    let pid_path = directory.join("sdkwork-validation-nginx.pid");
    let wrapper = validation_main_config(candidate, &pid_path)?;
    let mut file = File::create(&main_config).map_err(|error| {
        EdgeRuntimeError::Filesystem(format!("create nginx validation main config: {error}"))
    })?;
    write_and_sync(
        &mut file,
        wrapper.as_bytes(),
        "nginx validation main config",
    )?;

    let prefix = nginx_path(directory)?;
    let main = nginx_path(&main_config)?;
    run_nginx_command(
        config,
        "validation",
        [
            OsString::from("-t"),
            OsString::from("-q"),
            OsString::from("-p"),
            OsString::from(prefix),
            OsString::from("-c"),
            OsString::from(main),
        ],
    )
}

pub fn reload_nginx(config: &EdgeRuntimeConfig) -> EdgeRuntimeResult<()> {
    require_nginx_enabled(config)?;
    run_nginx_command(
        config,
        "reload",
        [
            OsString::from("-s"),
            OsString::from("reload"),
            OsString::from("-c"),
            config.nginx_main_config.as_os_str().to_owned(),
        ],
    )
}

/// Proves that the running Nginx master has actually loaded a configuration
/// containing `expected_fragment` (PRD-FR-020 served-revision evidence).
///
/// `nginx -T` dumps the *loaded* configuration with includes expanded. A
/// reload that fails validation keeps the previous revision serving, so the
/// fragment check fails instead of reporting a false success.
pub fn verify_served_config(
    config: &EdgeRuntimeConfig,
    expected_fragment: &str,
) -> EdgeRuntimeResult<()> {
    require_nginx_enabled(config)?;
    let fragment = expected_fragment.trim();
    if fragment.is_empty() {
        return Err(EdgeRuntimeError::Config(
            "served-config verification fragment is empty".to_string(),
        ));
    }
    let main = nginx_path(&config.nginx_main_config)?;
    let dumped = run_nginx_command_captured(
        config,
        "dump",
        [
            OsString::from("-T"),
            OsString::from("-c"),
            OsString::from(main),
        ],
    )?;
    if dumped.contains(fragment) {
        return Ok(());
    }
    Err(EdgeRuntimeError::Nginx(format!(
        "served nginx configuration does not contain the expected revision fragment `{fragment}`"
    )))
}

pub fn validate_active_nginx_config(config: &EdgeRuntimeConfig) -> EdgeRuntimeResult<()> {
    require_nginx_enabled(config)?;
    run_nginx_command(
        config,
        "active configuration validation",
        [
            OsString::from("-t"),
            OsString::from("-q"),
            OsString::from("-c"),
            config.nginx_main_config.as_os_str().to_owned(),
        ],
    )
}

fn run_nginx_command<I, S>(
    config: &EdgeRuntimeConfig,
    operation: &'static str,
    arguments: I,
) -> EdgeRuntimeResult<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_nginx_command_inner(config, operation, arguments, CaptureMode::Discard)?;
    Ok(())
}

/// Runs an Nginx command and returns its captured stdout (used by
/// [`verify_served_config`] to prove the loaded revision).
fn run_nginx_command_captured<I, S>(
    config: &EdgeRuntimeConfig,
    operation: &'static str,
    arguments: I,
) -> EdgeRuntimeResult<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_nginx_command_inner(config, operation, arguments, CaptureMode::Capture)
}

enum CaptureMode {
    Discard,
    Capture,
}

fn run_nginx_command_inner<I, S>(
    config: &EdgeRuntimeConfig,
    operation: &'static str,
    arguments: I,
    capture: CaptureMode,
) -> EdgeRuntimeResult<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut diagnostic = tempfile::tempfile().map_err(|error| {
        EdgeRuntimeError::Filesystem(format!("create nginx diagnostic file: {error}"))
    })?;
    let diagnostic_writer = diagnostic.try_clone().map_err(|error| {
        EdgeRuntimeError::Filesystem(format!("clone nginx diagnostic file: {error}"))
    })?;
    let stdout = match capture {
        CaptureMode::Discard => Stdio::null(),
        CaptureMode::Capture => Stdio::piped(),
    };
    let mut child = Command::new(&config.nginx_binary)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(Stdio::from(diagnostic_writer))
        .spawn()
        .map_err(|error| {
            EdgeRuntimeError::Nginx(format!(
                "nginx {operation} process unavailable ({})",
                error.kind()
            ))
        })?;

    let timeout = Duration::from_millis(config.nginx_command_timeout_ms);
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => thread::sleep(PROCESS_POLL_INTERVAL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(EdgeRuntimeError::Nginx(format!(
                    "nginx {operation} timed out after {} ms",
                    config.nginx_command_timeout_ms
                )));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(EdgeRuntimeError::Nginx(format!(
                    "wait for nginx {operation}: {error}"
                )));
            }
        }
    };

    let captured = if let CaptureMode::Capture = capture {
        let mut output = Vec::with_capacity(MAX_NGINX_CAPTURE_BYTES as usize);
        if let Some(stdout) = child.stdout.take() {
            stdout
                .take(MAX_NGINX_CAPTURE_BYTES + 1)
                .read_to_end(&mut output)
                .map_err(|error| {
                    EdgeRuntimeError::Nginx(format!("read nginx {operation} stdout: {error}"))
                })?;
        }
        let truncated = output.len() > MAX_NGINX_CAPTURE_BYTES as usize;
        output.truncate(MAX_NGINX_CAPTURE_BYTES as usize);
        let mut value = String::from_utf8_lossy(&output).into_owned();
        if truncated {
            value.push_str("\n[output truncated]");
        }
        value
    } else {
        String::new()
    };

    if status.success() {
        return Ok(captured);
    }

    let diagnostic = read_bounded_diagnostic(&mut diagnostic);
    tracing::warn!(
        operation,
        exit_code = status.code(),
        diagnostic = %diagnostic,
        "nginx command failed"
    );
    Err(EdgeRuntimeError::Nginx(match status.code() {
        Some(code) => format!("nginx {operation} failed with exit code {code}"),
        None => format!("nginx {operation} terminated without an exit code"),
    }))
}

fn validation_main_config(candidate: &Path, pid_path: &Path) -> EdgeRuntimeResult<String> {
    let candidate = nginx_path(candidate)?;
    let pid_path = nginx_path(pid_path)?;
    Ok(format!(
        "worker_processes 1;\nerror_log stderr notice;\npid \"{pid_path}\";\nevents {{ worker_connections 16; }}\nhttp {{\n  access_log off;\n  include \"{candidate}\";\n}}\n"
    ))
}

fn nginx_path(path: &Path) -> EdgeRuntimeResult<String> {
    let value = path.to_string_lossy().replace('\\', "/");
    if value.contains(['"', '\r', '\n', '\0']) {
        return Err(EdgeRuntimeError::Filesystem(
            "nginx path contains unsupported characters".to_string(),
        ));
    }
    Ok(value)
}

fn validate_candidate_content(content: &str) -> EdgeRuntimeResult<()> {
    if content.trim().is_empty() {
        return Err(EdgeRuntimeError::Nginx(
            "nginx candidate content is empty".to_string(),
        ));
    }
    if content.len() > MAX_NGINX_CONFIG_BYTES {
        return Err(EdgeRuntimeError::Nginx(format!(
            "nginx candidate exceeds {MAX_NGINX_CONFIG_BYTES} bytes"
        )));
    }
    Ok(())
}

fn require_nginx_enabled(config: &EdgeRuntimeConfig) -> EdgeRuntimeResult<()> {
    if config.nginx_enabled {
        Ok(())
    } else {
        Err(EdgeRuntimeError::Config(
            "nginx edge capability is disabled".to_string(),
        ))
    }
}

fn write_and_sync(file: &mut File, bytes: &[u8], label: &str) -> EdgeRuntimeResult<()> {
    file.write_all(bytes)
        .and_then(|_| file.flush())
        .and_then(|_| file.sync_all())
        .map_err(|error| EdgeRuntimeError::Filesystem(format!("write {label}: {error}")))
}

fn read_bounded_diagnostic(file: &mut File) -> String {
    if file.seek(SeekFrom::Start(0)).is_err() {
        return "diagnostic unavailable".to_string();
    }
    let mut bytes = Vec::with_capacity(MAX_NGINX_DIAGNOSTIC_BYTES as usize);
    let _ = file
        .take(MAX_NGINX_DIAGNOSTIC_BYTES + 1)
        .read_to_end(&mut bytes);
    let truncated = bytes.len() > MAX_NGINX_DIAGNOSTIC_BYTES as usize;
    bytes.truncate(MAX_NGINX_DIAGNOSTIC_BYTES as usize);
    let mut value = String::from_utf8_lossy(&bytes).trim().to_string();
    if truncated {
        value.push_str(" [truncated]");
    }
    if value.is_empty() {
        "no diagnostic output".to_string()
    } else {
        value
    }
}

#[cfg(unix)]
pub(crate) fn sync_directory(path: &Path) -> EdgeRuntimeResult<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            EdgeRuntimeError::Filesystem(format!("sync nginx sites directory: {error}"))
        })
}

#[cfg(not(unix))]
pub(crate) fn sync_directory(_path: &Path) -> EdgeRuntimeResult<()> {
    Ok(())
}

#[cfg(test)] // WORKSPACE-PATH:allow-fixture-block: this module is the file's #[cfg(test)] unit-test fixture data
mod tests {
    use std::sync::Mutex;

    use tempfile::TempDir;

    use super::*;

    static NGINX_PROCESS_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn config(root: &Path, nginx_binary: String) -> EdgeRuntimeConfig {
        EdgeRuntimeConfig {
            nginx_enabled: true,
            nginx_binary,
            nginx_main_config: root.join("nginx.conf"),
            nginx_sites_root: root.join("sites"),
            cert_live_root: root.join("certs"),
            site_family: "sdkwork".to_string(),
            nginx_command_timeout_ms: 10_000,
            tls_verify_address: "127.0.0.1:443".parse().unwrap(),
            tls_verify_timeout_ms: 5_000,
        }
    }

    fn installed_nginx() -> Option<String> {
        std::env::var("SDKWORK_WEBSERVER_TEST_NGINX_BINARY")
            .ok()
            .or_else(|| {
                Command::new("nginx")
                    .arg("-v")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .ok()
                    .filter(|status| status.success())
                    .map(|_| "nginx".to_string())
            })
    }

    #[test]
    fn disabled_and_unavailable_nginx_fail_closed() {
        let root = TempDir::new().unwrap();
        let mut disabled = config(root.path(), "nginx".to_string());
        disabled.nginx_enabled = false;
        assert!(validate_nginx_config(&disabled, "server { listen 8080; }").is_err());

        let unavailable = config(
            root.path(),
            "sdkwork-nginx-binary-that-does-not-exist".to_string(),
        );
        assert!(validate_nginx_config(&unavailable, "server { listen 8080; }").is_err());
        assert!(reload_nginx(&unavailable).is_err());
    }

    #[test]
    fn candidate_size_and_wrapper_are_bounded_and_exact() {
        assert!(validate_candidate_content("").is_err());
        assert!(validate_candidate_content(&"x".repeat(MAX_NGINX_CONFIG_BYTES + 1)).is_err());
        let wrapper = validation_main_config(
            Path::new("C:/safe/candidate.conf"),
            Path::new("C:/safe/nginx.pid"),
        )
        .unwrap();
        assert!(wrapper.contains("include \"C:/safe/candidate.conf\";"));
        assert!(wrapper.contains("pid \"C:/safe/nginx.pid\";"));

        let mut diagnostic = tempfile::tempfile().unwrap();
        diagnostic.write_all(&vec![b'x'; 9_000]).unwrap();
        let bounded = read_bounded_diagnostic(&mut diagnostic);
        assert_eq!(bounded.len(), MAX_NGINX_DIAGNOSTIC_BYTES as usize + 12);
        assert!(bounded.ends_with(" [truncated]"));
    }

    #[test]
    fn nginx_command_timeout_kills_the_child() {
        let _guard = NGINX_PROCESS_TEST_LOCK.lock().unwrap();
        let root = TempDir::new().unwrap();
        let mut config = config(root.path(), timeout_command().0.to_string());
        config.nginx_command_timeout_ms = 100;
        let started = Instant::now();
        let error = run_nginx_command(&config, "timeout-test", timeout_command().1)
            .expect_err("slow command must time out");
        assert!(error.to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn real_nginx_validates_exact_candidate_and_preserves_target_on_failure() {
        let _guard = NGINX_PROCESS_TEST_LOCK.lock().unwrap();
        let Some(binary) = installed_nginx() else {
            eprintln!("nginx unavailable; real edge validation evidence skipped");
            return;
        };
        let root = TempDir::new().unwrap();
        let config = config(root.path(), binary);
        let valid = "server { listen 127.0.0.1:18081; server_name localhost; return 204; }";
        let invalid = "server { sdkwork_invalid_directive on; }";

        validate_nginx_config(&config, valid).expect("real nginx accepts valid candidate");
        assert!(validate_nginx_config(&config, invalid).is_err());
        deploy_nginx_config(&config, "example.com", valid).expect("deploy valid candidate");
        let target = config.nginx_sites_root.join("example.com.conf");
        let before = std::fs::read_to_string(&target).unwrap();
        assert!(deploy_nginx_config(&config, "example.com", invalid).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), before);

        let replacement = "server { listen 127.0.0.1:18081; server_name localhost; return 205; }";
        deploy_nginx_config(&config, "example.com", replacement)
            .expect("replace with a second valid candidate");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), replacement);
        assert_eq!(
            std::fs::read_dir(&config.nginx_sites_root)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.path().extension() == Some(std::ffi::OsStr::new("conf")))
                .count(),
            1,
            "only the activated site configuration may remain in the sites directory"
        );
    }

    #[cfg(windows)]
    fn timeout_command() -> (&'static str, [&'static str; 4]) {
        (
            "powershell.exe",
            [
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 5",
            ],
        )
    }

    #[cfg(not(windows))]
    fn timeout_command() -> (&'static str, [&'static str; 2]) {
        ("sh", ["-c", "sleep 5"])
    }
}
