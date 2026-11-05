use std::{
    collections::HashMap,
    future::Future,
    io,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::State,
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};
use axum_server::{
    accept::{Accept, DefaultAcceptor},
    service::{MakeService, SendService},
    tls_rustls::{RustlsAcceptor, RustlsConfig},
};
use hyper::{body::Incoming, Request};
use hyper_util::{
    rt::{TokioExecutor, TokioIo, TokioTimer},
    server::conn::auto::Builder,
    service::TowerToHyperService,
};
use sdkwork_webserver_core::{
    CompiledWebServerApp, ListenerConfig, ListenerProtocol, ListenerTlsRuntime,
    TlsCertificateResolution, WebServerLimits,
};
use sdkwork_webserver_delivery_runtime::{AppConfigResourceExecutor, WebsiteDeliveryExecutor};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, TcpStream},
    sync::{watch, Semaphore},
    task::JoinSet,
    time::timeout,
};
use tower_http::compression::CompressionLayer;

use super::{
    connection_limit::{ConnectionLimitedStream, ConnectionLimiter},
    gzip_predicate::NginxGzipPredicate,
    handler::route_request,
    handshake_timeout::HandshakeTimeoutAcceptor,
    http1_wire::Http1WireGuardAcceptor,
    http2_wire::Http2WireGuardAcceptor,
    io_timeout::WriteTimeoutAcceptor,
    keep_alive_timeout::Http1KeepAliveTimeoutAcceptor,
    mail_frontend::{serve_mail_listener, PreparedMailListener},
    metrics::{ConnectionRejection, DataPlaneMetrics},
    operations::{
        prepare_operations_listener, serve_operations_listener, DataPlaneOperationsConfig,
    },
    proxy_protocol::{resolve_connection_info, DownstreamConnectionInfo},
    runtime::{RuntimeGeneration, SocketKind, SocketTopology},
    stream_proxy::PreparedStreamListener,
    tls::compile_policy_tls_material,
    tls_runtime::FileTlsRuntimeController,
    udp_stream_proxy::PreparedUdpStreamListener,
    DataPlaneError, DataPlaneRuntime, ListenerState,
};

use crate::deploy_fallback::DeployFallbackResolver;
use crate::usage_metering::{HttpUsageIngestChannel, UsageIngestChannel, UsageMeteringAggregator};
use sdkwork_webserver_core::config::{UsageMeteringChannel, WebServerAppConfig};

struct PreparedListener {
    config: ListenerConfig,
    socket: TcpListener,
    address: SocketAddr,
}

/// Owns every data-plane listening socket and applies each published
/// generation's socket-set diff. Added sockets bind **before** the generation
/// publishes (a bind failure retains the old generation, matching nginx
/// SIGHUP), removed sockets stop accepting **after** publication and drain
/// their in-flight connections detached. Kept sockets keep their file
/// descriptors; per-connection resolution picks up the new configuration.
pub(crate) struct ListenerSupervisor {
    runtime: Arc<DataPlaneRuntime>,
    website_delivery: Option<Arc<WebsiteDeliveryExecutor>>,
    provider_resources: Option<Arc<AppConfigResourceExecutor>>,
    deploy_fallback: Option<Arc<DeployFallbackResolver>>,
    usage_meter: Option<Arc<UsageMeteringAggregator>>,
    tls_runtime: Option<Arc<FileTlsRuntimeController>>,
    inner: tokio::sync::Mutex<SupervisorInner>,
    events_tx: tokio::sync::mpsc::UnboundedSender<SupervisorEvent>,
    events_rx:
        tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<SupervisorEvent>>,
}

struct SupervisorInner {
    live: HashMap<SocketTopology, LiveEntry>,
    /// Sockets bound by [`ListenerSupervisor::prepare_sockets`], served by
    /// [`ListenerSupervisor::commit_sockets`].
    pending: Vec<(SocketTopology, PendingSocket)>,
}

struct LiveEntry {
    shutdown_tx: watch::Sender<bool>,
    semaphore: Arc<Semaphore>,
    capacity: usize,
}

enum PendingSocket {
    Http(PreparedListener),
    StreamTcp(PreparedStreamListener),
    StreamUdp(PreparedUdpStreamListener),
    Mail(PreparedMailListener),
}

/// One serve task reported completion through the events channel.
pub(crate) enum SupervisorEvent {
    TaskFinished {
        socket: SocketTopology,
        listener_id: String,
        result: Result<(), DataPlaneError>,
    },
}

