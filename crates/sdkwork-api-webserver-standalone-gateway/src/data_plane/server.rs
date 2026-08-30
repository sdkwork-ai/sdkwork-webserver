use std::{
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
    response::Response,
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
    ProxyProtocolConfig, WebServerLimits,
};
use sdkwork_webserver_delivery_runtime::{AppConfigResourceExecutor, WebsiteDeliveryExecutor};
use tokio::{    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinSet,
    time::timeout,
};
use tower_http::{compression::CompressionLayer, timeout::TimeoutLayer};

use super::{
    connection_limit::{ConnectionLimitedStream, ConnectionLimiter},
    gzip_predicate::NginxGzipPredicate,
    handler::route_request,
    http1_wire::Http1WireGuardAcceptor,
    http2_wire::Http2WireGuardAcceptor,
    io_timeout::WriteTimeoutAcceptor,
    keep_alive_timeout::Http1KeepAliveTimeoutAcceptor,
    metrics::{ConnectionRejection, DataPlaneMetrics},
    operations::{
        prepare_operations_listener, serve_operations_listener, DataPlaneOperationsConfig,
    },
    proxy_protocol::{resolve_connection_info, DownstreamConnectionInfo},
    runtime::RuntimeGeneration,
    stream_proxy::{prepare_stream_listener, serve_stream_listener},
    udp_stream_proxy::{prepare_udp_stream_listener, serve_udp_stream_listener},
    tls::build_tls_config,
    tls_runtime::FileTlsRuntimeController,
    DataPlaneError, DataPlaneRuntime, ListenerState,
};

use crate::deploy_fallback::DeployFallbackResolver;
use crate::usage_metering::{
    HttpUsageIngestChannel, UsageIngestChannel, UsageMeteringAggregator,
};
use sdkwork_webserver_core::config::{UsageMeteringChannel, WebServerAppConfig};

struct PreparedListener {
    config: ListenerConfig,
    socket: TcpListener,
    address: SocketAddr,
    tls: Option<RustlsConfig>,
}

