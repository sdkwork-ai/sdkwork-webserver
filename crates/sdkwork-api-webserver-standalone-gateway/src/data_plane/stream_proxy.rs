//! Raw TCP stream proxying (`nginx stream` equivalent,
//! SDKWORK_WEBSERVER_SPEC section 12).
//!
//! Modes:
//! - plaintext byte forward
//! - TLS terminate (`listen … ssl` + certificate)
//! - TLS passthrough (`sslPreread`) with ClientHello peek
//!
//! Upstream-referenced targets share HTTP `ProxyUpstream` health state.

use std::{
    net::{IpAddr, SocketAddr},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use sdkwork_webserver_core::{
    evaluate_access_first_match_allow, AccessDecision, AccessRuleConfig, StreamServerConfig,
    StreamTargetConfig, StreamTlsMode, TlsVersion,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::watch,
    time::{sleep, timeout},
};
use tokio_rustls::TlsAcceptor;

use super::{
    proxy::{ProxyUpstream, StreamEndpoint},
    stream_proxy_protocol::{read_inbound_proxy_header, InboundProxyHeader},
    tls_material::{build_sni_server_config, install_crypto_provider, load_certified_key},
    DataPlaneError, DataPlaneRuntime,
};

const STREAM_COPY_BUFFER_BYTES: usize = 16 * 1024;
const STREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct PreparedStreamListener {
    pub(crate) id: String,
    socket: TcpListener,
    proxy_timeout: Duration,
    proxy_protocol: bool,
    round_robin: Arc<AtomicUsize>,
    /// Built once at listener preparation. Rebuilding the SNI resolver and
    /// re-reading the certificate files for every accepted connection put
    /// filesystem I/O on the connection path and scaled with connection rate.
    tls_acceptor: Option<TlsAcceptor>,
    /// Ordered nginx stream `allow`/`deny` rules (first match wins; an
    /// unmatched peer is allowed).
    access: Vec<AccessRuleConfig>,
    /// nginx stream `preread_timeout` (ClientHello read deadline).
    preread_timeout: Duration,
    /// nginx stream `preread_buffer_size` (ClientHello peek buffer bound).
    preread_buffer_bytes: usize,
    /// SNI virtual-server group: every stream config sharing this socket, in
    /// authored order. `None` for an ordinary single-server listener.
    group: Option<Vec<StreamServerConfig>>,
    /// nginx stream `listen … proxy_protocol`: accept one inbound PROXY header.
    accept_proxy_protocol: bool,
    /// nginx stream `proxy_protocol_timeout`.
    proxy_protocol_timeout: Duration,
    /// nginx stream `set_real_ip_from`: trusted peers whose PROXY header source
    /// replaces the connection address.
    real_ip_trusted: Vec<ipnet::IpNet>,
}

pub(crate) async fn prepare_stream_listener(
    config: &StreamServerConfig,
    generation: &Arc<super::runtime::RuntimeGeneration>,
) -> Result<PreparedStreamListener, DataPlaneError> {
    let address = format!("{}:{}", config.bind, config.port);
    let socket = TcpListener::bind(&address)
        .await
        .map_err(|source| DataPlaneError::Listener {
            listener_id: config.id.clone(),
            source,
        })?;
    let local = socket
        .local_addr()
        .map_err(|source| DataPlaneError::Listener {
            listener_id: config.id.clone(),
            source,
        })?;
    // Stream listeners are fixed for the process lifetime (reloads take effect
    // per connection for targets, not for the listener socket/TLS material), so
    // a bad certificate is a startup error rather than a per-connection log.
    let tls_acceptor = match config.tls.as_ref() {
        Some(StreamTlsMode::Terminate {
            certificate_ref,
            client_auth,
        }) => {
            let resolved = resolve_stream_client_auth(generation, &config.id, client_auth.as_ref());
            Some(
                build_stream_tls_acceptor(generation, certificate_ref, resolved.as_ref()).map_err(
                    |detail| DataPlaneError::StreamTlsMaterial {
                        stream_id: config.id.clone(),
                        detail,
                    },
                )?,
            )
        }
        _ => None,
    };
    tracing::info!(
        stream_id = %config.id,
        address = %local,
        tls = ?config.tls.as_ref().map(stream_tls_label),
        "stream listener prepared"
    );
    Ok(PreparedStreamListener {
        id: config.id.clone(),
        socket,
        proxy_timeout: Duration::from_millis(config.proxy_timeout_ms),
        proxy_protocol: config.proxy_protocol,
        round_robin: Arc::new(AtomicUsize::new(0)),
        tls_acceptor,
        access: config.access.clone(),
        preread_timeout: Duration::from_millis(config.preread_timeout_ms),
        preread_buffer_bytes: config.preread_buffer_bytes,
        group: None,
        accept_proxy_protocol: config.accept_proxy_protocol,
        proxy_protocol_timeout: Duration::from_millis(config.proxy_protocol_timeout_ms),
        real_ip_trusted: parse_trusted_cidrs(&config.real_ip_trusted_cidrs),
    })
}

/// Parse the resolved `set_real_ip_from` trusted networks.
fn parse_trusted_cidrs(values: &[String]) -> Vec<ipnet::IpNet> {
    values
        .iter()
        .filter_map(|value| {
            value.parse::<ipnet::IpNet>().ok().or_else(|| {
                value
                    .parse::<IpAddr>()
                    .ok()
                    .map(|address| ipnet::IpNet::from(address))
            })
        })
        .collect()
}

/// Group stream configs sharing one listening socket (`server_name` SNI virtual
/// servers) and bind the socket once. Every member must be an `ssl_preread`
/// server with at least one `server_name`; the first member is the socket's
/// default server unless one member sets `default_server`.
pub(crate) async fn prepare_stream_listener_group(
    configs: &[StreamServerConfig],
    _generation: &Arc<super::runtime::RuntimeGeneration>,
) -> Result<PreparedStreamListener, DataPlaneError> {
    let first = &configs[0];
    let address = format!("{}:{}", first.bind, first.port);
    let socket = TcpListener::bind(&address)
        .await
        .map_err(|source| DataPlaneError::Listener {
            listener_id: first.id.clone(),
            source,
        })?;
    let default = configs
        .iter()
        .position(|config| config.default_server)
        .unwrap_or(0);
    let default_config = &configs[default];
    tracing::info!(
        stream_id = %first.id,
        address = %address,
        servers = configs.len(),
        default_server = %default_config.id,
        "stream SNI virtual-server listener prepared"
    );
    Ok(PreparedStreamListener {
        id: default_config.id.clone(),
        socket,
        proxy_timeout: Duration::from_millis(default_config.proxy_timeout_ms),
        proxy_protocol: default_config.proxy_protocol,
        round_robin: Arc::new(AtomicUsize::new(0)),
        tls_acceptor: None,
        access: Vec::new(),
        preread_timeout: Duration::from_millis(default_config.preread_timeout_ms),
        preread_buffer_bytes: default_config.preread_buffer_bytes,
        group: Some(configs.to_vec()),
        accept_proxy_protocol: configs.iter().all(|config| config.accept_proxy_protocol),
        proxy_protocol_timeout: Duration::from_millis(default_config.proxy_protocol_timeout_ms),
        real_ip_trusted: configs
            .iter()
            .flat_map(|config| parse_trusted_cidrs(&config.real_ip_trusted_cidrs))
            .collect(),
    })
}

/// Resolve the stream listener's client-auth CA paths from the generation.
fn resolve_stream_client_auth(
    generation: &super::runtime::RuntimeGeneration,
    stream_id: &str,
    client_auth: Option<&sdkwork_webserver_core::ClientAuthConfig>,
) -> Option<sdkwork_webserver_core::ClientAuthConfig> {
    client_auth.map(|auth| {
        let resolved = generation
            .app
            .stream_client_auth_ca_paths(stream_id)
            .map(|paths| {
                paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| auth.ca_certificate_files.clone());
        sdkwork_webserver_core::ClientAuthConfig {
            mode: auth.mode,
            ca_certificate_files: resolved,
        }
    })
}

fn stream_tls_label(mode: &StreamTlsMode) -> &'static str {
    match mode {
        StreamTlsMode::Terminate { .. } => "terminate",
        StreamTlsMode::Preread => "preread",
    }
}

/// Accept loop for one stream listener. Connections resolve their target from
/// the current configuration generation (reloads take effect per connection);
/// the listener itself is fixed for the process lifetime.
pub(crate) async fn serve_stream_listener(
    runtime: Arc<DataPlaneRuntime>,
    listener: PreparedStreamListener,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), DataPlaneError> {
    let maximum_age =
        Duration::from_millis(runtime.current().app.config().limits.max_connection_age_ms);
    let mut tasks = tokio::task::JoinSet::new();
    let mut accept_policy = super::accept::AcceptRetryPolicy::new();
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            // Reap completed connection tasks while accepting. Without this
            // arm `JoinSet` keeps every finished task's output until shutdown,
            // so memory grew with the total number of closed stream
            // connections instead of the concurrent count.
            joined = tasks.join_next(), if !tasks.is_empty() => {
                if let Some(Err(error)) = joined {
                    tracing::warn!(stream_id = %listener.id, error = %error, "stream connection task failed");
                }
            }
            accepted = listener.socket.accept() => {
                let (downstream, peer) = match accepted {
                    Ok(accepted) => {
                        accept_policy.on_success();
                        accepted
                    }
                    Err(error) => {
                        // Transient accept failures must not stop the
                        // listener; only a broken listener socket ends the
                        // accept loop.
                        if accept_policy.on_error(&error) {
                            tokio::time::sleep(super::accept::ACCEPT_RETRY_PAUSE).await;
                            continue;
                        }
                        tracing::error!(stream_id = %listener.id, error = %error, "stream listener accept failed terminally");
                        return Err(DataPlaneError::Listener {
                            listener_id: listener.id.clone(),
                            source: error,
                        });
                    }
                };
                let mut downstream = downstream;
                // nginx stream `listen … proxy_protocol`: the header arrives
                // before the access/preread/content phases. Its source replaces
                // the peer only when `set_real_ip_from` trusts the connecting
                // peer (the bounded stream realip form).
                let inbound_header: Option<InboundProxyHeader> =
                    if listener.accept_proxy_protocol {
                        match read_inbound_proxy_header(
                            &mut downstream,
                            listener.proxy_protocol_timeout,
                        )
                        .await
                        {
                            Ok(header) => Some(header),
                            Err(error) => {
                                tracing::debug!(
                                    stream_id = %listener.id,
                                    peer = %peer,
                                    error = %error,
                                    "stream inbound PROXY protocol header rejected"
                                );
                                continue;
                            }
                        }
                    } else {
                        None
                    };
                let client_peer = effective_client_peer(
                    peer,
                    inbound_header,
                    &listener.real_ip_trusted,
                );
                // SNI virtual-server group: select the server by the
                // ClientHello SNI inside the connection task, then run the
                // bounded `ssl_preread` content path with the buffered
                // preface.
                if listener.group.is_some() {
                    let permit = match Arc::clone(&runtime.connection_permits)
                        .try_acquire_owned()
                    {
                        Ok(permit) => permit,
                        Err(_) => {
                            tracing::warn!(
                                stream_id = %listener.id,
                                peer = %peer,
                                "stream connection rejected by admission budget"
                            );
                            drop(downstream);
                            continue;
                        }
                    };
                    let runtime = Arc::clone(&runtime);
                    let group_listener = listener.group.clone().unwrap_or_default();
                    let default_timeout = listener.preread_timeout;
                    let default_buffer = listener.preread_buffer_bytes;
                    let proxy_timeout = listener.proxy_timeout;
                    tasks.spawn(async move {
                        let result = serve_sni_group_connection(
                            runtime,
                            downstream,
                            peer,
                            group_listener,
                            default_timeout,
                            default_buffer,
                            proxy_timeout,
                            maximum_age,
                            inbound_header,
                        )
                        .await;
                        drop(permit);
                        result
                    });
                    continue;
                }
                // nginx stream access phase and `return` handler run before
                // any upstream connection is considered.
                if evaluate_access_first_match_allow(client_peer.ip(), &listener.access)
                    == AccessDecision::Deny
                {
                    tracing::debug!(
                        stream_id = %listener.id,
                        peer = %peer,
                        "stream connection denied by allow/deny rules"
                    );
                    drop(downstream);
                    continue;
                }
                // nginx stream `limit_conn` runs in the preread phase, after
                // the access phase and before the content handler. The rules
                // and dry-run flag come from the current generation so a
                // reload applies per connection.
                let limit_lease = {
                    let generation = runtime.current();
                    let rules = generation
                        .app
                        .streams()
                        .iter()
                        .find(|stream| stream.id == listener.id)
                        .map(|stream| {
                            (
                                stream.limit_conn.clone(),
                                stream.limit_conn_dry_run,
                            )
                        })
                        .unwrap_or_default();
                    if rules.0.is_empty() {
                        None
                    } else {
                        match generation
                            .stream_limit_conn
                            .stream_admit(client_peer.ip(), &rules.0, rules.1)
                        {
                            Ok((lease, dry_run_reject)) => {
                                if dry_run_reject {
                                    tracing::info!(
                                        stream_id = %listener.id,
                                        peer = %peer,
                                        "stream limit_conn dry run: connection would be rejected"
                                    );
                                }
                                Some(lease)
                            }
                            Err(_) => {
                                tracing::debug!(
                                    stream_id = %listener.id,
                                    peer = %peer,
                                    "stream connection rejected by limit_conn"
                                );
                                drop(downstream);
                                continue;
                            }
                        }
                    }
                };
                let permit = match Arc::clone(&runtime.connection_permits).try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => {
                        tracing::warn!(
                            stream_id = %listener.id,
                            peer = %peer,
                            "stream connection rejected by admission budget"
                        );
                        drop(downstream);
                        continue;
                    }
                };
                let stream_id = listener.id.clone();
                let runtime = Arc::clone(&runtime);
                let limit_lease = limit_lease;
                let round_robin = Arc::clone(&listener.round_robin);
                let proxy_timeout = listener.proxy_timeout;
                let preread_timeout = listener.preread_timeout;
                let preread_buffer_bytes = listener.preread_buffer_bytes;
                let proxy_protocol = listener.proxy_protocol;
                let tls_acceptor = listener.tls_acceptor.clone();
                tasks.spawn(async move {
                    // Held for the whole connection: dropping it releases the
                    // zone counters.
                    let _limit_lease = limit_lease;
                    let result = serve_stream_connection(
                        runtime,
                        &stream_id,
                        downstream,
                        client_peer,
                        round_robin,
                        proxy_timeout,
                        proxy_protocol,
                        maximum_age,
                        tls_acceptor.as_ref(),
                        preread_timeout,
                        preread_buffer_bytes,
                        None,
                        None,
                        None,
                        inbound_header,
                    )
                    .await;
                    drop(permit);
                    result
                });
            }
        }
    }
    drop(listener.socket);
    // Bounded drain: stream connections terminate only through their idle
    // or maximum-age timers, which can exceed the process shutdown grace
    // period. Apply the same configured drain deadline as the HTTP path and
    // deterministically abort the remainder when it expires.
    let drain_timeout =
        Duration::from_millis(runtime.current().app.config().limits.drain_timeout_ms);
    let drain = async {
        while let Some(result) = tasks.join_next().await {
            if let Err(error) = result {
                tracing::warn!(stream_id = %listener.id, error = %error, "stream connection task failed");
            }
        }
    };
    if drain_timeout.is_zero() {
        drain.await;
    } else {
        tokio::select! {
            _ = drain => {}
            _ = sleep(drain_timeout) => {
                let remaining = tasks.len();
                tasks.abort_all();
                tracing::info!(
                    stream_id = %listener.id,
                    remaining,
                    drain_timeout_ms = drain_timeout.as_millis() as u64,
                    "stream drain deadline expired; aborted remaining connections"
                );
            }
        }
    }
    Ok(())
}