impl ListenerSupervisor {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        runtime: Arc<DataPlaneRuntime>,
        website_delivery: Option<Arc<WebsiteDeliveryExecutor>>,
        provider_resources: Option<Arc<AppConfigResourceExecutor>>,
        deploy_fallback: Option<Arc<DeployFallbackResolver>>,
        usage_meter: Option<Arc<UsageMeteringAggregator>>,
        tls_runtime: Option<Arc<FileTlsRuntimeController>>,
    ) -> Self {
        let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            runtime,
            website_delivery,
            provider_resources,
            deploy_fallback,
            usage_meter,
            tls_runtime,
            inner: tokio::sync::Mutex::new(SupervisorInner {
                live: HashMap::new(),
                pending: Vec::new(),
            }),
            events_tx,
            events_rx: tokio::sync::Mutex::new(events_rx),
        }
    }

    /// Binds every socket of the initial generation and starts serving it.
    pub(crate) async fn bootstrap(
        &self,
        initial: &Arc<RuntimeGeneration>,
    ) -> Result<(), DataPlaneError> {
        self.prepare_sockets(initial).await?;
        self.commit_sockets(initial).await;
        Ok(())
    }

    /// Binds the sockets the candidate generation adds. A bind failure closes
    /// everything this call bound and returns the error, so the reload keeps
    /// the previous generation (nginx: "bind() to … failed, configuration
    /// unchanged").
    pub(crate) async fn prepare_sockets(
        &self,
        candidate: &Arc<RuntimeGeneration>,
    ) -> Result<(), DataPlaneError> {
        let mut inner = self.inner.lock().await;
        // Drop any stale pending set (commit always follows prepare; this only
        // matters if a caller skips commit after a successful prepare).
        inner.pending.clear();
        let mut bound: Vec<(SocketTopology, PendingSocket)> = Vec::new();
        let outcome = async {
            for socket in &candidate.sockets {
                if inner.live.contains_key(socket) {
                    continue;
                }
                let prepared = self.prepare_socket(candidate, socket).await?;
                bound.push((socket.clone(), prepared));
            }
            Ok(())
        }
        .await;
        match outcome {
            Ok(()) => {
                inner.pending.extend(bound);
                Ok(())
            }
            Err(error) => {
                // Dropping each prepared socket closes it; the live set is
                // untouched and keeps serving the previous generation.
                for (socket, prepared) in bound {
                    drop(prepared);
                    tracing::warn!(
                        listener_id = %socket.id,
                        "listener handoff rolled back a freshly bound socket"
                    );
                }
                Err(error)
            }
        }
    }

    async fn prepare_socket(
        &self,
        candidate: &Arc<RuntimeGeneration>,
        socket: &SocketTopology,
    ) -> Result<PendingSocket, DataPlaneError> {
        match socket.kind {
            SocketKind::Http => {
                let listener =
                    candidate.app.listener(&socket.id).ok_or_else(|| {
                        DataPlaneError::Listener {
                            listener_id: socket.id.clone(),
                            source: io::Error::new(
                                io::ErrorKind::NotFound,
                                format!(
                                    "listener {} vanished from the candidate generation",
                                    socket.id
                                ),
                            ),
                        }
                    })?;
                Ok(PendingSocket::Http(
                    prepare_listener(candidate, listener, self.tls_runtime.as_deref()).await?,
                ))
            }
            SocketKind::StreamTcp => {
                let socket_key = socket.socket_key.clone().unwrap_or_default();
                let members: Vec<sdkwork_webserver_core::StreamServerConfig> = candidate
                    .app
                    .streams()
                    .iter()
                    .filter(|stream| {
                        stream.protocol == sdkwork_webserver_core::StreamProtocol::Tcp
                            && stream.socket_key() == socket_key
                    })
                    .cloned()
                    .collect();
                if members.is_empty() {
                    return Err(DataPlaneError::Listener {
                        listener_id: socket.id.clone(),
                        source: io::Error::new(
                            io::ErrorKind::NotFound,
                            "stream socket has no configured server",
                        ),
                    });
                }
                let prepared = if members.len() == 1 {
                    super::stream_proxy::prepare_stream_listener(&members[0]).await?
                } else {
                    super::stream_proxy::prepare_stream_listener_group(&members).await?
                };
                Ok(PendingSocket::StreamTcp(prepared))
            }
            SocketKind::StreamUdp => {
                let stream = candidate
                    .app
                    .streams()
                    .iter()
                    .find(|stream| stream.id == socket.id)
                    .ok_or_else(|| DataPlaneError::Listener {
                        listener_id: socket.id.clone(),
                        source: io::Error::new(
                            io::ErrorKind::NotFound,
                            "udp stream listener vanished from the candidate generation",
                        ),
                    })?;
                Ok(PendingSocket::StreamUdp(
                    super::udp_stream_proxy::prepare_udp_stream_listener(stream).await?,
                ))
            }
            SocketKind::Mail => {
                let mail_server = candidate
                    .app
                    .mail_servers()
                    .iter()
                    .find(|server| server.id == socket.id)
                    .ok_or_else(|| DataPlaneError::Listener {
                        listener_id: socket.id.clone(),
                        source: io::Error::new(
                            io::ErrorKind::NotFound,
                            "mail listener vanished from the candidate generation",
                        ),
                    })?;
                Ok(PendingSocket::Mail(
                    PreparedMailListener::bind(mail_server).await?,
                ))
            }
        }
    }

    /// After publication: serve the newly bound sockets, stop accepting on the
    /// removed ones (they drain detached through their own bounded deadlines),
    /// and reconcile per-listener admission capacity to the candidate.
    pub(crate) async fn commit_sockets(&self, candidate: &Arc<RuntimeGeneration>) {
        let mut inner = self.inner.lock().await;
        // Retire sockets the candidate removed. The entry leaves `live` first,
        // so its completion event reads as a normal retirement instead of an
        // unexpected listener death.
        let removed: Vec<SocketTopology> = inner
            .live
            .keys()
            .filter(|socket| !candidate.sockets.contains(socket))
            .cloned()
            .collect();
        for socket in removed {
            if let Some(entry) = inner.live.remove(&socket) {
                tracing::info!(
                    listener_id = %socket.id,
                    "listener handoff retires a removed socket; draining in-flight connections"
                );
                let _ = entry.shutdown_tx.send(true);
                drop(entry);
            }
        }
        // Serve the sockets prepared for this generation.
        let pending = std::mem::take(&mut inner.pending);
        for (socket, prepared) in pending {
            self.spawn_socket(&mut inner, socket, prepared, candidate);
        }
        // Reconcile per-listener admission capacity (limit changes hot-apply).
        for (socket, entry) in inner.live.iter_mut() {
            let capacity_target = match socket.kind {
                SocketKind::Http => candidate
                    .app
                    .listener(&socket.id)
                    .and_then(|listener| listener.max_connections)
                    .unwrap_or(candidate.app.config().limits.max_connections),
                _ => candidate.app.config().limits.max_connections,
            };
            let current = entry.capacity;
            match capacity_target.cmp(&current) {
                std::cmp::Ordering::Greater => {
                    entry.semaphore.add_permits(capacity_target - current);
                    entry.capacity = capacity_target;
                }
                std::cmp::Ordering::Less => {
                    let drop = (current - capacity_target).min(entry.semaphore.available_permits());
                    if drop > 0 {
                        entry.semaphore.forget_permits(drop);
                    }
                    entry.capacity = capacity_target;
                }
                std::cmp::Ordering::Equal => {}
            }
        }
    }

    fn spawn_socket(
        &self,
        inner: &mut SupervisorInner,
        socket: SocketTopology,
        prepared: PendingSocket,
        candidate: &Arc<RuntimeGeneration>,
    ) {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let limits = candidate.app.config().limits.clone();
        let capacity = match &prepared {
            PendingSocket::Http(prepared) => prepared
                .config
                .max_connections
                .unwrap_or(limits.max_connections),
            _ => limits.max_connections,
        };
        let semaphore = Arc::new(Semaphore::new(capacity));
        let events_tx = self.events_tx.clone();
        // Dropping the join handle detaches the serve task on purpose: its
        // lifecycle is reported through the supervisor event channel, and
        // `shutdown_all` awaits completions through that channel.
        match prepared {
            PendingSocket::Http(prepared) => {
                let runtime = self.runtime.clone();
                let website_delivery = self.website_delivery.clone();
                let provider_resources = self.provider_resources.clone();
                let deploy_fallback = self.deploy_fallback.clone();
                let usage_meter = self.usage_meter.clone();
                let tls_runtime = self.tls_runtime.clone();
                let listener_id = prepared.config.id.clone();
                let socket_key = socket.clone();
                let listener_semaphore = semaphore.clone();
                tracing::info!(
                    listener_id = %listener_id,
                    address = %prepared.address,
                    "data-plane listener prepared"
                );
                tokio::spawn(async move {
                    // A dynamic assignment listener is owned by the
                    // process-lifetime TLS runtime controller, which supplies
                    // its acceptor configuration (and hot-swaps certificates).
                    let dynamic_tls = match tls_runtime.as_ref() {
                        Some(tls_runtime) if tls_runtime.listener_id() == listener_id => {
                            Some(tls_runtime.rustls_config())
                        }
                        _ => None,
                    };
                    let result = serve_listener(
                        runtime,
                        website_delivery,
                        provider_resources,
                        deploy_fallback,
                        usage_meter,
                        prepared,
                        listener_semaphore,
                        shutdown_rx,
                        dynamic_tls,
                    )
                    .await;
                    let _ = events_tx.send(SupervisorEvent::TaskFinished {
                        socket: socket_key,
                        listener_id,
                        result,
                    });
                })
            }
            PendingSocket::StreamTcp(prepared) => {
                let runtime = self.runtime.clone();
                let listener_id = prepared.id.clone();
                let socket_key = socket.clone();
                tokio::spawn(async move {
                    let result =
                        super::stream_proxy::serve_stream_listener(runtime, prepared, shutdown_rx)
                            .await;
                    let _ = events_tx.send(SupervisorEvent::TaskFinished {
                        socket: socket_key,
                        listener_id,
                        result,
                    });
                })
            }
            PendingSocket::StreamUdp(prepared) => {
                let runtime = self.runtime.clone();
                let listener_id = prepared.id.clone();
                let socket_key = socket.clone();
                tokio::spawn(async move {
                    let result = super::udp_stream_proxy::serve_udp_stream_listener(
                        runtime, prepared, shutdown_rx,
                    )
                    .await;
                    let _ = events_tx.send(SupervisorEvent::TaskFinished {
                        socket: socket_key,
                        listener_id,
                        result,
                    });
                })
            }
            PendingSocket::Mail(prepared) => {
                let runtime = self.runtime.clone();
                let listener_id = prepared.id.clone();
                let socket_key = socket.clone();
                tokio::spawn(async move {
                    let result =
                        serve_mail_listener(runtime, prepared, shutdown_rx).await;
                    let _ = events_tx.send(SupervisorEvent::TaskFinished {
                        socket: socket_key,
                        listener_id,
                        result,
                    });
                })
            }
        };
        inner.live.insert(
            socket,
            LiveEntry {
                shutdown_tx,
                semaphore,
                capacity,
            },
        );
    }

    /// Whether the socket is still live (an unexpected serve-task death).
    pub(crate) async fn is_live(&self, socket: &SocketTopology) -> bool {
        self.inner.lock().await.live.contains_key(socket)
    }

    /// The next serve-task completion event; `None` when the supervisor is
    /// shutting down and its event channel closed.
    pub(crate) async fn next_event(&self) -> Option<SupervisorEvent> {
        self.events_rx.lock().await.recv().await
    }

    /// Stop accepting everywhere and collect the serve tasks within the drain
    /// deadline (plus the same small collection grace the process drain uses).
    /// Completion is awaited through the supervisor event channel, which also
    /// covers sockets retired by an earlier handoff.
    pub(crate) async fn shutdown_all(&self, deadline: Instant) -> Result<(), DataPlaneError> {
        let awaited: std::collections::HashSet<SocketTopology> = {
            let mut inner = self.inner.lock().await;
            for entry in inner.live.values() {
                let _ = entry.shutdown_tx.send(true);
            }
            inner.live.drain().map(|(socket, _)| socket).collect()
        };
        if awaited.is_empty() {
            return Ok(());
        }
        const COLLECTION_GRACE: Duration = Duration::from_secs(5);
        let mut pending = awaited;
        let mut first_error = None;
        let collect = async {
            let mut events = self.events_rx.lock().await;
            while !pending.is_empty() {
                match events.recv().await {
                    Some(SupervisorEvent::TaskFinished {
                        socket,
                        listener_id,
                        result,
                    }) => {
                        if pending.remove(&socket) {
                            if let Err(error) = result {
                                tracing::warn!(
                                    listener_id = %listener_id,
                                    %error,
                                    "listener serve task failed during drain"
                                );
                                first_error.get_or_insert(error);
                            }
                        }
                    }
                    None => break,
                }
            }
        };
        if tokio::time::timeout_at((deadline + COLLECTION_GRACE).into(), collect)
            .await
            .is_err()
        {
            // Serve tasks that missed their own bounded drain deadline end
            // through their internal timeouts; bound the report instead.
            first_error.get_or_insert(DataPlaneError::ListenerDrainTimeout {
                remaining: pending.len(),
            });
        }
        first_error.map_or(Ok(()), Err)
    }
}

