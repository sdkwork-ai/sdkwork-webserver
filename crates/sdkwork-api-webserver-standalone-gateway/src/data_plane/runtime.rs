use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use arc_swap::ArcSwap;
use sdkwork_utils_rust::crypto::sha256_hash;
use sdkwork_webserver_core::{
    CertificateConfig, CompiledWebServerApp, CompiledWebServerRevision, ListenerProtocol,
    ReloadConfig, TlsPolicyConfig,
};
use sdkwork_webserver_resolver_cache::{FileResolverSource, ResolutionChain};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, Semaphore};

use crate::metric_dimensions::CanonicalMetricDimensions;

use super::{
    active_health::ActiveHealthSupervisor,
    dns::BoundedSystemResolver,
    limit_conn::LimitConnRuntime,
    limit_req::LimitReqRuntime,
    metrics::{DataPlaneMetrics, ReloadResult},
    proxy::ProxyUpstream,
    request_gate::RequestAdmissionGate,
    resource_pressure::{ResourcePressureController, ResourcePressureSupervisor},
    tunnel::TunnelSupervisor,
    DataPlaneError,
};

const MAX_TLS_MATERIAL_BYTES: u64 = 1024 * 1024;

pub(crate) struct RuntimeGeneration {
    pub id: u64,
    pub revision: String,
    pub app: Arc<CompiledWebServerApp>,
    pub upstreams: HashMap<String, Arc<ProxyUpstream>>,
    /// Shared HTTP response cache for proxied surfaces, when enabled.
    pub proxy_cache: Option<Arc<super::cache::HttpResponseCache>>,
    /// System resolver for dynamic `proxy_pass` targets.
    pub resolver: Arc<BoundedSystemResolver>,
    /// Per-generation cache of synthesized dynamic `proxy_pass` upstreams.
    pub dynamic_upstreams: std::sync::Mutex<HashMap<String, Arc<ProxyUpstream>>>,
    /// Multi-layer resolution cache chain shared by upstream resolvers.
    pub resolution_chain: Option<Arc<ResolutionChain>>,
}

