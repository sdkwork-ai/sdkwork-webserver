use std::{
    env,
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

use sdkwork_drive_internal_sdk::{
    SdkworkConfig as DriveSdkworkConfig, SdkworkCustomClient as DriveSdkworkCustomClient,
};
use sdkwork_knowledgebase_internal_sdk::{
    SdkworkConfig as KnowledgebaseSdkworkConfig,
    SdkworkCustomClient as KnowledgebaseSdkworkCustomClient,
};
use sdkwork_webserver_contract::RuntimeObservationState;
use sdkwork_webserver_core::{
    load_and_compile_webserver_config_revision,
    runtime_env::web_environment_name,
    website_runtime::{
        CompiledWebsiteRuntimeSet, ProviderResourceReference, WebsiteProviderType,
        WebsiteRuntimeEnvironment, WebsiteRuntimeRegistry, WebsiteRuntimeSetError,
        MAX_WEBSITE_RUNTIME_SET_BYTES,
    },
    ConfigProviderType, ResourceConfig, WebServerConfigError,
};
use sdkwork_webserver_delivery_runtime::{
    probe_website_runtime_set_activation, AppConfigResourceExecutor, AppConfigResourceHandler,
    WebsiteDeliveryExecutor, WebsiteProviderEventInvalidator, WebsiteProviderRegistry,
    WebsiteProviderRegistryError, WebsiteRuntimeActivationProbeError,
    WebsiteRuntimeProviderValidationError, WebsiteRuntimeSetProviderEventReconciler,
    DEFAULT_PROVIDER_BUFFERED_CONTENT_BYTES, DEFAULT_PROVIDER_RESOLUTION_CACHE_ENTRIES,
    MAXIMUM_PROVIDER_RESOLUTION_CACHE_ENTRIES,
};
use sdkwork_webserver_drive_provider::{
    DriveWebsiteProvider, FixedDriveWebsiteSdkClientResolver,
    DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION, MAXIMUM_DRIVE_CONTENT_BYTES,
};
use sdkwork_webserver_knowledgebase_provider::{
    FixedKnowledgebaseWikiSdkClientResolver, KnowledgebaseWikiWebsiteProvider,
    KNOWLEDGEBASE_WIKI_PROVIDER_CONTRACT_VERSION,
};
use thiserror::Error;
use tokio::{
    sync::{oneshot, watch},
    time::MissedTickBehavior,
};
use url::Url;

use crate::{
    provider_event_ingress::WebsiteProviderEventIngress,
    run_website_data_plane_with_operations_until, run_website_data_plane_with_tls_operations_until,
    website_runtime_cloud::{
        CloudRuntimeAssignment, CloudRuntimeAssignmentError, CloudRuntimeAssignmentSource,
        CloudRuntimeDelivery,
    },
    website_runtime_recovery::{
        LoadedWebsiteRuntimeSet, WebsiteRuntimeSetRecoveryOpen, WebsiteRuntimeSetRecoveryStore,
    },
    DataPlaneError, DataPlaneOperationsConfig, FileTlsRuntimeConfig, FileTlsRuntimeController,
    FileTlsRuntimeError,
};

pub const WEBSITE_RUNTIME_SET_FILE_ENV: &str = "SDKWORK_WEBSERVER_WEBSITE_RUNTIME_SET_FILE";
pub const WEBSITE_RUNTIME_ASSIGNMENT_SOURCE_ENV: &str =
    "SDKWORK_WEBSERVER_RUNTIME_ASSIGNMENT_SOURCE";
pub const WEB_INTERNAL_API_BASE_URL_ENV: &str = "SDKWORK_WEBSERVER_INTERNAL_API_BASE_URL";
pub const WEB_NODE_TOKEN_FILE_ENV: &str = "SDKWORK_WEBSERVER_NODE_TOKEN_FILE";
pub const WEBSITE_NODE_UUID_ENV: &str = "SDKWORK_WEBSERVER_NODE_UUID";
pub const WEBSITE_RUNTIME_ENVIRONMENT_ENV: &str = "SDKWORK_WEBSERVER_WEBSITE_RUNTIME_ENVIRONMENT";
pub const WEBSITE_NODE_VERSION_ENV: &str = "SDKWORK_WEBSERVER_NODE_VERSION";
pub const WEBSITE_RUNTIME_SET_RECOVERY_DIRECTORY_ENV: &str =
    "SDKWORK_WEBSERVER_WEBSITE_RUNTIME_SET_RECOVERY_DIRECTORY";
pub const WEBSITE_RUNTIME_SET_POLL_INTERVAL_MS_ENV: &str =
    "SDKWORK_WEBSERVER_WEBSITE_RUNTIME_SET_POLL_INTERVAL_MS";
pub const WEBSITE_TENANT_SCOPE_HASH_ENV: &str = "SDKWORK_WEBSERVER_WEBSITE_TENANT_SCOPE_HASH";
pub const WEBSITE_PROVIDER_VALIDATION_CONCURRENCY_ENV: &str =
    "SDKWORK_WEBSERVER_WEBSITE_PROVIDER_VALIDATION_CONCURRENCY";
pub const WEBSITE_PROVIDER_BUFFERED_CONTENT_BYTES_ENV: &str =
    "SDKWORK_WEBSERVER_WEBSITE_PROVIDER_BUFFERED_CONTENT_BYTES";
pub const WEBSITE_PROVIDER_RESOLUTION_CACHE_ENTRIES_ENV: &str =
    "SDKWORK_WEBSERVER_WEBSITE_PROVIDER_RESOLUTION_CACHE_ENTRIES";
pub const WEBSITE_PROVIDER_EVENT_CONFIG_FILE_ENV: &str =
    "SDKWORK_WEBSERVER_WEBSITE_PROVIDER_EVENT_CONFIG_FILE";
pub const KNOWLEDGEBASE_INTERNAL_API_BASE_URL_ENV: &str =
    "SDKWORK_WEBSERVER_KNOWLEDGEBASE_INTERNAL_API_BASE_URL";
pub const KNOWLEDGEBASE_INTERNAL_API_INGRESS_TOKEN_FILE_ENV: &str =
    "SDKWORK_WEBSERVER_KNOWLEDGEBASE_INTERNAL_API_INGRESS_TOKEN_FILE";
pub const DRIVE_INTERNAL_API_BASE_URL_ENV: &str = "SDKWORK_WEBSERVER_DRIVE_INTERNAL_API_BASE_URL";
pub const DRIVE_INTERNAL_API_INGRESS_TOKEN_FILE_ENV: &str =
    "SDKWORK_WEBSERVER_DRIVE_INTERNAL_API_INGRESS_TOKEN_FILE";
pub const TLS_RUNTIME_SOURCE_ENV: &str = "SDKWORK_WEBSERVER_TLS_RUNTIME_SOURCE";
pub const TLS_RUNTIME_SNAPSHOT_FILE_ENV: &str = "SDKWORK_WEBSERVER_TLS_RUNTIME_SNAPSHOT_FILE";
pub const TLS_MATERIAL_ROOT_ENV: &str = "SDKWORK_WEBSERVER_TLS_MATERIAL_ROOT";
pub const TLS_LISTENER_ID_ENV: &str = "SDKWORK_WEBSERVER_TLS_LISTENER_ID";
pub const TLS_RUNTIME_POLL_INTERVAL_MS_ENV: &str = "SDKWORK_WEBSERVER_TLS_RUNTIME_POLL_INTERVAL_MS";
pub const TLS_RUNTIME_RECOVERY_DIRECTORY_ENV: &str =
    "SDKWORK_WEBSERVER_TLS_RUNTIME_RECOVERY_DIRECTORY";

const DEFAULT_RUNTIME_SET_POLL_INTERVAL_MS: u64 = 2_000;
const MINIMUM_RUNTIME_SET_POLL_INTERVAL_MS: u64 = 250;
const MAXIMUM_RUNTIME_SET_POLL_INTERVAL_MS: u64 = 60_000;
const DEFAULT_PROVIDER_VALIDATION_CONCURRENCY: usize = 16;
const MAXIMUM_PROVIDER_VALIDATION_CONCURRENCY: usize = 64;
const MINIMUM_PROVIDER_BUFFERED_CONTENT_BYTES: usize = 16 * 1024 * 1024;
const MAXIMUM_PROVIDER_BUFFERED_CONTENT_BYTES: usize = 2 * 1024 * 1024 * 1024;
const MAXIMUM_INGRESS_TOKEN_FILE_BYTES: u64 = 16 * 1024;
const MAXIMUM_KNOWLEDGEBASE_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum WebsiteDataPlaneBootstrapError {
    #[error(transparent)]
    HostConfig(#[from] WebServerConfigError),
    #[error("website runtime-set source is unavailable or not a regular bounded file")]
    RuntimeSetSource,
    #[error("website runtime-set source changed while it was being read")]
    RuntimeSetSourceChanged,
    #[error("website runtime-set recovery directory is required in staging and production")]
    RuntimeSetRecoveryRequired,
    #[error("website runtime-set recovery state is invalid or unavailable")]
    RuntimeSetRecovery,
    #[error(transparent)]
    RuntimeSet(#[from] WebsiteRuntimeSetError),
    #[error("website runtime-set requires the unavailable {provider_type:?} provider")]
    ProviderUnavailable { provider_type: WebsiteProviderType },
    #[error("website provider registry is invalid: {0}")]
    ProviderRegistry(#[from] WebsiteProviderRegistryError),
    #[error(
        "website runtime-set tenant scope does not match the configured service credential scope"
    )]
    TenantScopeMismatch,
    #[error("website runtime-set provider validation failed: {0}")]
    ProviderValidation(#[from] WebsiteRuntimeProviderValidationError),
    #[error("website runtime-set activation probe failed: {0}")]
    RuntimeActivationProbe(#[from] WebsiteRuntimeActivationProbeError),
    #[error("website provider configuration is invalid: {0}")]
    ProviderConfig(String),
    #[error("website provider event ingress configuration is required")]
    ProviderEventConfigRequired,
    #[error("website provider event ingress failed")]
    ProviderEventIngress,
    #[error("website provider event ingress stopped unexpectedly")]
    ProviderEventIngressStopped,
    #[error("generated Knowledgebase Internal SDK client construction failed")]
    KnowledgebaseSdkClient,
    #[error("generated Drive Internal SDK client construction failed")]
    DriveSdkClient,
    #[error("website runtime assignment source configuration is invalid: {0}")]
    RuntimeAssignmentConfig(String),
    #[error("the current website runtime assignment is terminally rejected")]
    RuntimeAssignmentRejected,
    #[error("website runtime assignment operation failed")]
    RuntimeAssignment,
    #[error(transparent)]
    DataPlane(#[from] DataPlaneError),
    #[error(transparent)]
    TlsRuntime(#[from] FileTlsRuntimeError),
}

impl From<CloudRuntimeAssignmentError> for WebsiteDataPlaneBootstrapError {
    fn from(_error: CloudRuntimeAssignmentError) -> Self {
        Self::RuntimeAssignment
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceFingerprint {
    length: u64,
    modified: Option<SystemTime>,
}

struct RuntimeSetWatchContext {
    registry: Arc<WebsiteRuntimeRegistry>,
    path: PathBuf,
    recovery_store: Option<Arc<WebsiteRuntimeSetRecoveryStore>>,
    providers: Arc<WebsiteProviderRegistry>,
    tenant_scope_hash: String,
    validation_concurrency: usize,
    poll_interval: Duration,
}

struct CloudRuntimeSetWatchContext {
    registry: Arc<WebsiteRuntimeRegistry>,
    recovery_store: Option<Arc<WebsiteRuntimeSetRecoveryStore>>,
    providers: Arc<WebsiteProviderRegistry>,
    tenant_scope_hash: String,
    validation_concurrency: usize,
    poll_interval: Duration,
    source: Arc<CloudRuntimeAssignmentSource>,
}

struct ProviderEventIngressBootstrapContext<'a> {
    environment: WebsiteRuntimeEnvironment,
    require_drive: bool,
    require_knowledgebase: bool,
    tenant_scope_hash: &'a str,
    node_uuid: &'a str,
    runtime_registry: Arc<WebsiteRuntimeRegistry>,
    providers: Arc<WebsiteProviderRegistry>,
    validation_concurrency: usize,
    invalidator: Arc<dyn WebsiteProviderEventInvalidator>,
}

enum ConfiguredRuntimeSetSource {
    File {
        path: PathBuf,
    },
    Cloud {
        source: Arc<CloudRuntimeAssignmentSource>,
    },
}

pub async fn run_website_data_plane_from_config_until<F>(
    host_config_path: impl Into<PathBuf>,
    runtime_set_path: Option<PathBuf>,
    operations: Option<DataPlaneOperationsConfig>,
    shutdown: F,
) -> Result<(), WebsiteDataPlaneBootstrapError>
where
    F: Future<Output = ()> + Send,
{
    let host_config = load_and_compile_webserver_config_revision(host_config_path.into())?;
    let tenant_scope_hash = website_tenant_scope_hash()?;
    let configured_source = configured_runtime_set_source(runtime_set_path)?;
    let recovery_open = open_runtime_set_recovery().await?;
    let (recovery_store, recovered, corruption_detected) = match recovery_open {
        Some(WebsiteRuntimeSetRecoveryOpen {
            store,
            recovered,
            corruption_detected,
        }) => (Some(store), recovered, corruption_detected),
        None => (None, None, false),
    };
    if corruption_detected {
        tracing::warn!("website runtime-set recovery ignored a corrupt slot");
    }
    let (initial, fingerprint, mut initial_cloud_delivery, source_available) =
        match &configured_source {
            ConfiguredRuntimeSetSource::File { path } => {
                let source = load_runtime_set(path);
                let source_available = source.is_ok();
                let (initial, fingerprint) =
                    select_initial_runtime_set(source, recovered, &tenant_scope_hash)?;
                (initial, fingerprint, None, source_available)
            }
            ConfiguredRuntimeSetSource::Cloud { source } => {
                let (initial, delivery, source_available) =
                    select_initial_cloud_runtime_set(source, recovered, &tenant_scope_hash).await?;
                (initial, None, delivery, source_available)
            }
        };
    require_runtime_set_recovery(initial.runtime_set().environment(), recovery_store.as_ref())?;
    if !source_available {
        tracing::warn!(
            website_runtime_generation = initial.runtime_set().generation(),
            website_runtime_sha256 = %initial.runtime_set().snapshot_sha256(),
            "website runtime-set restored from node recovery state"
        );
    }
    let runtime_registry = Arc::new(WebsiteRuntimeRegistry::new(
        initial.runtime_set().node_uuid(),
        initial.runtime_set().environment(),
    ));
    let validation_concurrency = provider_validation_concurrency()?;
    let require_knowledgebase = initial
        .runtime_set()
        .uses_provider_type(WebsiteProviderType::Knowledgebase);
    let require_drive = initial
        .runtime_set()
        .uses_provider_type(WebsiteProviderType::Drive);
    let mut providers = WebsiteProviderRegistry::new();
    let knowledgebase = build_knowledgebase_provider(
        &tenant_scope_hash,
        initial.runtime_set().environment(),
        require_knowledgebase,
    )?;
    let knowledgebase_registered = knowledgebase.is_some();
    if let Some(provider) = knowledgebase {
        providers.register_wiki(WebsiteProviderType::Knowledgebase, provider)?;
    }
    let drive = build_drive_provider(
        &tenant_scope_hash,
        initial.runtime_set().environment(),
        require_drive,
    )?;
    let drive_registered = drive.is_some();
    if let Some(provider) = drive {
        providers.register_static(WebsiteProviderType::Drive, provider)?;
    }
    let providers = Arc::new(providers);
    if let Some(delivery) = initial_cloud_delivery.as_mut() {
        advance_cloud_observation(
            source_from_config(&configured_source)?,
            &delivery.assignment,
            &mut delivery.latest_observation_state,
            RuntimeObservationState::Received,
        )
        .await?;
    }
    if let Err(error) = providers
        .validate_runtime_set(initial.runtime_set(), validation_concurrency)
        .await
    {
        if let Some(delivery) = initial_cloud_delivery.as_ref() {
            reject_cloud_assignment(
                source_from_config(&configured_source)?,
                &delivery.assignment,
                delivery.latest_observation_state,
                "PROVIDER_VALIDATION_FAILED",
                "runtime-set provider validation failed",
            )
            .await?;
        }
        return Err(WebsiteDataPlaneBootstrapError::ProviderValidation(error));
    }
    if let Some(delivery) = initial_cloud_delivery.as_mut() {
        advance_cloud_observation(
            source_from_config(&configured_source)?,
            &delivery.assignment,
            &mut delivery.latest_observation_state,
            RuntimeObservationState::Validated,
        )
        .await?;
    }
    let activation_probe = match probe_website_runtime_set_activation(
        Arc::clone(initial.runtime_set()),
        Arc::clone(&providers),
        validation_concurrency,
    )
    .await
    {
        Ok(report) => report,
        Err(error) => {
            if let Some(delivery) = initial_cloud_delivery.as_ref() {
                reject_cloud_assignment(
                    source_from_config(&configured_source)?,
                    &delivery.assignment,
                    delivery.latest_observation_state,
                    "ACTIVATION_PROBE_FAILED",
                    "node-local route and content activation probe failed",
                )
                .await?;
            }
            return Err(WebsiteDataPlaneBootstrapError::RuntimeActivationProbe(
                error,
            ));
        }
    };
    tracing::info!(
        website_runtime_generation = initial.runtime_set().generation(),
        activation_probe_bindings = activation_probe.probed_bindings,
        activation_probe_variants = activation_probe.probed_variants,
        activation_probe_routes = activation_probe.probed_routes,
        "website runtime-set passed node-local activation probes"
    );
    let environment = initial.runtime_set().environment();
    let tls_runtime = configured_tls_runtime(environment, initial.runtime_set().node_uuid())?;
    persist_runtime_set(recovery_store.as_ref(), &initial).await?;
    if let Some(delivery) = initial_cloud_delivery.as_mut() {
        advance_cloud_observation(
            source_from_config(&configured_source)?,
            &delivery.assignment,
            &mut delivery.latest_observation_state,
            RuntimeObservationState::Staged,
        )
        .await?;
    }
    if let Err(error) = runtime_registry.activate(Arc::clone(initial.runtime_set())) {
        if let Some(delivery) = initial_cloud_delivery.as_ref() {
            reject_cloud_assignment(
                source_from_config(&configured_source)?,
                &delivery.assignment,
                delivery.latest_observation_state,
                "ACTIVATION_FAILED",
                "runtime-set failed atomic registry activation",
            )
            .await?;
        }
        return Err(WebsiteDataPlaneBootstrapError::RuntimeSet(error));
    }
    if let Some(delivery) = initial_cloud_delivery.as_mut() {
        advance_cloud_observation(
            source_from_config(&configured_source)?,
            &delivery.assignment,
            &mut delivery.latest_observation_state,
            RuntimeObservationState::Active,
        )
        .await?;
    }

    let buffered_content_bytes = provider_buffered_content_bytes()?;
    let resolution_cache_entries = provider_resolution_cache_entries()?;
    let executor = Arc::new(
        WebsiteDeliveryExecutor::with_provider_runtime_limits(
            Arc::clone(&runtime_registry),
            Arc::clone(&providers),
            buffered_content_bytes,
            resolution_cache_entries,
        )
        .map_err(|error| WebsiteDataPlaneBootstrapError::ProviderConfig(error.to_string()))?,
    );
    let provider_event_ingress =
        build_provider_event_ingress(ProviderEventIngressBootstrapContext {
            environment,
            require_drive: drive_registered,
            require_knowledgebase: knowledgebase_registered,
            tenant_scope_hash: &tenant_scope_hash,
            node_uuid: initial.runtime_set().node_uuid(),
            runtime_registry: Arc::clone(&runtime_registry),
            providers: Arc::clone(&providers),
            validation_concurrency,
            invalidator: executor.provider_event_invalidator(),
        })
        .await?;
    let poll_interval = runtime_set_poll_interval()?;
    let (stop_tx, stop_rx) = watch::channel(false);
    let watcher = match configured_source {
        ConfiguredRuntimeSetSource::File { path } => {
            let watcher_context = RuntimeSetWatchContext {
                registry: Arc::clone(&runtime_registry),
                path,
                recovery_store,
                providers,
                tenant_scope_hash,
                validation_concurrency,
                poll_interval,
            };
            tokio::spawn(async move {
                watch_runtime_set(watcher_context, fingerprint, stop_rx).await;
            })
        }
        ConfiguredRuntimeSetSource::Cloud { source } => {
            let watcher_context = CloudRuntimeSetWatchContext {
                registry: Arc::clone(&runtime_registry),
                recovery_store,
                providers,
                tenant_scope_hash,
                validation_concurrency,
                poll_interval,
                source,
            };
            tokio::spawn(async move {
                watch_cloud_runtime_set(watcher_context, stop_rx).await;
            })
        }
    };

    let data_plane = async move {
        match tls_runtime {
            Some(tls_runtime) => {
                run_website_data_plane_with_tls_operations_until(
                    host_config.into_app(),
                    executor,
                    operations,
                    tls_runtime,
                    shutdown,
                )
                .await
            }
            None => {
                run_website_data_plane_with_operations_until(
                    host_config.into_app(),
                    executor,
                    operations,
                    shutdown,
                )
                .await
            }
        }
    };
    let result = run_with_provider_event_ingress(provider_event_ingress, data_plane).await;
    let _ = stop_tx.send(true);
    if let Err(error) = watcher.await {
        tracing::error!(error = %error, "website runtime-set watcher stopped unexpectedly");
    }
    result
}

fn configured_tls_runtime(
    environment: WebsiteRuntimeEnvironment,
    node_uuid: &str,
) -> Result<Option<Arc<FileTlsRuntimeController>>, WebsiteDataPlaneBootstrapError> {
    match optional_env(TLS_RUNTIME_SOURCE_ENV)?.as_deref() {
        None | Some("external") => {
            reject_external_tls_file_options()?;
            Ok(None)
        }
        Some("file") => {
            let snapshot_file = PathBuf::from(required_env(TLS_RUNTIME_SNAPSHOT_FILE_ENV)?);
            let material_root = PathBuf::from(required_env(TLS_MATERIAL_ROOT_ENV)?);
            let listener_id = required_env(TLS_LISTENER_ID_ENV)?;
            validate_opaque_config_id(&listener_id, 64, TLS_LISTENER_ID_ENV)?;
            let poll_interval = tls_runtime_poll_interval()?;
            let recovery_directory =
                optional_env(TLS_RUNTIME_RECOVERY_DIRECTORY_ENV)?.map(PathBuf::from);
            if recovery_directory.is_none()
                && matches!(
                    environment,
                    WebsiteRuntimeEnvironment::Staging | WebsiteRuntimeEnvironment::Production
                )
            {
                return Err(WebsiteDataPlaneBootstrapError::RuntimeAssignmentConfig(
                    format!(
                        "{TLS_RUNTIME_RECOVERY_DIRECTORY_ENV} is required for native TLS in staging and production"
                    ),
                ));
            }
            FileTlsRuntimeController::load(FileTlsRuntimeConfig {
                snapshot_file,
                material_root,
                listener_id,
                node_uuid: node_uuid.to_owned(),
                poll_interval,
                recovery_directory,
            })
            .map(Some)
            .map_err(WebsiteDataPlaneBootstrapError::from)
        }
        Some(_) => Err(WebsiteDataPlaneBootstrapError::RuntimeAssignmentConfig(
            format!("{TLS_RUNTIME_SOURCE_ENV} must select external or file"),
        )),
    }
}

fn reject_external_tls_file_options() -> Result<(), WebsiteDataPlaneBootstrapError> {
    for key in [
        TLS_RUNTIME_SNAPSHOT_FILE_ENV,
        TLS_MATERIAL_ROOT_ENV,
        TLS_LISTENER_ID_ENV,
        TLS_RUNTIME_POLL_INTERVAL_MS_ENV,
        TLS_RUNTIME_RECOVERY_DIRECTORY_ENV,
    ] {
        if optional_env(key)?.is_some() {
            return Err(WebsiteDataPlaneBootstrapError::RuntimeAssignmentConfig(
                format!("{key} must not be set when {TLS_RUNTIME_SOURCE_ENV}=external"),
            ));
        }
    }
    Ok(())
}

fn tls_runtime_poll_interval() -> Result<Duration, WebsiteDataPlaneBootstrapError> {
    let value = match optional_env(TLS_RUNTIME_POLL_INTERVAL_MS_ENV)? {
        Some(value) => value.parse::<u64>().map_err(|_| {
            WebsiteDataPlaneBootstrapError::RuntimeAssignmentConfig(format!(
                "{TLS_RUNTIME_POLL_INTERVAL_MS_ENV} must be an integer"
            ))
        })?,
        None => DEFAULT_RUNTIME_SET_POLL_INTERVAL_MS,
    };
    if !(MINIMUM_RUNTIME_SET_POLL_INTERVAL_MS..=MAXIMUM_RUNTIME_SET_POLL_INTERVAL_MS)
        .contains(&value)
    {
        return Err(WebsiteDataPlaneBootstrapError::RuntimeAssignmentConfig(
            format!(
                "{TLS_RUNTIME_POLL_INTERVAL_MS_ENV} must be between {MINIMUM_RUNTIME_SET_POLL_INTERVAL_MS} and {MAXIMUM_RUNTIME_SET_POLL_INTERVAL_MS}"
            ),
        ));
    }
    Ok(Duration::from_millis(value))
}

fn configured_runtime_set_source(
    runtime_set_path: Option<PathBuf>,
) -> Result<ConfiguredRuntimeSetSource, WebsiteDataPlaneBootstrapError> {
    let mode = optional_env(WEBSITE_RUNTIME_ASSIGNMENT_SOURCE_ENV)?;
    match mode.as_deref() {
        Some("file") => {
            let path = runtime_set_path.ok_or_else(|| {
                WebsiteDataPlaneBootstrapError::RuntimeAssignmentConfig(format!(
                    "{WEBSITE_RUNTIME_SET_FILE_ENV} is required for file source mode"
                ))
            })?;
            Ok(ConfiguredRuntimeSetSource::File { path })
        }
        None => runtime_set_path
            .map(|path| ConfiguredRuntimeSetSource::File { path })
            .ok_or_else(|| {
                WebsiteDataPlaneBootstrapError::RuntimeAssignmentConfig(format!(
                    "{WEBSITE_RUNTIME_ASSIGNMENT_SOURCE_ENV} must select file or cloud"
                ))
            }),
        Some("cloud") => {
            if runtime_set_path.is_some() {
                return Err(WebsiteDataPlaneBootstrapError::RuntimeAssignmentConfig(
                    format!("{WEBSITE_RUNTIME_SET_FILE_ENV} must not be set for cloud source mode"),
                ));
            }
            let environment = required_env(WEBSITE_RUNTIME_ENVIRONMENT_ENV)?;
            let parsed_environment = parse_runtime_environment(&environment)?;
            let base_url = validate_base_url(
                &required_env(WEB_INTERNAL_API_BASE_URL_ENV)?,
                parsed_environment,
                WEB_INTERNAL_API_BASE_URL_ENV,
            )?;
            let node_uuid = required_env(WEBSITE_NODE_UUID_ENV)?;
            validate_opaque_config_id(&node_uuid, 64, WEBSITE_NODE_UUID_ENV)?;
            let token_file = required_env(WEB_NODE_TOKEN_FILE_ENV)?;
            let node_token = read_ingress_token(Path::new(&token_file), WEB_NODE_TOKEN_FILE_ENV)?;
            let node_version = optional_env(WEBSITE_NODE_VERSION_ENV)?
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned());
            validate_bounded_config_text(&node_version, 64, WEBSITE_NODE_VERSION_ENV)?;
            Ok(ConfiguredRuntimeSetSource::Cloud {
                source: Arc::new(CloudRuntimeAssignmentSource::new(
                    base_url,
                    node_token,
                    node_uuid,
                    environment,
                    node_version,
                )?),
            })
        }
        Some(value) => Err(WebsiteDataPlaneBootstrapError::RuntimeAssignmentConfig(
            format!("{WEBSITE_RUNTIME_ASSIGNMENT_SOURCE_ENV} has unsupported value {value}"),
        )),
    }
}

fn source_from_config(
    source: &ConfiguredRuntimeSetSource,
) -> Result<&Arc<CloudRuntimeAssignmentSource>, WebsiteDataPlaneBootstrapError> {
    match source {
        ConfiguredRuntimeSetSource::Cloud { source } => Ok(source),
        ConfiguredRuntimeSetSource::File { .. } => {
            Err(WebsiteDataPlaneBootstrapError::RuntimeAssignmentConfig(
                "cloud assignment metadata cannot be used by file source mode".to_owned(),
            ))
        }
    }
}

fn optional_env(key: &'static str) -> Result<Option<String>, WebsiteDataPlaneBootstrapError> {
    match env::var(key) {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value.trim().to_owned())),
        Ok(_) => Err(WebsiteDataPlaneBootstrapError::RuntimeAssignmentConfig(
            format!("{key} must not be blank"),
        )),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(
            WebsiteDataPlaneBootstrapError::RuntimeAssignmentConfig(format!("{key} must be UTF-8")),
        ),
    }
}

fn required_env(key: &'static str) -> Result<String, WebsiteDataPlaneBootstrapError> {
    optional_env(key)?.ok_or_else(|| {
        WebsiteDataPlaneBootstrapError::RuntimeAssignmentConfig(format!("{key} is required"))
    })
}

fn parse_runtime_environment(
    value: &str,
) -> Result<WebsiteRuntimeEnvironment, WebsiteDataPlaneBootstrapError> {
    match value {
        "development" => Ok(WebsiteRuntimeEnvironment::Development),
        "test" => Ok(WebsiteRuntimeEnvironment::Test),
        "staging" => Ok(WebsiteRuntimeEnvironment::Staging),
        "production" => Ok(WebsiteRuntimeEnvironment::Production),
        _ => Err(WebsiteDataPlaneBootstrapError::RuntimeAssignmentConfig(
            format!("{WEBSITE_RUNTIME_ENVIRONMENT_ENV} is invalid"),
        )),
    }
}

fn runtime_environment_name(environment: WebsiteRuntimeEnvironment) -> &'static str {
    match environment {
        WebsiteRuntimeEnvironment::Development => "development",
        WebsiteRuntimeEnvironment::Test => "test",
        WebsiteRuntimeEnvironment::Staging => "staging",
        WebsiteRuntimeEnvironment::Production => "production",
    }
}

fn validate_opaque_config_id(
    value: &str,
    maximum: usize,
    key: &'static str,
) -> Result<(), WebsiteDataPlaneBootstrapError> {
    if value.is_empty()
        || value.len() > maximum
        || !value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphanumeric() || (index > 0 && matches!(byte, b'.' | b'_' | b':' | b'-'))
        })
    {
        return Err(WebsiteDataPlaneBootstrapError::RuntimeAssignmentConfig(
            format!("{key} must be a bounded opaque identifier"),
        ));
    }
    Ok(())
}

fn validate_bounded_config_text(
    value: &str,
    maximum: usize,
    key: &'static str,
) -> Result<(), WebsiteDataPlaneBootstrapError> {
    if value.is_empty()
        || value.len() > maximum
        || value.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(WebsiteDataPlaneBootstrapError::RuntimeAssignmentConfig(
            format!("{key} is invalid"),
        ));
    }
    Ok(())
}

