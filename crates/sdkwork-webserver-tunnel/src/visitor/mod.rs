//! Visitor client runtime (FRP `stcp`/`sudp` visitor parity).
//!
//! The other half of a secret route: an operator (or a service) runs a visitor
//! on the machine that needs access, and it publishes the remote private
//! service on a **local** port. Every local connection becomes one stream to
//! the gateway, which bridges it to the provider's own agent — no public
//! surface is ever opened, and the shared key is the only credential the
//! visitor presents beyond its authenticated session.
//!
//! Shape of the runtime: one session task that keeps a connection alive and
//! republishes it, plus one listener task per entry. The listeners bind once
//! and survive a gateway restart, so the local port an operator scripted
//! against stays available while the client reconnects.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use sdkwork_webserver_tunnel_core::{Device, SessionId, TunnelConfig, TunnelError};
use sdkwork_webserver_tunnel_protocol::{
    frame, AuthResult, Authenticate, ControlMessage, Hello, ProtocolVersion, VisitorStreamHeader,
};
use sdkwork_webserver_tunnel_transport::{
    tls, QuicClientTransport, RemoteEndpoint, TunnelClientTransport, TunnelConnection, TunnelStream,
};

use crate::metrics::TunnelMetrics;

/// Control-frame write budget: a stalled control stream must end the session,
/// not park the heartbeat loop forever (same rule as the agent runtime).
const CONTROL_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a local connection waits for a live session before giving up.
///
/// A visitor that is reconnecting must not silently hold thousands of local
/// connections open; the local client sees a closed socket and retries, which
/// is what an operator's tooling already handles.
const WAIT_FOR_SESSION: Duration = Duration::from_secs(10);

/// Which secret kind a visitor entry consumes.
///
/// The protocol is part of the request because the two sides must agree on
/// whether the bytes after the header are a stream or framed datagrams; the
/// gateway refuses a mismatch rather than guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisitorProtocol {
    /// Secret TCP (FRP `stcp`): one tunnel stream per local connection.
    Stcp,
    /// Secret UDP (FRP `sudp`): one tunnel stream per local *client*, carrying
    /// framed datagrams in both directions.
    Sudp,
}

impl VisitorProtocol {
    /// The label that travels with every visitor stream.
    pub const fn wire_label(self) -> &'static str {
        match self {
            Self::Stcp => "stcp",
            Self::Sudp => "sudp",
        }
    }

    /// Parses the CLI/config label.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "stcp" => Some(Self::Stcp),
            "sudp" => Some(Self::Sudp),
            _ => None,
        }
    }
}

/// How many datagrams may wait for one local client before the newest is
/// dropped: a relay must not turn UDP loss into unbounded memory growth.
const DATAGRAM_QUEUE: usize = 256;

/// Aggregate bytes one datagram listener may buffer across all its clients.
/// The per-client queue is counted in packets, so 256 × max payload × N clients
/// is unbounded in bytes; this budget is charged per queued datagram and
/// released when the framed packet has been written. Datagrams that cannot get
/// budget are dropped, never queued.
const DATAGRAM_BUFFER_BUDGET_KIB: usize = 8 * 1024;

/// How long a local datagram client may stay silent before its tunnel stream
/// is released. Long enough for a NAT mapping and a request/response pause,
/// short enough that idle clients do not accumulate streams.
const DATAGRAM_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// One secret route this visitor consumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisitorEntry {
    /// Secret kind to ask for.
    pub protocol: VisitorProtocol,
    /// Route name to ask for (the provider's template name).
    pub name: String,
    /// Shared key registered with the provider (FRP `sk`).
    pub secret_key: String,
    /// Local address the visitor binds.
    pub bind_addr: IpAddr,
    /// Local port the visitor binds (0 asks the OS for a free port).
    pub bind_port: u16,
}

impl VisitorEntry {
    /// A secret-TCP visitor bound to loopback — the safe default: a secret
    /// route is private, so publishing it to the local network must be
    /// explicit.
    pub fn loopback(
        name: impl Into<String>,
        secret_key: impl Into<String>,
        bind_port: u16,
    ) -> Self {
        Self {
            protocol: VisitorProtocol::Stcp,
            name: name.into(),
            secret_key: secret_key.into(),
            bind_addr: IpAddr::V4(Ipv4Addr::LOCALHOST),
            bind_port,
        }
    }

    /// The datagram form of [`Self::loopback`] (FRP `sudp`).
    pub fn loopback_datagram(
        name: impl Into<String>,
        secret_key: impl Into<String>,
        bind_port: u16,
    ) -> Self {
        Self {
            protocol: VisitorProtocol::Sudp,
            ..Self::loopback(name, secret_key, bind_port)
        }
    }

