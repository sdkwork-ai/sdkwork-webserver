use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use sdkwork_webserver_core::{
    inspect_webserver_config_revision, load_and_compile_webserver_config_revision,
    CompiledWebServerApp, CompiledWebServerRevision, ConfigFormat, ConfigLoadOptions, ReloadMode,
    WebServerConfigError, WebServerConfigLoader,
};
use sdkwork_webserver_delivery_runtime::AppConfigResourceExecutor;
use tokio::{sync::watch, time::MissedTickBehavior};

use super::{
    operations::DataPlaneOperationsConfig, runtime::DataPlaneRuntime,
    server::run_data_plane_runtime_until, DataPlaneError,
};
use crate::website::build_app_config_provider_executor;

pub async fn run_data_plane_from_config_until<F>(
    config_path: impl Into<PathBuf>,
    shutdown: F,
) -> Result<(), DataPlaneError>
where
    F: Future<Output = ()> + Send,
{
    run_data_plane_from_config_with_operations_until(config_path, None, shutdown).await
}

pub async fn run_data_plane_from_config_with_operations_until<F>(
    config_path: impl Into<PathBuf>,
    operations: Option<DataPlaneOperationsConfig>,
    shutdown: F,
) -> Result<(), DataPlaneError>
where
    F: Future<Output = ()> + Send,
{
    let config_path = config_path.into();
    let initial = load_and_compile_webserver_config_revision(&config_path)?;
    let reload = initial.app().config().deployment.reload.clone();
    let runtime = match operations.as_ref() {
        Some(config) => DataPlaneRuntime::build_revision_with_metric_dimensions(
            initial,
            config.dimensions.clone(),
        )?,
        None => DataPlaneRuntime::build_revision(initial)?,
    };
    // Assemble provider-backed (drive/knowledgebase) resources declared in the
    // application configuration. This fails closed when the configuration
    // references a provider that is not configured in the environment.
    let provider_resources = {
        let current = runtime.current();
        build_app_config_provider_executor(&current.app)
            .await
            .map_err(|error| DataPlaneError::ProviderBootstrap(Box::new(error)))?
    };
    if reload.mode == ReloadMode::Disabled {
        let result = run_data_plane_runtime_until(
            runtime.clone(),
            operations,
            None,
            provider_resources,
            None,
            None,
            shutdown,
        )
        .await;
        let health_result = runtime.stop_active_health().await;
        let resource_result = runtime.stop_resource_pressure().await;
        return result.and(health_result).and(resource_result);
    }

    let (stop_tx, stop_rx) = watch::channel(false);
    let worker_runtime = runtime.clone();
    let worker_path = config_path.clone();
    let worker_provider_resources = provider_resources.clone();
    let worker = tokio::spawn(async move {
        watch_config(
            worker_runtime,
            worker_path,
            worker_provider_resources,
            Duration::from_millis(reload.poll_interval_ms),
            stop_rx,
        )
        .await;
    });

    let result = run_data_plane_runtime_until(
        runtime.clone(),
        operations,
        None,
        provider_resources,
        None,
        None,
        shutdown,
    )
    .await;
    let _ = stop_tx.send(true);
    if let Err(error) = worker.await {
        if result.is_ok() {
            let _ = runtime.stop_active_health().await;
            let _ = runtime.stop_resource_pressure().await;
            return Err(DataPlaneError::ReloadWorker(error));
        }
    }
    let health_result = runtime.stop_active_health().await;
    let resource_result = runtime.stop_resource_pressure().await;
    result.and(health_result).and(resource_result)
}

