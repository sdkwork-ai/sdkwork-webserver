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

use sdkwork_webserver_core::{StreamServerConfig, StreamTargetConfig, StreamTlsMode, TlsVersion};
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
const STREAM_PREREAD_MAX_BYTES: usize = 16 * 1024;

pub(crate) struct PreparedStreamListener {
    pub(crate) id: String,
    socket: TcpListener,
    proxy_timeout: Duration,
    proxy_protocol: bool,
    round_robin: Arc<AtomicUsize>,
    tls: Option<StreamTlsMode>,
}

pub(crate) async fn prepare_stream_listener(
    config: &StreamServerConfig,
) -> Result<PreparedStreamListener, DataPlaneError> {
    let address = format!("{}:{}", config.bind, config.port);
    let socket = TcpListener::bind(&address).await.map_err(|source| {
        DataPlaneError::Listener {
            listener_id: config.id.clone(),
            source,
        }
    })?;
    let local = socket.local_addr().map_err(|source| DataPlaneError::Listener {
        listener_id: config.id.clone(),
        source,
    })?;
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
    let maximum_age = Duration::from_millis(
        runtime.current().app.config().limits.max_connection_age_ms,
    );
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            accepted = listener.socket.accept() => {
                let (downstream, peer) = match accepted {
                    Ok(accepted) => accepted,
                    Err(error) => {
                        tracing::warn!(stream_id = %listener.id, error = %error, "stream accept failed");
                        continue;
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
                let round_robin = Arc::clone(&listener.round_robin);
                let proxy_timeout = listener.proxy_timeout;
                let proxy_protocol = listener.proxy_protocol;
                let tls = listener.tls.clone();
                tasks.spawn(async move {
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
                    )
                    .await;
                    drop(permit);
                    result
                });
            }
        }
    }
    drop(listener.socket);
    while let Some(result) = tasks.join_next().await {
        if let Err(error) = result {
            tracing::warn!(stream_id = %listener.id, error = %error, "stream connection task failed");
        }
    }
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
    let target = &stream_config.target;

    match tls {
        None => {
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
        Some(StreamTlsMode::Terminate {
            certificate_ref,
            client_auth,
        }) => {
            let local = downstream.local_addr().unwrap_or(peer);
            let resolved_client_auth = client_auth.as_ref().map(|auth| {
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
            });
            let acceptor = match build_stream_tls_acceptor(
                &generation,
                certificate_ref,
                resolved_client_auth.as_ref(),
            ) {
                Ok(acceptor) => acceptor,
                Err(error) => {
                    tracing::warn!(stream_id, peer = %peer, error = %error, "stream TLS acceptor failed");
                    return Err(());
                }
            };
            let tls_downstream = match acceptor.accept(downstream).await {
                Ok(stream) => stream,
                Err(error) => {
                    tracing::warn!(stream_id, peer = %peer, error = %error, "stream TLS handshake failed");
                    return Err(());
                }
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
            let (downstream, preface, sni) = match preread_client_hello(downstream).await {
                Ok(peeked) => peeked,
                Err(error) => {
                    tracing::warn!(stream_id, peer = %peer, error = %error, "stream ssl_preread failed");
                    return Err(());
                }
            };
            if let Some(sni) = sni.as_deref() {
                tracing::debug!(stream_id, peer = %peer, sni, "stream ssl_preread observed SNI");
            }
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
        StreamTargetConfig::Literal { host, port } => {
            Some((host.clone(), *port, None))
        }
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
        &certificate.server_names,
        provider,
    )
    .map_err(|error| error.to_string())?;
    let server_names = if certificate.server_names.is_empty() {
        vec!["localhost".to_owned()]
    } else {
        certificate.server_names.clone()
    };
    let server_config = build_sni_server_config(
        vec![(server_names, loaded.certified_key)],
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
) -> Result<(TcpStream, Vec<u8>, Option<String>), String> {
    let mut buffer = vec![0_u8; STREAM_PREREAD_MAX_BYTES];
    let mut filled = 0_usize;
    loop {
        if filled >= STREAM_PREREAD_MAX_BYTES {
            return Err("ClientHello exceeds stream ssl_preread buffer".to_owned());
        }
        let read = timeout(STREAM_CONNECT_TIMEOUT, downstream.read(&mut buffer[filled..]))
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
            if total > STREAM_PREREAD_MAX_BYTES {
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
    let hello_len = ((handshake[1] as usize) << 16)
        | ((handshake[2] as usize) << 8)
        | handshake[3] as usize;
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
    let local = downstream.local_addr().unwrap_or_else(|_| peer);
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