fn load_cloud_runtime_delivery(
    delivery: &mut CloudRuntimeDelivery,
) -> Result<LoadedWebsiteRuntimeSet, WebsiteDataPlaneBootstrapError> {
    let bytes = delivery
        .runtime_set_bytes
        .take()
        .ok_or(WebsiteDataPlaneBootstrapError::RuntimeSetSource)?;
    let loaded = LoadedWebsiteRuntimeSet::compile(bytes)?;
    let runtime_set = loaded.runtime_set();
    if runtime_set.node_uuid() != delivery.assignment.node_uuid
        || runtime_environment_name(runtime_set.environment()) != delivery.assignment.environment
        || runtime_set.generation().to_string() != delivery.assignment.generation
        || runtime_set.snapshot_uuid() != delivery.assignment.snapshot_uuid
        || runtime_set.snapshot_sha256() != delivery.assignment.snapshot_sha256
    {
        return Err(CloudRuntimeAssignmentError::Response.into());
    }
    Ok(loaded)
}

fn validate_cloud_runtime_scope(
    loaded: &LoadedWebsiteRuntimeSet,
    source: &CloudRuntimeAssignmentSource,
    tenant_scope_hash: &str,
) -> Result<(), WebsiteDataPlaneBootstrapError> {
    if loaded.runtime_set().node_uuid() != source.node_uuid()
        || runtime_environment_name(loaded.runtime_set().environment()) != source.environment()
    {
        return Err(CloudRuntimeAssignmentError::Response.into());
    }
    validate_runtime_set_tenant_scope(loaded.runtime_set(), tenant_scope_hash)
}