/// Bind a listener socket, setting `IPV6_V6ONLY` on IPv6 wildcard binds so a
/// `[::]:port` listener can coexist with `0.0.0.0:port` (nginx `listen`
/// semantics; Linux would otherwise reject the second bind via IPv4-mapped).
pub(crate) async fn bind_listener(address: SocketAddr) -> std::io::Result<TcpListener> {
    if address.is_ipv6() {
        let domain = socket2::Domain::IPV6;
        let socket =
            socket2::Socket::new(domain, socket2::Type::STREAM, Some(socket2::Protocol::TCP))?;
        socket.set_reuse_address(true)?;
        socket.set_only_v6(true)?;
        socket.set_nonblocking(true)?;
        socket.bind(&address.into())?;
        socket.listen(1024)?;
        TcpListener::from_std(socket.into())
    } else {
        TcpListener::bind(address).await
    }
}

pub async fn run_data_plane_until<F>(
    app: CompiledWebServerApp,
    shutdown: F,
) -> Result<(), DataPlaneError>
where
    F: Future<Output = ()> + Send,
{
    run_data_plane_with_operations_until(app, None, shutdown).await
}

/// Runs the data plane with an injected cluster auto-routing overlay: the
/// discovery refresher (management assembly or tests) publishes topology
/// snapshots, and request hosts served by the cluster are routed to picked
/// healthy instances.
pub async fn run_data_plane_until_with_cluster_overlay<F>(
    app: CompiledWebServerApp,
    overlay: Arc<arc_swap::ArcSwap<sdkwork_webserver_cluster::ClusterTopology>>,
    shutdown: F,
) -> Result<(), DataPlaneError>
where
    F: Future<Output = ()> + Send,
{
    let runtime = DataPlaneRuntime::build_with_cluster_overlay(app, overlay)?;
    let result =
        run_data_plane_runtime_until(runtime.clone(), None, None, None, None, None, shutdown).await;
    let health_result = runtime.stop_active_health().await;
    let resource_result = runtime.stop_resource_pressure().await;
    result.and(health_result).and(resource_result)
}