/// Runs the data plane from a TOML layout directory, a typed TOML file, or an
/// nginx-compat `nginx.conf` file, with the same revision-based watch reload
/// contract the JSON plane has: `deployment.reload.mode = "watch"` polls a
/// bounded fingerprint of the configuration source tree (the nginx grammar
/// resolves `include` relatives against its own directory), `SIGHUP` forces a
/// reload attempt immediately, and a candidate that fails to load, validate or
/// publish retains the active generation.
pub async fn run_data_plane_from_source_with_operations_until<F>(
    config_path: impl Into<PathBuf>,
    format: ConfigFormat,
    document_root_allowlist: Vec<String>,
    operations: Option<DataPlaneOperationsConfig>,
    shutdown: F,
) -> Result<(), DataPlaneError>
where
    F: Future<Output = ()> + Send,
{
    let config_path = config_path.into();
    let loader = WebServerConfigLoader::new();
    let options = ConfigLoadOptions {
        format: Some(format),
        document_root_allowlist: document_root_allowlist.clone(),
        ..ConfigLoadOptions::default()
    };
    let compiled: CompiledWebServerApp = loader
        .load_and_compile(&config_path, &options)
        .map_err(|error| io_source_error(&config_path, &error))?;
    let mut reload = compiled.config().deployment.reload.clone();
    // The TOML/nginx dialects have no `[deployment]` table, so the watch mode
    // is operator-declared through the environment. The JSON plane keeps its
    // in-document declaration authoritative.
    if reload.mode == ReloadMode::Disabled {
        if let Ok(mode) = std::env::var("SDKWORK_WEBSERVER_CONFIG_RELOAD_MODE") {
            if mode.trim().eq_ignore_ascii_case("watch") {
                reload.mode = ReloadMode::Watch;
                if let Ok(interval) =
                    std::env::var("SDKWORK_WEBSERVER_CONFIG_RELOAD_POLL_INTERVAL_MS")
                {
                    if let Ok(parsed) = interval.trim().parse::<u64>() {
                        if (100..=60_000).contains(&parsed) {
                            reload.poll_interval_ms = parsed;
                        }
                    }
                }
            }
        }
    }
    let runtime = match operations.as_ref() {
        Some(config) => {
            DataPlaneRuntime::build_with_metric_dimensions(compiled, config.dimensions.clone())?
        }
        None => DataPlaneRuntime::build(compiled)?,
    };
    // Assemble provider-backed (drive/knowledgebase) resources declared in the
    // application configuration, exactly like the JSON plane.
    let provider_resources = {
        let current = runtime.current();
        build_app_config_provider_executor(&current.app)
            .await
            .map_err(|error| DataPlaneError::ProviderBootstrap(Box::new(error)))?
    };
    if reload.mode == ReloadMode::Disabled {
        let result = run_data_plane_runtime_until(
            runtime.clone(),
            operations,
            None,
            provider_resources,
            None,
            None,
            shutdown,
        )
        .await;
        let health_result = runtime.stop_active_health().await;
        let resource_result = runtime.stop_resource_pressure().await;
        return result.and(health_result).and(resource_result);
    }

    let (stop_tx, stop_rx) = watch::channel(false);
    let worker_runtime = runtime.clone();
    let worker_path = config_path.clone();
    let worker_provider_resources = provider_resources.clone();
    let worker = tokio::spawn(async move {
        watch_config_source(
            worker_runtime,
            worker_path,
            format,
            document_root_allowlist,
            worker_provider_resources,
            Duration::from_millis(reload.poll_interval_ms),
            stop_rx,
        )
        .await;
    });

    let result = run_data_plane_runtime_until(
        runtime.clone(),
        operations,
        None,
        provider_resources,
        None,
        None,
        shutdown,
    )
    .await;
    let _ = stop_tx.send(true);
    if let Err(error) = worker.await {
        if result.is_ok() {
            let _ = runtime.stop_active_health().await;
            let _ = runtime.stop_resource_pressure().await;
            return Err(DataPlaneError::ReloadWorker(error));
        }
    }
    let health_result = runtime.stop_active_health().await;
    let resource_result = runtime.stop_resource_pressure().await;
    result.and(health_result).and(resource_result)
}

fn io_source_error(path: &Path, error: &WebServerConfigError) -> DataPlaneError {
    DataPlaneError::Listener {
        listener_id: path.display().to_string(),
        source: std::io::Error::other(error.to_string()),
    }
}