/// Write one nginx stream `return` value and shut the connection down.
async fn write_stream_response<S: tokio::io::AsyncWrite + Unpin>(
    stream: &mut S,
    text: &str,
) -> Result<(), ()> {
    let write = timeout(Duration::from_secs(5), stream.write_all(text.as_bytes())).await;
    if write.is_err() {
        tracing::debug!("stream return response write failed");
    }
    let _ = stream.shutdown().await;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn serve_stream_connection(
    runtime: Arc<DataPlaneRuntime>,
    stream_id: &str,
    downstream: TcpStream,
    peer: SocketAddr,
    round_robin: Arc<AtomicUsize>,
    proxy_timeout: Duration,
    proxy_protocol: bool,
    maximum_age: Duration,
    tls_acceptor: Option<&TlsAcceptor>,
    preread_timeout: Duration,
    preread_buffer_bytes: usize,
    selected: Option<StreamServerConfig>,
    preface: Option<Vec<u8>>,
    sni: Option<String>,
    _inbound_header: Option<InboundProxyHeader>,
) -> Result<(), ()> {
    let generation = runtime.current();
    let stream_config = match selected {
        Some(selected) => Some(selected),
        None => generation
            .app
            .streams()
            .iter()
            .find(|stream| stream.id == stream_id)
            .cloned(),
    };
    let Some(stream_config) = stream_config.as_ref() else {
        tracing::warn!(stream_id, peer = %peer, "stream target is no longer configured");
        return Err(());
    };
    let tls_mode = stream_config.tls.clone();
    let relay_options = StreamRelayOptions::from_config(stream_config);
    // nginx stream `return <value>`: a content handler that writes the expanded
    // value and closes the connection. Variables are resolved from the facts the
    // plane knows at this point (`$ssl_preread_server_name` only after the
    // ClientHello is read).
    let return_template = stream_config.return_value.clone();
    let local = downstream.local_addr().unwrap_or(peer);
    let stream_return_text = |sni: Option<&str>| {
        return_template.as_ref().map(|template| {
            let variables = super::stream_variables::resolve_stream_variables(
                generation.app.config(),
                stream_config,
                peer,
                local,
                sni,
            );
            super::stream_variables::expand_stream_value(&variables, template)
        })
    };

    let target = stream_config.target.as_ref();

    match tls_mode.as_ref() {
        None => {
            if let Some(text) = stream_return_text(None) {
                let mut downstream = downstream;
                let _ = write_stream_response(&mut downstream, &text).await;
                return Ok(());
            }
            let Some(target) = target else {
                tracing::warn!(stream_id, peer = %peer, "stream server has no proxy target");
                return Err(());
            };
            let local = downstream.local_addr().unwrap_or(peer);
            let (upstream, authority, health) = connect_with_retry(
                &generation,
                stream_config,
                target,
                peer,
                local,
                proxy_protocol,
                &round_robin,
            )
            .await?;
            tracing::info!(stream_id, peer = %peer, target = %authority, "stream connection established");
            proxy_bidirectional(
                downstream,
                upstream,
                proxy_timeout,
                maximum_age,
                relay_options,
            )
            .await;
            if let Some((upstream_ref, endpoint)) = health {
                upstream_ref.record_stream_success(&endpoint);
            }
            Ok(())
        }
        Some(StreamTlsMode::Terminate { .. }) => {
            let local = downstream.local_addr().unwrap_or(peer);
            let Some(acceptor) = tls_acceptor else {
                tracing::warn!(
                    stream_id,
                    peer = %peer,
                    "stream TLS acceptor was not prepared for a terminate listener"
                );
                return Err(());
            };
            let mut tls_downstream = match acceptor.accept(downstream).await {
                Ok(stream) => stream,
                Err(error) => {
                    tracing::warn!(stream_id, peer = %peer, error = %error, "stream TLS handshake failed");
                    return Err(());
                }
            };
            if let Some(text) = stream_return_text(None) {
                let _ = write_stream_response(&mut tls_downstream, &text).await;
                return Ok(());
            }
            let Some(target) = target else {
                tracing::warn!(stream_id, peer = %peer, "stream server has no proxy target");
                return Err(());
            };
            let (upstream, authority, health) = connect_with_retry(
                &generation,
                stream_config,
                target,
                peer,
                local,
                proxy_protocol,
                &round_robin,
            )
            .await?;
            tracing::info!(
                stream_id,
                peer = %peer,
                target = %authority,
                "stream TLS-terminated connection established"
            );
            proxy_bidirectional(
                tls_downstream,
                upstream,
                proxy_timeout,
                maximum_age,
                relay_options,
            )
            .await;
            if let Some((upstream_ref, endpoint)) = health {
                upstream_ref.record_stream_success(&endpoint);
            }
            Ok(())
        }
        Some(StreamTlsMode::Preread) => {
            let local = downstream.local_addr().unwrap_or(peer);
            let (downstream, preface, sni) = if let Some(preface) = preface {
                (downstream, preface, sni)
            } else {
                match preread_client_hello(downstream, preread_timeout, preread_buffer_bytes).await
                {
                    Ok(peeked) => peeked,
                    Err(error) => {
                        tracing::warn!(stream_id, peer = %peer, error = %error, "stream ssl_preread failed");
                        return Err(());
                    }
                }
            };
            if let Some(sni) = sni.as_deref() {
                tracing::debug!(stream_id, peer = %peer, sni, "stream ssl_preread observed SNI");
            }
            if let Some(text) = stream_return_text(sni.as_deref()) {
                let mut downstream = downstream;
                let _ = write_stream_response(&mut downstream, &text).await;
                return Ok(());
            }
            let Some(target) = target else {
                tracing::warn!(stream_id, peer = %peer, "stream server has no proxy target");
                return Err(());
            };
            let (mut upstream, authority, health) = connect_with_retry(
                &generation,
                stream_config,
                target,
                peer,
                local,
                proxy_protocol,
                &round_robin,
            )
            .await?;
            if let Err(error) = upstream.write_all(&preface).await {
                tracing::warn!(stream_id, peer = %peer, error = %error, "stream ssl_preread preface write failed");
                if let Some((upstream_ref, endpoint)) = &health {
                    upstream_ref.record_stream_failure(endpoint);
                }
                return Err(());
            }
            tracing::info!(
                stream_id,
                peer = %peer,
                target = %authority,
                "stream ssl_preread connection established"
            );
            proxy_bidirectional(
                downstream,
                upstream,
                proxy_timeout,
                maximum_age,
                relay_options,
            )
            .await;
            if let Some((upstream_ref, endpoint)) = health {
                upstream_ref.record_stream_success(&endpoint);
            }
            Ok(())
        }
    }
}

/// One connected upstream stream: plain TCP or TLS over TCP when the stream
/// server enables `proxy_ssl on`.
pub(crate) enum UpstreamStream {
    Plain(TcpStream),
    Tls(Box<tokio_rustls::client::TlsStream<TcpStream>>),
}

impl AsyncRead for UpstreamStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => std::pin::Pin::new(stream).poll_read(context, buffer),
            Self::Tls(stream) => std::pin::Pin::new(stream.as_mut()).poll_read(context, buffer),
        }
    }
}