pub async fn run_data_plane_with_operations_until<F>(
    app: CompiledWebServerApp,
    operations: Option<DataPlaneOperationsConfig>,
    shutdown: F,
) -> Result<(), DataPlaneError>
where
    F: Future<Output = ()> + Send,
{
    let runtime = match operations.as_ref() {
        Some(config) => {
            DataPlaneRuntime::build_with_metric_dimensions(app, config.dimensions.clone())?
        }
        None => DataPlaneRuntime::build(app)?,
    };
    let result = run_data_plane_runtime_until(
        runtime.clone(),
        operations,
        None,
        None,
        None,
        None,
        shutdown,
    )
    .await;
    let health_result = runtime.stop_active_health().await;
    let resource_result = runtime.stop_resource_pressure().await;
    result.and(health_result).and(resource_result)
}

pub async fn run_website_data_plane_until<F>(
    app: CompiledWebServerApp,
    website_delivery: Arc<WebsiteDeliveryExecutor>,
    deploy_fallback: Option<Arc<DeployFallbackResolver>>,
    shutdown: F,
) -> Result<(), DataPlaneError>
where
    F: Future<Output = ()> + Send,
{
    run_website_data_plane_with_operations_until(
        app,
        website_delivery,
        deploy_fallback,
        None,
        shutdown,
    )
    .await
}

pub async fn run_website_data_plane_with_operations_until<F>(
    app: CompiledWebServerApp,
    website_delivery: Arc<WebsiteDeliveryExecutor>,
    deploy_fallback: Option<Arc<DeployFallbackResolver>>,
    operations: Option<DataPlaneOperationsConfig>,
    shutdown: F,
) -> Result<(), DataPlaneError>
where
    F: Future<Output = ()> + Send,
{
    let runtime = match operations.as_ref() {
        Some(config) => {
            DataPlaneRuntime::build_with_metric_dimensions(app, config.dimensions.clone())?
        }
        None => DataPlaneRuntime::build(app)?,
    };
    let result = run_data_plane_runtime_until(
        runtime.clone(),
        operations,
        Some(website_delivery),
        None,
        None,
        deploy_fallback,
        shutdown,
    )
    .await;
    let health_result = runtime.stop_active_health().await;
    let resource_result = runtime.stop_resource_pressure().await;
    result.and(health_result).and(resource_result)
}

pub async fn run_website_data_plane_with_tls_operations_until<F>(
    app: CompiledWebServerApp,
    website_delivery: Arc<WebsiteDeliveryExecutor>,
    deploy_fallback: Option<Arc<DeployFallbackResolver>>,
    operations: Option<DataPlaneOperationsConfig>,
    tls_runtime: Arc<FileTlsRuntimeController>,
    shutdown: F,
) -> Result<(), DataPlaneError>
where
    F: Future<Output = ()> + Send,
{
    let runtime = match operations.as_ref() {
        Some(config) => {
            DataPlaneRuntime::build_with_metric_dimensions(app, config.dimensions.clone())?
        }
        None => DataPlaneRuntime::build(app)?,
    };
    let result = run_data_plane_runtime_until(
        runtime.clone(),
        operations,
        Some(website_delivery),
        None,
        Some(tls_runtime),
        deploy_fallback,
        shutdown,
    )
    .await;
    let health_result = runtime.stop_active_health().await;
    let resource_result = runtime.stop_resource_pressure().await;
    result.and(health_result).and(resource_result)
}