    /// The socket the visitor asks the OS for.
    pub fn bind_socket(&self) -> SocketAddr {
        SocketAddr::new(self.bind_addr, self.bind_port)
    }
}

/// Everything the visitor runtime needs to run.
pub struct VisitorRuntimeOptions {
    /// Gateway endpoint.
    pub endpoint: RemoteEndpoint,
    /// Client TLS material (pin the gateway fingerprint).
    pub tls: tls::ClientTlsOptions,
    /// Device identity presented in the handshake.
    pub device: Device,
    /// Agent token accepted by the gateway.
    pub token: String,
    /// Secret routes to publish locally.
    pub entries: Vec<VisitorEntry>,
    /// Reconnect policy shared with the agent runtime.
    pub network: sdkwork_webserver_tunnel_core::TunnelNetworkConfig,
    /// Handshake and stream budgets shared with the agent runtime.
    pub timeouts: sdkwork_webserver_tunnel_core::TunnelTimeoutConfig,
    /// Counters for this visitor process.
    pub metrics: Arc<TunnelMetrics>,
}

impl VisitorRuntimeOptions {
    /// Builds options from a tunnel config plus the connection inputs, using
    /// the same network/timeout policy the agent runtime uses.
    pub fn from_config(
        config: &TunnelConfig,
        endpoint: RemoteEndpoint,
        tls: tls::ClientTlsOptions,
        device: Device,
        token: String,
        entries: Vec<VisitorEntry>,
        metrics: Arc<TunnelMetrics>,
    ) -> Self {
        Self {
            endpoint,
            tls,
            device,
            token,
            entries,
            network: config.network_or_default(),
            timeouts: config.timeout_or_default(),
            metrics,
        }
    }
}

/// Lifecycle notifications an operator (or the CLI) can watch.
#[derive(Debug, Clone)]
pub enum VisitorEvent {
    /// Authenticated; visitor streams can now be served.
    Connected {
        /// Gateway-assigned session id.
        session_id: SessionId,
    },
    /// A local listener is open and forwarding.
    Listening {
        /// Route the listener serves.
        name: String,
        /// The address actually bound (useful with port 0).
        local_addr: SocketAddr,
    },
    /// A listener could not bind; that entry is not served.
    BindFailed {
        /// Route the listener was meant to serve.
        name: String,
        /// Why the bind failed.
        error: String,
    },
    /// The session ended and the runtime is reconnecting.
    Disconnected {
        /// Why it ended, for logs.
        reason: String,
    },
}

/// A running visitor client.
pub struct VisitorRuntime {
    stop: watch::Sender<bool>,
    events: mpsc::Receiver<VisitorEvent>,
    tasks: Vec<JoinHandle<()>>,
}

impl VisitorRuntime {
    /// Spawns the runtime; it connects, binds, and reconnects on its own.
    pub fn spawn(options: VisitorRuntimeOptions) -> Self {
        let (stop_tx, stop_rx) = watch::channel(false);
        let (events_tx, events_rx) = mpsc::channel(64);
        let tasks = spawn_runtime(options, stop_rx, events_tx);
        Self {
            stop: stop_tx,
            events: events_rx,
            tasks,
        }
    }

    /// Next lifecycle event, or `None` once every task has ended.
    pub async fn next_event(&mut self) -> Option<VisitorEvent> {
        self.events.recv().await
    }

    /// Stops the runtime and waits for its tasks to finish.
    pub async fn shutdown(self) {
        let _ = self.stop.send(true);
        for task in self.tasks {
            let _ = task.await;
        }
    }
}

struct RuntimeTasks {
    stop: watch::Receiver<bool>,
    events: mpsc::Sender<VisitorEvent>,
    metrics: Arc<TunnelMetrics>,
}

fn spawn_runtime(
    options: VisitorRuntimeOptions,
    stop: watch::Receiver<bool>,
    events: mpsc::Sender<VisitorEvent>,
) -> Vec<JoinHandle<()>> {
    let options = Arc::new(options);
    // The session task publishes the live connection; listeners take the
    // current one when a local client arrives. `None` means "no session yet".
    let (connection_tx, connection_rx) = watch::channel::<Option<Arc<dyn TunnelConnection>>>(None);
    let tasks = RuntimeTasks {
        stop,
        events,
        metrics: options.metrics.clone(),
    };

    let mut handles = Vec::new();
    {
        let options = options.clone();
        let tasks = tasks.clone_for("session");
        let connection_rx = connection_rx.clone();
        handles.push(tokio::spawn(async move {
            session_loop(options, connection_tx, connection_rx, tasks).await;
        }));
    }

    // One listener per entry: binding happens once, so a reconnect never moves
    // the port an operator already scripted against.
    for entry in &options.entries {
        let options = options.clone();
        let entry = entry.clone();
        let tasks = tasks.clone_for("listener");
        let connection_rx = connection_rx.clone();
        handles.push(tokio::spawn(async move {
            match entry.protocol {
                VisitorProtocol::Stcp => {
                    listener_loop(options, entry, connection_rx, tasks).await;
                }
                VisitorProtocol::Sudp => {
                    datagram_listener_loop(entry, connection_rx, tasks).await;
                }
            }
        }));
    }
    handles
}