async fn select_initial_cloud_runtime_set(
    source: &CloudRuntimeAssignmentSource,
    recovered: Option<LoadedWebsiteRuntimeSet>,
    tenant_scope_hash: &str,
) -> Result<
    (LoadedWebsiteRuntimeSet, Option<CloudRuntimeDelivery>, bool),
    WebsiteDataPlaneBootstrapError,
> {
    if let Some(recovered) = recovered {
        validate_cloud_runtime_scope(&recovered, source, tenant_scope_hash)?;
        return Ok((recovered, None, false));
    }

    let mut delivery = source.pull(None, None).await?;
    if delivery.latest_observation_state == Some(RuntimeObservationState::Rejected) {
        return Err(WebsiteDataPlaneBootstrapError::RuntimeAssignmentRejected);
    }
    let initial = match load_cloud_runtime_delivery(&mut delivery) {
        Ok(initial) => initial,
        Err(error) => {
            reject_cloud_assignment(
                source,
                &delivery.assignment,
                delivery.latest_observation_state,
                "INVALID_RUNTIME_SET",
                "runtime-set failed canonical compilation",
            )
            .await?;
            return Err(error);
        }
    };
    if let Err(error) = validate_cloud_runtime_scope(&initial, source, tenant_scope_hash) {
        reject_cloud_assignment(
            source,
            &delivery.assignment,
            delivery.latest_observation_state,
            "SCOPE_MISMATCH",
            "runtime-set scope does not match this Web Node",
        )
        .await?;
        return Err(error);
    }
    Ok((initial, Some(delivery), true))
}

async fn advance_cloud_observation(
    source: &CloudRuntimeAssignmentSource,
    assignment: &CloudRuntimeAssignment,
    current: &mut Option<RuntimeObservationState>,
    target: RuntimeObservationState,
) -> Result<(), WebsiteDataPlaneBootstrapError> {
    if *current == Some(RuntimeObservationState::Rejected) {
        return Err(WebsiteDataPlaneBootstrapError::RuntimeAssignmentRejected);
    }
    while current.is_none_or(|state| state.rank() < target.rank()) {
        let next = match current {
            None => RuntimeObservationState::Received,
            Some(RuntimeObservationState::Received) => RuntimeObservationState::Validated,
            Some(RuntimeObservationState::Validated) => RuntimeObservationState::Staged,
            Some(RuntimeObservationState::Staged) => RuntimeObservationState::Active,
            Some(RuntimeObservationState::Active) => break,
            Some(RuntimeObservationState::Rejected) => {
                return Err(WebsiteDataPlaneBootstrapError::RuntimeAssignmentRejected)
            }
        };
        if next.rank() > target.rank() {
            break;
        }
        source.observe(assignment, next, None, None).await?;
        *current = Some(next);
    }
    Ok(())
}

async fn reject_cloud_assignment(
    source: &CloudRuntimeAssignmentSource,
    assignment: &CloudRuntimeAssignment,
    mut current: Option<RuntimeObservationState>,
    reason_code: &str,
    detail: &str,
) -> Result<(), WebsiteDataPlaneBootstrapError> {
    if matches!(
        current,
        Some(RuntimeObservationState::Active | RuntimeObservationState::Rejected)
    ) {
        return Ok(());
    }
    if current.is_none() {
        source
            .observe(assignment, RuntimeObservationState::Received, None, None)
            .await?;
        current = Some(RuntimeObservationState::Received);
    }
    if current.is_some() {
        source
            .observe(
                assignment,
                RuntimeObservationState::Rejected,
                Some(reason_code),
                Some(detail),
            )
            .await?;
    }
    Ok(())
}