pub(crate) async fn run_data_plane_runtime_until<F>(
    runtime: Arc<DataPlaneRuntime>,
    operations: Option<DataPlaneOperationsConfig>,
    website_delivery: Option<Arc<WebsiteDeliveryExecutor>>,
    provider_resources: Option<Arc<AppConfigResourceExecutor>>,
    tls_runtime: Option<Arc<FileTlsRuntimeController>>,
    deploy_fallback: Option<Arc<DeployFallbackResolver>>,
    shutdown: F,
) -> Result<(), DataPlaneError>
where
    F: Future<Output = ()> + Send,
{
    let initial = runtime.current();
    let usage_meter = build_usage_meter(initial.app.config());
    if let Some(tls_runtime) = tls_runtime.as_ref() {
        let matching_listeners = initial
            .app
            .listeners()
            .filter(|listener| listener.id == tls_runtime.listener_id())
            .count();
        if matching_listeners != 1 {
            return Err(DataPlaneError::DynamicTlsConfiguration {
                detail: format!(
                    "listener {} must resolve to exactly one configured listener",
                    tls_runtime.listener_id()
                ),
            });
        }
    }
    // The supervisor owns every data-plane socket (HTTP, stream TCP/UDP, mail)
    // for the process lifetime and applies each published generation's
    // socket-set diff. The operations/management listeners stay process-fixed
    // (management plane, not operator routing config).
    let supervisor = Arc::new(ListenerSupervisor::new(
        runtime.clone(),
        website_delivery.clone(),
        provider_resources.clone(),
        deploy_fallback.clone(),
        usage_meter.clone(),
        tls_runtime.clone(),
    ));
    runtime.install_listener_supervisor(supervisor.clone())?;
    supervisor.bootstrap(&initial).await?;
    drop(initial);
    // Tunnel gateway: QUIC listener rides the data-plane lifecycle (PRD SS77
    // backward compatibility: absent/disabled config leaves nothing running).
    let mut tunnel_gateway = match runtime.tunnel_options.as_ref() {
        Some(options) => {
            let shared = runtime
                .tunnel
                .clone()
                .ok_or(DataPlaneError::TunnelStateUnavailable)?;
            Some(
                sdkwork_webserver_tunnel::TunnelGateway::spawn_with_shared(
                    shared,
                    options.clone(),
                    None,
                )
                .await
                .map_err(|error| DataPlaneError::TunnelGateway(error.to_string()))?,
            )
        }
        None => None,
    };
    runtime.start_resource_pressure().await?;
    runtime.start_active_health().await?;

    let (tls_shutdown_tx, tls_watcher) = match tls_runtime {
        Some(tls_runtime) => {
            let (stop_tx, stop_rx) = watch::channel(false);
            let watcher = tokio::spawn(tls_runtime.watch_until(stop_rx));
            (Some(stop_tx), Some(watcher))
        }
        None => (None, None),
    };

    // Process-fixed operations listener (loopback health/metrics/tunnel
    // control). Its death is fatal; it is not part of the handoff surface.
    let mut operations_shutdown: Option<watch::Sender<bool>> = None;
    let mut operations_handle: Option<tokio::task::JoinHandle<(String, Result<(), DataPlaneError>)>> =
        None;
    if let Some(operations_config) = operations.as_ref() {
        let prepared_operations = prepare_operations_listener(operations_config).await?;
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let runtime = runtime.clone();
        let website_delivery = website_delivery.clone();
        let provider_resources = provider_resources.clone();
        operations_shutdown = Some(shutdown_tx);
        operations_handle = Some(tokio::spawn(async move {
            let result = serve_operations_listener(
                prepared_operations,
                runtime,
                website_delivery,
                provider_resources,
                shutdown_rx,
            )
            .await;
            ("host-operations".to_owned(), result)
        }));
    }

    let drain_timeout = || {
        let generation = runtime.current();
        let config = generation.app.config();
        Duration::from_millis(
            config
                .deployment
                .drain_timeout_ms
                .unwrap_or(config.limits.drain_timeout_ms),
        )
    };
    let stop_operations = |operations_shutdown: &Option<watch::Sender<bool>>| {
        if let Some(shutdown_tx) = operations_shutdown {
            let _ = shutdown_tx.send(true);
        }
    };
    let mut operations_handle = operations_handle;

    tokio::pin!(shutdown);
    let result = loop {
        let drain_deadline = Instant::now() + drain_timeout();
        let outcome = tokio::select! {
            () = &mut shutdown => {
                stop_operations(&operations_shutdown);
                supervisor.shutdown_all(drain_deadline).await
            }
            event = supervisor.next_event() => {
                match event {
                    // A live listener ended without a shutdown request: fatal,
                    // drain everything (same contract as the previous
                    // `tasks.join_next` arm).
                    Some(SupervisorEvent::TaskFinished { socket, listener_id, result })
                        if supervisor.is_live(&socket).await =>
                    {
                        stop_operations(&operations_shutdown);
                        let _ = supervisor.shutdown_all(drain_deadline).await;
                        Err(match result {
                            Err(error) => error,
                            Ok(()) => DataPlaneError::ListenerStopped { listener_id },
                        })
                    }
                    // A retired listener finished its detached drain: normal.
                    Some(_) => continue,
                    // The event channel closes only when the supervisor drops,
                    // which cannot happen while this loop holds an Arc to it.
                    None => Err(DataPlaneError::ListenerStopped {
                        listener_id: "listener-supervisor-events".to_owned(),
                    }),
                }
            }
            operations = async {
                match operations_handle.as_mut() {
                    Some(handle) => Some(handle.await),
                    None => {
                        std::future::pending::<()>().await;
                        None
                    }
                }
            } => {
                let _ = operations;
                stop_operations(&operations_shutdown);
                let _ = supervisor.shutdown_all(drain_deadline).await;
                Err(DataPlaneError::OperationsListenerStopped)
            }
        };
        break outcome;
    };
    // The operations handle may still be draining; it owns its own bounded
    // shutdown, so drop the sender and detach.
    drop(operations_shutdown);
    if let Some(handle) = operations_handle.take() {
        handle.abort();
    }
    let remaining_drain = drain_timeout();
    if let Some(gateway) = tunnel_gateway.take() {
        gateway.shutdown().await;
    }
    if let Some(stop_tx) = tls_shutdown_tx {
        let _ = stop_tx.send(true);
    }
    if let Some(watcher) = tls_watcher {
        match watcher.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::error!(error = %error, "TLS runtime watcher stopped unexpectedly")
            }
            Err(error) => tracing::error!(error = %error, "TLS runtime watcher task failed"),
        }
    }
    let tunnel_drained = runtime
        .tunnel_supervisor
        .stop_and_drain(remaining_drain)
        .await;
    // Final metering flush: the periodic loop only drains closed windows, so a
    // graceful restart would otherwise drop the open window (up to one window
    // plus one flush interval of billing facts) on every deploy.
    if let Some(meter) = usage_meter.as_ref() {
        meter.flush_all().await;
    }
    if tunnel_drained {
        result
    } else {
        Err(DataPlaneError::TunnelDrainTimeout {
            active: runtime.tunnel_supervisor.active(),
        })
    }
}

async fn observe_data_plane_request(
    State(metrics): State<Arc<DataPlaneMetrics>>,
    request: axum::http::Request<Body>,
    next: Next,
) -> Response {
    let lease = metrics.begin_request();
    let response = next.run(request).await;
    metrics.observe_response(response, lease)
}