/// Bounded fingerprint of a configuration source tree: file name, mtime and
/// size per file, hashed in sorted order. The nginx grammar resolves `include`
/// relatives against the conf directory, so a single-file source watches its
/// containing tree; a layout-v3 TOML source watches the whole directory.
const FINGERPRINT_MAX_FILES: usize = 4096;
const FINGERPRINT_MAX_DEPTH: usize = 8;
const FINGERPRINT_MAX_FILE_BYTES: u64 = 1024 * 1024;
const FINGERPRINT_MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;

fn fingerprint_source(config_path: &Path) -> Result<String, String> {
    use std::time::UNIX_EPOCH;

    fn walk(
        directory: &Path,
        relative: &Path,
        depth: usize,
        entries: &mut Vec<(PathBuf, u128, u64)>,
        total: &mut u64,
    ) -> Result<(), String> {
        if depth > FINGERPRINT_MAX_DEPTH {
            return Err(format!(
                "config source tree deeper than {FINGERPRINT_MAX_DEPTH} levels at {}",
                directory.display()
            ));
        }
        let read = std::fs::read_dir(directory)
            .map_err(|error| format!("cannot read {}: {error}", directory.display()))?;
        for entry in read {
            let entry = entry.map_err(|error| format!("cannot list {}: {error}", directory.display()))?;
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                // Include confinement is the loader's contract; a symlink in
                // the source tree is fingerprinted by its own metadata only.
                continue;
            }
            let path = entry.path();
            let child_relative = relative.join(entry.file_name());
            if file_type.is_dir() {
                walk(&path, &child_relative, depth + 1, entries, total)?;
                continue;
            }
            let metadata =
                entry.metadata().map_err(|error| format!("cannot stat {}: {error}", path.display()))?;
            if metadata.len() > FINGERPRINT_MAX_FILE_BYTES {
                return Err(format!(
                    "config source file {} exceeds {FINGERPRINT_MAX_FILE_BYTES} bytes",
                    path.display()
                ));
            }
            *total = total.saturating_add(metadata.len());
            if *total > FINGERPRINT_MAX_TOTAL_BYTES {
                return Err(format!(
                    "config source tree exceeds {FINGERPRINT_MAX_TOTAL_BYTES} bytes"
                ));
            }
            if entries.len() >= FINGERPRINT_MAX_FILES {
                return Err(format!(
                    "config source tree exceeds {FINGERPRINT_MAX_FILES} files"
                ));
            }
            let modified = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            entries.push((child_relative, modified, metadata.len()));
        }
        Ok(())
    }

    let root: &Path = if config_path.is_dir() {
        config_path
    } else {
        config_path.parent().unwrap_or(config_path)
    };
    let mut entries = Vec::new();
    let mut total = 0_u64;
    walk(root, Path::new(""), 0, &mut entries, &mut total)?;
    entries.sort();
    let mut payload = Vec::with_capacity(entries.len() * 48);
    for (relative, modified, size) in &entries {
        payload.extend_from_slice(relative.to_string_lossy().as_bytes());
        payload.push(0);
        payload.extend_from_slice(&modified.to_le_bytes());
        payload.extend_from_slice(&size.to_le_bytes());
    }
    Ok(sdkwork_utils_rust::crypto::sha256_hash(&payload))
}