async fn build_provider_event_ingress(
    context: ProviderEventIngressBootstrapContext<'_>,
) -> Result<Option<WebsiteProviderEventIngress>, WebsiteDataPlaneBootstrapError> {
    let config_path = match env::var(WEBSITE_PROVIDER_EVENT_CONFIG_FILE_ENV) {
        Ok(value) if !value.trim().is_empty() => Some(PathBuf::from(value.trim())),
        Ok(_) => {
            return Err(WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
                "{WEBSITE_PROVIDER_EVENT_CONFIG_FILE_ENV} must not be blank"
            )))
        }
        Err(env::VarError::NotPresent) => None,
        Err(env::VarError::NotUnicode(_)) => {
            return Err(WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
                "{WEBSITE_PROVIDER_EVENT_CONFIG_FILE_ENV} must be UTF-8"
            )))
        }
    };
    let Some(config_path) = config_path else {
        if (context.require_drive || context.require_knowledgebase)
            && matches!(
                context.environment,
                WebsiteRuntimeEnvironment::Staging | WebsiteRuntimeEnvironment::Production
            )
        {
            return Err(WebsiteDataPlaneBootstrapError::ProviderEventConfigRequired);
        }
        return Ok(None);
    };
    let reconciler = Arc::new(WebsiteRuntimeSetProviderEventReconciler::new(
        context.runtime_registry,
        context.providers,
        context.validation_concurrency,
    ));
    WebsiteProviderEventIngress::bind_from_file(
        &config_path,
        context.tenant_scope_hash,
        context.node_uuid,
        context.require_drive,
        context.require_knowledgebase,
        context.invalidator,
        reconciler,
    )
    .await
    .map(Some)
    .map_err(|_| WebsiteDataPlaneBootstrapError::ProviderEventIngress)
}

async fn run_with_provider_event_ingress<F>(
    ingress: Option<WebsiteProviderEventIngress>,
    data_plane: F,
) -> Result<(), WebsiteDataPlaneBootstrapError>
where
    F: Future<Output = Result<(), DataPlaneError>> + Send,
{
    let Some(ingress) = ingress else {
        return data_plane
            .await
            .map_err(WebsiteDataPlaneBootstrapError::from);
    };
    let (stop_tx, stop_rx) = oneshot::channel();
    let ingress = ingress.run_until(async move {
        let _ = stop_rx.await;
    });
    tokio::pin!(data_plane);
    tokio::pin!(ingress);
    tokio::select! {
        data_plane_result = &mut data_plane => {
            let _ = stop_tx.send(());
            ingress
                .await
                .map_err(|_| WebsiteDataPlaneBootstrapError::ProviderEventIngress)?;
            data_plane_result.map_err(WebsiteDataPlaneBootstrapError::from)
        }
        ingress_result = &mut ingress => {
            match ingress_result {
                Ok(()) => Err(WebsiteDataPlaneBootstrapError::ProviderEventIngressStopped),
                Err(_) => Err(WebsiteDataPlaneBootstrapError::ProviderEventIngress),
            }
        }
    }
}

fn build_knowledgebase_provider(
    tenant_scope_hash: &str,
    environment: WebsiteRuntimeEnvironment,
    required: bool,
) -> Result<Option<Arc<KnowledgebaseWikiWebsiteProvider>>, WebsiteDataPlaneBootstrapError> {
    let config = ProviderSdkConfig::from_env(
        environment,
        required,
        WebsiteProviderType::Knowledgebase,
        KNOWLEDGEBASE_INTERNAL_API_BASE_URL_ENV,
        KNOWLEDGEBASE_INTERNAL_API_INGRESS_TOKEN_FILE_ENV,
    )?;
    let Some(config) = config else {
        return Ok(None);
    };
    let mut sdk_config = KnowledgebaseSdkworkConfig::new(config.base_url);
    sdk_config.timeout_ms = 30_000;
    sdk_config.max_response_body_bytes = MAXIMUM_KNOWLEDGEBASE_RESPONSE_BYTES;
    let client = KnowledgebaseSdkworkCustomClient::new(sdk_config)
        .map_err(|_| WebsiteDataPlaneBootstrapError::KnowledgebaseSdkClient)?;
    client.set_api_key(config.ingress_token);
    let wiki_client = Arc::new(client.knowledgebase_internal_wiki());
    let resolver = Arc::new(
        FixedKnowledgebaseWikiSdkClientResolver::new(tenant_scope_hash, wiki_client)
            .map_err(WebsiteDataPlaneBootstrapError::ProviderConfig)?,
    );
    Ok(Some(Arc::new(KnowledgebaseWikiWebsiteProvider::new(
        resolver,
    ))))
}

fn build_drive_provider(
    tenant_scope_hash: &str,
    environment: WebsiteRuntimeEnvironment,
    required: bool,
) -> Result<Option<Arc<DriveWebsiteProvider>>, WebsiteDataPlaneBootstrapError> {
    let config = ProviderSdkConfig::from_env(
        environment,
        required,
        WebsiteProviderType::Drive,
        DRIVE_INTERNAL_API_BASE_URL_ENV,
        DRIVE_INTERNAL_API_INGRESS_TOKEN_FILE_ENV,
    )?;
    let Some(config) = config else {
        return Ok(None);
    };
    let mut sdk_config = DriveSdkworkConfig::new(config.base_url);
    sdk_config.timeout_ms = 30_000;
    sdk_config.max_response_body_bytes = usize::try_from(MAXIMUM_DRIVE_CONTENT_BYTES)
        .map_err(|_| WebsiteDataPlaneBootstrapError::DriveSdkClient)?;
    let client = DriveSdkworkCustomClient::new(sdk_config)
        .map_err(|_| WebsiteDataPlaneBootstrapError::DriveSdkClient)?;
    client.set_api_key(config.ingress_token);
    let drive_client = Arc::new(client.drive_internal_publishing());
    let resolver = Arc::new(
        FixedDriveWebsiteSdkClientResolver::new(tenant_scope_hash, drive_client)
            .map_err(WebsiteDataPlaneBootstrapError::ProviderConfig)?,
    );
    Ok(Some(Arc::new(DriveWebsiteProvider::new(resolver))))
}

/// Assembles the provider executor for `drive` and `knowledgebase` resources
/// declared directly in the application Web Server configuration.
///
/// Returns `Ok(None)` when the configuration references no provider-backed
/// resource, so a local-only configuration never introduces provider
/// environment requirements. When provider resources exist, the bootstrap
/// fails closed: the tenant scope hash and provider SDK environment must be
/// configured, every referenced resource must validate against its provider
/// within the configured provider timeout, and any mismatch aborts startup.
pub(crate) async fn build_app_config_provider_executor(
    app: &sdkwork_webserver_core::CompiledWebServerApp,
) -> Result<Option<Arc<AppConfigResourceExecutor>>, WebsiteDataPlaneBootstrapError> {
    let mut requires_drive = false;
    let mut requires_knowledgebase = false;
    for resource in app.provider_resources() {
        match resource.provider_type().expect("provider-backed resource") {
            ConfigProviderType::Drive => requires_drive = true,
            ConfigProviderType::Knowledgebase => requires_knowledgebase = true,
        }
    }
    if !requires_drive && !requires_knowledgebase {
        return Ok(None);
    }

    let tenant_scope_hash = website_tenant_scope_hash()?;
    let environment = app_config_runtime_environment()?;
    let mut registry = WebsiteProviderRegistry::new();
    if requires_drive {
        let provider = build_drive_provider(&tenant_scope_hash, environment, true)?
            .expect("required drive provider is configured");
        registry.register_static(WebsiteProviderType::Drive, provider)?;
    }
    if requires_knowledgebase {
        let provider = build_knowledgebase_provider(&tenant_scope_hash, environment, true)?
            .expect("required knowledgebase provider is configured");
        registry.register_wiki(WebsiteProviderType::Knowledgebase, provider)?;
    }

    let executor = AppConfigResourceExecutor::new(
        Arc::new(registry),
        app.config().app_key.clone(),
        tenant_scope_hash,
    )
    .map_err(|error| {
        WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
            "provider executor configuration is invalid: {error}"
        ))
    })?;
    let provider_timeout_ms = app.config().limits.provider_timeout_ms;
    for resource in app.provider_resources() {
        let (handler, provider_type, contract_version) = match resource {
            ResourceConfig::Drive { .. } => (
                AppConfigResourceHandler::Static,
                WebsiteProviderType::Drive,
                DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION,
            ),
            ResourceConfig::Knowledgebase { .. } => (
                AppConfigResourceHandler::Wiki,
                WebsiteProviderType::Knowledgebase,
                KNOWLEDGEBASE_WIKI_PROVIDER_CONTRACT_VERSION,
            ),
            _ => continue,
        };
        let provider_reference = ProviderResourceReference {
            provider_type,
            provider_resource_uuid: resource
                .provider_resource_uuid()
                .expect("provider-backed resource uuid")
                .to_owned(),
            provider_contract_version: contract_version.to_owned(),
        };
        executor
            .validate_resource(
                resource.id(),
                &provider_reference,
                handler,
                provider_timeout_ms,
            )
            .await
            .inspect_err(|_error| {
                tracing::error!(
                    resource_id = resource.id(),
                    provider_type = ?provider_type,
                    "provider-backed application resource validation failed"
                );
            })?;
    }
    Ok(Some(Arc::new(executor)))
}

fn app_config_runtime_environment(
) -> Result<WebsiteRuntimeEnvironment, WebsiteDataPlaneBootstrapError> {
    let name = web_environment_name();
    match name.as_str() {
        "development" | "dev" => Ok(WebsiteRuntimeEnvironment::Development),
        "test" => Ok(WebsiteRuntimeEnvironment::Test),
        "staging" | "stage" => Ok(WebsiteRuntimeEnvironment::Staging),
        "production" | "prod" => Ok(WebsiteRuntimeEnvironment::Production),
        other => Err(WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
            "SDKWORK_WEBSERVER_ENVIRONMENT is invalid: {other}"
        ))),
    }
}

struct ProviderSdkConfig {
    base_url: String,
    ingress_token: String,
}

impl ProviderSdkConfig {
    fn from_env(
        environment: WebsiteRuntimeEnvironment,
        required: bool,
        provider_type: WebsiteProviderType,
        base_url_key: &'static str,
        token_file_key: &'static str,
    ) -> Result<Option<Self>, WebsiteDataPlaneBootstrapError> {
        let base_url = env::var(base_url_key).ok();
        let token_file = env::var(token_file_key).ok();
        match (base_url, token_file) {
            (None, None) if !required => Ok(None),
            (None, None) => {
                Err(WebsiteDataPlaneBootstrapError::ProviderUnavailable { provider_type })
            }
            (Some(base_url), Some(token_file)) => Ok(Some(Self {
                base_url: validate_base_url(&base_url, environment, base_url_key)?,
                ingress_token: read_ingress_token(Path::new(token_file.trim()), token_file_key)?,
            })),
            _ => Err(WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
                "{base_url_key} and {token_file_key} must be configured together"
            ))),
        }
    }
}

fn validate_base_url(
    value: &str,
    environment: WebsiteRuntimeEnvironment,
    config_key: &str,
) -> Result<String, WebsiteDataPlaneBootstrapError> {
    let value = value.trim();
    let parsed = Url::parse(value).map_err(|_| {
        WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
            "{config_key} must be an absolute HTTP(S) URL"
        ))
    })?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.path() != "/"
    {
        return Err(WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
            "{config_key} must be an absolute credential-free HTTP(S) origin"
        )));
    }
    if matches!(
        environment,
        WebsiteRuntimeEnvironment::Staging | WebsiteRuntimeEnvironment::Production
    ) && parsed.scheme() != "https"
    {
        return Err(WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
            "{config_key} must use HTTPS in staging and production"
        )));
    }
    Ok(value.trim_end_matches('/').to_owned())
}