async fn prepare_listener(
    generation: &Arc<RuntimeGeneration>,
    listener: &ListenerConfig,
    tls_runtime: Option<&FileTlsRuntimeController>,
) -> Result<PreparedListener, DataPlaneError> {
    let ip = listener
        .bind
        .parse()
        .map_err(|source| DataPlaneError::InvalidBind {
            bind: listener.bind.clone(),
            source,
        })?;
    let requested_address = SocketAddr::new(ip, listener.port);
    let socket =
        bind_listener(requested_address)
            .await
            .map_err(|source| DataPlaneError::Listener {
                listener_id: listener.id.clone(),
                source,
            })?;
    let address = socket
        .local_addr()
        .map_err(|source| DataPlaneError::Listener {
            listener_id: listener.id.clone(),
            source,
        })?;
    // A listener serves the assigned certificate set when it declares a TLS
    // runtime, the configured certificate files when it declares a policy, and
    // both — configured files layered above the assigned set — when it declares
    // `tlsCertificateResolution: policy-first`. Which sources combine is
    // validated at config load; here the resolution only selects the wiring.
    // Static policy material was already built (and its files read) when the
    // generation compiled; a dynamic assignment listener is wired to its
    // process-lifetime controller here, exactly once.
    let resolution = TlsCertificateResolution::effective(
        listener.tls_certificate_resolution,
        listener.tls_policy_ref.is_some(),
        listener.tls_runtime.is_some(),
    );
    match tls_runtime.filter(|runtime| runtime.listener_id() == listener.id) {
        Some(runtime) => {
            if listener.tls_runtime != Some(ListenerTlsRuntime::Assignment) {
                return Err(DataPlaneError::DynamicTlsConfiguration {
                    detail: format!(
                        "listener {} must declare tlsRuntime=assignment",
                        listener.id
                    ),
                });
            }
            let policy_material = match resolution {
                Some(TlsCertificateResolution::AssignmentOnly) => None,
                Some(TlsCertificateResolution::PolicyFirst) => {
                    let policy_id = listener.tls_policy_ref.clone().ok_or_else(|| {
                        DataPlaneError::DynamicTlsConfiguration {
                            detail: format!(
                                "listener {} declares policy-first without tlsPolicyRef",
                                listener.id
                            ),
                        }
                    })?;
                    Some(
                        compile_policy_tls_material(
                            generation.app.as_ref(),
                            listener,
                            // A policy-first listener has the assigned set
                            // underneath, so an unusable file degrades that
                            // server name instead of failing the listener.
                            true,
                        )?
                        .ok_or(DataPlaneError::MissingTlsPolicy {
                            listener_id: listener.id.clone(),
                            policy_id,
                        })?,
                    )
                }
                _ => {
                    return Err(DataPlaneError::DynamicTlsConfiguration {
                        detail: format!(
                            "listener {} must declare tlsRuntime=assignment for dynamic TLS",
                            listener.id
                        ),
                    });
                }
            };
            runtime
                .configure_listener(listener, policy_material)
                .map_err(|error| DataPlaneError::DynamicTlsConfiguration {
                    detail: error.to_string(),
                })?;
        }
        None if listener.tls_runtime.is_some() => {
            return Err(DataPlaneError::DynamicTlsConfiguration {
                detail: format!(
                    "listener {} declares tlsRuntime=assignment but no TLS runtime is configured",
                    listener.id
                ),
            });
        }
        None => {}
    }
    Ok(PreparedListener {
        config: listener.clone(),
        socket,
        address,
    })
}

async fn serve_listener(
    runtime: Arc<DataPlaneRuntime>,
    website_delivery: Option<Arc<WebsiteDeliveryExecutor>>,
    provider_resources: Option<Arc<AppConfigResourceExecutor>>,
    deploy_fallback: Option<Arc<DeployFallbackResolver>>,
    usage_meter: Option<Arc<UsageMeteringAggregator>>,
    listener: PreparedListener,
    listener_permits: Arc<Semaphore>,
    shutdown: watch::Receiver<bool>,
    dynamic_tls: Option<RustlsConfig>,
) -> Result<(), DataPlaneError> {
    let listener_id = listener.config.id.clone();
    let state = ListenerState {
        runtime: runtime.clone(),
        website_delivery,
        provider_resources,
        deploy_fallback,
        usage_meter,
        listener_id: listener_id.clone(),
    };
    let app = Router::new()
        .fallback(route_request)
        .with_state(state)
        // nginx request timeouts resolve per request from the published
        // generation, so a limit change hot-applies without a rebind.
        .layer(middleware::from_fn_with_state(
            runtime.clone(),
            apply_request_timeout,
        ))
        .layer(middleware::from_fn_with_state(
            runtime.metrics.clone(),
            observe_data_plane_request,
        ))
        // Inside compression: nginx filter order substitutes the response
        // body before gzip compresses it.
        .layer(middleware::from_fn(
            super::sub_filter::apply_sub_filters_middleware,
        ))
        // Outermost: compress eligible responses per http gzip / gzipTypes /
        // gzipMinLength (nginx.enabled profile http-core-v1).
        .layer(
            CompressionLayer::new()
                .gzip(true)
                .no_br()
                .no_deflate()
                .no_zstd()
                .compress_when(NginxGzipPredicate::new(runtime.clone())),
        )
        // Outermost: nginx `gzip_vary off` removes the compression layer's
        // automatic `Vary: Accept-Encoding`.
        .layer(super::gzip_predicate::SuppressGzipVaryLayer)
        // Outermost: nginx `http2_chunk_size` DATA-frame bounding. It must run
        // after every body-producing layer (notably compression), so it wraps
        // the fully finalized response and only rewrites HTTP/2 bodies.
        .layer(middleware::from_fn(
            super::http2_chunk::apply_http2_chunking,
        ));
    let service = app.into_make_service_with_connect_info::<DownstreamConnectionInfo>();
    let limiter = ConnectionLimiter::new(
        runtime.connection_permits.clone(),
        listener_permits,
        runtime.metrics.clone(),
    );
    serve_connections(
        listener.socket,
        limiter,
        runtime.resource_pressure.clone(),
        runtime.metrics.clone(),
        runtime,
        listener_id.clone(),
        dynamic_tls,
        service,
        shutdown,
    )
    .await
    .map_err(|source| DataPlaneError::Listener {
        listener_id,
        source,
    })
}

/// Per-request equivalent of the fixed `TimeoutLayer`: the limit resolves from
/// the published generation, so a `requestTimeoutMs` change hot-applies.
async fn apply_request_timeout(
    State(runtime): State<Arc<DataPlaneRuntime>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let limit = Duration::from_millis(runtime.current().app.config().limits.request_timeout_ms);
    match timeout(limit, next.run(request)).await {
        Ok(response) => response,
        Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
    }
}