/// Watch loop for TOML/nginx configuration sources. Same retain-on-error
/// contract as the JSON plane; `SIGHUP` (unix) forces an immediate reload
/// attempt, matching nginx.
#[allow(clippy::too_many_arguments)]
async fn watch_config_source(
    runtime: Arc<DataPlaneRuntime>,
    config_path: PathBuf,
    format: ConfigFormat,
    document_root_allowlist: Vec<String>,
    provider_resources: Option<Arc<AppConfigResourceExecutor>>,
    poll_interval: Duration,
    mut stop: watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(poll_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut last_observed_fingerprint = fingerprint_source(&config_path).ok();
    let mut last_error = None;
    #[cfg(unix)]
    let mut hangup = match tokio::signal::unix::signal(
        tokio::signal::unix::SignalKind::hangup(),
    ) {
        Ok(signal) => Some(signal),
        Err(error) => {
            tracing::warn!(%error, "SIGHUP watching unavailable; reload stays poll-driven");
            None
        }
    };

    loop {
        #[cfg(unix)]
        let hangup_received = async {
            match hangup.as_mut() {
                Some(signal) => {
                    signal.recv().await;
                    true
                }
                None => {
                    std::future::pending::<()>().await;
                    false
                }
            }
        };
        #[cfg(not(unix))]
        let hangup_received = std::future::pending::<bool>();

        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    return;
                }
            }
            forced = hangup_received => {
                // nginx: SIGHUP re-reads the configuration even when the
                // fingerprint looks unchanged; the runtime's revision
                // short-circuit makes the redundant attempt a no-op.
                let _ = forced;
                if !reload_source_once(
                    &runtime,
                    &config_path,
                    format,
                    &document_root_allowlist,
                    provider_resources.as_ref(),
                    &mut last_error,
                )
                .await
                {
                    continue;
                }
                last_observed_fingerprint = fingerprint_source(&config_path).ok();
                last_error = None;
            }
            _ = ticker.tick() => {
                let fingerprint = match fingerprint_source(&config_path) {
                    Ok(fingerprint) => Some(fingerprint),
                    Err(error) => {
                        log_reload_error_once(
                            &mut last_error,
                            format!("cannot fingerprint watched config source: {error}"),
                            &config_path,
                        );
                        continue;
                    }
                };
                if last_observed_fingerprint.is_some()
                    && last_observed_fingerprint == fingerprint
                {
                    continue;
                }
                if !reload_source_once(
                    &runtime,
                    &config_path,
                    format,
                    &document_root_allowlist,
                    provider_resources.as_ref(),
                    &mut last_error,
                )
                .await
                {
                    // A failed candidate retains the generation; remember the
                    // fingerprint so the next poll does not retry the same
                    // broken source until it changes again.
                    last_observed_fingerprint = fingerprint;
                    continue;
                }
                last_observed_fingerprint = fingerprint;
                last_error = None;
            }
        }
    }
}

/// Loads and publishes one candidate from the watched source. Returns whether
/// the candidate published (or was an unchanged no-op); `false` means the
/// active generation was retained.
async fn reload_source_once(
    runtime: &Arc<DataPlaneRuntime>,
    config_path: &Path,
    format: ConfigFormat,
    document_root_allowlist: &[String],
    provider_resources: Option<&Arc<AppConfigResourceExecutor>>,
    last_error: &mut Option<String>,
) -> bool {
    let loader = WebServerConfigLoader::new();
    let options = ConfigLoadOptions {
        format: Some(format),
        document_root_allowlist: document_root_allowlist.to_vec(),
        ..ConfigLoadOptions::default()
    };
    let candidate_path = config_path.to_path_buf();
    let loaded =
        tokio::task::spawn_blocking(move || loader.load_and_compile(&candidate_path, &options))
            .await;
    let candidate = match loaded {
        Ok(Ok(candidate)) => candidate,
        Ok(Err(error)) => {
            log_reload_error_once(
                last_error,
                format!("candidate-{error}"),
                config_path,
            );
            return false;
        }
        Err(error) => {
            log_reload_error_once(
                last_error,
                format!("candidate-loader-task-{error}"),
                config_path,
            );
            return false;
        }
    };
    let candidate = CompiledWebServerRevision::from_compiled_app(candidate);

    // Reject candidates that reference a provider type that was not assembled
    // at bootstrap, retaining the active generation.
    if let Some(executor) = provider_resources {
        let provider_types = candidate
            .app()
            .provider_resources()
            .filter_map(|resource| match resource.provider_type()? {
                sdkwork_webserver_core::ConfigProviderType::Drive => {
                    Some(sdkwork_webserver_core::website_runtime::WebsiteProviderType::Drive)
                }
                sdkwork_webserver_core::ConfigProviderType::Knowledgebase => Some(
                    sdkwork_webserver_core::website_runtime::WebsiteProviderType::Knowledgebase,
                ),
            })
            .collect::<Vec<_>>();
        if !executor.can_serve_config(&provider_types) {
            log_reload_error_once(last_error, "candidate-provider-unavailable".to_owned(), config_path);
            return false;
        }
    }

    match runtime.reload(candidate).await {
        Ok(report) => {
            if report.changed {
                tracing::info!(
                    config_path = %config_path.display(),
                    config_generation = report.generation,
                    previous_revision = %report.previous_revision,
                    config_revision = %report.revision,
                    sockets_changed = report.sockets_changed,
                    "data-plane configuration generation published"
                );
            }
            true
        }
        Err(error) => {
            log_reload_error_once(
                last_error,
                format!("candidate-{}", publication_error_class(&error)),
                config_path,
            );
            false
        }
    }
}