impl RuntimeTasks {
    fn clone_for(&self, _role: &str) -> Self {
        Self {
            stop: self.stop.clone(),
            events: self.events.clone(),
            metrics: self.metrics.clone(),
        }
    }

    fn emit(&self, event: VisitorEvent) {
        // A full queue means the owner is not draining events; lifecycle
        // notifications are informational, and dropping one must never stop
        // the data path.
        let _ = self.events.try_send(event);
    }
}

/// Keeps one authenticated session alive, reconnecting with jittered backoff.
async fn session_loop(
    options: Arc<VisitorRuntimeOptions>,
    connection_tx: watch::Sender<Option<Arc<dyn TunnelConnection>>>,
    mut connection_rx: watch::Receiver<Option<Arc<dyn TunnelConnection>>>,
    tasks: RuntimeTasks,
) {
    let mut stop = tasks.stop.clone();
    let mut backoff = Backoff::new(
        Duration::from_secs(options.network.initial_backoff_secs.max(1)),
        Duration::from_secs(options.network.max_backoff_secs.max(1)),
    );
    loop {
        if *stop.borrow() {
            return;
        }
        let attempt_started = std::time::Instant::now();
        match open_session(&options).await {
            Ok((connection, control, session_id)) => {
                options.metrics.record_connection_open();
                options.metrics.record_session_open();
                tasks.emit(VisitorEvent::Connected {
                    session_id: session_id.clone(),
                });
                let _ = connection_tx.send(Some(connection.clone()));
                serving_session(&options, connection, control, &mut stop, &tasks).await;
                let _ = connection_tx.send(None);
                options.metrics.record_session_close();
                options.metrics.record_connection_close();
                // A session that lasted a full backoff interval was healthy:
                // do not punish it (and the operator) with the accumulated
                // delay of an earlier outage.
                if attempt_started.elapsed() >= backoff.initial() {
                    backoff.reset_on_success();
                }
            }
            Err(error) => {
                options.metrics.record_error();
                tracing::warn!(%error, "visitor session attempt failed");
            }
        }
        if *stop.borrow() {
            return;
        }
        tasks.emit(VisitorEvent::Disconnected {
            reason: "reconnecting".to_owned(),
        });
        // The published connection is gone: listeners wait for the next one.
        let delay = backoff.next();
        tokio::select! {
            biased;
            _ = stop.changed() => return,
            _ = tokio::time::sleep(delay) => {}
        }
        // Keep the watch channel's value observable to listeners that are
        // currently waiting on a change.
        let _ = connection_rx.borrow_and_update();
    }
}

/// Runs one authenticated session: heartbeats out, control errors noticed.
async fn serving_session(
    options: &Arc<VisitorRuntimeOptions>,
    connection: Arc<dyn TunnelConnection>,
    mut control: Box<dyn TunnelStream>,
    stop: &mut watch::Receiver<bool>,
    tasks: &RuntimeTasks,
) {
    let heartbeat = Duration::from_secs(options.network.heartbeat_interval_secs.max(1));
    let mut ticker = tokio::time::interval(heartbeat);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await;

    let closed = connection.closed();
    tokio::pin!(closed);
    loop {
        tokio::select! {
            biased;
            _ = stop.changed() => return,
            _ = &mut closed => {
                tracing::info!("visitor session closed by the gateway");
                return;
            }
            _ = ticker.tick() => {
                let write = async {
                    write_control(&mut *control, &ControlMessage::Heartbeat).await
                };
                if tokio::time::timeout(CONTROL_WRITE_TIMEOUT, write).await.is_err() {
                    tracing::warn!("visitor heartbeat timed out; reconnecting");
                    tasks.metrics.record_error();
                    return;
                }
            }
            message = read_control(&mut *control) => {
                match message {
                    Ok(ControlMessage::HeartbeatAck) => {}
                    Ok(ControlMessage::Error(error)) => {
                        // The gateway refusing a visitor request is reported
                        // per stream, not here; a control error means this
                        // session is not usable.
                        tracing::warn!(code = ?error.code, "gateway reported: {}", error.message);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::debug!(%error, "visitor control stream ended");
                        return;
                    }
                }
            }
        }
    }
}

