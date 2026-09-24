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
use tokio::{net::UdpSocket, sync::watch, sync::OwnedSemaphorePermit};

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

/// Per-listener session table shared between the accept loop and the spawned
/// new-session setup tasks. Locks are held only across map operations.
#[derive(Default)]
struct UdpSessions {
    sessions: HashMap<SocketAddr, Arc<Mutex<Session>>>,
    /// Client addresses whose session setup task is still resolving/binding.
    /// Datagrams from a pending address are dropped (UDP senders retransmit);
    /// without this guard every retransmission would spawn a second setup.
    pending: HashMap<SocketAddr, Instant>,
}

impl UdpSessions {
    fn reap(&mut self, now: Instant, proxy_timeout: Duration) {
        // Idle sessions older than proxy_timeout are reaped; the session's
        // own forwarder also stops after the same window.
        self.sessions.retain(|_, session| {
            now.duration_since(
                session
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .last_activity,
            ) < proxy_timeout
        });
        // Pending entries clear themselves on every setup exit path; this is
        // a crash-only backstop so a cancelled task cannot wedge an address.
        self.pending
            .retain(|_, started| now.duration_since(*started) < UDP_TARGET_RESOLVE_TIMEOUT * 2);
    }
}

/// Accept loop for one UDP stream listener. Targets are resolved from the
/// current configuration generation (reloads take effect per datagram).
///
/// New-session setup (DNS resolve + upstream bind) runs in a spawned task so
/// one slow resolution cannot stall the single datagram loop: a flood of
/// spoofed source addresses is bounded by `connection_permits`, which the
/// pending setup holds for its whole lifetime, and datagrams for still-pending
/// addresses are dropped rather than queued.
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
    let shared = Arc::new(Mutex::new(UdpSessions::default()));
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
                let session = {
                    let mut state = shared
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.reap(now, proxy_timeout);
                    match state.sessions.get(&client) {
                        Some(session) => {
                            if let Ok(mut guard) = session.lock() {
                                guard.last_activity = now;
                            }
                            Some(session.clone())
                        }
                        None => {
                            if state.pending.contains_key(&client) {
                                // Setup in flight; the client retransmits.
                                continue;
                            }
                            state.pending.insert(client, now);
                            let Some((host, port, _health)) = resolve_udp_target(
                                &generation,
                                &stream_config.target,
                                client.ip(),
                                &round_robin,
                            ) else {
                                state.pending.remove(&client);
                                continue;
                            };
                            // Acquiring the permit before the spawn keeps the
                            // count of resolving + live sessions inside the
                            // listener's connection budget.
                            let Ok(permit) = runtime.connection_permits.clone().try_acquire_owned()
                            else {
                                state.pending.remove(&client);
                                continue;
                            };
                            let first = buffer[..length].to_vec();
                            tokio::spawn(setup_new_session(
                                shared.clone(),
                                client,
                                first,
                                host,
                                port,
                                proxy_timeout,
                                permit,
                            ));
                            // The setup task forwards this datagram once the
                            // upstream socket exists.
                            None
                        }
                    }
                };
                let Some(session) = session else {
                    continue;
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
                    shared
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .sessions
                        .remove(&client);
                }
            }
        }
    }
}

/// Resolves and binds one new client session off the accept loop, forwards
/// the triggering datagram, and starts the reply forwarder. Every failure
/// path clears the pending marker and drops the permit.
async fn setup_new_session(
    shared: Arc<Mutex<UdpSessions>>,
    client: SocketAddr,
    first: Vec<u8>,
    host: String,
    port: u16,
    proxy_timeout: Duration,
    permit: OwnedSemaphorePermit,
) {
    let clear_pending = |shared: &Arc<Mutex<UdpSessions>>| {
        if let Ok(mut state) = shared.lock() {
            state.pending.remove(&client);
        }
    };
    let resolved = tokio::time::timeout(UDP_TARGET_RESOLVE_TIMEOUT, async {
        tokio::net::lookup_host((host.as_str(), port))
            .await
            .ok()
            .and_then(|mut addresses| addresses.next())
    })
    .await;
    let Some(target) = resolved.unwrap_or(None) else {
        clear_pending(&shared);
        return;
    };
    let Ok(upstream) = UdpSocket::bind("0.0.0.0:0").await else {
        clear_pending(&shared);
        return;
    };
    let upstream = Arc::new(upstream);
    let new_session = Arc::new(Mutex::new(Session {
        upstream: upstream.clone(),
        client,
        target,
        last_activity: Instant::now(),
        _permit: permit,
    }));
    {
        let Ok(mut state) = shared.lock() else {
            return;
        };
        state.pending.remove(&client);
        state.sessions.insert(client, new_session.clone());
    }
    spawn_reply_forwarder(new_session, proxy_timeout);
    let _ = upstream.send_to(&first, target).await;
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
) -> Option<(
    String,
    u16,
    Option<(
        Arc<super::proxy::ProxyUpstream>,
        super::proxy::StreamEndpoint,
    )>,
)> {
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