impl RuntimeGeneration {
    fn build(
        app: CompiledWebServerApp,
        revision: String,
        id: u64,
        metrics: Arc<DataPlaneMetrics>,
    ) -> Result<Arc<Self>, DataPlaneError> {
        let app = Arc::new(app);
        let implicit_resolver = BoundedSystemResolver::implicit();
        let resolver = implicit_resolver.clone();
        let resolvers = app
            .config()
            .resolvers
            .iter()
            .map(|resolver| {
                (
                    resolver.id.clone(),
                    BoundedSystemResolver::from_config(resolver),
                )
            })
            .collect::<HashMap<_, _>>();
        let resolution_chain = build_resolution_chain(&app)?;
        let upstreams = app
            .config()
            .upstreams
            .iter()
            .map(|upstream| {
                let resolver = upstream
                    .resolver_ref
                    .as_ref()
                    .map(|resolver_ref| {
                        resolvers
                            .get(resolver_ref)
                            .expect("semantic validation guarantees resolver references")
                    })
                    .unwrap_or(&implicit_resolver)
                    .clone();
                ProxyUpstream::build(
                    &app,
                    upstream,
                    resolver,
                    metrics.clone(),
                    resolution_chain.clone(),
                )
                .map(|runtime| (upstream.id.clone(), Arc::new(runtime)))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let proxy_cache = app.config().proxy_cache.enabled.then(|| {
            super::cache::HttpResponseCache::new(&app.config().proxy_cache, metrics.clone())
        });
        Ok(Arc::new(Self {
            id,
            revision,
            app,
            upstreams,
            proxy_cache,
            resolver,
            dynamic_upstreams: std::sync::Mutex::new(HashMap::new()),
            resolution_chain,
        }))
    }

    pub(crate) fn aggregate_target_health(&self) -> [u64; 4] {
        self.upstreams
            .values()
            .fold([0_u64; 4], |mut total, upstream| {
                let counts = upstream.aggregate_target_health();
                for (target, count) in total.iter_mut().zip(counts) {
                    *target = target.saturating_add(count);
                }
                total
            })
    }

    pub(crate) fn aggregate_upstream_request_capacity(&self) -> [u64; 3] {
        self.upstreams
            .values()
            .fold([0_u64; 3], |mut total, upstream| {
                add_capacity(&mut total, upstream.request_capacity());
                total
            })
    }

    pub(crate) fn aggregate_upstream_connection_capacity(&self) -> [u64; 3] {
        self.upstreams
            .values()
            .fold([0_u64; 3], |mut total, upstream| {
                add_capacity(&mut total, upstream.connection_capacity());
                total
            })
    }

    pub(crate) fn aggregate_upstream_target_connection_capacity(&self) -> [u64; 3] {
        self.upstreams
            .values()
            .fold([0_u64; 3], |mut total, upstream| {
                add_capacity(&mut total, upstream.target_connection_capacity());
                total
            })
    }
}

fn add_capacity(total: &mut [u64; 3], value: [u64; 3]) {
    for (total, value) in total.iter_mut().zip(value) {
        *total = total.saturating_add(value);
    }
}

pub(crate) struct DataPlaneRuntime {
    current: ArcSwap<RuntimeGeneration>,
    topology: ReloadTopology,
    reload_lifecycle: Semaphore,
    active_health: Mutex<ActiveHealthRuntime>,
    resource_pressure_lifecycle: Semaphore,
    resource_pressure_runtime: Mutex<ResourcePressureRuntime>,
    pub connection_permits: Arc<Semaphore>,
    pub request_gate: RequestAdmissionGate,
    pub resource_pressure: Arc<ResourcePressureController>,
    pub limit_req: ArcSwap<LimitReqRuntime>,
    pub limit_conn: ArcSwap<LimitConnRuntime>,
    pub tunnel_supervisor: Arc<TunnelSupervisor>,
    pub metrics: Arc<DataPlaneMetrics>,
}
/// Build the multi-layer resolution cache chain from the app config.
/// Failure to load the configured file layer fails closed; Redis and
/// database layers degrade to a warning (the chain still serves the upper
/// layers).
fn build_resolution_chain(
    app: &sdkwork_webserver_core::CompiledWebServerApp,
) -> Result<Option<Arc<ResolutionChain>>, DataPlaneError> {
    let Some(config) = app.config().resolution_cache.as_ref() else {
        return Ok(None);
    };
    if !config.enabled {
        return Ok(None);
    }
    let file = match config.file.as_deref() {
        Some(path) => Some(Arc::new(
            FileResolverSource::load(std::path::Path::new(path))
                .map_err(|message| DataPlaneError::ResolverCache(message))?,
        )),
        None => None,
    };
    // External Redis (WSL host service) wiring: SDKWORK_WEBSERVER_REDIS_URL
    // overrides the configured resolution-cache Redis URL so compose-provided
    // external Redis is used without editing the app config.
    let redis_config = config.redis.as_ref().map(|redis_config| {
        if let Ok(url) = std::env::var("SDKWORK_WEBSERVER_REDIS_URL") {
            let url = url.trim();
            if !url.is_empty() {
                let mut effective = redis_config.clone();
                effective.url = url.to_owned();
                return effective;
            }
        }
        redis_config.clone()
    });
    let redis = redis_config.as_ref().and_then(|redis_config| {
        match tokio::runtime::Handle::try_current().ok().map(|handle| {
            handle.block_on(
                sdkwork_webserver_resolver_cache::redis::RedisResolverCache::connect(redis_config),
            )
        }) {
            Some(Ok(backend)) => {
                Some(backend as std::sync::Arc<dyn sdkwork_webserver_resolver_cache::ResolverCacheBackend>)
            }
            _ => {
                tracing::warn!(url = %redis_config.url, "Redis resolution cache unavailable; continuing with upper layers");
                None
            }
        }
    });
    #[cfg(feature = "management")]
    let database = if config.database {
        sdkwork_api_webserver_assembly::resolution_cache_from_shared_pool()
    } else {
        None
    };
    #[cfg(not(feature = "management"))]
    let database = None;
    if database.is_none() && config.database {
        tracing::warn!(
            "resolution cache database layer requested but no shared database pool is active"
        );
    }
    tracing::info!(
        file = config.file.as_deref().unwrap_or(""),
        redis = redis.is_some(),
        database = database.is_some(),
        memory = config.memory,
        "resolution cache chain built"
    );
    Ok(Some(Arc::new(ResolutionChain::build(
        config, file, redis, database,
    ))))
}

impl DataPlaneRuntime {
    pub fn build(app: CompiledWebServerApp) -> Result<Arc<Self>, DataPlaneError> {
        let revision = revision_for_compiled_app(&app);
        Self::build_inner(app, revision, CanonicalMetricDimensions::default())
    }

    pub(crate) fn build_with_metric_dimensions(
        app: CompiledWebServerApp,
        dimensions: CanonicalMetricDimensions,
    ) -> Result<Arc<Self>, DataPlaneError> {
        let revision = revision_for_compiled_app(&app);
        Self::build_inner(app, revision, dimensions)
    }

    pub fn build_revision(
        revision: CompiledWebServerRevision,
    ) -> Result<Arc<Self>, DataPlaneError> {
        let sha256 = revision.sha256().to_owned();
        Self::build_inner(
            revision.into_app(),
            sha256,
            CanonicalMetricDimensions::default(),
        )
    }

    pub(crate) fn build_revision_with_metric_dimensions(
        revision: CompiledWebServerRevision,
        dimensions: CanonicalMetricDimensions,
    ) -> Result<Arc<Self>, DataPlaneError> {
        let sha256 = revision.sha256().to_owned();
        Self::build_inner(revision.into_app(), sha256, dimensions)
    }

    fn build_inner(
        app: CompiledWebServerApp,
        revision: String,
        metric_dimensions: CanonicalMetricDimensions,
    ) -> Result<Arc<Self>, DataPlaneError> {
        let topology = ReloadTopology::from_app(&app)?;
        let maximum_connections = app.config().limits.max_connections;
        let maximum_requests = app.config().limits.max_concurrent_requests;
        let operations_reserve = app
            .config()
            .deployment
            .resource_pressure
            .as_ref()
            .map_or(0, |policy| policy.operations_reserve_requests);
        let resource_pressure =
            ResourcePressureController::new(app.config().deployment.resource_pressure.is_some());
        let metrics = DataPlaneMetrics::new(metric_dimensions);
        let tunnel_supervisor = TunnelSupervisor::new(
            Duration::from_millis(app.config().limits.max_connection_age_ms),
            metrics.clone(),
        );
        let initial = RuntimeGeneration::build(app, revision, 1, metrics.clone())?;
        let limit_req = ArcSwap::from_pointee(LimitReqRuntime::from_zones(
            &initial.app.config().limit_req_zones,
        ));
        let limit_conn = ArcSwap::from_pointee(LimitConnRuntime::from_zones(
            &initial.app.config().limit_conn_zones,
        ));
        Ok(Arc::new(Self {
            current: ArcSwap::from(initial),
            topology,
            reload_lifecycle: Semaphore::new(1),
            active_health: Mutex::new(ActiveHealthRuntime::default()),
            resource_pressure_lifecycle: Semaphore::new(1),
            resource_pressure_runtime: Mutex::new(ResourcePressureRuntime::default()),
            connection_permits: Arc::new(Semaphore::new(maximum_connections)),
            request_gate: RequestAdmissionGate::new(
                maximum_requests,
                operations_reserve,
                resource_pressure.clone(),
            ),
            resource_pressure,
            limit_req,
            limit_conn,
            tunnel_supervisor,
            metrics,
        }))
    }

    pub fn current(&self) -> Arc<RuntimeGeneration> {
        self.current.load_full()
    }

    pub(crate) async fn start_active_health(&self) -> Result<(), DataPlaneError> {
        let _reload_lifecycle = self.reload_lifecycle.acquire().await.map_err(|_| {
            DataPlaneError::LifecycleCoordinatorUnavailable {
                subsystem: "data-plane reload",
            }
        })?;
        let mut active_health = self.active_health.lock().await;
        if active_health.started {
            return Ok(());
        }
        active_health.supervisor = ActiveHealthSupervisor::start(self.current());
        active_health.started = true;
        Ok(())
    }

    pub(crate) async fn start_resource_pressure(&self) -> Result<(), DataPlaneError> {
        let _lifecycle = self
            .resource_pressure_lifecycle
            .acquire()
            .await
            .map_err(|_| DataPlaneError::LifecycleCoordinatorUnavailable {
                subsystem: "resource pressure",
            })?;
        {
            let runtime = self.resource_pressure_runtime.lock().await;
            if runtime.started {
                return Ok(());
            }
        }
        let policy = self
            .current()
            .app
            .config()
            .deployment
            .resource_pressure
            .clone();
        let supervisor = match policy {
            Some(policy) => Some(
                ResourcePressureSupervisor::start(self.resource_pressure.clone(), policy).await?,
            ),
            None => None,
        };
        let mut runtime = self.resource_pressure_runtime.lock().await;
        runtime.supervisor = supervisor;
        runtime.started = true;
        Ok(())
    }

    pub(crate) async fn stop_resource_pressure(&self) -> Result<(), DataPlaneError> {
        let _lifecycle = self
            .resource_pressure_lifecycle
            .acquire()
            .await
            .map_err(|_| DataPlaneError::LifecycleCoordinatorUnavailable {
                subsystem: "resource pressure",
            })?;
        let supervisor = {
            let mut runtime = self.resource_pressure_runtime.lock().await;
            runtime.started = false;
            runtime.supervisor.take()
        };
        match supervisor {
            Some(supervisor) => supervisor
                .stop()
                .await
                .map_err(DataPlaneError::ResourcePressureTask),
            None => Ok(()),
        }
    }

    pub(crate) async fn stop_active_health(&self) -> Result<(), DataPlaneError> {
        let _reload_lifecycle = self.reload_lifecycle.acquire().await.map_err(|_| {
            DataPlaneError::LifecycleCoordinatorUnavailable {
                subsystem: "data-plane reload",
            }
        })?;
        let supervisor = {
            let mut active_health = self.active_health.lock().await;
            active_health.started = false;
            active_health.supervisor.take()
        };
        match supervisor {
            Some(supervisor) => supervisor
                .stop()
                .await
                .map_err(DataPlaneError::ActiveHealthTask),
            None => Ok(()),
        }
    }

    pub async fn reload(
        &self,
        revision: CompiledWebServerRevision,
    ) -> Result<DataPlaneReloadReport, DataPlaneError> {
        let _reload_lifecycle = self.reload_lifecycle.acquire().await.map_err(|_| {
            DataPlaneError::LifecycleCoordinatorUnavailable {
                subsystem: "data-plane reload",
            }
        })?;
        let candidate_topology = match ReloadTopology::from_app(revision.app()) {
            Ok(topology) => topology,
            Err(error) => {
                self.metrics.record_reload(ReloadResult::Failed);
                return Err(error);
            }
        };
        if candidate_topology != self.topology {
            self.metrics.record_reload(ReloadResult::RestartRequired);
            return Err(DataPlaneError::ReloadRequiresRestart);
        }

        let current = self.current();
        if current.revision == revision.sha256() {
            self.metrics.record_reload(ReloadResult::Unchanged);
            return Ok(DataPlaneReloadReport {
                generation: current.id,
                previous_revision: current.revision.clone(),
                revision: current.revision.clone(),
                changed: false,
            });
        }

        let generation = current.id.saturating_add(1);
        let next_revision = revision.sha256().to_owned();
        let candidate = match RuntimeGeneration::build(
            revision.into_app(),
            next_revision.clone(),
            generation,
            self.metrics.clone(),
        ) {
            Ok(candidate) => candidate,
            Err(error) => {
                self.metrics.record_reload(ReloadResult::Failed);
                return Err(error);
            }
        };
        let previous_supervisor = {
            let mut active_health = self.active_health.lock().await;
            let next_supervisor = active_health
                .started
                .then(|| ActiveHealthSupervisor::start(candidate.clone()))
                .flatten();
            self.current.store(candidate.clone());
            self.limit_req.store(Arc::new(LimitReqRuntime::from_zones(
                &candidate.app.config().limit_req_zones,
            )));
            self.limit_conn.store(Arc::new(LimitConnRuntime::from_zones(
                &candidate.app.config().limit_conn_zones,
            )));
            std::mem::replace(&mut active_health.supervisor, next_supervisor)
        };
        if let Some(supervisor) = previous_supervisor {
            if let Err(error) = supervisor.stop().await {
                tracing::warn!(%error, "previous active health generation failed while stopping");
            }
        }
        self.metrics.record_reload(ReloadResult::Published);
        Ok(DataPlaneReloadReport {
            generation,
            previous_revision: current.revision.clone(),
            revision: next_revision,
            changed: true,
        })
    }
}

#[derive(Default)]
struct ActiveHealthRuntime {
    started: bool,
    supervisor: Option<ActiveHealthSupervisor>,
}

#[derive(Default)]
struct ResourcePressureRuntime {
    started: bool,
    supervisor: Option<ResourcePressureSupervisor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataPlaneReloadReport {
    pub generation: u64,
    pub previous_revision: String,
    pub revision: String,
    pub changed: bool,
}

#[derive(PartialEq, Eq)]
struct ReloadTopology {
    app_key: String,
    maximum_connections: usize,
    maximum_requests: usize,
    request_timeout_ms: u64,
    request_body_start_timeout_ms: u64,
    request_body_idle_timeout_ms: u64,
    response_body_idle_timeout_ms: u64,
    connection_write_timeout_ms: u64,
    http1_keep_alive_idle_timeout_ms: u64,
    max_connection_age_ms: u64,
    drain_timeout_ms: u64,
    reload: ReloadConfig,
    resource_pressure: Option<sdkwork_webserver_core::ResourcePressureConfig>,
    protocol_limits: HttpProtocolTopology,
    listeners: Vec<ListenerTopology>,
    streams: Vec<StreamTopology>,
}

/// The reload-fixed portion of one `[[stream.server]]`: a stream listener set
/// change requires a restart (same contract as HTTP listeners).
#[derive(PartialEq, Eq)]
struct StreamTopology {
    id: String,
    bind: String,
    port: u16,
}

impl ReloadTopology {
    fn from_app(app: &CompiledWebServerApp) -> Result<Self, DataPlaneError> {
        let mut listeners = app
            .listeners()
            .map(|listener| {
                let tls = listener
                    .tls_policy_ref
                    .as_deref()
                    .map(|policy_id| tls_topology(app, policy_id))
                    .transpose()?;
                Ok(ListenerTopology {
                    id: listener.id.clone(),
                    bind: listener.bind.clone(),
                    port: listener.port,
                    protocols: listener.protocols.clone(),
                    maximum_connections: listener
                        .max_connections
                        .unwrap_or(app.config().limits.max_connections),
                    proxy_protocol: listener.proxy_protocol.clone(),
                    tls,
                })
            })
            .collect::<Result<Vec<_>, DataPlaneError>>()?;
        listeners.sort_by(|left, right| left.id.cmp(&right.id));
        let mut streams = app
            .streams()
            .iter()
            .map(|stream| StreamTopology {
                id: stream.id.clone(),
                bind: stream.bind.clone(),
                port: stream.port,
            })
            .collect::<Vec<_>>();
        streams.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(Self {
            app_key: app.config().app_key.clone(),
            maximum_connections: app.config().limits.max_connections,
            maximum_requests: app.config().limits.max_concurrent_requests,
            request_timeout_ms: app.config().limits.request_timeout_ms,
            request_body_start_timeout_ms: app.config().limits.request_body_start_timeout_ms,
            request_body_idle_timeout_ms: app.config().limits.request_body_idle_timeout_ms,
            response_body_idle_timeout_ms: app.config().limits.response_body_idle_timeout_ms,
            connection_write_timeout_ms: app.config().limits.connection_write_timeout_ms,
            http1_keep_alive_idle_timeout_ms: app.config().limits.http1_keep_alive_idle_timeout_ms,
            max_connection_age_ms: app.config().limits.max_connection_age_ms,
            drain_timeout_ms: app
                .config()
                .deployment
                .drain_timeout_ms
                .unwrap_or(app.config().limits.drain_timeout_ms),
            reload: app.config().deployment.reload.clone(),
            resource_pressure: app.config().deployment.resource_pressure.clone(),
            protocol_limits: HttpProtocolTopology::from_app(app),
            listeners,
            streams,
        })
    }
}

#[derive(PartialEq, Eq)]
struct HttpProtocolTopology {
    http1_max_pipeline_depth: usize,
    max_request_header_bytes: usize,
    max_request_line_bytes: usize,
    max_request_method_bytes: usize,
    max_request_target_bytes: usize,
    max_header_name_bytes: usize,
    max_header_value_bytes: usize,
    max_request_headers: usize,
    request_header_timeout_ms: u64,
    max_chunk_line_bytes: usize,
    max_trailer_bytes: usize,
    max_trailers: usize,
    http2_max_concurrent_streams: u32,
    http2_keep_alive_interval_ms: u64,
    http2_keep_alive_timeout_ms: u64,
    http2_max_pending_accept_reset_streams: usize,
    http2_max_local_error_reset_streams: usize,
    http2_max_send_buffer_bytes: usize,
    http2_max_header_list_bytes: u32,
    http2_max_frame_bytes: u32,
    http2_abuse_window_ms: u64,
    http2_max_frames_per_window: usize,
    http2_max_new_streams_per_window: usize,
    http2_max_reset_frames_per_window: usize,
    http2_max_continuation_frames: usize,
    http2_max_encoded_header_block_bytes: usize,
}

impl HttpProtocolTopology {
    fn from_app(app: &CompiledWebServerApp) -> Self {
        let limits = &app.config().limits;
        Self {
            http1_max_pipeline_depth: limits.http1_max_pipeline_depth,
            max_request_header_bytes: limits.max_request_header_bytes,
            max_request_line_bytes: limits.max_request_line_bytes,
            max_request_method_bytes: limits.max_request_method_bytes,
            max_request_target_bytes: limits.max_request_target_bytes,
            max_header_name_bytes: limits.max_header_name_bytes,
            max_header_value_bytes: limits.max_header_value_bytes,
            max_request_headers: limits.max_request_headers,
            request_header_timeout_ms: limits.request_header_timeout_ms,
            max_chunk_line_bytes: limits.max_chunk_line_bytes,
            max_trailer_bytes: limits.max_trailer_bytes,
            max_trailers: limits.max_trailers,
            http2_max_concurrent_streams: limits.http2_max_concurrent_streams,
            http2_keep_alive_interval_ms: limits.http2_keep_alive_interval_ms,
            http2_keep_alive_timeout_ms: limits.http2_keep_alive_timeout_ms,
            http2_max_pending_accept_reset_streams: limits.http2_max_pending_accept_reset_streams,
            http2_max_local_error_reset_streams: limits.http2_max_local_error_reset_streams,
            http2_max_send_buffer_bytes: limits.http2_max_send_buffer_bytes,
            http2_max_header_list_bytes: limits.http2_max_header_list_bytes,
            http2_max_frame_bytes: limits.http2_max_frame_bytes,
            http2_abuse_window_ms: limits.http2_abuse_window_ms,
            http2_max_frames_per_window: limits.http2_max_frames_per_window,
            http2_max_new_streams_per_window: limits.http2_max_new_streams_per_window,
            http2_max_reset_frames_per_window: limits.http2_max_reset_frames_per_window,
            http2_max_continuation_frames: limits.http2_max_continuation_frames,
            http2_max_encoded_header_block_bytes: limits.http2_max_encoded_header_block_bytes,
        }
    }
}

#[derive(PartialEq, Eq)]
struct ListenerTopology {
    id: String,
    bind: String,
    port: u16,
    protocols: Vec<ListenerProtocol>,
    maximum_connections: usize,
    proxy_protocol: Option<sdkwork_webserver_core::ProxyProtocolConfig>,
    tls: Option<TlsTopology>,
}

#[derive(PartialEq, Eq)]
struct TlsTopology {
    policy: TlsPolicyConfig,
    certificates: Vec<TlsCertificateTopology>,
}

#[derive(PartialEq, Eq)]
struct TlsCertificateTopology {
    certificate: CertificateConfig,
    certificate_path: PathBuf,
    certificate_sha256: [u8; 32],
    private_key_path: PathBuf,
    private_key_sha256: [u8; 32],
}

fn tls_topology(
    app: &CompiledWebServerApp,
    policy_id: &str,
) -> Result<TlsTopology, DataPlaneError> {
    let policy = app
        .tls_policy(policy_id)
        .expect("semantic validation guarantees TLS policy references");
    let certificates = policy
        .certificate_refs()
        .map(|certificate_ref| tls_certificate_topology(app, certificate_ref))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(TlsTopology {
        policy: policy.clone(),
        certificates,
    })
}

fn tls_certificate_topology(
    app: &CompiledWebServerApp,
    certificate_ref: &str,
) -> Result<TlsCertificateTopology, DataPlaneError> {
    let certificate = app
        .certificate(certificate_ref)
        .expect("semantic validation guarantees certificate references");
    let (certificate_path, private_key_path) = app
        .certificate_paths(&certificate.id)
        .expect("compilation resolves certificate paths");
    Ok(TlsCertificateTopology {
        certificate: certificate.clone(),
        certificate_path: certificate_path.to_path_buf(),
        certificate_sha256: bounded_file_sha256(certificate_path)?,
        private_key_path: private_key_path.to_path_buf(),
        private_key_sha256: bounded_file_sha256(private_key_path)?,
    })
}

pub(crate) fn read_bounded_tls_material(path: &Path) -> Result<Vec<u8>, DataPlaneError> {
    let mut file = File::open(path).map_err(|source| DataPlaneError::TlsMaterialRead {
        path: path.to_path_buf(),
        source,
    })?;
    let mut bytes = Vec::with_capacity(16 * 1024);
    file.by_ref()
        .take(MAX_TLS_MATERIAL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| DataPlaneError::TlsMaterialRead {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() as u64 > MAX_TLS_MATERIAL_BYTES {
        return Err(DataPlaneError::TlsMaterialTooLarge {
            path: path.to_path_buf(),
            actual_bytes: bytes.len() as u64,
            maximum_bytes: MAX_TLS_MATERIAL_BYTES,
        });
    }
    Ok(bytes)
}

fn bounded_file_sha256(path: &Path) -> Result<[u8; 32], DataPlaneError> {
    Ok(Sha256::digest(read_bounded_tls_material(path)?).into())
}

fn revision_for_compiled_app(app: &CompiledWebServerApp) -> String {
    let bytes = serde_json::to_vec(app.config()).expect("Web Server config always serializes");
    format!("runtime:{}", sha256_hash(&bytes))
}