/// Binds one local UDP port and forwards datagrams to the provider.
///
/// Datagram semantics decide the shape here: one tunnel stream per local
/// *client* (not per datagram, which would add a round trip and a stream per
/// packet), framed packets in both directions, and a datagram dropped rather
/// than queued when a client outruns the tunnel — UDP is lossy, and buffering
/// inside a relay only converts loss into latency.
async fn datagram_listener_loop(
    entry: VisitorEntry,
    connection_rx: watch::Receiver<Option<Arc<dyn TunnelConnection>>>,
    tasks: RuntimeTasks,
) {
    let mut stop = tasks.stop.clone();
    let bind = entry.bind_socket();
    let socket = match tokio::net::UdpSocket::bind(bind).await {
        Ok(socket) => Arc::new(socket),
        Err(error) => {
            tasks.emit(VisitorEvent::BindFailed {
                name: entry.name.clone(),
                error: error.to_string(),
            });
            tracing::error!(
                route = %entry.name,
                bind = %bind,
                %error,
                "visitor could not bind its local datagram port"
            );
            return;
        }
    };
    let local_addr = socket.local_addr().unwrap_or(bind);
    tasks.emit(VisitorEvent::Listening {
        name: entry.name.clone(),
        local_addr,
    });
    tracing::info!(
        route = %entry.name,
        %local_addr,
        "visitor listening for datagrams locally"
    );

    let clients: DatagramClients =
        Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
    let datagram_budget = Arc::new(tokio::sync::Semaphore::new(DATAGRAM_BUFFER_BUDGET_KIB));
    let mut buffer = vec![0_u8; sdkwork_webserver_tunnel_protocol::MAX_PACKET_PAYLOAD];
    let mut generation = 0_u64;
    loop {
        let received = tokio::select! {
            biased;
            _ = stop.changed() => return,
            received = socket.recv_from(&mut buffer) => received,
        };
        let (length, peer) = match received {
            Ok(received) => received,
            Err(error) => {
                // A datagram socket reports per-packet errors (an ICMP port
                // unreachable from a previous send, for instance) on the next
                // receive: they are not fatal to the listener.
                tracing::debug!(route = %entry.name, %error, "visitor datagram receive failed");
                continue;
            }
        };
        let mut payload = Some(buffer[..length].to_vec());

        // Send through the existing client's channel when it is alive.
        let mut sent = false;
        {
            let mut guard = clients.lock().await;
            if let Some(client) = guard.get(&peer) {
                let Some(bytes) = payload.take() else {
                    continue;
                };
                let Some(budget) = reserve_datagram_budget(&datagram_budget, bytes.len()) else {
                    // The listener's buffered-byte budget is exhausted; UDP
                    // datagrams are dropped rather than buffered without bound.
                    continue;
                };
                match client.sender.try_send(DatagramFrame {
                    payload: bytes,
                    _budget: budget,
                }) {
                    Ok(()) => sent = true,
                    // Full queue: the tunnel is slower than this client. Drop
                    // the newest datagram instead of growing an unbounded
                    // buffer.
                    Err(mpsc::error::TrySendError::Full(frame)) => {
                        sent = true;
                        drop(frame);
                    }
                    // The writer task is gone; the entry is stale.
                    Err(mpsc::error::TrySendError::Closed(frame)) => {
                        payload = Some(frame.payload);
                        guard.remove(&peer);
                    }
                }
            }
        }
        if sent {
            continue;
        }
        let Some(payload) = payload else {
            continue;
        };

        // First datagram from this client (or the previous stream ended):
        // open a stream and start its two pumps.
        let Some(connection) = wait_for_connection(&mut connection_rx.clone()).await else {
            tracing::warn!(
                route = %entry.name,
                %peer,
                "no visitor session available; dropping a local datagram"
            );
            continue;
        };
        let stream_id = tasks.metrics.next_stream_id();
        let mut stream = match connection.open_stream().await {
            Ok(stream) => stream,
            Err(error) => {
                tasks.metrics.record_error();
                tracing::warn!(route = %entry.name, %error, "visitor stream open failed");
                continue;
            }
        };
        let header = VisitorStreamHeader {
            name: entry.name.clone(),
            secret_key: entry.secret_key.clone(),
            protocol: entry.protocol.wire_label().to_owned(),
            stream_id,
        };
        let framed = match header.to_frame() {
            Ok(framed) => framed,
            Err(error) => {
                tracing::warn!(%error, "visitor header could not be built");
                continue;
            }
        };
        if stream.write_all(&framed).await.is_err() || stream.flush().await.is_err() {
            tasks.metrics.record_error();
            tracing::warn!(route = %entry.name, "visitor header write failed");
            continue;
        }
        tasks.metrics.record_stream_open();

        generation += 1;
        let generation = generation;
        let Some(budget) = reserve_datagram_budget(&datagram_budget, payload.len()) else {
            continue;
        };
        let (sender, receiver) = mpsc::channel::<DatagramFrame>(DATAGRAM_QUEUE);
        let _ = sender.try_send(DatagramFrame {
            payload,
            _budget: budget,
        });
        clients
            .lock()
            .await
            .insert(peer, DatagramClient { generation, sender });
        spawn_datagram_client(
            &entry,
            Arc::clone(&socket),
            Arc::clone(&clients),
            peer,
            generation,
            stream,
            receiver,
            tasks.stop.clone(),
            tasks.metrics.clone(),
        );
    }
}

