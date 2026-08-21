//! UDP stream proxying (nginx `listen … udp`).
//!
//! One datagram listener per `protocol = "udp"` stream server. Client
//! addresses are NAT-mapped to per-client upstream sockets; replies are
//! forwarded back to the originating client. Sessions idle out after
//! `proxy_timeout` (nginx `proxy_timeout` semantics). TLS terminate,
//! `ssl_preread`, and PROXY protocol are rejected at materialization for
//! UDP listeners.

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use sdkwork_webserver_core::{StreamServerConfig, StreamTargetConfig};
use tokio::{
    net::UdpSocket,
    sync::watch,
    sync::OwnedSemaphorePermit,
};

use super::runtime::RuntimeGeneration;
use crate::DataPlaneError;

const UDP_DATAGRAM_MAX_BYTES: usize = 65_507;
const UDP_TARGET_RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct PreparedUdpStreamListener {
    pub(crate) id: String,
    socket: Arc<UdpSocket>,
    proxy_timeout: Duration,
    round_robin: Arc<AtomicUsize>,
}

pub(crate) async fn prepare_udp_stream_listener(
    config: &StreamServerConfig,
) -> Result<PreparedUdpStreamListener, DataPlaneError> {
    let address = format!("{}:{}", config.bind, config.port);
    let socket = UdpSocket::bind(&address)
        .await
        .map_err(|source| DataPlaneError::Listener {
            listener_id: config.id.clone(),
            source,
        })?;
    tracing::info!(
        stream_id = %config.id,
        address = %socket.local_addr().map(|addr| addr.to_string()).unwrap_or_else(|_| address),
        protocol = "udp",
        "udp stream listener prepared"
    );
    Ok(PreparedUdpStreamListener {
        id: config.id.clone(),
        socket: Arc::new(socket),
        proxy_timeout: Duration::from_millis(config.proxy_timeout_ms),
        round_robin: Arc::new(AtomicUsize::new(0)),
    })
}

struct Session {
    upstream: Arc<UdpSocket>,
    client: SocketAddr,
    target: SocketAddr,
    last_activity: Instant,
    _permit: OwnedSemaphorePermit,
}

/// Accept loop for one UDP stream listener. Targets are resolved from the
/// current configuration generation (reloads take effect per datagram).
pub(crate) async fn serve_udp_stream_listener(
    runtime: Arc<super::DataPlaneRuntime>,
    listener: PreparedUdpStreamListener,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), DataPlaneError> {
    let PreparedUdpStreamListener {
        id,
        socket,
        proxy_timeout,
        round_robin,
    } = listener;
    let mut sessions: HashMap<SocketAddr, Arc<Mutex<Session>>> = HashMap::new();
    let mut buffer = vec![0_u8; UDP_DATAGRAM_MAX_BYTES];
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return Ok(());
                }
            }
            received = socket.recv_from(&mut buffer) => {
                let Ok((length, client)) = received else {
                    return Ok(());
                };
                let generation = runtime.current();
                let Some(stream_config) = generation.app.streams().iter().find(|stream| stream.id == id)
                else {
                    continue;
                };
                let now = Instant::now();
                // Idle sessions older than proxy_timeout are reaped; the
                // session's own forwarder also stops after the same window.
                sessions.retain(|_, session| {
                    now.duration_since(session.lock().map_or(now, |guard| guard.last_activity)) < proxy_timeout
                });
                let session = match sessions.entry(client) {
                    std::collections::hash_map::Entry::Occupied(mut occupied) => {
                        if let Ok(mut guard) = occupied.get_mut().lock() {
                            guard.last_activity = now;
                        }
                        occupied.into_mut().clone()
                    }
                    std::collections::hash_map::Entry::Vacant(vacant) => {
                        let Some((host, port, _health)) = resolve_udp_target(
                            &generation,
                            &stream_config.target,
                            client.ip(),
                            &round_robin,
                        ) else {
                            continue;
                        };
                        let Ok(permit) = runtime.connection_permits.clone().try_acquire_owned() else {
                            continue;
                        };
                        let Ok(mut addresses) =
                            tokio::time::timeout(UDP_TARGET_RESOLVE_TIMEOUT, tokio::net::lookup_host((host.as_str(), port))).await
                        else {
                            continue;
                        };
                        let Some(target) = addresses.ok().and_then(|mut addresses| addresses.next()) else {
                            continue;
                        };
                        let Ok(upstream) = UdpSocket::bind("0.0.0.0:0").await else {
                            continue;
                        };
                        let upstream = Arc::new(upstream);
                        let new_session = Arc::new(Mutex::new(Session {
                            upstream: upstream.clone(),
                            client,
                            target,
                            last_activity: now,
                            _permit: permit,
                        }));
                        spawn_reply_forwarder(new_session.clone(), proxy_timeout);
                        vacant.insert(new_session.clone());
                        new_session
                    }
                };
                let send_target = match session.lock() {
                    Ok(guard) => Some((guard.target, guard.upstream.clone())),
                    Err(_) => None,
                };
                let forwarded = match send_target {
                    Some((target, upstream)) => {
                        upstream.send_to(&buffer[..length], target).await
                    }
                    None => Ok(0),
                };
                if forwarded.is_err() {
                    sessions.remove(&client);
                }
            }
        }
    }
}

fn spawn_reply_forwarder(session: Arc<Mutex<Session>>, proxy_timeout: Duration) {
    tokio::spawn(async move {
        let mut buffer = vec![0_u8; UDP_DATAGRAM_MAX_BYTES];
        loop {
            let (upstream, client) = {
                let Ok(guard) = session.lock() else {
                    return;
                };
                (guard.upstream.clone(), guard.client)
            };
            let received =
                tokio::time::timeout(proxy_timeout, upstream.recv_from(&mut buffer)).await;
            match received {
                Ok(Ok((length, _source))) => {
                    if upstream.send_to(&buffer[..length], client).await.is_err() {
                        return;
                    }
                    if let Ok(mut guard) = session.lock() {
                        guard.last_activity = Instant::now();
                    }
                }
                _ => return, // idle timeout or socket error: end the session
            }
        }
    });
}

fn resolve_udp_target(
    generation: &Arc<RuntimeGeneration>,
    target: &StreamTargetConfig,
    client_ip: std::net::IpAddr,
    round_robin: &AtomicUsize,
) -> Option<(String, u16, Option<(Arc<super::proxy::ProxyUpstream>, super::proxy::StreamEndpoint)>)> {
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