fn read_ingress_token(
    path: &Path,
    config_key: &str,
) -> Result<String, WebsiteDataPlaneBootstrapError> {
    if path.as_os_str().is_empty() {
        return Err(WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
            "{config_key} must not be blank"
        )));
    }
    let metadata = std::fs::metadata(path).map_err(|_| {
        WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
            "{config_key} must reference a readable secret file"
        ))
    })?;
    if !metadata.is_file()
        || metadata.len() < 16
        || metadata.len() > MAXIMUM_INGRESS_TOKEN_FILE_BYTES
    {
        return Err(WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
            "{config_key} must reference a non-empty bounded secret file"
        )));
    }
    validate_secret_file_permissions(&metadata, config_key)?;
    let token = std::fs::read_to_string(path).map_err(|_| {
        WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
            "{config_key} must reference a UTF-8 secret file"
        ))
    })?;
    if token.len() < 16 || token.len() > 4_096 || token.chars().any(char::is_whitespace) {
        return Err(WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
            "{config_key} contains an invalid ingress token"
        )));
    }
    Ok(token)
}

/// Rejects secret files that group or other users can read or write. On Unix
/// the mode must not expose any group/other bits (for example `0644` and
/// `0600` are accepted, `0660`/`0664` are not); Windows ACLs are not checked.
fn validate_secret_file_permissions(
    metadata: &std::fs::Metadata,
    config_key: &str,
) -> Result<(), WebsiteDataPlaneBootstrapError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        if mode & 0o077 != 0 {
            return Err(WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
                "{config_key} must not be readable or writable by group or other users (mode {mode:o})"
            )));
        }
    }
    let _ = (metadata, config_key);
    Ok(())
}

fn runtime_set_poll_interval() -> Result<Duration, WebsiteDataPlaneBootstrapError> {
    let value = match env::var(WEBSITE_RUNTIME_SET_POLL_INTERVAL_MS_ENV) {
        Ok(value) => value.trim().parse::<u64>().map_err(|_| {
            WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
                "{WEBSITE_RUNTIME_SET_POLL_INTERVAL_MS_ENV} must be an integer"
            ))
        })?,
        Err(_) => DEFAULT_RUNTIME_SET_POLL_INTERVAL_MS,
    };
    if !(MINIMUM_RUNTIME_SET_POLL_INTERVAL_MS..=MAXIMUM_RUNTIME_SET_POLL_INTERVAL_MS)
        .contains(&value)
    {
        return Err(WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
            "{WEBSITE_RUNTIME_SET_POLL_INTERVAL_MS_ENV} must be between {MINIMUM_RUNTIME_SET_POLL_INTERVAL_MS} and {MAXIMUM_RUNTIME_SET_POLL_INTERVAL_MS}"
        )));
    }
    Ok(Duration::from_millis(value))
}

fn website_tenant_scope_hash() -> Result<String, WebsiteDataPlaneBootstrapError> {
    let value = env::var(WEBSITE_TENANT_SCOPE_HASH_ENV).map_err(|_| {
        WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
            "{WEBSITE_TENANT_SCOPE_HASH_ENV} is required"
        ))
    })?;
    parse_tenant_scope_hash(&value)
}

fn parse_tenant_scope_hash(value: &str) -> Result<String, WebsiteDataPlaneBootstrapError> {
    let value = value.trim();
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
            "{WEBSITE_TENANT_SCOPE_HASH_ENV} must be a 64-character lowercase hexadecimal digest"
        )));
    }
    Ok(value.to_owned())
}

fn provider_validation_concurrency() -> Result<usize, WebsiteDataPlaneBootstrapError> {
    let value = match env::var(WEBSITE_PROVIDER_VALIDATION_CONCURRENCY_ENV) {
        Ok(value) => value.trim().parse::<usize>().map_err(|_| {
            WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
                "{WEBSITE_PROVIDER_VALIDATION_CONCURRENCY_ENV} must be an integer"
            ))
        })?,
        Err(_) => DEFAULT_PROVIDER_VALIDATION_CONCURRENCY,
    };
    if !(1..=MAXIMUM_PROVIDER_VALIDATION_CONCURRENCY).contains(&value) {
        return Err(WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
            "{WEBSITE_PROVIDER_VALIDATION_CONCURRENCY_ENV} must be between 1 and {MAXIMUM_PROVIDER_VALIDATION_CONCURRENCY}"
        )));
    }
    Ok(value)
}

fn provider_buffered_content_bytes() -> Result<usize, WebsiteDataPlaneBootstrapError> {
    match env::var(WEBSITE_PROVIDER_BUFFERED_CONTENT_BYTES_ENV) {
        Ok(value) => parse_provider_buffered_content_bytes(Some(&value)),
        Err(env::VarError::NotPresent) => parse_provider_buffered_content_bytes(None),
        Err(env::VarError::NotUnicode(_)) => Err(WebsiteDataPlaneBootstrapError::ProviderConfig(
            format!("{WEBSITE_PROVIDER_BUFFERED_CONTENT_BYTES_ENV} must be UTF-8"),
        )),
    }
}

fn parse_provider_buffered_content_bytes(
    value: Option<&str>,
) -> Result<usize, WebsiteDataPlaneBootstrapError> {
    let value = match value {
        Some(value) => value.trim().parse::<usize>().map_err(|_| {
            WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
                "{WEBSITE_PROVIDER_BUFFERED_CONTENT_BYTES_ENV} must be an integer"
            ))
        })?,
        None => DEFAULT_PROVIDER_BUFFERED_CONTENT_BYTES,
    };
    if !(MINIMUM_PROVIDER_BUFFERED_CONTENT_BYTES..=MAXIMUM_PROVIDER_BUFFERED_CONTENT_BYTES)
        .contains(&value)
    {
        return Err(WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
            "{WEBSITE_PROVIDER_BUFFERED_CONTENT_BYTES_ENV} must be between {MINIMUM_PROVIDER_BUFFERED_CONTENT_BYTES} and {MAXIMUM_PROVIDER_BUFFERED_CONTENT_BYTES} bytes"
        )));
    }
    Ok(value)
}

fn provider_resolution_cache_entries() -> Result<usize, WebsiteDataPlaneBootstrapError> {
    match env::var(WEBSITE_PROVIDER_RESOLUTION_CACHE_ENTRIES_ENV) {
        Ok(value) => parse_provider_resolution_cache_entries(Some(&value)),
        Err(env::VarError::NotPresent) => parse_provider_resolution_cache_entries(None),
        Err(env::VarError::NotUnicode(_)) => Err(WebsiteDataPlaneBootstrapError::ProviderConfig(
            format!("{WEBSITE_PROVIDER_RESOLUTION_CACHE_ENTRIES_ENV} must be UTF-8"),
        )),
    }
}

fn parse_provider_resolution_cache_entries(
    value: Option<&str>,
) -> Result<usize, WebsiteDataPlaneBootstrapError> {
    let value = match value {
        Some(value) => value.trim().parse::<usize>().map_err(|_| {
            WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
                "{WEBSITE_PROVIDER_RESOLUTION_CACHE_ENTRIES_ENV} must be an integer"
            ))
        })?,
        None => DEFAULT_PROVIDER_RESOLUTION_CACHE_ENTRIES,
    };
    if !(1..=MAXIMUM_PROVIDER_RESOLUTION_CACHE_ENTRIES).contains(&value) {
        return Err(WebsiteDataPlaneBootstrapError::ProviderConfig(format!(
            "{WEBSITE_PROVIDER_RESOLUTION_CACHE_ENTRIES_ENV} must be between 1 and {MAXIMUM_PROVIDER_RESOLUTION_CACHE_ENTRIES} entries"
        )));
    }
    Ok(value)
}

fn validate_runtime_set_tenant_scope(
    runtime_set: &CompiledWebsiteRuntimeSet,
    tenant_scope_hash: &str,
) -> Result<(), WebsiteDataPlaneBootstrapError> {
    if !runtime_set.is_empty_or_single_tenant_scope(tenant_scope_hash) {
        return Err(WebsiteDataPlaneBootstrapError::TenantScopeMismatch);
    }
    Ok(())
}

fn runtime_set_recovery_directory() -> Result<Option<PathBuf>, WebsiteDataPlaneBootstrapError> {
    match env::var(WEBSITE_RUNTIME_SET_RECOVERY_DIRECTORY_ENV) {
        Ok(value) if !value.trim().is_empty() => Ok(Some(PathBuf::from(value.trim()))),
        Ok(_) => Err(WebsiteDataPlaneBootstrapError::RuntimeSetRecovery),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => {
            Err(WebsiteDataPlaneBootstrapError::RuntimeSetRecovery)
        }
    }
}

async fn open_runtime_set_recovery(
) -> Result<Option<WebsiteRuntimeSetRecoveryOpen>, WebsiteDataPlaneBootstrapError> {
    let Some(directory) = runtime_set_recovery_directory()? else {
        return Ok(None);
    };
    tokio::task::spawn_blocking(move || WebsiteRuntimeSetRecoveryStore::open(directory))
        .await
        .map_err(|_| WebsiteDataPlaneBootstrapError::RuntimeSetRecovery)?
        .map(Some)
        .map_err(|_| WebsiteDataPlaneBootstrapError::RuntimeSetRecovery)
}

fn require_runtime_set_recovery(
    environment: WebsiteRuntimeEnvironment,
    store: Option<&Arc<WebsiteRuntimeSetRecoveryStore>>,
) -> Result<(), WebsiteDataPlaneBootstrapError> {
    if store.is_none()
        && matches!(
            environment,
            WebsiteRuntimeEnvironment::Staging | WebsiteRuntimeEnvironment::Production
        )
    {
        return Err(WebsiteDataPlaneBootstrapError::RuntimeSetRecoveryRequired);
    }
    Ok(())
}

fn select_initial_runtime_set(
    source: Result<(LoadedWebsiteRuntimeSet, SourceFingerprint), WebsiteDataPlaneBootstrapError>,
    recovered: Option<LoadedWebsiteRuntimeSet>,
    tenant_scope_hash: &str,
) -> Result<(LoadedWebsiteRuntimeSet, Option<SourceFingerprint>), WebsiteDataPlaneBootstrapError> {
    if let Ok((source, _)) = source.as_ref() {
        validate_runtime_set_tenant_scope(source.runtime_set(), tenant_scope_hash)?;
    }
    if let Some(recovered) = recovered.as_ref() {
        validate_runtime_set_tenant_scope(recovered.runtime_set(), tenant_scope_hash)?;
    }
    match (source, recovered) {
        (Ok((source, fingerprint)), Some(recovered)) => {
            if source.runtime_set().node_uuid() != recovered.runtime_set().node_uuid()
                || source.runtime_set().environment() != recovered.runtime_set().environment()
            {
                return Err(WebsiteDataPlaneBootstrapError::RuntimeSetRecovery);
            }
            match source
                .runtime_set()
                .generation()
                .cmp(&recovered.runtime_set().generation())
            {
                std::cmp::Ordering::Greater => Ok((source, Some(fingerprint))),
                std::cmp::Ordering::Less => Ok((recovered, Some(fingerprint))),
                std::cmp::Ordering::Equal
                    if source.runtime_set().snapshot_sha256()
                        == recovered.runtime_set().snapshot_sha256() =>
                {
                    Ok((source, Some(fingerprint)))
                }
                std::cmp::Ordering::Equal => {
                    Err(WebsiteDataPlaneBootstrapError::RuntimeSetRecovery)
                }
            }
        }
        (Ok((source, fingerprint)), None) => Ok((source, Some(fingerprint))),
        (Err(_), Some(recovered)) => Ok((recovered, None)),
        (Err(error), None) => Err(error),
    }
}

async fn persist_runtime_set(
    store: Option<&Arc<WebsiteRuntimeSetRecoveryStore>>,
    candidate: &LoadedWebsiteRuntimeSet,
) -> Result<(), WebsiteDataPlaneBootstrapError> {
    let Some(store) = store else {
        return Ok(());
    };
    store
        .persist(candidate)
        .await
        .map(|_| ())
        .map_err(|_| WebsiteDataPlaneBootstrapError::RuntimeSetRecovery)
}

fn load_runtime_set(
    path: &Path,
) -> Result<(LoadedWebsiteRuntimeSet, SourceFingerprint), WebsiteDataPlaneBootstrapError> {
    let before = source_fingerprint(path)?;
    let bytes =
        std::fs::read(path).map_err(|_| WebsiteDataPlaneBootstrapError::RuntimeSetSource)?;
    let after = source_fingerprint(path)?;
    if before != after || bytes.len() as u64 != after.length {
        return Err(WebsiteDataPlaneBootstrapError::RuntimeSetSourceChanged);
    }
    Ok((LoadedWebsiteRuntimeSet::compile(bytes)?, after))
}

