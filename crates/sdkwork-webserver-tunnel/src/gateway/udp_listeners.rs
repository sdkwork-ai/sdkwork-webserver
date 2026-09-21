//! Gateway UDP listener set: datagram sessions over tunnel streams
//! (PRD: UDP 内网穿透, FRP UDP proxy parity).
//!
//! Model (mirrors NAT behavior):
//!
//! - The well-known UDP listener receives every visitor datagram and
//!   demultiplexes by source address: the first datagram from a new visitor
//!   opens a **session** (one tunnel data stream); subsequent datagrams are
//!   forwarded into that session's stream (framed).
//! - Each session owns a reader task: framed packets read from the tunnel
//!   stream are sent back to the visitor **from the well-known port** (so
//!   clients see replies from the address they sent to).
//! - Sessions expire after [`UDP_SESSION_IDLE`] without activity (NAT entry
//!   expiry parity); releasing the port aborts all sessions on it.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use tokio::io::AsyncWriteExt;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use sdkwork_webserver_tunnel_core::{RouteId, TunnelError};
use sdkwork_webserver_tunnel_protocol::packet_frame;

use super::{dispatch, GatewayShared};

const UDP_SESSION_IDLE: Duration = Duration::from_secs(60);
const UDP_REAP_INTERVAL: Duration = Duration::from_secs(5);
const UDP_DATAGRAM_CEILING: usize =
    sdkwork_webserver_tunnel_protocol::packet_frame::MAX_PACKET_PAYLOAD
        + sdkwork_webserver_tunnel_protocol::packet_frame::PACKET_LENGTH_BYTES;

#[derive(Default)]
pub(crate) struct UdpListenerSet {
    ports: HashMap<u16, PortState>,
    route_ports: HashMap<RouteId, u16>,
}

struct PortState {
    routes: HashSet<RouteId>,
    session_task: tokio::task::JoinHandle<()>,
    _listener: Arc<UdpSocket>,
}

impl UdpListenerSet {
    /// Registers a bound UDP listener for `route_id` on `port`, spawning the
    /// session loop on first use.
    pub(crate) fn serve(
        &mut self,
        shared: &Arc<GatewayShared>,
        port: u16,
        route_id: RouteId,
        listener: UdpSocket,
    ) {
        self.route_ports.insert(route_id.clone(), port);
        match self.ports.get_mut(&port) {
            Some(state) => {
                state.routes.insert(route_id);
                drop(listener);
            }
            None => {
                let listener = Arc::new(listener);
                let session_task =
                    tokio::spawn(udp_port_loop(shared.clone(), port, Arc::clone(&listener)));
                self.ports.insert(
                    port,
                    PortState {
                        routes: HashSet::from([route_id]),
                        session_task,
                        _listener: listener,
                    },
                );
                tracing::info!(port, "tunnel udp listener bound");
            }
        }
    }

    /// Releases one route's port claim, closing the listener when it was the
    /// last route on the port.
    pub(crate) fn release(&mut self, port: u16, route_id: &RouteId) {
        self.route_ports.remove(route_id);
        let Some(state) = self.ports.get_mut(&port) else {
            return;
        };
        state.routes.remove(route_id);
        if state.routes.is_empty() {
            if let Some(state) = self.ports.remove(&port) {
                state.session_task.abort();
                drop(state._listener);
                tracing::info!(port, "tunnel udp listener released");
            }
        }
    }

    /// Releases a route's claim when its port is unknown to the caller
    /// (session teardown).
    pub(crate) fn release_route(&mut self, route_id: &RouteId) {
        let Some(port) = self.route_ports.get(route_id).copied() else {
            return;
        };
        self.release(port, route_id);
    }

    /// Ports currently served (sorted, for diagnostics).
    pub(crate) fn served_ports(&self) -> Vec<u16> {
        let mut ports: Vec<u16> = self.ports.keys().copied().collect();
        ports.sort_unstable();
        ports
    }

    /// Shuts every listener down; called from gateway shutdown.
    pub(crate) fn shutdown(&mut self) {
        for (port, state) in self.ports.drain() {
            state.session_task.abort();
            drop(state._listener);
            tracing::debug!(port, "tunnel udp listener closed at shutdown");
        }
        self.route_ports.clear();
    }
}