impl AsyncWrite for UpstreamStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
        buffer: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(stream) => std::pin::Pin::new(stream).poll_write(context, buffer),
            Self::Tls(stream) => std::pin::Pin::new(stream.as_mut()).poll_write(context, buffer),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => std::pin::Pin::new(stream).poll_flush(context),
            Self::Tls(stream) => std::pin::Pin::new(stream.as_mut()).poll_flush(context),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => std::pin::Pin::new(stream).poll_shutdown(context),
            Self::Tls(stream) => std::pin::Pin::new(stream.as_mut()).poll_shutdown(context),
        }
    }
}

type ConnectedUpstream = (
    UpstreamStream,
    String,
    Option<(Arc<ProxyUpstream>, StreamEndpoint)>,
);

/// Connect the stream server's target, applying `proxy_bind`, the emitted
/// upstream PROXY v1 header (before TLS starts, matching nginx's ordering) and
/// the `proxy_ssl*` upstream TLS client policy.
async fn connect_once(
    generation: &Arc<super::runtime::RuntimeGeneration>,
    server: &StreamServerConfig,
    target: &StreamTargetConfig,
    peer: SocketAddr,
    local: SocketAddr,
    proxy_protocol: bool,
    round_robin: &AtomicUsize,
) -> Result<ConnectedUpstream, (ExchangeKind, Option<(Arc<ProxyUpstream>, StreamEndpoint)>)> {
    let (host, port, health) = match resolve_target(generation, target, peer.ip(), round_robin) {
        Some(resolved) => resolved,
        None => {
            tracing::warn!(peer = %peer, "stream target cannot be resolved");
            return Err((ExchangeKind::Transport, None));
        }
    };
    let authority = format!("{host}:{port}");
    let mut tcp = match connect_tcp(&host, port, server.bind_address).await {
        Ok(upstream) => upstream,
        Err(error) => {
            tracing::warn!(peer = %peer, target = %authority, error = %error, "stream upstream connect failed");
            return Err((ExchangeKind::Transport, health));
        }
    };
    if proxy_protocol {
        if let Err(error) = send_proxy_protocol_v1_addrs(peer, local, &mut tcp).await {
            tracing::warn!(peer = %peer, error = %error, "stream PROXY protocol header failed");
            return Err((ExchangeKind::Transport, health));
        }
    }
    // nginx `proxy_ssl on`: the upstream handshake starts after the emitted
    // PROXY header.
    if server.upstream_tls.is_some() {
        let Some(policy) = server.upstream_tls.as_ref() else {
            unreachable!("checked above")
        };
        let config = match build_stream_upstream_tls(generation, server) {
            Ok(config) => config,
            Err(()) => return Err((ExchangeKind::Transport, health)),
        };
        let name = policy.server_name.clone().unwrap_or_else(|| host.clone());
        let Ok(server_name) = rustls::pki_types::ServerName::try_from(name) else {
            tracing::warn!(peer = %peer, target = %authority, "stream proxy_ssl server name is invalid");
            return Err((ExchangeKind::Transport, health));
        };
        let connector = tokio_rustls::TlsConnector::from(config);
        let tls = match connector.connect(server_name, tcp).await {
            Ok(stream) => stream,
            Err(error) => {
                tracing::debug!(peer = %peer, target = %authority, error = %error, "stream proxy_ssl handshake failed");
                return Err((ExchangeKind::Transport, health));
            }
        };
        return Ok((UpstreamStream::Tls(Box::new(tls)), authority, health));
    }
    Ok((UpstreamStream::Plain(tcp), authority, health))
}