#[allow(clippy::too_many_arguments)]
async fn serve_connections<M>(
    listener: TcpListener,
    limiter: ConnectionLimiter,
    resource_pressure: Arc<super::resource_pressure::ResourcePressureController>,
    metrics: Arc<DataPlaneMetrics>,
    runtime: Arc<DataPlaneRuntime>,
    listener_id: String,
    dynamic_tls: Option<RustlsConfig>,
    make_service: M,
    mut shutdown: watch::Receiver<bool>,
) -> io::Result<()>
where
    M: MakeService<DownstreamConnectionInfo, Request<Incoming>> + Clone + Send + 'static,
    M::MakeFuture: Send + 'static,
{
    let mut connections = JoinSet::new();
    let mut accept_policy = super::accept::AcceptRetryPolicy::new();
    loop {
        tokio::select! {
            biased;
            () = wait_for_shutdown(&mut shutdown) => break,
            Some(result) = connections.join_next(), if !connections.is_empty() => {
                if let Err(error) = result {
                    tracing::warn!(%error, "connection task failed");
                }
            }
            accepted = listener.accept() => {
                let (stream, peer) = match accepted {
                    Ok(accepted) => {
                        accept_policy.on_success();
                        accepted
                    }
                    Err(error) => {
                        // Transient accept failures (fd exhaustion, aborted
                        // connections) must not stop a production listener;
                        // only a broken listener is fatal.
                        if accept_policy.on_error(&error) {
                            tokio::time::sleep(super::accept::ACCEPT_RETRY_PAUSE).await;
                            continue;
                        }
                        return Err(error);
                    }
                };
                metrics.record_connection_accepted();
                if resource_pressure.is_pressured() {
                    metrics.record_connection_rejection(ConnectionRejection::ResourcePressure);
                    drop(stream);
                    continue;
                }
                let stream = match limiter.try_admit(stream) {
                    Ok(stream) => stream,
                    Err(_) => continue,
                };
                let runtime = runtime.clone();
                let listener_id = listener_id.clone();
                let dynamic_tls = dynamic_tls.clone();
                let metrics = metrics.clone();
                let make_service = make_service.clone();
                let connection_shutdown = shutdown.clone();
                connections.spawn(async move {
                    drive_connection(
                        runtime,
                        listener_id,
                        dynamic_tls,
                        stream,
                        peer,
                        make_service,
                        connection_shutdown,
                        metrics,
                    )
                    .await;
                });
            }
        }
    }

    // Bounded drain: the deadline resolves from the published generation.
    let drain_timeout = {
        let generation = runtime.current();
        let config = generation.app.config();
        Duration::from_millis(
            config
                .deployment
                .drain_timeout_ms
                .unwrap_or(config.limits.drain_timeout_ms),
        )
    };
    drain_connection_tasks(&mut connections, drain_timeout).await;
    Ok(())
}

/// Everything one accepted connection observes resolves from the generation it
/// was admitted under: TLS material, protocol limits, timeouts, proxy-protocol
/// policy and admission policy. In-flight connections keep their pinned
/// generation, so a handoff or reload never disturbs them.
#[allow(clippy::too_many_arguments)]
async fn drive_connection<M>(
    runtime: Arc<DataPlaneRuntime>,
    listener_id: String,
    dynamic_tls: Option<RustlsConfig>,
    mut stream: ConnectionLimitedStream<TcpStream>,
    peer: SocketAddr,
    make_service: M,
    connection_shutdown: watch::Receiver<bool>,
    metrics: Arc<DataPlaneMetrics>,
) where
    M: MakeService<DownstreamConnectionInfo, Request<Incoming>> + Clone + Send + 'static,
    M::MakeFuture: Send + 'static,
{
    let generation = runtime.current();
    let Some(listener_config) = generation.app.listener(&listener_id).cloned() else {
        // The socket is retiring after a handoff and this connection raced its
        // shutdown; close it without serving.
        return;
    };
    let limits = generation.app.config().limits.clone();
    let drain_timeout = Duration::from_millis(
        generation
            .app
            .config()
            .deployment
            .drain_timeout_ms
            .unwrap_or(limits.drain_timeout_ms),
    );
    let maximum_connection_age = Duration::from_millis(limits.max_connection_age_ms);
    let http1_only = listener_config.protocols == [ListenerProtocol::Http1];
    let http2_only = listener_config.protocols == [ListenerProtocol::Http2];
    let allow_upgrades = !http2_only;
    let mut builder = Builder::new(TokioExecutor::new());
    configure_http_protocols(&mut builder, &limits);
    let builder = if http1_only {
        builder.http1_only()
    } else if http2_only {
        builder.http2_only()
    } else {
        builder
    };
    let builder = Arc::new(builder);
    let tls = dynamic_tls
        .or_else(|| generation.tls_configs.get(&listener_id).cloned().flatten());
    let connection_info = match resolve_connection_info(
        &mut stream,
        peer,
        listener_config.proxy_protocol.as_ref(),
    )
    .await
    {
        Ok(info) => info.with_tls(tls.is_some()),
        Err(_) => {
            metrics.record_connection_rejection(ConnectionRejection::ProxyProtocol);
            return;
        }
    };
    let mut make_service = make_service.clone();
    if std::future::poll_fn(|context| make_service.poll_ready(context))
        .await
        .is_err()
    {
        return;
    }
    let Ok(service) = make_service.make_service(connection_info).await else {
        return;
    };
    if let Some(tls) = tls {
        // TLS acceptor chain for one connection: nginx-ordered wire guards and
        // timeouts over the generation's certificate material.
        let connection_write_timeout = Duration::from_millis(limits.connection_write_timeout_ms);
        let handshake_timeout = Duration::from_millis(limits.tls_handshake_timeout_ms);
        let http1_keep_alive_idle_timeout =
            Duration::from_millis(limits.http1_keep_alive_idle_timeout_ms);
        let http1 = Http1WireGuardAcceptor::new_observed(
            RustlsAcceptor::new(tls).acceptor(DefaultAcceptor::new()),
            &limits,
            metrics.clone(),
        );
        let http2 = Http2WireGuardAcceptor::new_observed(http1, &limits, metrics.clone());
        let keep_alive =
            Http1KeepAliveTimeoutAcceptor::new(http2, http1_keep_alive_idle_timeout);
        let acceptor = HandshakeTimeoutAcceptor::new_observed(
            WriteTimeoutAcceptor::new_observed(
                keep_alive,
                connection_write_timeout,
                metrics.clone(),
            ),
            handshake_timeout,
            metrics,
        );
        let Ok((stream, service)) = acceptor.accept(stream, service).await else {
            return;
        };
        serve_connection_tail(
            stream,
            service,
            builder,
            allow_upgrades,
            connection_shutdown,
            maximum_connection_age,
            drain_timeout,
        )
        .await;
    } else {
        // Plaintext acceptor chain for one connection.
        let connection_write_timeout = Duration::from_millis(limits.connection_write_timeout_ms);
        let handshake_timeout = Duration::from_millis(limits.tls_handshake_timeout_ms);
        let http1_keep_alive_idle_timeout =
            Duration::from_millis(limits.http1_keep_alive_idle_timeout_ms);
        let http1 =
            Http1WireGuardAcceptor::new_observed(DefaultAcceptor::new(), &limits, metrics.clone());
        let http2 = Http2WireGuardAcceptor::new_observed(http1, &limits, metrics.clone());
        let keep_alive =
            Http1KeepAliveTimeoutAcceptor::new(http2, http1_keep_alive_idle_timeout);
        let acceptor = HandshakeTimeoutAcceptor::new_observed(
            WriteTimeoutAcceptor::new_observed(
                keep_alive,
                connection_write_timeout,
                metrics.clone(),
            ),
            handshake_timeout,
            metrics,
        );
        let Ok((stream, service)) = acceptor.accept(stream, service).await else {
            return;
        };
        serve_connection_tail(
            stream,
            service,
            builder,
            allow_upgrades,
            connection_shutdown,
            maximum_connection_age,
            drain_timeout,
        )
        .await;
    }
}