fn source_fingerprint(path: &Path) -> Result<SourceFingerprint, WebsiteDataPlaneBootstrapError> {
    let metadata =
        std::fs::metadata(path).map_err(|_| WebsiteDataPlaneBootstrapError::RuntimeSetSource)?;
    if !metadata.is_file()
        || metadata.len() > u64::try_from(MAX_WEBSITE_RUNTIME_SET_BYTES).unwrap_or(u64::MAX)
    {
        return Err(WebsiteDataPlaneBootstrapError::RuntimeSetSource);
    }
    Ok(SourceFingerprint {
        length: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

async fn watch_runtime_set(
    context: RuntimeSetWatchContext,
    mut observed: Option<SourceFingerprint>,
    mut stop: watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(context.poll_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut last_error = None;
    loop {
        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    return;
                }
            }
            _ = ticker.tick() => {
                let fingerprint = match source_fingerprint(&context.path) {
                    Ok(fingerprint) if Some(fingerprint) == observed => continue,
                    Ok(fingerprint) => fingerprint,
                    Err(error) => {
                        log_watcher_error_once(&mut last_error, &error);
                        continue;
                    }
                };
                observed = Some(fingerprint);
                let candidate_path = context.path.clone();
                let loaded = tokio::task::spawn_blocking(move || load_runtime_set(&candidate_path)).await;
                let (candidate, stable_fingerprint) = match loaded {
                    Ok(Ok((candidate, stable_fingerprint))) => {
                        observed = Some(stable_fingerprint);
                        (candidate, stable_fingerprint)
                    }
                    Ok(Err(error)) => {
                        log_watcher_error_once(&mut last_error, &error);
                        continue;
                    }
                    Err(_) => {
                        observed = None;
                        let error = WebsiteDataPlaneBootstrapError::RuntimeSetSource;
                        log_watcher_error_once(&mut last_error, &error);
                        continue;
                    }
                };
                if let Err(error) = validate_runtime_set_tenant_scope(
                    candidate.runtime_set(),
                    &context.tenant_scope_hash,
                ) {
                    log_watcher_error_once(&mut last_error, &error);
                    continue;
                }
                if let Err(error) = context.providers
                    .validate_runtime_set(candidate.runtime_set(), context.validation_concurrency)
                    .await
                {
                    observed = None;
                    log_watcher_error_once(
                        &mut last_error,
                        &WebsiteDataPlaneBootstrapError::ProviderValidation(error),
                    );
                    continue;
                }
                if let Err(error) = probe_website_runtime_set_activation(
                    Arc::clone(candidate.runtime_set()),
                    Arc::clone(&context.providers),
                    context.validation_concurrency,
                ).await {
                    observed = None;
                    log_watcher_error_once(
                        &mut last_error,
                        &WebsiteDataPlaneBootstrapError::RuntimeActivationProbe(error),
                    );
                    continue;
                }
                if let Err(error) = persist_runtime_set(
                    context.recovery_store.as_ref(),
                    &candidate,
                ).await {
                    observed = None;
                    log_watcher_error_once(&mut last_error, &error);
                    continue;
                }
                match context.registry.activate(Arc::clone(candidate.runtime_set())) {
                    Ok(report) => {
                        observed = Some(stable_fingerprint);
                        last_error = None;
                        if report.changed {
                            tracing::info!(
                                website_runtime_generation = report.generation,
                                website_runtime_sha256 = %report.snapshot_sha256,
                                "website runtime-set activated"
                            );
                        }
                    }
                    Err(error) => {
                        log_watcher_error_once(
                            &mut last_error,
                            &WebsiteDataPlaneBootstrapError::RuntimeSet(error),
                        );
                    }
                }
            }
        }
    }
}

async fn watch_cloud_runtime_set(
    context: CloudRuntimeSetWatchContext,
    mut stop: watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(context.poll_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut last_error = None;
    loop {
        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    return;
                }
            }
            _ = ticker.tick() => {
                let current = context.registry.current();
                let generation = current.as_ref().map(|runtime_set| runtime_set.generation().to_string());
                let snapshot_sha256 = current.as_ref().map(|runtime_set| runtime_set.snapshot_sha256().to_owned());
                let mut delivery = match context.source
                    .pull(generation.as_deref(), snapshot_sha256.as_deref())
                    .await
                {
                    Ok(delivery) => delivery,
                    Err(error) => {
                        log_watcher_error_once(
                            &mut last_error,
                            &WebsiteDataPlaneBootstrapError::from(error),
                        );
                        continue;
                    }
                };
                if delivery.latest_observation_state == Some(RuntimeObservationState::Rejected) {
                    log_watcher_error_once(
                        &mut last_error,
                        &WebsiteDataPlaneBootstrapError::RuntimeAssignmentRejected,
                    );
                    continue;
                }
                if delivery.unchanged {
                    match delivery.latest_observation_state {
                        Some(RuntimeObservationState::Active) => {
                            last_error = None;
                            continue;
                        }
                        Some(RuntimeObservationState::Staged) => {
                            let Some(current) = current else {
                                log_watcher_error_once(
                                    &mut last_error,
                                    &WebsiteDataPlaneBootstrapError::RuntimeAssignment,
                                );
                                continue;
                            };
                            if let Err(error) = probe_website_runtime_set_activation(
                                current,
                                Arc::clone(&context.providers),
                                context.validation_concurrency,
                            ).await {
                                let probe_error = WebsiteDataPlaneBootstrapError::RuntimeActivationProbe(error);
                                if let Err(observation_error) = reject_cloud_assignment(
                                    &context.source,
                                    &delivery.assignment,
                                    delivery.latest_observation_state,
                                    "ACTIVATION_PROBE_FAILED",
                                    "node-local route and content activation probe failed",
                                ).await {
                                    log_watcher_error_once(&mut last_error, &observation_error);
                                } else {
                                    log_watcher_error_once(&mut last_error, &probe_error);
                                }
                                continue;
                            }
                            if let Err(error) = context.source
                                .observe(
                                    &delivery.assignment,
                                    RuntimeObservationState::Active,
                                    None,
                                    None,
                                )
                                .await
                            {
                                log_watcher_error_once(
                                    &mut last_error,
                                    &WebsiteDataPlaneBootstrapError::from(error),
                                );
                                continue;
                            }
                            last_error = None;
                            continue;
                        }
                        _ => {
                            delivery = match context.source.pull(None, None).await {
                                Ok(delivery) => delivery,
                                Err(error) => {
                                    log_watcher_error_once(
                                        &mut last_error,
                                        &WebsiteDataPlaneBootstrapError::from(error),
                                    );
                                    continue;
                                }
                            };
                        }
                    }
                }

                let candidate = match load_cloud_runtime_delivery(&mut delivery) {
                    Ok(candidate) => candidate,
                    Err(error) => {
                        if let Err(observation_error) = reject_cloud_assignment(
                            &context.source,
                            &delivery.assignment,
                            delivery.latest_observation_state,
                            "INVALID_RUNTIME_SET",
                            "runtime-set failed canonical compilation",
                        ).await {
                            log_watcher_error_once(&mut last_error, &observation_error);
                        } else {
                            log_watcher_error_once(&mut last_error, &error);
                        }
                        continue;
                    }
                };
                if let Err(error) = validate_cloud_runtime_scope(
                    &candidate,
                    &context.source,
                    &context.tenant_scope_hash,
                ) {
                    if let Err(observation_error) = reject_cloud_assignment(
                        &context.source,
                        &delivery.assignment,
                        delivery.latest_observation_state,
                        "SCOPE_MISMATCH",
                        "runtime-set scope does not match this Web Node",
                    ).await {
                        log_watcher_error_once(&mut last_error, &observation_error);
                    } else {
                        log_watcher_error_once(&mut last_error, &error);
                    }
                    continue;
                }
                if let Err(error) = advance_cloud_observation(
                    &context.source,
                    &delivery.assignment,
                    &mut delivery.latest_observation_state,
                    RuntimeObservationState::Received,
                ).await {
                    log_watcher_error_once(&mut last_error, &error);
                    continue;
                }
                if let Err(error) = context.providers
                    .validate_runtime_set(candidate.runtime_set(), context.validation_concurrency)
                    .await
                {
                    let provider_error = WebsiteDataPlaneBootstrapError::ProviderValidation(error);
                    if let Err(observation_error) = reject_cloud_assignment(
                        &context.source,
                        &delivery.assignment,
                        delivery.latest_observation_state,
                        "PROVIDER_VALIDATION_FAILED",
                        "runtime-set provider validation failed",
                    ).await {
                        log_watcher_error_once(&mut last_error, &observation_error);
                    } else {
                        log_watcher_error_once(&mut last_error, &provider_error);
                    }
                    continue;
                }
                if let Err(error) = advance_cloud_observation(
                    &context.source,
                    &delivery.assignment,
                    &mut delivery.latest_observation_state,
                    RuntimeObservationState::Validated,
                ).await {
                    log_watcher_error_once(&mut last_error, &error);
                    continue;
                }
                if let Err(error) = probe_website_runtime_set_activation(
                    Arc::clone(candidate.runtime_set()),
                    Arc::clone(&context.providers),
                    context.validation_concurrency,
                ).await {
                    let probe_error = WebsiteDataPlaneBootstrapError::RuntimeActivationProbe(error);
                    if let Err(observation_error) = reject_cloud_assignment(
                        &context.source,
                        &delivery.assignment,
                        delivery.latest_observation_state,
                        "ACTIVATION_PROBE_FAILED",
                        "node-local route and content activation probe failed",
                    ).await {
                        log_watcher_error_once(&mut last_error, &observation_error);
                    } else {
                        log_watcher_error_once(&mut last_error, &probe_error);
                    }
                    continue;
                }
                if let Err(error) = persist_runtime_set(
                    context.recovery_store.as_ref(),
                    &candidate,
                ).await {
                    log_watcher_error_once(&mut last_error, &error);
                    continue;
                }
                if let Err(error) = advance_cloud_observation(
                    &context.source,
                    &delivery.assignment,
                    &mut delivery.latest_observation_state,
                    RuntimeObservationState::Staged,
                ).await {
                    log_watcher_error_once(&mut last_error, &error);
                    continue;
                }
                let report = match context.registry.activate(Arc::clone(candidate.runtime_set())) {
                    Ok(report) => report,
                    Err(error) => {
                        let activation_error = WebsiteDataPlaneBootstrapError::RuntimeSet(error);
                        if let Err(observation_error) = reject_cloud_assignment(
                            &context.source,
                            &delivery.assignment,
                            delivery.latest_observation_state,
                            "ACTIVATION_FAILED",
                            "runtime-set failed atomic registry activation",
                        ).await {
                            log_watcher_error_once(&mut last_error, &observation_error);
                        } else {
                            log_watcher_error_once(&mut last_error, &activation_error);
                        }
                        continue;
                    }
                };
                if let Err(error) = advance_cloud_observation(
                    &context.source,
                    &delivery.assignment,
                    &mut delivery.latest_observation_state,
                    RuntimeObservationState::Active,
                ).await {
                    log_watcher_error_once(&mut last_error, &error);
                    continue;
                }
                last_error = None;
                if report.changed {
                    tracing::info!(
                        website_runtime_generation = report.generation,
                        website_runtime_sha256 = %report.snapshot_sha256,
                        "cloud website runtime-set activated"
                    );
                }
            }
        }
    }
}

fn log_watcher_error_once(last_error: &mut Option<String>, error: &WebsiteDataPlaneBootstrapError) {
    let classification = watcher_error_class(error);
    if last_error.as_deref() == Some(classification) {
        return;
    }
    tracing::warn!(
        reason = classification,
        "website runtime-set update rejected"
    );
    *last_error = Some(classification.to_owned());
}