/// Connect a TCP upstream with an optional `proxy_bind` source address.
async fn connect_tcp(
    host: &str,
    port: u16,
    bind_address: Option<IpAddr>,
) -> std::io::Result<TcpStream> {
    let Some(bind_address) = bind_address else {
        return TcpStream::connect((host, port)).await;
    };
    let remote = tokio::net::lookup_host((host, port))
        .await?
        .next()
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::AddrNotAvailable,
                "stream upstream target resolved no address",
            )
        })?;
    let socket = if remote.is_ipv4() {
        tokio::net::TcpSocket::new_v4()?
    } else {
        tokio::net::TcpSocket::new_v6()?
    };
    socket.bind(SocketAddr::new(bind_address, 0))?;
    socket.connect(remote).await
}

/// Build the stream server's `proxy_ssl*` client config from the shared
/// upstream TLS policy builder. The compiled app resolves the material paths
/// under a synthetic upstream id (`stream-tls:<id>`), so the cache and sandbox
/// rules are the same as the HTTP proxy/gRPC adapters.
fn build_stream_upstream_tls(
    generation: &Arc<super::runtime::RuntimeGeneration>,
    server: &StreamServerConfig,
) -> Result<Arc<rustls::ClientConfig>, ()> {
    let policy = server.upstream_tls.clone().ok_or(())?;
    let synthetic: sdkwork_webserver_core::UpstreamConfig = serde_json::from_value(serde_json::json!({
        "id": format!("stream-tls:{}", server.id),
        "targets": [],
        "tls": policy,
    }))
    .map_err(|error| {
        tracing::warn!(stream_id = %server.id, error = %error, "stream upstream TLS policy is invalid");
    })?;
    super::upstream_tls::build_upstream_tls_config(&generation.app, &synthetic)
        .map(Arc::new)
        .map_err(|error| {
            tracing::warn!(stream_id = %server.id, error = %error, "stream upstream TLS config could not be built");
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExchangeKind {
    Transport,
    Timeout,
}

/// Connect with the nginx stream `proxy_next_upstream*` retry budget.
///
/// Retries apply to a declared upstream (a literal target has one server, so
/// nginx cannot select another); the budget is bounded by
/// `proxy_next_upstream_tries` (0 = one attempt per target, capped) and the
/// `proxy_next_upstream_timeout` deadline.
async fn connect_with_retry(
    generation: &Arc<super::runtime::RuntimeGeneration>,
    server: &StreamServerConfig,
    target: &StreamTargetConfig,
    peer: SocketAddr,
    local: SocketAddr,
    proxy_protocol: bool,
    round_robin: &AtomicUsize,
) -> Result<ConnectedUpstream, ()> {
    let retry_conditions = if server.next_upstream.is_empty() {
        vec![
            sdkwork_webserver_core::UpstreamRetryCondition::TransportFailure,
            sdkwork_webserver_core::UpstreamRetryCondition::Timeout,
        ]
    } else {
        server.next_upstream.clone()
    };
    let max_attempts = match &target {
        StreamTargetConfig::Upstream { name } => {
            let target_count = generation
                .upstreams
                .get(name)
                .map(|upstream| upstream.target_count())
                .unwrap_or(1)
                .max(1);
            server
                .next_upstream_tries
                .map(usize::from)
                .filter(|tries| *tries > 1)
                .unwrap_or(target_count)
                .min(target_count)
        }
        StreamTargetConfig::Literal { .. } => 1,
    };
    let budget = Duration::from_millis(server.next_upstream_timeout_ms.unwrap_or(30_000).max(1));
    let started = std::time::Instant::now();
    let mut last: Option<(ExchangeKind, Option<(Arc<ProxyUpstream>, StreamEndpoint)>)> = None;
    for attempt in 0..max_attempts {
        if attempt > 0 && started.elapsed() >= budget {
            break;
        }
        match timeout(
            STREAM_CONNECT_TIMEOUT,
            connect_once(
                generation,
                server,
                target,
                peer,
                local,
                proxy_protocol,
                round_robin,
            ),
        )
        .await
        {
            Ok(Ok(connected)) => return Ok(connected),
            Ok(Err(failure)) => {
                if let Some((upstream, endpoint)) = &failure.1 {
                    upstream.record_stream_failure(endpoint);
                }
                let retryable = match failure.0 {
                    ExchangeKind::Transport => retry_conditions.contains(
                        &sdkwork_webserver_core::UpstreamRetryCondition::TransportFailure,
                    ),
                    ExchangeKind::Timeout => retry_conditions
                        .contains(&sdkwork_webserver_core::UpstreamRetryCondition::Timeout),
                };
                last = Some(failure);
                if !retryable {
                    break;
                }
            }
            Err(_) => {
                let retryable = retry_conditions
                    .contains(&sdkwork_webserver_core::UpstreamRetryCondition::Timeout);
                last = Some((ExchangeKind::Timeout, None));
                if !retryable {
                    break;
                }
            }
        }
    }
    let _ = last;
    Err(())
}

/// The effective client address for one connection: a PROXY protocol header
/// source replaces the peer only when `set_real_ip_from` trusts the connecting
/// peer (bounded stream realip form); otherwise the socket peer is used.
fn effective_client_peer(
    peer: SocketAddr,
    header: Option<InboundProxyHeader>,
    trusted: &[ipnet::IpNet],
) -> SocketAddr {
    let Some(header) = header else {
        return peer;
    };
    let Some(source) = header.source else {
        return peer;
    };
    if trusted.iter().any(|network| network.contains(&peer.ip())) {
        source
    } else {
        peer
    }
}

/// Relay options resolved from one stream server config.
#[derive(Clone, Copy, Default)]
struct StreamRelayOptions {
    half_close: bool,
    download_rate: u64,
    upload_rate: u64,
}

impl StreamRelayOptions {
    fn from_config(config: &StreamServerConfig) -> Self {
        Self {
            half_close: config.half_close,
            download_rate: config.download_rate.unwrap_or(0),
            upload_rate: config.upload_rate.unwrap_or(0),
        }
    }
}

/// Select the stream virtual server for one SNI value.
fn select_sni_server<'a>(
    group: &'a [StreamServerConfig],
    sni: Option<&str>,
    default_index: usize,
) -> &'a StreamServerConfig {
    let Some(sni) = sni else {
        return &group[default_index];
    };
    let sni = sni.trim_end_matches('.').to_ascii_lowercase();
    // Exact names outrank leading wildcards, which outrank trailing wildcards
    // (the nginx stream `server_name` search order).
    if let Some(exact) = group.iter().find(|server| {
        server
            .server_names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&sni))
    }) {
        return exact;
    }
    let mut best: Option<(&StreamServerConfig, usize)> = None;
    for server in group {
        for name in &server.server_names {
            if let Some(suffix) = name.strip_prefix("*.") {
                let suffix = suffix.to_ascii_lowercase();
                if sni.len() > suffix.len()
                    && sni.ends_with(&format!(".{suffix}"))
                    && best.map_or(true, |(_, length)| suffix.len() > length)
                {
                    best = Some((server, suffix.len()));
                }
            }
        }
    }
    if let Some((server, _)) = best {
        return server;
    }
    for server in group {
        for name in &server.server_names {
            if let Some(prefix) = name.strip_suffix(".*") {
                let prefix = prefix.to_ascii_lowercase();
                if sni.starts_with(&format!("{prefix}.")) {
                    return server;
                }
            }
        }
    }
    &group[default_index]
}

