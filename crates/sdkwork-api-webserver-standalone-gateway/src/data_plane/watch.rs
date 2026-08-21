use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use sdkwork_webserver_core::{
    inspect_webserver_config_revision, load_and_compile_webserver_config_revision,
    CompiledWebServerRevision, ReloadMode, WebServerConfigError,
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

    loop {
        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    return;
                }
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
                last_observed_revision = Some(source_revision.sha256().to_owned());
                last_error = None;

                let candidate_path = config_path.clone();
                let loaded = tokio::task::spawn_blocking(move || {
                    load_and_compile_webserver_config_revision(candidate_path)
                })
                .await;
                let candidate = match loaded {
                    Ok(Ok(candidate)) => candidate,
                    Ok(Err(error)) => {
                        log_reload_error_once(
                            &mut last_error,
                            format!("candidate-{}", config_error_class(&error)),
                            &config_path,
                        );
                        continue;
                    }
                    Err(error) => {
                        log_reload_error_once(
                            &mut last_error,
                            format!("candidate-loader-task-{error}"),
                            &config_path,
                        );
                        continue;
                    }
                };

                // Reject candidates that reference a provider type that was
                // not assembled at bootstrap, retaining the active generation.
                if let Some(executor) = provider_resources.as_ref() {
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
                            &mut last_error,
                            "candidate-provider-unavailable".to_owned(),
                            &config_path,
                        );
                        continue;
                    }
                }

                match publish_candidate(&runtime, candidate).await {
                    Ok(report) => {
                        last_error = None;
                        if report.changed {
                            tracing::info!(
                                config_path = %config_path.display(),
                                config_generation = report.generation,
                                previous_revision = %report.previous_revision,
                                config_revision = %report.revision,
                                "data-plane configuration generation published"
                            );
                        }
                    }
                    Err(error) => {
                        log_reload_error_once(
                            &mut last_error,
                            format!("candidate-{}", publication_error_class(&error)),
                            &config_path,
                        );
                    }
                }
            }
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
