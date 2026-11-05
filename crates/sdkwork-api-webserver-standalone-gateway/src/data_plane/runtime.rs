use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, OnceLock,
    },
    time::Duration,
};

use arc_swap::ArcSwap;
use axum_server::tls_rustls::RustlsConfig;
use sdkwork_utils_rust::crypto::sha256_hash;
use sdkwork_webserver_core::{CompiledWebServerApp, CompiledWebServerRevision, GeoIpDatabases};
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

/// Builds the tunnel gateway shared state when the app config enables the
/// tunnel; `Ok(None)` leaves the data plane unchanged. A tunnel that is enabled
/// but misconfigured (invalid address, unreadable/missing TLS material) is an
/// error: silently disabling it or downgrading to self-signed would hide a
/// provisioning failure behind an availability or trust incident.
fn build_tunnel(
    config: &sdkwork_webserver_core::WebServerAppConfig,
) -> Result<
    (
        Option<Arc<sdkwork_webserver_tunnel::gateway::GatewayShared>>,
        Option<sdkwork_webserver_tunnel::TunnelGatewayOptions>,
    ),
    DataPlaneError,
> {
    let Some(tunnel_config) = config.tunnel.as_ref() else {
        return Ok((None, None));
    };
    if !tunnel_config.enabled {
        return Ok((None, None));
    }
    tunnel_config
        .validate()
        .map_err(|error| DataPlaneError::TunnelConfiguration(error.to_string()))?;
    let tunnel_metrics = Arc::new(sdkwork_webserver_tunnel::TunnelMetrics::new());
    let Some(options) = crate::tunnel_bridge::build_gateway_options(tunnel_config, tunnel_metrics)
        .map_err(DataPlaneError::TunnelConfiguration)?
    else {
        return Ok((None, None));
    };
    let shared = sdkwork_webserver_tunnel::gateway::GatewayShared::from_options(&options);
    Ok((Some(shared), Some(options)))
}

