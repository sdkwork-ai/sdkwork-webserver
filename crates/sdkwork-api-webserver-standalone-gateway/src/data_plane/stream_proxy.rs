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
    tls: Option<StreamTlsMode>,
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
        tls: config.tls.clone(),
        tls_acceptor,
        access: config.access.clone(),
        preread_timeout: Duration::from_millis(config.preread_timeout_ms),
        preread_buffer_bytes: config.preread_buffer_bytes,
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
                // nginx stream access phase and `return` handler run before
                // any upstream connection is considered.
                if evaluate_access_first_match_allow(peer.ip(), &listener.access)
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
                            .stream_admit(peer.ip(), &rules.0, rules.1)
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
                let tls = listener.tls.clone();
                let tls_acceptor = listener.tls_acceptor.clone();
                tasks.spawn(async move {
                    // Held for the whole connection: dropping it releases the
                    // zone counters.
                    let _limit_lease = limit_lease;
                    let result = serve_stream_connection(
                        runtime,
                        &stream_id,
                        downstream,
                        peer,
                        round_robin,
                        proxy_timeout,
                        proxy_protocol,
                        maximum_age,
                        tls.as_ref(),
                        tls_acceptor.as_ref(),
                        preread_timeout,
                        preread_buffer_bytes,
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

async fn serve_stream_connection(
    runtime: Arc<DataPlaneRuntime>,
    stream_id: &str,
    downstream: TcpStream,
    peer: SocketAddr,
    round_robin: Arc<AtomicUsize>,
    proxy_timeout: Duration,
    proxy_protocol: bool,
    maximum_age: Duration,
    tls: Option<&StreamTlsMode>,
    tls_acceptor: Option<&TlsAcceptor>,
    preread_timeout: Duration,
    preread_buffer_bytes: usize,
) -> Result<(), ()> {
    let generation = runtime.current();
    let stream_config = generation
        .app
        .streams()
        .iter()
        .find(|stream| stream.id == stream_id);
    let Some(stream_config) = stream_config else {
        tracing::warn!(stream_id, peer = %peer, "stream target is no longer configured");
        return Err(());
    };
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

    match tls {
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
            let (upstream, authority, health) =
                connect_upstream(&generation, target, peer, &round_robin).await?;
            if proxy_protocol {
                if let Err(error) = send_proxy_protocol_v1(&downstream, &upstream, peer).await {
                    tracing::warn!(stream_id, peer = %peer, error = %error, "stream PROXY protocol header failed");
                    if let Some((upstream_ref, endpoint)) = &health {
                        upstream_ref.record_stream_failure(endpoint);
                    }
                    return Err(());
                }
            }
            tracing::info!(stream_id, peer = %peer, target = %authority, "stream connection established");
            proxy_bidirectional(downstream, upstream, proxy_timeout, maximum_age).await;
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
            let (upstream, authority, health) =
                connect_upstream(&generation, target, peer, &round_robin).await?;
            if proxy_protocol {
                if let Err(error) = send_proxy_protocol_v1_addrs(peer, local, &upstream).await {
                    tracing::warn!(stream_id, peer = %peer, error = %error, "stream PROXY protocol header failed");
                    if let Some((upstream_ref, endpoint)) = &health {
                        upstream_ref.record_stream_failure(endpoint);
                    }
                    return Err(());
                }
            }
            tracing::info!(
                stream_id,
                peer = %peer,
                target = %authority,
                "stream TLS-terminated connection established"
            );
            proxy_bidirectional(tls_downstream, upstream, proxy_timeout, maximum_age).await;
            if let Some((upstream_ref, endpoint)) = health {
                upstream_ref.record_stream_success(&endpoint);
            }
            Ok(())
        }
        Some(StreamTlsMode::Preread) => {
            let local = downstream.local_addr().unwrap_or(peer);
            let (downstream, preface, sni) = match preread_client_hello(
                downstream,
                preread_timeout,
                preread_buffer_bytes,
            )
            .await
            {
                Ok(peeked) => peeked,
                Err(error) => {
                    tracing::warn!(stream_id, peer = %peer, error = %error, "stream ssl_preread failed");
                    return Err(());
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
            let (mut upstream, authority, health) =
                connect_upstream(&generation, target, peer, &round_robin).await?;
            if proxy_protocol {
                if let Err(error) = send_proxy_protocol_v1_addrs(peer, local, &upstream).await {
                    tracing::warn!(stream_id, peer = %peer, error = %error, "stream PROXY protocol header failed");
                    if let Some((upstream_ref, endpoint)) = &health {
                        upstream_ref.record_stream_failure(endpoint);
                    }
                    return Err(());
                }
            }
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
            proxy_bidirectional(downstream, upstream, proxy_timeout, maximum_age).await;
            if let Some((upstream_ref, endpoint)) = health {
                upstream_ref.record_stream_success(&endpoint);
            }
            Ok(())
        }
    }
}

type ConnectedUpstream = (
    TcpStream,
    String,
    Option<(Arc<ProxyUpstream>, StreamEndpoint)>,
);

async fn connect_upstream(
    generation: &Arc<super::runtime::RuntimeGeneration>,
    target: &StreamTargetConfig,
    peer: SocketAddr,
    round_robin: &AtomicUsize,
) -> Result<ConnectedUpstream, ()> {
    let (host, port, health) = match resolve_target(generation, target, peer.ip(), round_robin) {
        Some(resolved) => resolved,
        None => {
            tracing::warn!(peer = %peer, "stream target cannot be resolved");
            return Err(());
        }
    };
    let authority = format!("{host}:{port}");
    let connect = async {
        match TcpStream::connect(&authority).await {
            Ok(upstream) => Ok(upstream),
            Err(error) => {
                tracing::warn!(peer = %peer, target = %authority, error = %error, "stream upstream connect failed");
                Err(())
            }
        }
    };
    match timeout(STREAM_CONNECT_TIMEOUT, connect).await {
        Ok(Ok(upstream)) => Ok((upstream, authority, health)),
        _ => {
            if let Some((upstream_ref, endpoint)) = &health {
                upstream_ref.record_stream_failure(endpoint);
            }
            Err(())
        }
    }
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
    let server_config = build_sni_server_config(
        vec![(server_names, loaded)],
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

async fn send_proxy_protocol_v1(
    downstream: &TcpStream,
    upstream: &TcpStream,
    peer: SocketAddr,
) -> std::io::Result<()> {
    let local = downstream.local_addr().unwrap_or(peer);
    send_proxy_protocol_v1_addrs(peer, local, upstream).await
}

async fn send_proxy_protocol_v1_addrs(
    peer: SocketAddr,
    local: SocketAddr,
    upstream: &TcpStream,
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
    upstream.writable().await?;
    upstream.try_write(header.as_bytes())?;
    Ok(())
}

async fn proxy_bidirectional<D, U>(
    downstream: D,
    upstream: U,
    idle: Duration,
    maximum_age: Duration,
) where
    D: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    let (mut downstream_read, mut downstream_write) = tokio::io::split(downstream);
    let (mut upstream_read, mut upstream_write) = tokio::io::split(upstream);
    let copy_to_upstream = copy_with_idle_timeout(&mut downstream_read, &mut upstream_write, idle);
    let copy_to_downstream =
        copy_with_idle_timeout(&mut upstream_read, &mut downstream_write, idle);
    let age_limit = async {
        if maximum_age.is_zero() {
            std::future::pending::<()>().await
        } else {
            sleep(maximum_age).await
        }
    };
    tokio::select! {
        _ = copy_to_upstream => {}
        _ = copy_to_downstream => {}
        _ = age_limit => {}
    }
}

async fn copy_with_idle_timeout<R, W>(reader: &mut R, writer: &mut W, idle: Duration)
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buffer = vec![0_u8; STREAM_COPY_BUFFER_BYTES];
    loop {
        let read = match timeout(idle, reader.read(&mut buffer)).await {
            Ok(Ok(read)) => read,
            Ok(Err(_)) | Err(_) => return,
        };
        if read == 0 {
            return;
        }
        if writer.write_all(&buffer[..read]).await.is_err() {
            return;
        }
    }
}