/// Live local datagram clients: `peer` → its tunnel stream sender.
type DatagramClients =
    Arc<tokio::sync::Mutex<std::collections::HashMap<SocketAddr, DatagramClient>>>;

/// One local datagram client's tunnel stream.
struct DatagramClient {
    /// Generation guard: a stream that ends must not remove the entry that
    /// replaced it for the same client address.
    generation: u64,
    /// Datagrams waiting to be written to the tunnel.
    sender: mpsc::Sender<DatagramFrame>,
}

/// One queued datagram plus its share of the listener's byte budget. The
/// permit is released only after the framed packet has been written (or the
/// datagram is dropped), so the budget measures buffered bytes, not lifetime.
struct DatagramFrame {
    payload: Vec<u8>,
    _budget: tokio::sync::OwnedSemaphorePermit,
}

/// Charge one datagram's buffered bytes to the listener budget. `None` means
/// the listener is already holding its allowed bytes and this datagram must be
/// dropped (UDP semantics), never queued.
fn reserve_datagram_budget(
    budget: &Arc<tokio::sync::Semaphore>,
    payload_bytes: usize,
) -> Option<tokio::sync::OwnedSemaphorePermit> {
    let kib = payload_bytes.div_ceil(1024).max(1).min(u32::MAX as usize) as u32;
    Arc::clone(budget).try_acquire_many_owned(kib).ok()
}