async fn watch_config(
    runtime: Arc<DataPlaneRuntime>,
    config_path: PathBuf,
    provider_resources: Option<Arc<AppConfigResourceExecutor>>,
    poll_interval: Duration,
    mut stop: watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(poll_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut last_observed_revision = Some(runtime.current().revision.clone());
    let mut last_error = None;
    #[cfg(unix)]
    let mut hangup = match tokio::signal::unix::signal(
        tokio::signal::unix::SignalKind::hangup(),
    ) {
        Ok(signal) => Some(signal),
        Err(error) => {
            tracing::warn!(%error, "SIGHUP watching unavailable; reload stays poll-driven");
            None
        }
    };

    loop {
        #[cfg(unix)]
        let hangup_received = async {
            match hangup.as_mut() {
                Some(signal) => {
                    signal.recv().await;
                    true
                }
                None => {
                    std::future::pending::<()>().await;
                    false
                }
            }
        };
        #[cfg(not(unix))]
        let hangup_received = std::future::pending::<bool>();

        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    return;
                }
            }
            forced = hangup_received => {
                // nginx: SIGHUP re-reads the configuration even when the file
                // hash looks unchanged; the runtime's revision short-circuit
                // makes the redundant attempt a no-op.
                let _ = forced;
                last_observed_revision = None;
                if !reload_json_once(
                    &runtime,
                    &config_path,
                    provider_resources.as_ref(),
                    &mut last_observed_revision,
                    &mut last_error,
                )
                .await
                {
                    continue;
                }
                last_error = None;
            }
            _ = ticker.tick() => {
                let inspection_path = config_path.clone();
                let inspection = tokio::task::spawn_blocking(move || {
                    inspect_webserver_config_revision(inspection_path)
                })
                .await;
                let source_revision = match inspection {
                    Ok(Ok(revision)) => revision,
                    Ok(Err(error)) => {
                        log_reload_error_once(
                            &mut last_error,
                            format!("cannot inspect watched config: {error}"),
                            &config_path,
                        );
                        continue;
                    }
                    Err(error) => {
                        log_reload_error_once(
                            &mut last_error,
                            format!("config inspection task failed: {error}"),
                            &config_path,
                        );
                        continue;
                    }
                };
                if last_observed_revision.as_deref() == Some(source_revision.sha256()) {
                    continue;
                }
                if !reload_json_once(
                    &runtime,
                    &config_path,
                    provider_resources.as_ref(),
                    &mut last_observed_revision,
                    &mut last_error,
                )
                .await
                {
                    continue;
                }
                last_error = None;
            }
        }
    }
}