/// Serve one connection on an SNI virtual-server (`server_name`) listener: read
/// the ClientHello once, select the server, then run the selected server's
/// bounded `ssl_preread` content path with the buffered preface.
#[allow(clippy::too_many_arguments)]
async fn serve_sni_group_connection(
    runtime: Arc<DataPlaneRuntime>,
    downstream: TcpStream,
    peer: SocketAddr,
    group: Vec<StreamServerConfig>,
    preread_timeout: Duration,
    preread_buffer_bytes: usize,
    proxy_timeout: Duration,
    maximum_age: Duration,
    inbound_header: Option<InboundProxyHeader>,
) -> Result<(), ()> {
    if group.is_empty() {
        return Err(());
    }
    let default_index = group
        .iter()
        .position(|server| server.default_server)
        .unwrap_or(0);
    let (downstream, preface, sni) = match preread_client_hello(
        downstream,
        preread_timeout,
        preread_buffer_bytes,
    )
    .await
    {
        Ok(peeked) => peeked,
        Err(error) => {
            tracing::warn!(peer = %peer, error = %error, "stream SNI virtual-server preread failed");
            return Err(());
        }
    };
    let selected = select_sni_server(&group, sni.as_deref(), default_index).clone();
    let generation = runtime.current();
    let selected_peer = effective_client_peer(
        peer,
        inbound_header,
        &parse_trusted_cidrs(&selected.real_ip_trusted_cidrs),
    );
    if evaluate_access_first_match_allow(selected_peer.ip(), &selected.access)
        == AccessDecision::Deny
    {
        tracing::debug!(
            stream_id = %selected.id,
            peer = %peer,
            "stream SNI virtual-server denied by allow/deny rules"
        );
        return Ok(());
    }
    let limit_lease = {
        let rules = (selected.limit_conn.clone(), selected.limit_conn_dry_run);
        if rules.0.is_empty() {
            None
        } else {
            match generation
                .stream_limit_conn
                .stream_admit(selected_peer.ip(), &rules.0, rules.1)
            {
                Ok((lease, _)) => Some(lease),
                Err(_) => {
                    tracing::debug!(
                        stream_id = %selected.id,
                        peer = %peer,
                        "stream SNI virtual-server rejected by limit_conn"
                    );
                    return Ok(());
                }
            }
        }
    };
    let _limit_lease = limit_lease;
    let round_robin = Arc::new(AtomicUsize::new(0));
    let server_id = selected.id.clone();
    serve_stream_connection(
        runtime,
        &server_id,
        downstream,
        selected_peer,
        round_robin,
        proxy_timeout,
        selected.proxy_protocol,
        maximum_age,
        None,
        preread_timeout,
        preread_buffer_bytes,
        Some(selected),
        Some(preface),
        sni,
        inbound_header,
    )
    .await
}