/// Splits one client's stream into a write pump and a reply pump.
#[allow(clippy::too_many_arguments)]
fn spawn_datagram_client(
    entry: &VisitorEntry,
    socket: Arc<tokio::net::UdpSocket>,
    clients: DatagramClients,
    peer: SocketAddr,
    generation: u64,
    stream: Box<dyn TunnelStream>,
    receiver: mpsc::Receiver<DatagramFrame>,
    stop: watch::Receiver<bool>,
    metrics: Arc<TunnelMetrics>,
) {
    let (reader, writer) = tokio::io::split(stream);
    let route_name = entry.name.clone();
    // Replies: framed packets from the provider become datagrams back to the
    // client that asked.
    {
        let socket = Arc::clone(&socket);
        let clients = Arc::clone(&clients);
        let metrics = Arc::clone(&metrics);
        let mut stop = stop.clone();
        tokio::spawn(async move {
            let mut reader = reader;
            // `read_packet` is not cancel-safe (a partial frame must survive),
            // so the read owns this task and cancellation happens at the
            // select boundary, where abandoning the stream is intended.
            let mut scratch = bytes::BytesMut::with_capacity(4096);
            loop {
                let read = tokio::select! {
                    biased;
                    _ = stop.changed() => break,
                    read = sdkwork_webserver_tunnel_protocol::read_packet(&mut reader, &mut scratch) => read,
                };
                match read {
                    Ok(payload) => {
                        metrics.add_bytes_out(payload.len() as u64);
                        if socket.send_to(&payload, peer).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            reap_datagram_client(&clients, peer, generation).await;
        });
    }
    // Writes: local datagrams become framed packets, ending on idle so a
    // silent client does not hold a tunnel stream forever.
    tokio::spawn(async move {
        let mut writer = writer;
        let mut receiver = receiver;
        let mut stop = stop;
        loop {
            let frame = tokio::select! {
                biased;
                _ = stop.changed() => break,
                received = tokio::time::timeout(DATAGRAM_IDLE_TIMEOUT, receiver.recv()) => {
                    match received {
                        Ok(Some(frame)) => frame,
                        // Channel closed: the listener is gone.
                        Ok(None) => break,
                        // Idle: release the tunnel stream.
                        Err(_) => break,
                    }
                }
            };
            let payload = &frame.payload;
            let mut framed = bytes::BytesMut::with_capacity(
                payload.len() + sdkwork_webserver_tunnel_protocol::PACKET_LENGTH_BYTES,
            );
            if sdkwork_webserver_tunnel_protocol::encode_packet(payload, &mut framed).is_err() {
                continue;
            }
            if writer.write_all(&framed).await.is_err() || writer.flush().await.is_err() {
                break;
            }
            metrics.add_bytes_in(payload.len() as u64);
        }
        metrics.record_stream_close();
        reap_datagram_client(&clients, peer, generation).await;
    });
    tracing::debug!(
        route = %route_name,
        %peer,
        "visitor datagram client started"
    );
}

/// Removes a client entry, but only if it is still the generation that ended.
async fn reap_datagram_client(clients: &DatagramClients, peer: SocketAddr, generation: u64) {
    let mut guard = clients.lock().await;
    if guard
        .get(&peer)
        .is_some_and(|client| client.generation == generation)
    {
        guard.remove(&peer);
    }
}

/// Binds one local port and forwards every accepted connection.
async fn listener_loop(
    options: Arc<VisitorRuntimeOptions>,
    entry: VisitorEntry,
    connection_rx: watch::Receiver<Option<Arc<dyn TunnelConnection>>>,
    tasks: RuntimeTasks,
) {
    let mut stop = tasks.stop.clone();
    let listener = match tokio::net::TcpListener::bind(entry.bind_socket()).await {
        Ok(listener) => listener,
        Err(error) => {
            tasks.emit(VisitorEvent::BindFailed {
                name: entry.name.clone(),
                error: error.to_string(),
            });
            tracing::error!(
                route = %entry.name,
                bind = %entry.bind_socket(),
                %error,
                "visitor could not bind its local port"
            );
            return;
        }
    };
    let local_addr = listener
        .local_addr()
        .unwrap_or_else(|_| entry.bind_socket());
    tasks.emit(VisitorEvent::Listening {
        name: entry.name.clone(),
        local_addr,
    });
    tracing::info!(
        route = %entry.name,
        %local_addr,
        "visitor listening locally"
    );

    loop {
        let accepted = tokio::select! {
            biased;
            _ = stop.changed() => return,
            accepted = listener.accept() => accepted,
        };
        let (downstream, peer) = match accepted {
            Ok(accepted) => accepted,
            Err(error) => {
                tracing::debug!(route = %entry.name, %error, "visitor accept failed");
                continue;
            }
        };
        let options = options.clone();
        let entry = entry.clone();
        let connection_rx = connection_rx.clone();
        let metrics = tasks.metrics.clone();
        tokio::spawn(async move {
            serve_local_connection(options, entry, downstream, peer, connection_rx, metrics).await;
        });
    }
}

/// Bridges one local connection to the provider over a fresh visitor stream.
async fn serve_local_connection(
    options: Arc<VisitorRuntimeOptions>,
    entry: VisitorEntry,
    mut downstream: tokio::net::TcpStream,
    peer: SocketAddr,
    mut connection_rx: watch::Receiver<Option<Arc<dyn TunnelConnection>>>,
    metrics: Arc<TunnelMetrics>,
) {
    let _ = downstream.set_nodelay(true);
    let Some(connection) = wait_for_connection(&mut connection_rx).await else {
        tracing::warn!(
            route = %entry.name,
            %peer,
            "no visitor session available; closing the local connection"
        );
        return;
    };
    let stream_id = metrics.next_stream_id();
    let mut upstream = match connection.open_stream().await {
        Ok(stream) => stream,
        Err(error) => {
            metrics.record_error();
            tracing::warn!(route = %entry.name, %error, "visitor stream open failed");
            return;
        }
    };
    let header = VisitorStreamHeader {
        name: entry.name.clone(),
        secret_key: entry.secret_key.clone(),
        protocol: entry.protocol.wire_label().to_owned(),
        stream_id,
    };
    let framed = match header.to_frame() {
        Ok(framed) => framed,
        Err(error) => {
            tracing::warn!(%error, "visitor header could not be built");
            return;
        }
    };
    if let Err(error) = upstream.write_all(&framed).await {
        metrics.record_error();
        tracing::warn!(route = %entry.name, %error, "visitor header write failed");
        return;
    }
    if let Err(error) = upstream.flush().await {
        metrics.record_error();
        tracing::warn!(route = %entry.name, %error, "visitor header flush failed");
        return;
    }

    metrics.record_stream_open();
    match tokio::io::copy_bidirectional(&mut downstream, &mut upstream).await {
        Ok((inbound, outbound)) => {
            metrics.add_bytes_in(inbound);
            metrics.add_bytes_out(outbound);
        }
        Err(error) => {
            // A local client hanging up mid-transfer is routine.
            tracing::debug!(route = %entry.name, %peer, %error, "visitor relay ended");
        }
    }
    metrics.record_stream_close();
    let _ = options;
}

/// Waits for a live session, bounded: a local client must not hang forever
/// while the visitor reconnects.
async fn wait_for_connection(
    connection_rx: &mut watch::Receiver<Option<Arc<dyn TunnelConnection>>>,
) -> Option<Arc<dyn TunnelConnection>> {
    if let Some(connection) = connection_rx.borrow().clone() {
        return Some(connection);
    }
    let wait = async {
        loop {
            if connection_rx.changed().await.is_err() {
                return None;
            }
            if let Some(connection) = connection_rx.borrow().clone() {
                return Some(connection);
            }
        }
    };
    tokio::time::timeout(WAIT_FOR_SESSION, wait)
        .await
        .ok()
        .flatten()
}

/// Connects and authenticates one visitor session.
async fn open_session(
    options: &Arc<VisitorRuntimeOptions>,
) -> Result<(Arc<dyn TunnelConnection>, Box<dyn TunnelStream>, SessionId), String> {
    let transport = QuicClientTransport::new(&options.tls, Default::default())
        .map_err(|error| error.to_string())?;
    let server_name = tls::server_name_for(&options.endpoint);
    let connect = transport.connect(&options.endpoint, &server_name);
    let connection = tokio::time::timeout(options.timeouts.handshake_duration(), connect)
        .await
        .map_err(|_| "visitor connect timed out".to_owned())?
        .map_err(|error| error.to_string())?;

    let mut control = connection
        .open_stream()
        .await
        .map_err(|error| format!("control stream open failed: {error}"))?;
    write_control(
        &mut *control,
        &ControlMessage::Hello(Hello {
            protocol_versions: vec![ProtocolVersion::v1()],
            device_id: options.device.id.to_string(),
            device_name: options.device.name.clone(),
            platform: options.device.platform.as_str().to_owned(),
            user: options.device.user.clone(),
        }),
    )
    .await?;
    write_control(
        &mut *control,
        &ControlMessage::Authenticate(Authenticate {
            token: options.token.clone(),
        }),
    )
    .await?;
    let reply = tokio::time::timeout(
        options.timeouts.handshake_duration(),
        read_control(&mut *control),
    )
    .await
    .map_err(|_| "visitor handshake timed out".to_owned())??;
    let session_id = match reply {
        ControlMessage::AuthResult(AuthResult {
            ok: true,
            session_id,
            ..
        }) => session_id.ok_or_else(|| "gateway omitted the session id".to_owned())?,
        ControlMessage::AuthResult(AuthResult {
            ok: false,
            error: Some(detail),
            ..
        }) => return Err(format!("visitor authentication rejected: {detail}")),
        ControlMessage::Error(error) => {
            return Err(format!(
                "gateway error during visitor handshake: {:?} {}",
                error.code, error.message
            ))
        }
        other => return Err(format!("unexpected visitor handshake reply: {other:?}")),
    };
    let session_id = SessionId::parse(&session_id).map_err(|error| error.to_string())?;
    Ok((Arc::from(connection), control, session_id))
}

async fn write_control(
    control: &mut dyn TunnelStream,
    message: &ControlMessage,
) -> std::result::Result<(), String> {
    let payload = message.encode().map_err(|error| error.to_string())?;
    let framed = frame::frame_bytes(&payload).map_err(|error| error.to_string())?;
    control
        .write_all(&framed)
        .await
        .map_err(|error| error.to_string())?;
    control.flush().await.map_err(|error| error.to_string())
}

async fn read_control(
    control: &mut dyn TunnelStream,
) -> std::result::Result<ControlMessage, String> {
    let payload = frame::read_exact_frame(&mut *control)
        .await
        .map_err(|error| error.to_string())?;
    ControlMessage::decode(&payload).map_err(|error| error.to_string())
}

/// Jittered exponential backoff, same shape as the agent runtime's: the two
/// clients must not synchronize their reconnect storms on one gateway.
struct Backoff {
    current: Duration,
    initial: Duration,
    max: Duration,
}

impl Backoff {
    fn new(initial: Duration, max: Duration) -> Self {
        Self {
            current: initial,
            initial,
            max,
        }
    }

    fn initial(&self) -> Duration {
        self.initial
    }

    fn next(&mut self) -> Duration {
        let jitter_scale = 0.8 + rand::random::<f64>() * 0.4;
        let delay = self.current.mul_f64(jitter_scale);
        self.current = (self.current * 2).min(self.max);
        delay
    }

    fn reset_on_success(&mut self) {
        self.current = self.initial;
    }
}

/// Errors a visitor entry can carry before the runtime starts.
pub fn validate_entries(entries: &[VisitorEntry]) -> Result<(), TunnelError> {
    if entries.is_empty() {
        return Err(TunnelError::Validation {
            field: sdkwork_webserver_tunnel_core::ValidationField::Config,
            reason: "a visitor runtime needs at least one entry".to_owned(),
        });
    }
    let mut names = std::collections::HashSet::new();
    for entry in entries {
        if !names.insert(entry.name.as_str()) {
            return Err(TunnelError::Validation {
                field: sdkwork_webserver_tunnel_core::ValidationField::Config,
                reason: format!("visitor entry `{}` is declared twice", entry.name),
            });
        }
        if entry.secret_key.len() < 8 {
            return Err(TunnelError::Validation {
                field: sdkwork_webserver_tunnel_core::ValidationField::Config,
                reason: format!(
                    "visitor entry `{}` has a key shorter than 8 characters",
                    entry.name
                ),
            });
        }
        if entry.name.trim().is_empty() {
            return Err(TunnelError::Validation {
                field: sdkwork_webserver_tunnel_core::ValidationField::Config,
                reason: "visitor entry name must not be empty".to_owned(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_validated_before_the_runtime_starts() {
        assert!(validate_entries(&[]).is_err());

        let good = VisitorEntry::loopback("private-ssh", "s3cret-key-123", 0);
        assert!(validate_entries(&[good.clone()]).is_ok());

        // Two entries for one route would fight over the name; the second one
        // could never be distinguished at the gateway either.
        assert!(validate_entries(&[good.clone(), good.clone()]).is_err());

        let short_key = VisitorEntry::loopback("private-ssh", "short", 0);
        assert!(validate_entries(&[short_key]).is_err());

        let unnamed = VisitorEntry::loopback("  ", "s3cret-key-123", 0);
        assert!(validate_entries(&[unnamed]).is_err());
    }

    #[test]
    fn a_visitor_binds_loopback_unless_told_otherwise() {
        let entry = VisitorEntry::loopback("private-ssh", "s3cret-key-123", 2222);
        assert_eq!(entry.bind_addr, IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(entry.bind_port, 2222);
        assert_eq!(entry.bind_socket().port(), 2222);
        assert_eq!(entry.protocol, VisitorProtocol::Stcp);
        assert_eq!(entry.protocol.wire_label(), "stcp");
        assert_eq!(VisitorProtocol::parse("sudp"), Some(VisitorProtocol::Sudp));
        assert_eq!(VisitorProtocol::parse("SUDP"), Some(VisitorProtocol::Sudp));
        assert_eq!(VisitorProtocol::parse("tcp"), None);

        let shared = VisitorEntry {
            bind_addr: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            ..entry
        };
        assert_eq!(shared.bind_socket().to_string(), "0.0.0.0:2222");
    }

    #[test]
    fn visitor_backoff_doubles_and_resets_like_the_agent() {
        let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(4));
        for expected_secs in [1_u64, 2, 4, 4] {
            let delay = backoff.next();
            let secs = delay.as_secs_f64();
            assert!(
                secs >= expected_secs as f64 * 0.8 && secs <= expected_secs as f64 * 1.2,
                "expected about {expected_secs}s, got {secs}"
            );
        }
        backoff.reset_on_success();
        assert!(backoff.next().as_secs_f64() <= 1.2);
        assert_eq!(backoff.initial(), Duration::from_secs(1));
    }

    #[test]
    fn datagram_budget_is_charged_in_kib_and_released_on_drop() {
        let budget = Arc::new(tokio::sync::Semaphore::new(2));
        let first = reserve_datagram_budget(&budget, 1).expect("one KiB fits");
        let second = reserve_datagram_budget(&budget, 1024).expect("second KiB fits");
        assert!(
            reserve_datagram_budget(&budget, 1).is_none(),
            "a third datagram must be dropped when the listener budget is full"
        );
        drop(first);
        assert!(
            reserve_datagram_budget(&budget, 1024).is_some(),
            "releasing a written datagram frees its bytes for the next one"
        );
        drop(second);
    }
}