/// Bind a listener socket, setting `IPV6_V6ONLY` on IPv6 wildcard binds so a
/// `[::]:port` listener can coexist with `0.0.0.0:port` (nginx `listen`
/// semantics; Linux would otherwise reject the second bind via IPv4-mapped).
async fn bind_listener(address: SocketAddr) -> std::io::Result<TcpListener> {
    if address.is_ipv6() {
        let domain = socket2::Domain::IPV6;
        let socket = socket2::Socket::new(domain, socket2::Type::STREAM, Some(socket2::Protocol::TCP))?;
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
    run_website_data_plane_with_operations_until(app, website_delivery, deploy_fallback, None, shutdown).await
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
    let mut prepared = Vec::with_capacity(initial.app.config().listeners.len());
    for listener in initial.app.listeners() {
        prepared.push(prepare_listener(&initial, listener, tls_runtime.as_deref()).await?);
    }
    let mut prepared_streams = Vec::with_capacity(initial.app.streams().len());
    let mut prepared_udp_streams = Vec::with_capacity(initial.app.streams().len());
    for stream in initial.app.streams() {
        match stream.protocol {
            sdkwork_webserver_core::StreamProtocol::Tcp => {
                prepared_streams.push(prepare_stream_listener(stream).await?);
            }
            sdkwork_webserver_core::StreamProtocol::Udp => {
                prepared_udp_streams.push(prepare_udp_stream_listener(stream).await?);
            }
        }
    }
    let prepared_operations = match operations.as_ref() {
        Some(config) => Some(prepare_operations_listener(config).await?),
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

    let drain_timeout = Duration::from_millis(
        initial
            .app
            .config()
            .deployment
            .drain_timeout_ms
            .unwrap_or(initial.app.config().limits.drain_timeout_ms),
    );
    drop(initial);
    let mut shutdown_senders = Vec::with_capacity(prepared.len());
    let mut tasks = JoinSet::new();
    for listener in prepared {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        shutdown_senders.push(shutdown_tx);
        let runtime = runtime.clone();
        let website_delivery = website_delivery.clone();
        let provider_resources = provider_resources.clone();
        let deploy_fallback = deploy_fallback.clone();
        let usage_meter = usage_meter.clone();
        let listener_id = listener.config.id.clone();
        tracing::info!(
            listener_id = %listener_id,
            address = %listener.address,
            tls = listener.tls.is_some(),
            "data-plane listener prepared"
        );
        tasks.spawn(async move {
            let result = serve_listener(
                runtime,
                website_delivery,
                provider_resources,
                deploy_fallback,
                usage_meter,
                listener,
                shutdown_rx,
                drain_timeout,
            )
            .await;
            (listener_id, result)
        });
    }
    for stream in prepared_streams {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        shutdown_senders.push(shutdown_tx);
        let runtime = runtime.clone();
        let stream_id = stream.id.clone();
        tasks.spawn(async move {
            let result = serve_stream_listener(runtime, stream, shutdown_rx).await;
            (stream_id, result)
        });
    }
    for stream in prepared_udp_streams {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        shutdown_senders.push(shutdown_tx);
        let runtime = runtime.clone();
        let stream_id = stream.id.clone();
        tasks.spawn(async move {
            let result = serve_udp_stream_listener(runtime, stream, shutdown_rx).await;
            (stream_id, result)
        });
    }
    if let Some(prepared_operations) = prepared_operations {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        shutdown_senders.push(shutdown_tx);
        let runtime = runtime.clone();
        let website_delivery = website_delivery.clone();
        let provider_resources = provider_resources.clone();
        tasks.spawn(async move {
            let result = serve_operations_listener(
                prepared_operations,
                runtime,
                website_delivery,
                provider_resources,
                shutdown_rx,
            )
            .await;
            ("host-operations".to_owned(), result)
        });
    }

    tokio::pin!(shutdown);
    let (result, drain_deadline) = tokio::select! {
        () = &mut shutdown => {
            let drain_deadline = Instant::now() + drain_timeout;
            request_listener_shutdown(&shutdown_senders);
            (collect_shutdown_results(&mut tasks).await, drain_deadline)
        }
        result = tasks.join_next() => {
            let drain_deadline = Instant::now() + drain_timeout;
            request_listener_shutdown(&shutdown_senders);
            let result = match result {
                Some(Ok((listener_id, Ok(())))) => {
                    let _ = collect_shutdown_results(&mut tasks).await;
                    if listener_id == "host-operations" {
                        Err(DataPlaneError::OperationsListenerStopped)
                    } else {
                        Err(DataPlaneError::ListenerStopped { listener_id })
                    }
                }
                Some(Ok((_listener_id, Err(error)))) => {
                    let _ = collect_shutdown_results(&mut tasks).await;
                    Err(error)
                }
                Some(Err(error)) => {
                    let _ = collect_shutdown_results(&mut tasks).await;
                    Err(DataPlaneError::ListenerTask(error))
                }
                None => Ok(()),
            };
            (result, drain_deadline)
        }
    };
    let remaining_drain = drain_deadline.saturating_duration_since(Instant::now());
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
    if runtime
        .tunnel_supervisor
        .stop_and_drain(remaining_drain)
        .await
    {
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
    let socket = bind_listener(requested_address)
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
    let tls = match tls_runtime.filter(|runtime| runtime.listener_id() == listener.id) {
        Some(runtime) => {
            if listener.tls_policy_ref.is_some() {
                return Err(DataPlaneError::DynamicTlsConfiguration {
                    detail: format!(
                        "listener {} cannot combine tlsPolicyRef with dynamic TLS",
                        listener.id
                    ),
                });
            }
            if listener.tls_runtime != Some(ListenerTlsRuntime::Assignment) {
                return Err(DataPlaneError::DynamicTlsConfiguration {
                    detail: format!(
                        "listener {} must declare tlsRuntime=assignment",
                        listener.id
                    ),
                });
            }
            runtime.configure_listener(listener).map_err(|error| {
                DataPlaneError::DynamicTlsConfiguration {
                    detail: error.to_string(),
                }
            })?;
            Some(runtime.rustls_config())
        }
        None if listener.tls_runtime.is_some() => {
            return Err(DataPlaneError::DynamicTlsConfiguration {
                detail: format!(
                    "listener {} declares tlsRuntime=assignment but no TLS runtime is configured",
                    listener.id
                ),
            });
        }
        None => build_tls_config(generation, listener)?,
    };
    Ok(PreparedListener {
        config: listener.clone(),
        socket,
        address,
        tls,
    })
}

async fn serve_listener(
    runtime: Arc<DataPlaneRuntime>,
    website_delivery: Option<Arc<WebsiteDeliveryExecutor>>,
    provider_resources: Option<Arc<AppConfigResourceExecutor>>,
    deploy_fallback: Option<Arc<DeployFallbackResolver>>,
    usage_meter: Option<Arc<UsageMeteringAggregator>>,
    listener: PreparedListener,
    shutdown: watch::Receiver<bool>,
    drain_timeout: Duration,
) -> Result<(), DataPlaneError> {
    let listener_id = listener.config.id.clone();
    let initial = runtime.current();
    let limits = initial.app.config().limits.clone();
    let maximum_connections = listener
        .config
        .max_connections
        .unwrap_or(limits.max_connections);
    drop(initial);
    let state = ListenerState {
        runtime: runtime.clone(),
        website_delivery,
        provider_resources,
        deploy_fallback,
        usage_meter,
        listener_id: listener_id.clone(),
        is_tls: listener.tls.is_some(),
    };
    let app = Router::new()
        .fallback(route_request)
        .with_state(state)
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_millis(limits.request_timeout_ms),
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
        );
    let service = app.into_make_service_with_connect_info::<DownstreamConnectionInfo>();
    let global_permits = runtime.connection_permits.clone();
    let resource_pressure = runtime.resource_pressure.clone();
    let http1_only = listener.config.protocols == [ListenerProtocol::Http1];
    let http2_only = listener.config.protocols == [ListenerProtocol::Http2];
    let connection_write_timeout = Duration::from_millis(limits.connection_write_timeout_ms);
    let http1_keep_alive_idle_timeout =
        Duration::from_millis(limits.http1_keep_alive_idle_timeout_ms);
    let maximum_connection_age = Duration::from_millis(limits.max_connection_age_ms);
    let mut builder = Builder::new(TokioExecutor::new());
    configure_http_protocols(&mut builder, &limits);
    let allow_upgrades = !http2_only;
    let builder = if http1_only {
        builder.http1_only()
    } else if http2_only {
        builder.http2_only()
    } else {
        builder
    };
    let builder = Arc::new(builder);

    let result = if let Some(tls) = listener.tls {
        let limiter =
            ConnectionLimiter::new(global_permits, maximum_connections, runtime.metrics.clone());
        let http1 = Http1WireGuardAcceptor::new_observed(
            RustlsAcceptor::new(tls).acceptor(DefaultAcceptor::new()),
            &limits,
            runtime.metrics.clone(),
        );
        let http2 = Http2WireGuardAcceptor::new_observed(http1, &limits, runtime.metrics.clone());
        let keep_alive = Http1KeepAliveTimeoutAcceptor::new(http2, http1_keep_alive_idle_timeout);
        let acceptor = WriteTimeoutAcceptor::new_observed(
            keep_alive,
            connection_write_timeout,
            runtime.metrics.clone(),
        );
        serve_connections(
            listener.socket,
            limiter,
            resource_pressure,
            runtime.metrics.clone(),
            listener.config.proxy_protocol.clone(),
            acceptor,
            service,
            builder,
            allow_upgrades,
            shutdown,
            maximum_connection_age,
            drain_timeout,
        )
        .await
    } else {
        let limiter =
            ConnectionLimiter::new(global_permits, maximum_connections, runtime.metrics.clone());
        let acceptor = WriteTimeoutAcceptor::new_observed(
            Http1KeepAliveTimeoutAcceptor::new(
                Http2WireGuardAcceptor::new_observed(
                    Http1WireGuardAcceptor::new_observed(
                        DefaultAcceptor::new(),
                        &limits,
                        runtime.metrics.clone(),
                    ),
                    &limits,
                    runtime.metrics.clone(),
                ),
                http1_keep_alive_idle_timeout,
            ),
            connection_write_timeout,
            runtime.metrics.clone(),
        );
        serve_connections(
            listener.socket,
            limiter,
            resource_pressure,
            runtime.metrics.clone(),
            listener.config.proxy_protocol.clone(),
            acceptor,
            service,
            builder,
            allow_upgrades,
            shutdown,
            maximum_connection_age,
            drain_timeout,
        )
        .await
    };
    result.map_err(|source| DataPlaneError::Listener {
        listener_id,
        source,
    })
}

#[allow(clippy::too_many_arguments)]
async fn serve_connections<A, M>(
    listener: TcpListener,
    limiter: ConnectionLimiter,
    resource_pressure: Arc<super::resource_pressure::ResourcePressureController>,
    metrics: Arc<DataPlaneMetrics>,
    proxy_protocol: Option<ProxyProtocolConfig>,
    acceptor: A,
    make_service: M,
    builder: Arc<Builder<TokioExecutor>>,
    allow_upgrades: bool,
    mut shutdown: watch::Receiver<bool>,
    maximum_connection_age: Duration,
    drain_timeout: Duration,
) -> io::Result<()>
where
    M: MakeService<DownstreamConnectionInfo, Request<Incoming>> + Clone + Send + 'static,
    M::MakeFuture: Send + 'static,
    A: Accept<ConnectionLimitedStream<TcpStream>, M::Service> + Clone + Send + Sync + 'static,
    A::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    A::Service: SendService<Request<Incoming>> + Send + 'static,
    A::Future: Send,
{
    let mut connections = JoinSet::new();
    let proxy_protocol = Arc::new(proxy_protocol);
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
                let (stream, peer) = accepted?;
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
                let acceptor = acceptor.clone();
                let builder = builder.clone();
                let metrics = metrics.clone();
                let proxy_protocol = proxy_protocol.clone();
                let mut make_service = make_service.clone();
                let mut connection_shutdown = shutdown.clone();
                connections.spawn(async move {
                    let mut stream = stream;
                    let connection_info = match resolve_connection_info(
                        &mut stream,
                        peer,
                        proxy_protocol.as_ref().as_ref(),
                    ).await {
                        Ok(info) => info,
                        Err(_) => {
                            metrics.record_connection_rejection(ConnectionRejection::ProxyProtocol);
                            return;
                        }
                    };
                    if std::future::poll_fn(|context| make_service.poll_ready(context)).await.is_err() {
                        return;
                    }
                    let Ok(service) = make_service.make_service(connection_info).await else {
                        return;
                    };
                    let Ok((stream, service)) = acceptor.accept(stream, service).await else {
                        return;
                    };
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
                });
            }
        }
    }

    drain_connection_tasks(&mut connections, drain_timeout).await;
    Ok(())
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

fn request_listener_shutdown(shutdown_senders: &[watch::Sender<bool>]) {
    for sender in shutdown_senders {
        let _ = sender.send(true);
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

async fn collect_shutdown_results(
    tasks: &mut JoinSet<(String, Result<(), DataPlaneError>)>,
) -> Result<(), DataPlaneError> {
    let mut first_error = None;
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((_listener_id, Ok(()))) => {}
            Ok((_listener_id, Err(error))) if first_error.is_none() => first_error = Some(error),
            Err(error) if first_error.is_none() => {
                first_error = Some(DataPlaneError::ListenerTask(error))
            }
            _ => {}
        }
    }
    first_error.map_or(Ok(()), Err)
}


/// Build the SaaS traffic usage meter when the app config enables
/// `usageMetering` and an ingest channel is available. The embedded channel
/// writes through the shared Deploy database (management feature); the http
/// channel posts to the Deploy control-plane ingest endpoint. An
/// unavailable channel disables metering with a warning — it never blocks
/// the data plane.
fn build_usage_meter(
    app_config: &WebServerAppConfig,
) -> Option<Arc<UsageMeteringAggregator>> {
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
                tracing::warn!(
                    "usage metering embedded channel requires the management feature"
                );
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