fn resolve_target(
    generation: &Arc<super::runtime::RuntimeGeneration>,
    target: &StreamTargetConfig,
    client_ip: IpAddr,
    round_robin: &AtomicUsize,
) -> Option<(String, u16, Option<(Arc<ProxyUpstream>, StreamEndpoint)>)> {
    match target {
        StreamTargetConfig::Literal { host, port } => Some((host.clone(), *port, None)),
        StreamTargetConfig::Upstream { name } => {
            if let Some(upstream) = generation.upstreams.get(name) {
                let endpoint = upstream.select_stream_endpoint(client_ip)?;
                return Some((
                    endpoint.host.clone(),
                    endpoint.port,
                    Some((Arc::clone(upstream), endpoint)),
                ));
            }
            // Fallback when generation has no live ProxyUpstream (should be rare).
            let upstream = generation.app.upstream(name)?;
            let targets = upstream
                .targets
                .iter()
                .filter(|target| !target.backup)
                .collect::<Vec<_>>();
            if targets.is_empty() {
                return None;
            }
            let index = round_robin.fetch_add(1, Ordering::Relaxed) % targets.len();
            let url = url::Url::parse(&targets[index].url).ok()?;
            let port = url.port_or_known_default()?;
            Some((url.host_str()?.to_owned(), port, None))
        }
    }
}