/// Loads and publishes one JSON candidate. Returns whether the candidate
/// published (or was an unchanged no-op); `false` means the active generation
/// was retained and `last_observed_revision` stays untouched.
async fn reload_json_once(
    runtime: &Arc<DataPlaneRuntime>,
    config_path: &Path,
    provider_resources: Option<&Arc<AppConfigResourceExecutor>>,
    last_observed_revision: &mut Option<String>,
    last_error: &mut Option<String>,
) -> bool {
    let candidate_path = config_path.to_path_buf();
    let loaded = tokio::task::spawn_blocking(move || {
        load_and_compile_webserver_config_revision(candidate_path)
    })
    .await;
    let candidate = match loaded {
        Ok(Ok(candidate)) => candidate,
        Ok(Err(error)) => {
            log_reload_error_once(
                last_error,
                format!("candidate-{} {error:?}", config_error_class(&error)),
                config_path,
            );
            return false;
        }
        Err(error) => {
            log_reload_error_once(
                last_error,
                format!("candidate-loader-task-{error}"),
                config_path,
            );
            return false;
        }
    };
    *last_observed_revision = Some(candidate.sha256().to_owned());

    // Reject candidates that reference a provider type that was not assembled
    // at bootstrap, retaining the active generation.
    if let Some(executor) = provider_resources {
        let provider_types = candidate
            .app()
            .provider_resources()
            .filter_map(|resource| match resource.provider_type()? {
                sdkwork_webserver_core::ConfigProviderType::Drive => {
                    Some(sdkwork_webserver_core::website_runtime::WebsiteProviderType::Drive)
                }
                sdkwork_webserver_core::ConfigProviderType::Knowledgebase => Some(
                    sdkwork_webserver_core::website_runtime::WebsiteProviderType::Knowledgebase,
                ),
            })
            .collect::<Vec<_>>();
        if !executor.can_serve_config(&provider_types) {
            log_reload_error_once(
                last_error,
                "candidate-provider-unavailable".to_owned(),
                config_path,
            );
            return false;
        }
    }

    match publish_candidate(runtime, candidate).await {
        Ok(report) => {
            if report.changed {
                tracing::info!(
                    config_path = %config_path.display(),
                    config_generation = report.generation,
                    previous_revision = %report.previous_revision,
                    config_revision = %report.revision,
                    "data-plane configuration generation published"
                );
            }
            true
        }
        Err(error) => {
            log_reload_error_once(
                last_error,
                format!("candidate-{}", publication_error_class(&error)),
                config_path,
            );
            false
        }
    }
}

async fn publish_candidate(
    runtime: &DataPlaneRuntime,
    candidate: CompiledWebServerRevision,
) -> Result<super::DataPlaneReloadReport, DataPlaneError> {
    runtime.reload(candidate).await
}

fn log_reload_error_once(last_error: &mut Option<String>, error: String, path: &Path) {
    if last_error.as_deref() == Some(error.as_str()) {
        return;
    }
    tracing::warn!(
        config_path = %path.display(),
        error = %error,
        "data-plane configuration reload retained the active generation"
    );
    *last_error = Some(error);
}

fn config_error_class(error: &WebServerConfigError) -> &'static str {
    match error {
        WebServerConfigError::Inspect { .. } => "inspect-failed",
        WebServerConfigError::TooLarge { .. } => "too-large",
        WebServerConfigError::Read { .. } => "read-failed",
        WebServerConfigError::Json { .. } => "invalid-json",
        WebServerConfigError::Toml { .. } => "invalid-toml",
        WebServerConfigError::Materialize(_) => "materialize-failed",
        WebServerConfigError::InvalidSchema(_) => "invalid-embedded-schema",
        WebServerConfigError::Validation { .. } => "validation-failed",
        WebServerConfigError::Nginx { .. } => "nginx-materialize-failed",
    }
}

fn publication_error_class(error: &DataPlaneError) -> &'static str {
    match error {
        DataPlaneError::ReloadRequiresRestart => "restart-required",
        DataPlaneError::UpstreamClient { .. } => "upstream-client-build-failed",
        DataPlaneError::InvalidUpstreamTarget { .. } => "invalid-upstream-target",
        DataPlaneError::TlsMaterialRead { .. } => "tls-material-read-failed",
        DataPlaneError::TlsMaterialTooLarge { .. } => "tls-material-too-large",
        _ => "runtime-generation-build-failed",
    }
}