pub(crate) struct RuntimeGeneration {
    pub id: u64,
    pub revision: String,
    pub app: Arc<CompiledWebServerApp>,
    pub upstreams: HashMap<String, Arc<ProxyUpstream>>,
    /// Shared HTTP response cache for proxied surfaces, when enabled.
    pub proxy_cache: Option<Arc<super::cache::HttpResponseCache>>,
    /// System resolver for dynamic `proxy_pass` targets.
    pub resolver: Arc<BoundedSystemResolver>,
    /// Per-generation cache of synthesized dynamic `proxy_pass` upstreams,
    /// LRU-ordered so eviction targets the least recently used authority.
    pub dynamic_upstreams: std::sync::Mutex<hashlink::LinkedHashMap<String, Arc<ProxyUpstream>>>,
    /// Multi-layer resolution cache chain shared by upstream resolvers.
    pub resolution_chain: Option<Arc<ResolutionChain>>,
    /// Admission tables for this exact generation. Keeping them inside the
    /// generation makes a request that pinned one generation use that
    /// generation's zones and limits, instead of a table swapped mid-request.
    pub limit_req: LimitReqRuntime,
    pub limit_conn: LimitConnRuntime,
    /// Bounded idle-connection pool for the CGI family (`fastcgi_keep_conn`).
    /// It belongs to the generation, so a reload drops every idle socket.
    pub protocol_pool: Arc<super::protocol_pool::ProtocolPool>,
    /// Stream `limit_conn_zone` counters (separate zone table from HTTP).
    pub stream_limit_conn: LimitConnRuntime,
    /// Per-key request counters for nginx CGI `*_cache_min_uses`.
    pub cgi_cache_min_uses: super::cgi_cache::CgiMinUsesCounters,
    /// nginx GeoIP MMDB readers and `geoip_proxy` trust policy, built once per
    /// generation (never reopened per request).
    pub geoip: Option<Arc<GeoIpDatabases>>,
    /// nginx **stream** GeoIP MMDB readers (accept-time lookup).
    pub stream_geoip: Option<Arc<GeoIpDatabases>>,
    /// Bounded CGI-family temp-spool tier for this generation. It belongs to
    /// the generation, so a reload starts a fresh budget and a per-generation
    /// directory; every spool file is removed on completion/error/drop.
    pub temp_spool: Arc<super::temp_spool::TempSpool>,
    /// The listening-socket set this generation declares. The supervisor diffs
    /// it against the live socket set at handoff time.
    pub sockets: Vec<SocketTopology>,
    /// Per-listener static TLS acceptor material, built once per generation so
    /// a kept socket serves new connections with the new certificates without a
    /// rebind. Listeners owned by a dynamic TLS runtime controller map to
    /// `None` here; the controller supplies their configuration.
    pub tls_configs: HashMap<String, Option<RustlsConfig>>,
    /// Per-generation resolved mail session profiles (bounded DNS + TLS
    /// material load on first use).
    pub mail_profiles: super::mail_frontend::MailProfileCache,
    /// Per-stream TLS terminate acceptors, built once per generation so a kept
    /// socket serves new connections with the new certificates after handoff.
    pub stream_tls_acceptors: HashMap<String, Option<tokio_rustls::TlsAcceptor>>,
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
        let limit_req = LimitReqRuntime::from_zones(&app.config().limit_req_zones);
        let limit_conn = LimitConnRuntime::from_zones(&app.config().limit_conn_zones);
        let stream_limit_conn = LimitConnRuntime::from_zones(&app.config().stream_limit_conn_zones);
        let geoip = app
            .config()
            .geoip
            .as_ref()
            .map(|config| {
                GeoIpDatabases::load(config, app.config().nginx.geoip_database_root.as_deref())
                    .map(Arc::new)
            })
            .transpose()?;
        let stream_geoip = app
            .config()
            .stream_geoip
            .as_ref()
            .map(|config| {
                GeoIpDatabases::load(config, app.config().nginx.geoip_database_root.as_deref())
                    .map(Arc::new)
            })
            .transpose()?;
        let temp_spool = Arc::new(super::temp_spool::TempSpool::new(
            app.config().nginx.temp_root.as_deref(),
            id,
            app.config().nginx.temp_spool_budget_bytes,
            app.config().nginx.temp_spool_max_files,
        ));
        // Static TLS acceptor material per listener, built once per generation.
        // A certificate file that is unreadable/malformed fails the generation
        // build, so a reload keeps the previous generation (fail closed).
        let mut tls_configs = HashMap::new();
        for listener in app.listeners() {
            let tls = if listener.tls_runtime.is_some() {
                // Dynamic assignment listeners are owned by the process-lifetime
                // TLS runtime controller; the supervisor wires them at bind time.
                None
            } else {
                super::tls::build_tls_config(app.as_ref(), listener)?
            };
            tls_configs.insert(listener.id.clone(), tls);
        }
        let sockets = ReloadTopology::socket_set(&app);
        // Stream TLS terminate acceptors, built once per generation. A bad
        // certificate fails the generation build (fail closed, like HTTP TLS).
        let mut stream_tls_acceptors = HashMap::new();
        for stream in app.streams() {
            let acceptor = match stream.tls.as_ref() {
                Some(sdkwork_webserver_core::StreamTlsMode::Terminate {
                    certificate_ref,
                    client_auth,
                }) => {
                    let resolved = super::stream_proxy::resolve_stream_client_auth(
                        app.as_ref(),
                        &stream.id,
                        client_auth.as_ref(),
                    );
                    Some(
                        super::stream_proxy::build_stream_tls_acceptor(
                            app.as_ref(),
                            certificate_ref,
                            resolved.as_ref(),
                        )
                        .map_err(|detail| {
                            DataPlaneError::StreamTlsMaterial {
                                stream_id: stream.id.clone(),
                                detail,
                            }
                        })?,
                    )
                }
                _ => None,
            };
            stream_tls_acceptors.insert(stream.id.clone(), acceptor);
        }
        Ok(Arc::new(Self {
            id,
            revision,
            app,
            upstreams,
            proxy_cache,
            resolver,
            dynamic_upstreams: std::sync::Mutex::new(hashlink::LinkedHashMap::new()),
            resolution_chain,
            limit_req,
            limit_conn,
            protocol_pool: Arc::new(super::protocol_pool::ProtocolPool::new()),
            stream_limit_conn,
            cgi_cache_min_uses: super::cgi_cache::CgiMinUsesCounters::default(),
            geoip,
            stream_geoip,
            temp_spool,
            sockets,
            tls_configs,
            mail_profiles: super::mail_frontend::MailProfileCache::default(),
            stream_tls_acceptors,
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
    topology: Mutex<ReloadTopology>,
    reload_lifecycle: Semaphore,
    active_health: Mutex<ActiveHealthRuntime>,
    resource_pressure_lifecycle: Semaphore,
    resource_pressure_runtime: Mutex<ResourcePressureRuntime>,
    pub connection_permits: Arc<Semaphore>,
    /// Live capacity of `connection_permits`; the supervisor reconciles the
    /// semaphore to the published generation's `max_connections` on handoff.
    connection_permits_capacity: AtomicUsize,
    /// The socket supervisor, installed by the serving loop once the initial
    /// listeners are bound. A runtime that never served (unit tests calling
    /// `reload` directly) has none, and a socket-set change stays
    /// restart-required for it.
    listener_supervisor: OnceLock<Arc<super::server::ListenerSupervisor>>,
    pub request_gate: RequestAdmissionGate,
    pub resource_pressure: Arc<ResourcePressureController>,
    pub tunnel_supervisor: Arc<TunnelSupervisor>,
    /// Live tunnel gateway state when the app config enables the tunnel
    /// capability; `None` keeps the runtime unchanged (PRD SS77).
    pub tunnel: Option<Arc<sdkwork_webserver_tunnel::gateway::GatewayShared>>,
    /// Resolved tunnel gateway options for the QUIC listener spawn; present
    /// exactly when `tunnel` is present.
    pub tunnel_options: Option<sdkwork_webserver_tunnel::TunnelGatewayOptions>,
    /// Cluster auto-routing overlay: an atomically swapped topology handle.
    /// Empty until a discovery refresher publishes a snapshot; readers never
    /// block. Cluster hosts not present in the snapshot fall through to
    /// normal virtual-host routing (PRD §77 compatibility).
    pub cluster_overlay: Arc<arc_swap::ArcSwap<sdkwork_webserver_cluster::ClusterTopology>>,
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
                .map_err(DataPlaneError::ResolverCache)?,
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
    let redis = redis_config.as_ref().map(|redis_config| {
        // The backend connects lazily on first async use with bounded
        // timeouts, so generation builds never block an executor worker on
        // Redis availability (a stalled Redis degrades to the upper layers).
        let backend: std::sync::Arc<dyn sdkwork_webserver_resolver_cache::ResolverCacheBackend> =
            std::sync::Arc::new(
                sdkwork_webserver_resolver_cache::redis::RedisResolverCache::new(
                    redis_config.clone(),
                ),
            );
        backend
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
        Self::build_inner(app, revision, CanonicalMetricDimensions::default(), None)
    }

    pub(crate) fn build_with_metric_dimensions(
        app: CompiledWebServerApp,
        dimensions: CanonicalMetricDimensions,
    ) -> Result<Arc<Self>, DataPlaneError> {
        let revision = revision_for_compiled_app(&app);
        Self::build_inner(app, revision, dimensions, None)
    }

    pub fn build_revision(
        revision: CompiledWebServerRevision,
    ) -> Result<Arc<Self>, DataPlaneError> {
        let sha256 = revision.sha256().to_owned();
        Self::build_inner(
            revision.into_app(),
            sha256,
            CanonicalMetricDimensions::default(),
            None,
        )
    }

    pub(crate) fn build_revision_with_metric_dimensions(
        revision: CompiledWebServerRevision,
        dimensions: CanonicalMetricDimensions,
    ) -> Result<Arc<Self>, DataPlaneError> {
        let sha256 = revision.sha256().to_owned();
        Self::build_inner(revision.into_app(), sha256, dimensions, None)
    }

    fn build_inner(
        app: CompiledWebServerApp,
        revision: String,
        metric_dimensions: CanonicalMetricDimensions,
        cluster_overlay: Option<Arc<arc_swap::ArcSwap<sdkwork_webserver_cluster::ClusterTopology>>>,
    ) -> Result<Arc<Self>, DataPlaneError> {
        let topology = ReloadTopology::from_app(&app);
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
        let (tunnel, tunnel_options) = build_tunnel(app.config())?;
        let initial = RuntimeGeneration::build(app, revision, 1, metrics.clone())?;
        Ok(Arc::new(Self {
            current: ArcSwap::from(initial),
            topology: Mutex::new(topology),
            reload_lifecycle: Semaphore::new(1),
            active_health: Mutex::new(ActiveHealthRuntime::default()),
            resource_pressure_lifecycle: Semaphore::new(1),
            resource_pressure_runtime: Mutex::new(ResourcePressureRuntime::default()),
            connection_permits: Arc::new(Semaphore::new(maximum_connections)),
            connection_permits_capacity: AtomicUsize::new(maximum_connections),
            listener_supervisor: OnceLock::new(),
            request_gate: RequestAdmissionGate::new(
                maximum_requests,
                operations_reserve,
                resource_pressure.clone(),
            ),
            resource_pressure,
            tunnel_supervisor,
            tunnel,
            tunnel_options,
            cluster_overlay: cluster_overlay.unwrap_or_else(|| {
                Arc::new(arc_swap::ArcSwap::from_pointee(
                    sdkwork_webserver_cluster::ClusterTopology::default(),
                ))
            }),
            metrics,
        }))
    }

    pub fn current(&self) -> Arc<RuntimeGeneration> {
        self.current.load_full()
    }

    /// Builds the runtime with a caller-owned topology handle: the
    /// discovery refresher publishes snapshots into it while the data plane
    /// routes by them.
    pub fn build_with_cluster_overlay(
        app: CompiledWebServerApp,
        overlay: Arc<arc_swap::ArcSwap<sdkwork_webserver_cluster::ClusterTopology>>,
    ) -> Result<Arc<Self>, DataPlaneError> {
        let revision = revision_for_compiled_app(&app);
        let runtime = Self::build_inner(
            app,
            revision,
            CanonicalMetricDimensions::default(),
            Some(overlay),
        )?;
        Ok(runtime)
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

    /// Installs the socket supervisor. Called once by the serving loop after
    /// the initial listeners are bound; later calls fail closed.
    pub(crate) fn install_listener_supervisor(
        &self,
        supervisor: Arc<super::server::ListenerSupervisor>,
    ) -> Result<(), DataPlaneError> {
        self.listener_supervisor.set(supervisor).map_err(|_| {
            DataPlaneError::LifecycleCoordinatorUnavailable {
                subsystem: "data-plane listener supervisor is already installed",
            }
        })
    }

    fn supervisor(&self) -> Option<Arc<super::server::ListenerSupervisor>> {
        self.listener_supervisor.get().cloned()
    }

    /// Reconciles the global connection-admission semaphore to the published
    /// generation's `max_connections` (limit changes hot-apply). A shrinking
    /// capacity only drops currently-available permits; the remainder
    /// converges as in-flight connections release theirs.
    fn reconcile_connection_capacity(&self, maximum_connections: usize) {
        let current = self
            .connection_permits_capacity
            .swap(maximum_connections, Ordering::AcqRel);
        match maximum_connections.cmp(&current) {
            std::cmp::Ordering::Greater => {
                self.connection_permits
                    .add_permits(maximum_connections - current);
            }
            std::cmp::Ordering::Less => {
                let available = self.connection_permits.available_permits();
                let drop = (current - maximum_connections).min(available);
                if drop > 0 {
                    self.connection_permits.forget_permits(drop);
                }
            }
            std::cmp::Ordering::Equal => {}
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
        let candidate_topology = ReloadTopology::from_app(revision.app());
        let live_topology = self.topology.lock().await.clone();
        let sockets_changed = candidate_topology.sockets != live_topology.sockets;
        if candidate_topology != live_topology {
            // A socket-set change can only hand off when a supervisor owns the
            // live sockets; process-built subsystems (tunnel gateway, usage
            // metering channel) cannot hot-apply at all.
            let handoff_possible = sockets_changed && self.supervisor().is_some();
            if !handoff_possible {
                self.metrics.record_reload(ReloadResult::RestartRequired);
                return Err(DataPlaneError::ReloadRequiresRestart);
            }
        }

        let current = self.current();
        if current.revision == revision.sha256() {
            self.metrics.record_reload(ReloadResult::Unchanged);
            return Ok(DataPlaneReloadReport {
                generation: current.id,
                previous_revision: current.revision.clone(),
                revision: current.revision.clone(),
                changed: false,
                sockets_changed: false,
            });
        }

        let generation = current.id.saturating_add(1);
        let next_revision = revision.sha256().to_owned();
        // Building a generation reads every certificate/private-key file and
        // serializes the compiled configuration. That is blocking filesystem
        // work; running it inline parks a runtime worker (and a stalled secret
        // volume can consume the pool), so it runs on the blocking pool.
        let build_app = revision.into_app();
        let build_revision = next_revision.clone();
        let build_metrics = self.metrics.clone();
        let candidate = match tokio::task::spawn_blocking(move || {
            RuntimeGeneration::build(build_app, build_revision, generation, build_metrics)
        })
        .await
        {
            Ok(Ok(candidate)) => candidate,
            Ok(Err(error)) => {
                self.metrics.record_reload(ReloadResult::Failed);
                return Err(error);
            }
            Err(error) => {
                self.metrics.record_reload(ReloadResult::Failed);
                return Err(DataPlaneError::ReloadWorker(error));
            }
        };

        // nginx SIGHUP ordering: bind NEW listening sockets before the new
        // configuration takes effect, so a bind failure retains the old
        // generation and nothing is published half-bound.
        let supervisor = self.supervisor();
        if let Some(supervisor) = supervisor.as_ref() {
            if sockets_changed {
                if let Err(error) = supervisor.prepare_sockets(&candidate).await {
                    self.metrics.record_reload(ReloadResult::Failed);
                    return Err(error);
                }
            }
        }

        let previous_supervisor = {
            let mut active_health = self.active_health.lock().await;
            let next_supervisor = active_health
                .started
                .then(|| ActiveHealthSupervisor::start(candidate.clone()))
                .flatten();
            self.current.store(candidate.clone());
            std::mem::replace(&mut active_health.supervisor, next_supervisor)
        };
        if let Some(supervisor) = previous_supervisor {
            if let Err(error) = supervisor.stop().await {
                tracing::warn!(%error, "previous active health generation failed while stopping");
            }
        }
        if let Some(supervisor) = supervisor.as_ref() {
            if sockets_changed {
                // After publication: serve the new sockets, stop accepting on
                // the removed ones (they drain their in-flight connections
                // detached), and reconcile admission capacity.
                supervisor.commit_sockets(&candidate).await;
            }
        }
        let limits = candidate.app.config().limits.clone();
        self.reconcile_connection_capacity(limits.max_connections);
        let operations_reserve = candidate
            .app
            .config()
            .deployment
            .resource_pressure
            .as_ref()
            .map_or(0, |policy| policy.operations_reserve_requests);
        self.request_gate
            .resize(limits.max_concurrent_requests, operations_reserve);
        *self.topology.lock().await = candidate_topology;
        self.metrics.record_reload(if sockets_changed {
            ReloadResult::HandoffPublished
        } else {
            ReloadResult::Published
        });
        Ok(DataPlaneReloadReport {
            generation,
            previous_revision: current.revision.clone(),
            revision: next_revision,
            changed: true,
            sockets_changed,
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
    /// True when the published generation changed the listening-socket set:
    /// added sockets bound before publication, removed sockets drain after.
    pub sockets_changed: bool,
}

/// The restart-required identity of a configuration, shrunk to what genuinely
/// cannot hot-apply:
///
/// - the listening-socket set (a socket can only change by binding a new one
///   or closing an old one — the supervisor hands those off instead), and
/// - subsystems whose runtime state is built once per process (tunnel gateway,
///   usage-metering channel), and
/// - the resource-pressure controller, whose enabled flag is fixed at runtime
///   construction.
///
/// Everything else — connection/request limits, HTTP protocol parameters,
/// TLS policy and certificate file contents, proxy-protocol policy, drain and
/// reload tuning — hot-applies: per-connection resolution reads the published
/// generation, and admission semaphores reconcile their capacity.
#[derive(Clone, PartialEq, Eq)]
struct ReloadTopology {
    app_key: String,
    sockets: Vec<SocketTopology>,
    tunnel_fingerprint: [u8; 32],
    usage_metering_fingerprint: [u8; 32],
    resource_pressure: Option<sdkwork_webserver_core::ResourcePressureConfig>,
}

/// One listening socket identity. `socket_key` groups stream TCP virtual
/// servers that share one socket (SNI preread); group membership changes do
/// not change the socket and therefore hot-apply.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct SocketTopology {
    pub(crate) kind: SocketKind,
    pub(crate) id: String,
    pub(crate) bind: String,
    pub(crate) port: u16,
    pub(crate) socket_key: Option<String>,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) enum SocketKind {
    Http,
    StreamTcp,
    StreamUdp,
    Mail,
}

impl ReloadTopology {
    /// The listening-socket set of a compiled app. Stream TCP servers sharing
    /// a `socket_key` collapse to one socket entry (the authored-order first
    /// member owns the identity).
    fn socket_set(app: &CompiledWebServerApp) -> Vec<SocketTopology> {
        let mut sockets: Vec<SocketTopology> = Vec::new();
        for listener in app.listeners() {
            sockets.push(SocketTopology {
                kind: SocketKind::Http,
                id: listener.id.clone(),
                bind: listener.bind.clone(),
                port: listener.port,
                socket_key: None,
            });
        }
        for stream in app.streams() {
            match stream.protocol {
                sdkwork_webserver_core::StreamProtocol::Tcp => {
                    let socket_key = stream.socket_key().to_owned();
                    if sockets.iter().any(|socket| {
                        socket.kind == SocketKind::StreamTcp
                            && socket.socket_key.as_deref() == Some(socket_key.as_str())
                    }) {
                        continue;
                    }
                    sockets.push(SocketTopology {
                        kind: SocketKind::StreamTcp,
                        id: stream.id.clone(),
                        bind: stream.bind.clone(),
                        port: stream.port,
                        socket_key: Some(socket_key),
                    });
                }
                sdkwork_webserver_core::StreamProtocol::Udp => {
                    sockets.push(SocketTopology {
                        kind: SocketKind::StreamUdp,
                        id: stream.id.clone(),
                        bind: stream.bind.clone(),
                        port: stream.port,
                        socket_key: None,
                    });
                }
            }
        }
        for mail_server in app.mail_servers() {
            sockets.push(SocketTopology {
                kind: SocketKind::Mail,
                id: mail_server.id.clone(),
                bind: mail_server.bind.clone(),
                port: mail_server.port,
                socket_key: None,
            });
        }
        sockets.sort();
        sockets
    }

    fn from_app(app: &CompiledWebServerApp) -> Self {
        Self {
            app_key: app.config().app_key.clone(),
            sockets: Self::socket_set(app),
            tunnel_fingerprint: subsystem_fingerprint(app.config().tunnel.as_ref()),
            usage_metering_fingerprint: subsystem_fingerprint(app.config().usage_metering.as_ref()),
            resource_pressure: app.config().deployment.resource_pressure.clone(),
        }
    }
}

/// Hashes a process-built subsystem's configuration. The config types always
/// serialize (the compiler validated them), like `revision_for_compiled_app`.
fn subsystem_fingerprint<T: serde::Serialize>(value: Option<&T>) -> [u8; 32] {
    match value {
        None => Sha256::digest(b"absent").into(),
        Some(value) => {
            let bytes = serde_json::to_vec(value).expect("validated config always serializes");
            Sha256::digest(bytes).into()
        }
    }
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

fn revision_for_compiled_app(app: &CompiledWebServerApp) -> String {
    let bytes = serde_json::to_vec(app.config()).expect("Web Server config always serializes");
    format!("runtime:{}", sha256_hash(&bytes))
}