/// One visitor session: the tunnel stream write half owned by the port loop
/// for forwarding, plus a reader task that owns the read half.
struct VisitorSession {
    writer: tokio::io::WriteHalf<Box<dyn sdkwork_webserver_tunnel_transport::TunnelStream>>,
    reader_task: tokio::task::JoinHandle<()>,
    last_activity: Arc<AtomicU64>,
    /// Stream-admission permit held until the session ends.
    _permit: tokio::sync::OwnedSemaphorePermit,
}

/// Events the per-session reader tasks report back to the port loop.
enum SessionEvent {
    /// The session's tunnel read half ended; reap the session.
    Ended(SocketAddr),
}

async fn udp_port_loop(shared: Arc<GatewayShared>, port: u16, listener: Arc<UdpSocket>) {
    let mut sessions: HashMap<SocketAddr, VisitorSession> = HashMap::new();
    let (events_tx, mut events_rx) = mpsc::channel::<SessionEvent>(64);
    let mut visitor_buffer = vec![0_u8; UDP_DATAGRAM_CEILING];
    let mut reap = tokio::time::interval(UDP_REAP_INTERVAL);
    reap.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            biased;
            event = events_rx.recv() => {
                match event {
                    Some(SessionEvent::Ended(visitor)) => {
                        if sessions.remove(&visitor).is_some() {
                            shared.metrics.record_stream_close();
                        }
                    }
                    None => break,
                }
            }
            _ = reap.tick() => {
                sessions.retain(|visitor, session| {
                    let alive = !idle_since(&session.last_activity);
                    if !alive {
                        session.reader_task.abort();
                        shared.metrics.record_stream_close();
                        tracing::debug!(%visitor, "udp session expired");
                    }
                    alive
                });
            }
            read = listener.recv_from(&mut visitor_buffer) => {
                match read {
                    Ok((size, visitor)) => {
                        let payload =
                            Bytes::copy_from_slice(&visitor_buffer[..size]);
                        match sessions.get_mut(&visitor) {
                            Some(session) => {
                                if let Err(error) =
                                    forward_to_session(session, payload).await
                                {
                                    tracing::debug!(
                                        %visitor, port, error = %error,
                                        "udp forward failed; ending session"
                                    );
                                    sessions.remove(&visitor);
                                    shared.metrics.record_stream_close();
                                }
                            }
                            None => {
                                shared.metrics.record_stream_open();
                                match open_session(
                                    &shared, port, &listener, visitor, payload, &events_tx,
                                )
                                .await
                                {
                                    Ok(session) => {
                                        sessions.insert(visitor, session);
                                    }
                                    Err(error) => {
                                        shared.metrics.record_stream_close();
                                        tracing::debug!(
                                            %visitor, port, error = %error,
                                            "udp session refused"
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!(port, error = %error, "udp listener read failed");
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                }
            }
        }
    }
    for (_, session) in sessions.drain() {
        session.reader_task.abort();
    }
}

async fn forward_to_session(
    session: &mut VisitorSession,
    payload: Bytes,
) -> Result<(), TunnelError> {
    let mut framed = BytesMut::with_capacity(
        sdkwork_webserver_tunnel_protocol::packet_frame::PACKET_LENGTH_BYTES + payload.len(),
    );
    sdkwork_webserver_tunnel_protocol::packet_frame::encode_packet(&payload, &mut framed)?;
    session
        .writer
        .write_all(&framed)
        .await
        .map_err(|error| TunnelError::ConnectionFailed(error.to_string()))?;
    session
        .writer
        .flush()
        .await
        .map_err(|error| TunnelError::ConnectionFailed(error.to_string()))?;
    touch(&session.last_activity);
    Ok(())
}

async fn open_session(
    shared: &Arc<GatewayShared>,
    port: u16,
    listener: &Arc<UdpSocket>,
    visitor: SocketAddr,
    first_payload: Bytes,
    events: &mpsc::Sender<SessionEvent>,
) -> Result<VisitorSession, TunnelError> {
    let registered =
        shared.registry.match_udp_port(port).ok_or(TunnelError::RouteNotFound)?;
    // Same admission gate as the HTTP and TCP relay planes: network
    // allow-list first, then the route publicity policy. Raw datagram
    // visitors cannot present a bearer token, so bearer-protected routes
    // deny anonymous datagrams (FRP SUDP parity: use a visitor client).
    dispatch::admit(shared, &registered.route.policy, visitor.ip(), None)?;
    let Some(owner) = registered.session().cloned() else {
        return Err(TunnelError::RouteNotFound);
    };
    let permit = shared.sessions.try_acquire_stream(&owner)?;
    let connection = shared
        .sessions
        .connection(&owner)
        .ok_or(TunnelError::SessionNotFound)?;
    let stream_id = shared.next_stream_id();
    let open_budget = shared.timeouts.stream_open_duration();
    let open = async {
        let mut stream = connection.open_stream().await?;
        let header =
            sdkwork_webserver_tunnel_protocol::DataStreamHeader::new(&registered.route.id, stream_id);
        let frame = header.to_frame()?;
        stream
            .write_all(&frame)
            .await
            .map_err(|error| TunnelError::ConnectionFailed(error.to_string()))?;
        stream
            .flush()
            .await
            .map_err(|error| TunnelError::ConnectionFailed(error.to_string()))?;
        let (reader, writer) = tokio::io::split(stream);
        Ok::<
            (
                tokio::io::ReadHalf<
                    Box<dyn sdkwork_webserver_tunnel_transport::TunnelStream>,
                >,
                tokio::io::WriteHalf<
                    Box<dyn sdkwork_webserver_tunnel_transport::TunnelStream>,
                >,
            ),
            TunnelError,
        >((reader, writer))
    };
    let (mut read_half, mut write_half) = match tokio::time::timeout(open_budget, open).await {
        Ok(Ok(halves)) => halves,
        Ok(Err(error)) => {
            shared.metrics.record_error();
            return Err(error);
        }
        Err(_) => {
            shared.metrics.record_error();
            return Err(TunnelError::Timeout("udp stream open"));
        }
    };
    // Forward the first visitor datagram into the fresh stream.
    let mut framed = BytesMut::with_capacity(
        sdkwork_webserver_tunnel_protocol::packet_frame::PACKET_LENGTH_BYTES
            + first_payload.len(),
    );
    sdkwork_webserver_tunnel_protocol::packet_frame::encode_packet(&first_payload, &mut framed)?;
    write_half
        .write_all(&framed)
        .await
        .map_err(|error| TunnelError::ConnectionFailed(error.to_string()))?;
    write_half
        .flush()
        .await
        .map_err(|error| TunnelError::ConnectionFailed(error.to_string()))?;

    // Reader task: framed tunnel packets → back to the visitor from the
    // well-known port. Ends on FIN (ConnectionClosed) or error.
    let last_activity = Arc::new(AtomicU64::new(now_millis()));
    let reader_task = {
        let listener = Arc::clone(listener);
        let events = events.clone();
        let metrics = shared.metrics.clone();
        let idle_marker = Arc::clone(&last_activity);
        let mut scratch = BytesMut::with_capacity(UDP_DATAGRAM_CEILING);
        tokio::spawn(async move {
            let outcome = async {
                loop {
                    let datagram =
                        packet_frame::read_packet(&mut read_half, &mut scratch).await?;
                    touch(&idle_marker);
                    metrics.add_bytes_out(u64::try_from(datagram.len()).unwrap_or(u64::MAX));
                    listener
                        .send_to(&datagram, visitor)
                        .await
                        .map_err(|error| TunnelError::ConnectionFailed(error.to_string()))?;
                }
            };
            match outcome.await {
                Ok(()) => {}
                Err(TunnelError::ConnectionClosed) => {}
                Err(error) => {
                    tracing::debug!(%visitor, error = %error, "udp session reader ended");
                }
            }
            let _ = events
                .send(SessionEvent::Ended(visitor))
                .await;
        })
    };
    tracing::debug!(%visitor, port, route = %registered.route.id, "udp session opened");
    Ok(VisitorSession {
        writer: write_half,
        reader_task,
        last_activity,
        _permit: permit,
    })
}

fn idle_since(last_activity: &AtomicU64) -> bool {
    let now = now_millis();
    now.saturating_sub(last_activity.load(Ordering::Relaxed))
        > UDP_SESSION_IDLE.as_millis() as u64
}

fn touch(last_activity: &AtomicU64) {
    last_activity.store(now_millis(), Ordering::Relaxed);
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}