fn build_stream_tls_acceptor(
    generation: &Arc<super::runtime::RuntimeGeneration>,
    certificate_ref: &str,
    client_auth: Option<&sdkwork_webserver_core::ClientAuthConfig>,
) -> Result<TlsAcceptor, String> {
    install_crypto_provider().map_err(|error| error.to_string())?;
    let provider = rustls::crypto::CryptoProvider::get_default()
        .ok_or_else(|| "rustls crypto provider is not installed".to_owned())?;
    let certificate = generation
        .app
        .certificate(certificate_ref)
        .ok_or_else(|| format!("missing stream certificate {certificate_ref}"))?;
    let (certificate_file, private_key_file) = generation
        .app
        .certificate_paths(&certificate.id)
        .ok_or_else(|| format!("missing stream certificate files for {certificate_ref}"))?;
    let loaded = load_certified_key(
        certificate_file,
        private_key_file,
        generation.app.certificate_chain_file(&certificate.id),
        generation
            .app
            .certificate_ocsp_stapling_file(&certificate.id),
        &certificate.server_names,
        provider,
    )
    .map_err(|error| error.to_string())?;
    // A certificate always declares at least one server name, and the schema
    // enforces it. Substituting a name would index this certificate under a name
    // it need not cover, so an empty declaration is refused rather than guessed.
    if certificate.server_names.is_empty() {
        return Err(format!(
            "stream certificate {certificate_ref} declares no server name"
        ));
    }
    let server_names = certificate.server_names.clone();
    let server_name_regexes = certificate.server_name_regexes.clone();
    let server_config = build_sni_server_config(
        vec![(server_names, server_name_regexes, loaded)],
        TlsVersion::Tls12,
        TlsVersion::Tls13,
        &[],
        client_auth,
    )?;
    Ok(TlsAcceptor::from(server_config))
}

/// Peek a TLS ClientHello, returning the buffered preface and optional SNI.
async fn preread_client_hello(
    mut downstream: TcpStream,
    preread_timeout: Duration,
    maximum_bytes: usize,
) -> Result<(TcpStream, Vec<u8>, Option<String>), String> {
    let maximum_bytes = maximum_bytes.max(512);
    let mut buffer = vec![0_u8; maximum_bytes];
    let mut filled = 0_usize;
    loop {
        if filled >= maximum_bytes {
            return Err("ClientHello exceeds stream ssl_preread buffer".to_owned());
        }
        let read = timeout(preread_timeout, downstream.read(&mut buffer[filled..]))
            .await
            .map_err(|_| "ssl_preread idle timeout".to_owned())?
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("downstream closed before ClientHello".to_owned());
        }
        filled += read;
        if let Some((needed, sni)) = try_parse_client_hello_sni(&buffer[..filled]) {
            if filled < needed {
                continue;
            }
            buffer.truncate(filled.max(needed));
            return Ok((downstream, buffer, sni));
        }
        if filled >= 5 {
            let record_len = u16::from_be_bytes([buffer[3], buffer[4]]) as usize;
            let total = 5 + record_len;
            if total > maximum_bytes {
                return Err("TLS record exceeds stream ssl_preread buffer".to_owned());
            }
            if filled >= total {
                buffer.truncate(filled);
                return Ok((downstream, buffer, None));
            }
        }
    }
}