/// Serve one accepted connection to completion: bounded by the shutdown watch
/// (graceful), the maximum-connection-age timer, then the drain deadline.
/// Identical contract for TLS and plaintext streams.
async fn serve_connection_tail<S, Service>(
    stream: S,
    service: Service,
    builder: Arc<Builder<TokioExecutor>>,
    allow_upgrades: bool,
    mut connection_shutdown: watch::Receiver<bool>,
    maximum_connection_age: Duration,
    drain_timeout: Duration,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    Service: SendService<Request<Incoming>> + Send + 'static,
{
    let io = TokioIo::new(stream);
    let service = TowerToHyperService::new(service.into_service());
    let age = tokio::time::sleep(maximum_connection_age);
    tokio::pin!(age);
    if allow_upgrades {
        let connection = builder.serve_connection_with_upgrades(io, service);
        tokio::pin!(connection);
        tokio::select! {
            biased;
            _ = &mut connection => return,
            () = wait_for_shutdown(&mut connection_shutdown) => {}
            () = &mut age => {}
        }
        connection.as_mut().graceful_shutdown();
        let _ = timeout(drain_timeout, &mut connection).await;
    } else {
        let connection = builder.serve_connection(io, service);
        tokio::pin!(connection);
        tokio::select! {
            biased;
            _ = &mut connection => return,
            () = wait_for_shutdown(&mut connection_shutdown) => {}
            () = &mut age => {}
        }
        connection.as_mut().graceful_shutdown();
        let _ = timeout(drain_timeout, &mut connection).await;
    }
}

async fn wait_for_shutdown(shutdown: &mut watch::Receiver<bool>) {
    if *shutdown.borrow() {
        return;
    }
    while shutdown.changed().await.is_ok() {
        if *shutdown.borrow() {
            return;
        }
    }
}

async fn drain_connection_tasks(connections: &mut JoinSet<()>, drain_timeout: Duration) {
    if timeout(drain_timeout, async {
        while let Some(result) = connections.join_next().await {
            if let Err(error) = result {
                tracing::warn!(%error, "connection task failed during drain");
            }
        }
    })
    .await
    .is_err()
    {
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
}

fn configure_http_protocols(builder: &mut Builder<TokioExecutor>, limits: &WebServerLimits) {
    let mut http1 = builder.http1();
    http1
        .half_close(true)
        .ignore_invalid_headers(false)
        .max_buf_size(limits.max_request_header_bytes)
        .header_read_timeout(Duration::from_millis(limits.request_header_timeout_ms))
        .timer(TokioTimer::new());
    if limits.max_request_headers != 100 {
        http1.max_headers(limits.max_request_headers);
    }

    let mut http2 = builder.http2();
    http2
        .adaptive_window(false)
        .initial_stream_window_size(65_535_u32)
        .initial_connection_window_size(65_535_u32)
        .max_frame_size(limits.http2_max_frame_bytes)
        .max_concurrent_streams(limits.http2_max_concurrent_streams)
        .max_pending_accept_reset_streams(limits.http2_max_pending_accept_reset_streams)
        .max_local_error_reset_streams(limits.http2_max_local_error_reset_streams)
        .max_send_buf_size(limits.http2_max_send_buffer_bytes)
        .max_header_list_size(limits.http2_max_header_list_bytes)
        .keep_alive_interval(Duration::from_millis(limits.http2_keep_alive_interval_ms))
        .keep_alive_timeout(Duration::from_millis(limits.http2_keep_alive_timeout_ms))
        .timer(TokioTimer::new());
}

/// Build the SaaS traffic usage meter when the app config enables
/// `usageMetering` and an ingest channel is available. The embedded channel
/// writes through the shared Deploy database (management feature); the http
/// channel posts to the Deploy control-plane ingest endpoint. An
/// unavailable channel disables metering with a warning — it never blocks
/// the data plane.
fn build_usage_meter(app_config: &WebServerAppConfig) -> Option<Arc<UsageMeteringAggregator>> {
    let config = app_config.usage_metering.as_ref()?;
    if !config.enabled {
        return None;
    }
    let channel: Arc<dyn UsageIngestChannel> = match &config.channel {
        UsageMeteringChannel::Embedded => {
            #[cfg(feature = "management")]
            {
                use sdkwork_database_sqlx::process_shared_database_pool;
                let Some(pool) = process_shared_database_pool() else {
                    tracing::warn!(
                        "usage metering is enabled but the shared database pool is unavailable"
                    );
                    return None;
                };
                let sdkwork_database_sqlx::DatabasePool::Postgres(pool, _) = pool;
                Arc::new(crate::usage_metering::EmbeddedUsageIngestChannel::new(
                    sdkwork_api_webserver_assembly::DeployRepository::new_lookup(pool),
                ))
            }
            #[cfg(not(feature = "management"))]
            {
                tracing::warn!("usage metering embedded channel requires the management feature");
                return None;
            }
        }
        UsageMeteringChannel::Http {
            endpoint,
            auth_token_file,
        } => {
            let auth_token = auth_token_file.as_ref().and_then(|path| {
                std::fs::read_to_string(path)
                    .ok()
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty())
            });
            match HttpUsageIngestChannel::new(endpoint.clone(), auth_token) {
                Ok(channel) => Arc::new(channel),
                Err(error) => {
                    tracing::warn!(error = %error, "usage metering http channel unavailable");
                    return None;
                }
            }
        }
    };
    let node_uuid = std::env::var("SDKWORK_WEBSERVER_NODE_UUID")
        .unwrap_or_else(|_| "webserver-node".to_owned());
    let meter = Arc::new(UsageMeteringAggregator::new(
        Arc::new(config.clone()),
        channel,
        node_uuid,
    ));
    meter.spawn_flush();
    Some(meter)
}