fn watcher_error_class(error: &WebsiteDataPlaneBootstrapError) -> &'static str {
    match error {
        WebsiteDataPlaneBootstrapError::RuntimeSetSource => "source-unavailable",
        WebsiteDataPlaneBootstrapError::RuntimeSetSourceChanged => "source-changed-during-read",
        WebsiteDataPlaneBootstrapError::RuntimeSetRecoveryRequired => "recovery-required",
        WebsiteDataPlaneBootstrapError::RuntimeSetRecovery => "recovery-invalid",
        WebsiteDataPlaneBootstrapError::RuntimeSet(_) => "snapshot-invalid",
        WebsiteDataPlaneBootstrapError::ProviderUnavailable { .. } => "provider-unavailable",
        WebsiteDataPlaneBootstrapError::TenantScopeMismatch => "tenant-scope-mismatch",
        WebsiteDataPlaneBootstrapError::ProviderValidation(_) => "provider-validation-failed",
        WebsiteDataPlaneBootstrapError::RuntimeActivationProbe(_) => "activation-probe-failed",
        WebsiteDataPlaneBootstrapError::RuntimeAssignmentConfig(_) => {
            "runtime-assignment-config-invalid"
        }
        WebsiteDataPlaneBootstrapError::RuntimeAssignmentRejected => "runtime-assignment-rejected",
        WebsiteDataPlaneBootstrapError::RuntimeAssignment => "runtime-assignment-unavailable",
        _ => "bootstrap-contract-invalid",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };

    use async_trait::async_trait;
    use axum::{http::StatusCode, routing::any, Json, Router};
    use sdkwork_webserver_contract::provider::{
        OpenWebsiteContentRequest, OpenedWebsiteContent, ResolveWebsiteStaticPathRequest,
        ResolvedWebsiteContent, ValidateWebsiteResourceRequest, ValidatedWebsiteResource,
        WebsiteContentMetadata, WebsiteContentResolution, WebsiteProviderContentHandle,
        WebsiteProviderError, WebsiteProviderErrorKind, WebsiteProviderResult,
        WebsiteResourceProvider, WebsiteStaticContentProvider,
    };
    use sdkwork_webserver_core::website_runtime::{
        website_runtime_descriptor_sha256, website_runtime_set_snapshot_sha256,
        WebsiteProviderType, WebsiteRuntimeDescriptor, WebsiteRuntimeSetSnapshot,
    };
    use serde_json::{json, Value};

    #[test]
    fn provider_buffered_content_budget_is_bounded_and_has_a_safe_default() {
        assert_eq!(
            parse_provider_buffered_content_bytes(None).unwrap(),
            DEFAULT_PROVIDER_BUFFERED_CONTENT_BYTES
        );
        assert_eq!(
            parse_provider_buffered_content_bytes(Some("268435456")).unwrap(),
            268_435_456
        );
        for value in ["", "not-a-number", "16777215", "2147483649"] {
            assert!(parse_provider_buffered_content_bytes(Some(value)).is_err());
        }
    }

    #[test]
    fn provider_resolution_cache_capacity_is_bounded_and_has_a_safe_default() {
        assert_eq!(
            parse_provider_resolution_cache_entries(None).unwrap(),
            DEFAULT_PROVIDER_RESOLUTION_CACHE_ENTRIES
        );
        assert_eq!(
            parse_provider_resolution_cache_entries(Some("16384")).unwrap(),
            16_384
        );
        for value in ["", "not-a-number", "0", "1048577"] {
            assert!(parse_provider_resolution_cache_entries(Some(value)).is_err());
        }
    }

    use super::*;

    #[test]
    fn knowledgebase_base_url_is_credential_free_and_https_in_production() {
        assert_eq!(
            validate_base_url(
                "https://knowledgebase.internal.example/",
                WebsiteRuntimeEnvironment::Production,
                KNOWLEDGEBASE_INTERNAL_API_BASE_URL_ENV,
            )
            .unwrap(),
            "https://knowledgebase.internal.example"
        );
        for invalid in [
            "http://knowledgebase.internal.example",
            "https://user:secret@knowledgebase.internal.example",
            "https://knowledgebase.internal.example/path",
            "https://knowledgebase.internal.example?token=secret",
        ] {
            assert!(
                validate_base_url(
                    invalid,
                    WebsiteRuntimeEnvironment::Production,
                    KNOWLEDGEBASE_INTERNAL_API_BASE_URL_ENV,
                )
                .is_err(),
                "{invalid} must fail"
            );
        }
        assert!(validate_base_url(
            "http://127.0.0.1:3801",
            WebsiteRuntimeEnvironment::Development,
            DRIVE_INTERNAL_API_BASE_URL_ENV,
        )
        .is_ok());
    }

    #[test]
    fn secret_file_reader_rejects_blank_whitespace_and_oversize_values() {
        let root = tempfile::tempdir().unwrap();
        let token = root.path().join("token");
        std::fs::write(&token, "0123456789abcdef").unwrap();
        assert_eq!(
            read_ingress_token(&token, DRIVE_INTERNAL_API_INGRESS_TOKEN_FILE_ENV).unwrap(),
            "0123456789abcdef"
        );
        std::fs::write(&token, "0123456789abcde f").unwrap();
        assert!(read_ingress_token(&token, DRIVE_INTERNAL_API_INGRESS_TOKEN_FILE_ENV).is_err());
        std::fs::write(
            &token,
            vec![b'x'; MAXIMUM_INGRESS_TOKEN_FILE_BYTES as usize + 1],
        )
        .unwrap();
        assert!(read_ingress_token(&token, DRIVE_INTERNAL_API_INGRESS_TOKEN_FILE_ENV).is_err());
    }

    #[test]
    fn tenant_scope_hash_is_exact_lowercase_sha256_shape() {
        let expected = "a".repeat(64);
        assert_eq!(parse_tenant_scope_hash(&expected).unwrap(), expected);
        for invalid in [
            "a".repeat(63),
            "A".repeat(64),
            "g".repeat(64),
            format!("{} ", "a".repeat(63)),
        ] {
            assert!(parse_tenant_scope_hash(&invalid).is_err());
        }
    }

    #[test]
    fn startup_prefers_newer_recovery_and_uses_it_when_source_is_unavailable() {
        let tenant_scope_hash = "1".repeat(64);
        let source = LoadedWebsiteRuntimeSet::compile(runtime_set(
            1,
            "node-0001",
            WebsiteRuntimeEnvironment::Production,
            "source",
        ))
        .unwrap();
        let recovered = LoadedWebsiteRuntimeSet::compile(runtime_set(
            2,
            "node-0001",
            WebsiteRuntimeEnvironment::Production,
            "recovered",
        ))
        .unwrap();
        let fingerprint = SourceFingerprint {
            length: 1,
            modified: None,
        };
        let (selected, observed) = select_initial_runtime_set(
            Ok((source, fingerprint)),
            Some(recovered),
            &tenant_scope_hash,
        )
        .unwrap();
        assert_eq!(selected.runtime_set().generation(), 2);
        assert_eq!(observed, Some(fingerprint));

        let recovered = LoadedWebsiteRuntimeSet::compile(runtime_set(
            3,
            "node-0001",
            WebsiteRuntimeEnvironment::Production,
            "restart",
        ))
        .unwrap();
        let (selected, observed) = select_initial_runtime_set(
            Err(WebsiteDataPlaneBootstrapError::RuntimeSetSource),
            Some(recovered),
            &tenant_scope_hash,
        )
        .unwrap();
        assert_eq!(selected.runtime_set().generation(), 3);
        assert_eq!(observed, None);
    }

    #[test]
    fn startup_rejects_recovery_conflicts_and_cross_scope_snapshots() {
        let tenant_scope_hash = "1".repeat(64);
        let fingerprint = SourceFingerprint {
            length: 1,
            modified: None,
        };
        let source = LoadedWebsiteRuntimeSet::compile(runtime_set(
            7,
            "node-0001",
            WebsiteRuntimeEnvironment::Production,
            "source",
        ))
        .unwrap();
        let conflict = LoadedWebsiteRuntimeSet::compile(runtime_set(
            7,
            "node-0001",
            WebsiteRuntimeEnvironment::Production,
            "conflict",
        ))
        .unwrap();
        assert!(matches!(
            select_initial_runtime_set(
                Ok((source, fingerprint)),
                Some(conflict),
                &tenant_scope_hash,
            ),
            Err(WebsiteDataPlaneBootstrapError::RuntimeSetRecovery)
        ));

        let source = LoadedWebsiteRuntimeSet::compile(runtime_set(
            8,
            "node-0001",
            WebsiteRuntimeEnvironment::Production,
            "source",
        ))
        .unwrap();
        let cross_scope = LoadedWebsiteRuntimeSet::compile(runtime_set(
            9,
            "node-0002",
            WebsiteRuntimeEnvironment::Production,
            "cross-scope",
        ))
        .unwrap();
        assert!(matches!(
            select_initial_runtime_set(
                Ok((source, fingerprint)),
                Some(cross_scope),
                &tenant_scope_hash,
            ),
            Err(WebsiteDataPlaneBootstrapError::RuntimeSetRecovery)
        ));
    }

    #[test]
    fn durable_recovery_is_required_in_staging_and_production() {
        assert!(matches!(
            require_runtime_set_recovery(WebsiteRuntimeEnvironment::Production, None),
            Err(WebsiteDataPlaneBootstrapError::RuntimeSetRecoveryRequired)
        ));
        assert!(matches!(
            require_runtime_set_recovery(WebsiteRuntimeEnvironment::Staging, None),
            Err(WebsiteDataPlaneBootstrapError::RuntimeSetRecoveryRequired)
        ));
        assert!(require_runtime_set_recovery(WebsiteRuntimeEnvironment::Development, None).is_ok());
        assert!(require_runtime_set_recovery(WebsiteRuntimeEnvironment::Test, None).is_ok());
    }

    #[tokio::test]
    async fn cloud_startup_uses_valid_recovery_and_requires_control_plane_on_first_start() {
        let request_count = Arc::new(AtomicUsize::new(0));
        let handler_count = Arc::clone(&request_count);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let app = Router::new().fallback(any(move || {
                let handler_count = Arc::clone(&handler_count);
                async move {
                    handler_count.fetch_add(1, Ordering::SeqCst);
                    StatusCode::SERVICE_UNAVAILABLE
                }
            }));
            axum::serve(listener, app).await.unwrap();
        });
        let source = CloudRuntimeAssignmentSource::new(
            format!("http://{address}"),
            "node-token-7".to_owned(),
            "node-0001".to_owned(),
            "production".to_owned(),
            "1.2.3".to_owned(),
        )
        .unwrap();
        let tenant_scope_hash = "1".repeat(64);
        let recovered = LoadedWebsiteRuntimeSet::compile(runtime_set(
            7,
            "node-0001",
            WebsiteRuntimeEnvironment::Production,
            "recovered",
        ))
        .unwrap();

        let (selected, delivery, source_available) =
            select_initial_cloud_runtime_set(&source, Some(recovered), &tenant_scope_hash)
                .await
                .unwrap();
        assert_eq!(selected.runtime_set().generation(), 7);
        assert!(delivery.is_none());
        assert!(!source_available);
        assert_eq!(request_count.load(Ordering::SeqCst), 0);

        let wrong_node = LoadedWebsiteRuntimeSet::compile(runtime_set(
            8,
            "node-0002",
            WebsiteRuntimeEnvironment::Production,
            "wrong-node",
        ))
        .unwrap();
        assert!(matches!(
            select_initial_cloud_runtime_set(&source, Some(wrong_node), &tenant_scope_hash,).await,
            Err(WebsiteDataPlaneBootstrapError::RuntimeAssignment)
        ));
        assert_eq!(request_count.load(Ordering::SeqCst), 0);

        assert!(matches!(
            select_initial_cloud_runtime_set(&source, None, &tenant_scope_hash).await,
            Err(WebsiteDataPlaneBootstrapError::RuntimeAssignment)
        ));
        assert_eq!(request_count.load(Ordering::SeqCst), 1);
        server.abort();
    }

    #[tokio::test]
    async fn cloud_observation_resume_reports_only_missing_phases() {
        let observed = Arc::new(Mutex::new(Vec::new()));
        let handler_observed = Arc::clone(&observed);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let app = Router::new().fallback(any(move |Json(body): Json<Value>| {
                let handler_observed = Arc::clone(&handler_observed);
                async move {
                    let state = body
                        .get("state")
                        .and_then(Value::as_str)
                        .unwrap()
                        .to_owned();
                    handler_observed.lock().unwrap().push(state.clone());
                    (
                        StatusCode::CREATED,
                        Json(json!({
                            "code": 0,
                            "message": "success",
                            "data": {"item": {
                                "observationUuid": format!("observation-{state}"),
                                "tenantId": "7",
                                "assignmentUuid": "assignment-7",
                                "nodeUuid": "node-0001",
                                "environment": "production",
                                "generation": "7",
                                "snapshotUuid": "snapshot-7",
                                "snapshotSha256": "a".repeat(64),
                                "state": state,
                                "nodeVersion": "1.2.3",
                                "observedAt": "2026-07-22T00:00:00Z"
                            }},
                            "traceId": "trace-observation",
                            "timestamp": "2026-07-22T00:00:01Z"
                        })),
                    )
                }
            }));
            axum::serve(listener, app).await.unwrap();
        });
        let source = CloudRuntimeAssignmentSource::new(
            format!("http://{address}"),
            "node-token-7".to_owned(),
            "node-0001".to_owned(),
            "production".to_owned(),
            "1.2.3".to_owned(),
        )
        .unwrap();
        let assignment = CloudRuntimeAssignment {
            node_uuid: "node-0001".to_owned(),
            environment: "production".to_owned(),
            generation: "7".to_owned(),
            snapshot_uuid: "snapshot-7".to_owned(),
            snapshot_sha256: "a".repeat(64),
        };

        let mut current = Some(RuntimeObservationState::Validated);
        advance_cloud_observation(
            &source,
            &assignment,
            &mut current,
            RuntimeObservationState::Active,
        )
        .await
        .unwrap();
        assert_eq!(current, Some(RuntimeObservationState::Active));
        assert_eq!(observed.lock().unwrap().as_slice(), ["STAGED", "ACTIVE"]);

        observed.lock().unwrap().clear();
        let mut current = Some(RuntimeObservationState::Staged);
        advance_cloud_observation(
            &source,
            &assignment,
            &mut current,
            RuntimeObservationState::Active,
        )
        .await
        .unwrap();
        assert_eq!(observed.lock().unwrap().as_slice(), ["ACTIVE"]);

        observed.lock().unwrap().clear();
        let mut current = Some(RuntimeObservationState::Rejected);
        assert!(matches!(
            advance_cloud_observation(
                &source,
                &assignment,
                &mut current,
                RuntimeObservationState::Active,
            )
            .await,
            Err(WebsiteDataPlaneBootstrapError::RuntimeAssignmentRejected)
        ));
        assert!(observed.lock().unwrap().is_empty());
        server.abort();
    }

    #[tokio::test]
    async fn watcher_retains_last_known_good_and_recovers_on_a_new_generation() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("runtime-set.json");
        std::fs::write(&source, empty_runtime_set(1)).unwrap();
        let (initial, fingerprint) = load_runtime_set(&source).unwrap();
        let recovery_directory = root.path().join("recovery");
        let recovery = WebsiteRuntimeSetRecoveryStore::open(&recovery_directory)
            .unwrap()
            .store;
        let registry = Arc::new(WebsiteRuntimeRegistry::new(
            initial.runtime_set().node_uuid(),
            initial.runtime_set().environment(),
        ));
        registry
            .activate(Arc::clone(initial.runtime_set()))
            .unwrap();
        recovery.persist(&initial).await.unwrap();
        let (stop_tx, stop_rx) = watch::channel(false);
        let watcher_registry = registry.clone();
        let watcher_source = source.clone();
        let watcher = tokio::spawn(async move {
            watch_runtime_set(
                RuntimeSetWatchContext {
                    registry: watcher_registry,
                    path: watcher_source,
                    recovery_store: Some(recovery),
                    providers: Arc::new(WebsiteProviderRegistry::new()),
                    tenant_scope_hash: "1".repeat(64),
                    validation_concurrency: 1,
                    poll_interval: Duration::from_millis(10),
                },
                Some(fingerprint),
                stop_rx,
            )
            .await;
        });

        std::fs::write(&source, b"not-json").unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(registry.current().unwrap().generation(), 1);

        std::fs::write(&source, empty_runtime_set(2)).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if registry.current().unwrap().generation() == 2 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();

        stop_tx.send(true).unwrap();
        watcher.await.unwrap();
        assert_eq!(
            WebsiteRuntimeSetRecoveryStore::open(recovery_directory)
                .unwrap()
                .recovered
                .unwrap()
                .runtime_set()
                .generation(),
            2
        );
    }

    struct FlakyStaticProvider {
        validations: AtomicUsize,
    }

    #[async_trait]
    impl WebsiteResourceProvider for FlakyStaticProvider {
        fn maximum_content_bytes(&self) -> u64 {
            1024
        }

        async fn validate_resource(
            &self,
            request: &ValidateWebsiteResourceRequest,
        ) -> WebsiteProviderResult<ValidatedWebsiteResource> {
            if self.validations.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(WebsiteProviderError::new(
                    WebsiteProviderErrorKind::Unavailable,
                ));
            }
            Ok(ValidatedWebsiteResource {
                provider_resource_uuid: request.provider.provider_resource_uuid.clone(),
                provider_generation: "2".to_owned(),
                public_generation: "2".to_owned(),
                capabilities: request.required_capabilities.clone(),
            })
        }
    }

    #[async_trait]
    impl WebsiteStaticContentProvider for FlakyStaticProvider {
        async fn resolve_static_path(
            &self,
            _request: &ResolveWebsiteStaticPathRequest,
        ) -> WebsiteProviderResult<WebsiteContentResolution> {
            Ok(WebsiteContentResolution::Found(ResolvedWebsiteContent {
                content_handle: WebsiteProviderContentHandle::new("activation-content").unwrap(),
                metadata: WebsiteContentMetadata {
                    content_type: "text/html".to_owned(),
                    content_length: 16,
                    etag: "\"activation\"".to_owned(),
                    last_modified: "2026-07-22T00:00:00Z".to_owned(),
                    content_version: "2".to_owned(),
                    provider_generation: "2".to_owned(),
                    range_supported: true,
                },
            }))
        }

        async fn open_static_content(
            &self,
            _request: &OpenWebsiteContentRequest,
        ) -> WebsiteProviderResult<OpenedWebsiteContent> {
            unreachable!("HEAD activation probes must not open response bodies")
        }
    }

    #[tokio::test]
    async fn watcher_retries_an_unchanged_candidate_after_transient_provider_failure() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("runtime-set.json");
        std::fs::write(&source, empty_runtime_set(1)).unwrap();
        let (initial, fingerprint) = load_runtime_set(&source).unwrap();
        let registry = Arc::new(WebsiteRuntimeRegistry::new(
            initial.runtime_set().node_uuid(),
            initial.runtime_set().environment(),
        ));
        registry
            .activate(Arc::clone(initial.runtime_set()))
            .unwrap();
        let provider = Arc::new(FlakyStaticProvider {
            validations: AtomicUsize::new(0),
        });
        let mut providers = WebsiteProviderRegistry::new();
        providers
            .register_static(WebsiteProviderType::Drive, provider.clone())
            .unwrap();
        let (stop_tx, stop_rx) = watch::channel(false);
        let watcher_registry = Arc::clone(&registry);
        let watcher_source = source.clone();
        let watcher = tokio::spawn(async move {
            watch_runtime_set(
                RuntimeSetWatchContext {
                    registry: watcher_registry,
                    path: watcher_source,
                    recovery_store: None,
                    providers: Arc::new(providers),
                    tenant_scope_hash: "1".repeat(64),
                    validation_concurrency: 1,
                    poll_interval: Duration::from_millis(10),
                },
                Some(fingerprint),
                stop_rx,
            )
            .await;
        });

        std::fs::write(&source, provider_runtime_set(2)).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if registry.current().unwrap().generation() == 2 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();

        stop_tx.send(true).unwrap();
        watcher.await.unwrap();
        assert_eq!(provider.validations.load(Ordering::SeqCst), 2);
    }

    fn empty_runtime_set(generation: u64) -> Vec<u8> {
        runtime_set(
            generation,
            "node-0001",
            WebsiteRuntimeEnvironment::Production,
            "default",
        )
    }

    fn provider_runtime_set(generation: u64) -> Vec<u8> {
        let mut value: Value = serde_json::from_slice(&runtime_set(
            generation,
            "node-0001",
            WebsiteRuntimeEnvironment::Production,
            "provider",
        ))
        .unwrap();
        let mut descriptor = json!({
            "schemaVersion": "sdkwork.website-runtime.v1",
            "kind": "sdkwork.website-runtime.descriptor",
            "revisionUuid": format!("revision-{generation:04}"),
            "siteUuid": "site-provider-retry",
            "tenantScopeHash": "1".repeat(64),
            "environment": "production",
            "generatedAt": "2026-07-22T00:00:00Z",
            "compilerVersion": "sdkwork-deploy-runtime-compiler/1",
            "descriptorSha256": "0".repeat(64),
            "siteDefaultVariantUuid": "variant-default",
            "bindings": [{
                "bindingUuid": "binding-primary",
                "hostname": "retry.example.com",
                "pathPrefix": "/",
                "action": { "type": "SERVE" }
            }],
            "variants": [{
                "variantUuid": "variant-default",
                "label": "Default"
            }],
            "variantRules": [],
            "resources": [{
                "resourceUuid": "resource-drive",
                "provider": {
                    "providerType": "DRIVE",
                    "providerResourceUuid": "website-root-0001",
                    "providerContractVersion": "drive.website-root.v1"
                },
                "capabilities": {
                    "staticContent": true,
                    "wikiRoutes": false,
                    "wikiSearch": false,
                    "rangeRequests": true
                }
            }],
            "mounts": [{
                "mountUuid": "mount-default",
                "variantUuid": "variant-default",
                "pathPrefix": "/",
                "resourceUuid": "resource-drive",
                "handler": "STATIC",
                "translation": { "mode": "ROOT", "resourceSubpath": "/" },
                "indexFiles": ["index.html"]
            }],
            "deliveryPolicy": {
                "providerTimeoutMs": 2500,
                "metadataCacheTtlSeconds": 60,
                "negativeCacheTtlSeconds": 5,
                "staleWhileRevalidateSeconds": 30,
                "maximumObjectBytes": 1024
            },
            "securityPolicy": {
                "forceHttps": true,
                "denyDotFiles": true,
                "deniedPathPrefixes": []
            },
            "limits": {
                "maximumBindings": 8,
                "maximumVariants": 8,
                "maximumVariantRules": 8,
                "maximumResources": 8,
                "maximumMounts": 8,
                "maximumIndexFilesPerMount": 8,
                "maximumPathBytes": 2048,
                "maximumPathSegments": 64
            },
            "observabilityPolicy": {
                "accessLogEnabled": true,
                "usageMeteringEnabled": true,
                "traceSampleRatePerMille": 10
            }
        });
        let descriptor_contract: WebsiteRuntimeDescriptor =
            serde_json::from_value(descriptor.clone()).unwrap();
        descriptor["descriptorSha256"] =
            Value::String(website_runtime_descriptor_sha256(&descriptor_contract).unwrap());
        value["descriptors"] = json!([descriptor]);
        value["snapshotSha256"] = Value::String("0".repeat(64));
        let snapshot: WebsiteRuntimeSetSnapshot = serde_json::from_value(value.clone()).unwrap();
        value["snapshotSha256"] =
            Value::String(website_runtime_set_snapshot_sha256(&snapshot).unwrap());
        serde_json::to_vec(&value).unwrap()
    }

    fn runtime_set(
        generation: u64,
        node_uuid: &str,
        environment: WebsiteRuntimeEnvironment,
        identity: &str,
    ) -> Vec<u8> {
        let environment = match environment {
            WebsiteRuntimeEnvironment::Development => "development",
            WebsiteRuntimeEnvironment::Test => "test",
            WebsiteRuntimeEnvironment::Staging => "staging",
            WebsiteRuntimeEnvironment::Production => "production",
        };
        let mut value = json!({
            "schemaVersion": "sdkwork.website-runtime-set.v1",
            "kind": "sdkwork.website-runtime-set.snapshot",
            "snapshotUuid": format!("snapshot-{generation:04}-{identity}"),
            "nodeUuid": node_uuid,
            "environment": environment,
            "generation": generation,
            "generatedAt": "2026-07-21T00:00:00Z",
            "compilerVersion": "deploy-runtime-set-compiler/1",
            "snapshotSha256": "0".repeat(64),
            "maximumSites": 8,
            "descriptors": []
        });
        let snapshot: WebsiteRuntimeSetSnapshot = serde_json::from_value(value.clone()).unwrap();
        value["snapshotSha256"] =
            Value::String(website_runtime_set_snapshot_sha256(&snapshot).unwrap());
        serde_json::to_vec(&value).unwrap()
    }
}