/// Returns `(record_bytes_needed, sni)` when the first TLS handshake record is
/// complete enough to inspect; `None` while more bytes are required.
fn try_parse_client_hello_sni(bytes: &[u8]) -> Option<(usize, Option<String>)> {
    if bytes.len() < 5 {
        return None;
    }
    if bytes[0] != 0x16 {
        return Some((bytes.len(), None));
    }
    let record_len = u16::from_be_bytes([bytes[3], bytes[4]]) as usize;
    let total = 5 + record_len;
    if bytes.len() < total {
        return None;
    }
    let handshake = &bytes[5..total];
    if handshake.len() < 4 || handshake[0] != 0x01 {
        return Some((total, None));
    }
    let hello_len =
        ((handshake[1] as usize) << 16) | ((handshake[2] as usize) << 8) | handshake[3] as usize;
    if handshake.len() < 4 + hello_len {
        return None;
    }
    let body = &handshake[4..4 + hello_len];
    // legacy_version(2) + random(32) + session_id
    if body.len() < 35 {
        return Some((total, None));
    }
    let mut offset = 34;
    let session_len = body[offset] as usize;
    offset += 1;
    if body.len() < offset + session_len + 2 {
        return Some((total, None));
    }
    offset += session_len;
    let cipher_len = u16::from_be_bytes([body[offset], body[offset + 1]]) as usize;
    offset += 2;
    if body.len() < offset + cipher_len + 1 {
        return Some((total, None));
    }
    offset += cipher_len;
    let compression_len = body[offset] as usize;
    offset += 1;
    if body.len() < offset + compression_len + 2 {
        return Some((total, None));
    }
    offset += compression_len;
    let extensions_len = u16::from_be_bytes([body[offset], body[offset + 1]]) as usize;
    offset += 2;
    if body.len() < offset + extensions_len {
        return Some((total, None));
    }
    let mut extensions = &body[offset..offset + extensions_len];
    while extensions.len() >= 4 {
        let ext_type = u16::from_be_bytes([extensions[0], extensions[1]]);
        let ext_len = u16::from_be_bytes([extensions[2], extensions[3]]) as usize;
        extensions = &extensions[4..];
        if extensions.len() < ext_len {
            break;
        }
        let ext_data = &extensions[..ext_len];
        extensions = &extensions[ext_len..];
        if ext_type == 0 {
            // server_name
            if ext_data.len() < 5 {
                break;
            }
            let list_len = u16::from_be_bytes([ext_data[0], ext_data[1]]) as usize;
            if ext_data.len() < 2 + list_len {
                break;
            }
            let mut list = &ext_data[2..2 + list_len];
            while list.len() >= 3 {
                let name_type = list[0];
                let name_len = u16::from_be_bytes([list[1], list[2]]) as usize;
                list = &list[3..];
                if list.len() < name_len {
                    break;
                }
                if name_type == 0 {
                    let sni = String::from_utf8_lossy(&list[..name_len]).into_owned();
                    return Some((total, Some(sni)));
                }
                list = &list[name_len..];
            }
            break;
        }
    }
    Some((total, None))
}

async fn send_proxy_protocol_v1_addrs(
    peer: SocketAddr,
    local: SocketAddr,
    upstream: &mut TcpStream,
) -> std::io::Result<()> {
    let (family, source, destination) = match (peer.ip(), local.ip()) {
        (std::net::IpAddr::V4(source), std::net::IpAddr::V4(destination)) => {
            ("TCP4", source.to_string(), destination.to_string())
        }
        (std::net::IpAddr::V6(source), std::net::IpAddr::V6(destination)) => {
            ("TCP6", source.to_string(), destination.to_string())
        }
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "mixed IPv4/IPv6 stream peer families are not representable in PROXY v1",
            ))
        }
    };
    let header = format!(
        "PROXY {family} {source} {destination} {} {}\r\n",
        peer.port(),
        local.port()
    );
    upstream.write_all(header.as_bytes()).await?;
    upstream.flush().await?;
    Ok(())
}

async fn proxy_bidirectional<D, U>(
    downstream: D,
    upstream: U,
    idle: Duration,
    maximum_age: Duration,
    options: StreamRelayOptions,
) where
    D: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    let (mut downstream_read, mut downstream_write) = tokio::io::split(downstream);
    let (mut upstream_read, mut upstream_write) = tokio::io::split(upstream);
    let mut upload = Box::pin(copy_with_idle_timeout(
        &mut downstream_read,
        &mut upstream_write,
        idle,
        options.upload_rate,
        options.half_close,
    ));
    let mut download = Box::pin(copy_with_idle_timeout(
        &mut upstream_read,
        &mut downstream_write,
        idle,
        options.download_rate,
        options.half_close,
    ));
    let deadline = if maximum_age.is_zero() {
        None
    } else {
        Some(tokio::time::Instant::now() + maximum_age)
    };
    let finished = tokio::select! {
        _ = &mut upload => Direction::Upload,
        _ = &mut download => Direction::Download,
        _ = wait_for_deadline(deadline) => return,
    };
    if !options.half_close {
        return;
    }
    // nginx `proxy_half_close on`: keep the surviving direction flowing after
    // one side reaches EOF (the pump already half-closed the dead direction).
    let mut upload_done = matches!(finished, Direction::Upload);
    let mut download_done = matches!(finished, Direction::Download);
    while !(upload_done && download_done) {
        tokio::select! {
            _ = &mut upload, if !upload_done => upload_done = true,
            _ = &mut download, if !download_done => download_done = true,
            _ = wait_for_deadline(deadline) => return,
        }
    }
}

#[derive(Clone, Copy)]
enum Direction {
    Upload,
    Download,
}

async fn wait_for_deadline(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending::<()>().await,
    }
}

/// Copy one direction with an idle deadline and an optional byte-rate cap
/// (`proxy_download_rate` / `proxy_upload_rate`; 0 = unlimited).
async fn copy_with_idle_timeout<R, W>(
    reader: &mut R,
    writer: &mut W,
    idle: Duration,
    rate: u64,
    half_close: bool,
) where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buffer = vec![0_u8; STREAM_COPY_BUFFER_BYTES];
    let mut transferred = 0_u64;
    let started = std::time::Instant::now();
    loop {
        if rate > 0 && transferred > 0 {
            let expected = Duration::from_secs_f64(transferred as f64 / rate as f64);
            if expected > started.elapsed() {
                sleep(expected - started.elapsed()).await;
            }
        }
        let read = match timeout(idle, reader.read(&mut buffer)).await {
            Ok(Ok(read)) => read,
            Ok(Err(_)) | Err(_) => {
                if half_close {
                    let _ = writer.shutdown().await;
                }
                return;
            }
        };
        if read == 0 {
            if half_close {
                let _ = writer.shutdown().await;
            }
            return;
        }
        if writer.write_all(&buffer[..read]).await.is_err() {
            return;
        }
        transferred = transferred.saturating_add(read as u64);
    }
}
