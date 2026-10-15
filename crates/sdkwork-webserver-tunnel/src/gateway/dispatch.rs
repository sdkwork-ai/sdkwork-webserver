//! Gateway relay dispatch: stream-level admission and opening (PRD §20,
//! §30, §31, §44–§46).
//!
//! The tunnel crate stays free of HTTP semantics: the webserver integration
//! asks for a relayed byte stream by visitor host or gateway TCP port, and
//! this module performs route lookup, ACL evaluation, session resolution,
//! data-stream opening, and stream-header framing. The caller owns the
//! protocol spoken over the returned stream.

use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};

use sdkwork_webserver_tunnel_core::{
    Result, RouteId, RoutePolicy, SessionId, StreamId, TunnelError,
};
use sdkwork_webserver_tunnel_protocol::{DataStreamHeader, VisitorAddress};
use sdkwork_webserver_tunnel_transport::TunnelStream;

use super::GatewayShared;

/// Visitor identity inputs for relay admission.
#[derive(Debug, Clone, Copy)]
pub struct RelayVisitor<'a> {
    /// Client IP, after the caller's trusted-proxy resolution.
    pub ip: IpAddr,
    /// Client source port, or 0 when the calling plane does not expose it.
    /// Forwarded to the agent so a PROXY-protocol route can name the real
    /// source port; `0` is the protocol's own "unknown" encoding.
    pub port: u16,
    /// Visitor bearer token (HTTP `Authorization: Bearer …`), if any.
    pub bearer: Option<&'a str>,
}

impl RelayVisitor<'_> {
    /// The visitor address as the agent receives it.
    fn address(&self) -> VisitorAddress {
        VisitorAddress::new(self.ip, self.port)
    }
}

/// A relayed data stream with byte accounting wired into tunnel metrics.
/// Dropping the stream releases the session's stream-admission permit.
pub struct RelayedStream {
    inner: Box<dyn TunnelStream>,
    metrics: Arc<crate::TunnelMetrics>,
    /// Machine inventory, so relayed bytes are also credited to the device
    /// that served them — an admin console shows per-machine traffic, and the
    /// process-wide counters alone cannot answer "which machine moved this".
    devices: Arc<crate::gateway::devices::DeviceRegistry>,
    /// Device owning the session this stream rides on, resolved once when the
    /// stream opens (the session table is keyed by session, not by device).
    device_id: Option<sdkwork_webserver_tunnel_core::DeviceId>,
    stream_id: StreamId,
    route_id: String,
    session_id: String,
    _permit: tokio::sync::OwnedSemaphorePermit,
    read_bytes: AtomicU64,
}

impl RelayedStream {
    /// The assigned stream id.
    pub fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    /// The route the stream belongs to.
    pub fn route_id(&self) -> &str {
        &self.route_id
    }

    /// The session the stream rides on.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Bytes read from the agent (egress) so far.
    pub fn egress_bytes(&self) -> u64 {
        self.read_bytes.load(Ordering::Relaxed)
    }
}

impl AsyncRead for RelayedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let filled_before = buf.filled().len();
        let outcome = Pin::new(&mut *self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &outcome {
            let read = (buf.filled().len() - filled_before) as u64;
            if read > 0 {
                self.read_bytes.fetch_add(read, Ordering::Relaxed);
                self.metrics.add_bytes_out(read);
                // Per-machine accounting rides the same observation: the byte
                // was just counted globally, and the operator also wants to
                // know which machine moved it.
                if let Some(device_id) = self.device_id.as_ref() {
                    self.devices.add_traffic(device_id, 0, read);
                }
            }
        }
        outcome
    }
}

impl AsyncWrite for RelayedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let outcome = Pin::new(&mut *self.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(written)) = &outcome {
            let written = u64::try_from(*written).unwrap_or(u64::MAX);
            self.metrics.add_bytes_in(written);
            if written > 0 {
                if let Some(device_id) = self.device_id.as_ref() {
                    self.devices.add_traffic(device_id, written, 0);
                }
            }
        }
        outcome
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut *self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut *self.inner).poll_shutdown(cx)
    }
}

impl TunnelStream for RelayedStream {}

/// Diagnostics without leaking the transport: the stream identity is what a
/// caller (or a failing assertion) needs to correlate a relay with a route.
impl std::fmt::Debug for RelayedStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelayedStream")
            .field("stream_id", &self.stream_id.to_string())
            .field("route_id", &self.route_id)
            .field("session_id", &self.session_id)
            .field("egress_bytes", &self.read_bytes.load(Ordering::Relaxed))
            .finish()
    }
}

impl Drop for RelayedStream {
    fn drop(&mut self) {
        self.metrics.record_stream_close();
    }
}

/// An admitted HTTP relay: the byte stream plus the route policy the caller
/// must apply to the HTTP messages it writes and reads on it.
///
/// It deliberately stays usable *as* the stream (`AsyncRead`/`AsyncWrite`
/// delegate to the relayed bytes): a caller that only pumps bytes keeps
/// working, while a caller that parses HTTP can reach the rewriting policy
/// instead of silently relaying without it.
#[derive(Debug)]
pub struct HttpRelay {
    /// The visitor ⇄ agent byte stream.
    pub stream: RelayedStream,
    /// Route id, so a caller can log or measure per route without a second
    /// lookup.
    pub route_id: String,
    /// Header rewriting this route declares (empty for most routes).
    pub headers: sdkwork_webserver_tunnel_core::RouteHeaderRewrite,
}

impl AsyncRead for HttpRelay {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for HttpRelay {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

/// HTTP relay admission: matches `host` against registered domain routes
/// and opens a data stream toward the owning agent (PRD §30 flow).
/// HTTP relay admission: matches `host` and `path` against registered domain
/// routes and opens a data stream toward the owning agent (PRD §30 flow).
///
/// The path decides between several routes that share a host with disjoint
/// location prefixes (FRP `locations` parity); a host whose routes all decline
/// the path has no route here, and the caller keeps its own routing.
pub async fn connect_http_stream(
    shared: &Arc<GatewayShared>,
    host: &str,
    path: &str,
    visitor: RelayVisitor<'_>,
) -> Result<HttpRelay> {
    let lowered = host.trim().to_ascii_lowercase();
    // Selection, not lookup: a load-balancer group may offer several replicas,
    // and the unhealthy ones are skipped here (FRP `healthCheck` +
    // `loadBalancer` parity).
    let registered = match shared.registry.select_domain_path(&lowered, path) {
        crate::gateway::registry::RouteSelection::Selected(route) => route,
        crate::gateway::registry::RouteSelection::NoHealthyReplica => {
            return Err(TunnelError::NoHealthyReplica)
        }
        crate::gateway::registry::RouteSelection::NoRoute => {
            return Err(TunnelError::RouteNotFound)
        }
    };
    admit(shared, &registered.route.policy, visitor.ip, visitor.bearer)?;
    let owner: SessionId = registered
        .session()
        .cloned()
        .ok_or(TunnelError::RouteNotFound)?;
    let stream = open_data_stream(
        shared,
        &owner,
        &registered.route.id,
        Some(visitor.address()),
        registered.limiter.clone(),
    )
    .await?;
    Ok(HttpRelay {
        stream,
        route_id: registered.route.id.to_string(),
        headers: registered.route.headers.clone(),
    })
}

/// Secret-route admission (FRP `stcp`/`sudp` parity): the visitor names the
/// route and presents its shared key, and the gateway bridges the resulting
/// stream to the provider's local target.
///
/// Unlike the public planes there is no host or port to match, so the key *is*
/// the admission control: a wrong key and an unknown name produce the same
/// refusal, because telling a caller that a name exists but the key is wrong
/// would turn the endpoint into a name oracle.
pub async fn connect_secret_stream(
    shared: &Arc<GatewayShared>,
    name: &str,
    key: &str,
    requested_protocol: &str,
    visitor: Option<VisitorAddress>,
) -> Result<RelayedStream> {
    let registered = match shared.registry.select_secret(name) {
        crate::gateway::registry::RouteSelection::Selected(route) => route,
        crate::gateway::registry::RouteSelection::NoHealthyReplica => {
            return Err(TunnelError::NoHealthyReplica)
        }
        crate::gateway::registry::RouteSelection::NoRoute => {
            return Err(TunnelError::AuthenticationFailed)
        }
    };
    if !registered.route.secret_key_matches(key) {
        shared.metrics.record_visitor_auth_failure();
        return Err(TunnelError::AuthenticationFailed);
    }
    // The protocol is part of the request: a datagram visitor must never be
    // handed a stream route (or the other way round), because the two sides
    // would then disagree about whether the bytes are framed.
    if registered.route.protocol.wire_label() != requested_protocol {
        return Err(TunnelError::InvalidRoute(format!(
            "route `{name}` is {} and cannot serve a {requested_protocol} visitor",
            registered.route.protocol.wire_label()
        )));
    }
    let owner: SessionId = registered
        .session()
        .cloned()
        .ok_or(TunnelError::RouteNotFound)?;
    open_data_stream(
        shared,
        &owner,
        &registered.route.id,
        visitor,
        registered.limiter.clone(),
    )
    .await
}

/// TCP relay admission: matches a gateway listener port against registered
/// TCP routes and opens a data stream toward the owning agent (PRD §31).
/// UDP routes are never matched here: their gateway listeners are datagram
/// sockets and a TCP visitor hitting the same port number finds no listener.
pub async fn connect_tcp_stream(
    shared: &Arc<GatewayShared>,
    port: u16,
    client: SocketAddr,
) -> Result<RelayedStream> {
    let registered = match shared.registry.select_port(port) {
        crate::gateway::registry::RouteSelection::Selected(route) => route,
        crate::gateway::registry::RouteSelection::NoHealthyReplica => {
            return Err(TunnelError::NoHealthyReplica)
        }
        crate::gateway::registry::RouteSelection::NoRoute => {
            return Err(TunnelError::RouteNotFound)
        }
    };
    admit(shared, &registered.route.policy, client.ip(), None)?;
    let owner: SessionId = registered
        .session()
        .cloned()
        .ok_or(TunnelError::RouteNotFound)?;
    open_data_stream(
        shared,
        &owner,
        &registered.route.id,
        Some(VisitorAddress::new(client.ip(), client.port())),
        registered.limiter.clone(),
    )
    .await
}

/// Shared admission gate for every relay plane (HTTP, TCP, UDP datagram
/// sessions). Raw datagram visitors cannot carry a bearer header, so
/// bearer-protected routes are only reachable over HTTP; private UDP routes
/// are reachable through the network allow-list.
pub(crate) fn admit(
    shared: &Arc<GatewayShared>,
    policy: &RoutePolicy,
    ip: IpAddr,
    bearer: Option<&str>,
) -> Result<()> {
    match shared.acl.evaluate(policy, ip, bearer) {
        crate::security::AclDecision::Admit => Ok(()),
        crate::security::AclDecision::DeniedByNetwork => {
            shared.metrics.record_relay_denied();
            tracing::info!(%ip, "tunnel relay denied by network policy");
            Err(TunnelError::AuthorizationDenied)
        }
        crate::security::AclDecision::DeniedByAuth => {
            shared.metrics.record_relay_denied();
            tracing::info!(%ip, "tunnel relay denied: visitor authentication required");
            Err(TunnelError::AuthorizationDenied)
        }
    }
}

pub(crate) async fn open_data_stream(
    shared: &Arc<GatewayShared>,
    session_id: &SessionId,
    route_id: &RouteId,
    visitor: Option<VisitorAddress>,
    limiter: Option<Arc<super::rate_limit::RouteRateLimiter>>,
) -> Result<RelayedStream> {
    let permit = shared.sessions.try_acquire_stream(session_id)?;
    let connection = shared
        .sessions
        .connection(session_id)
        .ok_or(TunnelError::SessionNotFound)?;
    let next_id = shared.next_stream_id();
    let open_budget = shared.timeouts.stream_open_duration();
    let open = async {
        let mut stream = connection.open_stream().await?;
        let header = DataStreamHeader::with_visitor(route_id, next_id, visitor);
        let frame = header.to_frame()?;
        stream
            .write_all(&frame)
            .await
            .map_err(|error| TunnelError::ConnectionFailed(error.to_string()))?;
        stream
            .flush()
            .await
            .map_err(|error| TunnelError::ConnectionFailed(error.to_string()))?;
        Ok::<Box<dyn TunnelStream>, TunnelError>(stream)
    };
    let opened = match tokio::time::timeout(open_budget, open).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => {
            shared.metrics.record_error();
            tracing::debug!(
                session = %session_id,
                route = %route_id,
                stream = %next_id,
                error = %error,
                "tunnel data stream open failed"
            );
            return Err(error);
        }
        Err(_) => {
            shared.metrics.record_error();
            return Err(TunnelError::Timeout("stream open"));
        }
    };
    // A route with a ceiling relays through a pacing wrapper; the limiter is
    // shared per route, so concurrent streams split one budget rather than
    // each getting the full rate.
    let stream: Box<dyn TunnelStream> = match limiter {
        Some(limiter) => {
            tracing::debug!(
                route = %route_id,
                bytes_per_second = limiter.bytes_per_second(),
                "relaying through a rate-limited stream"
            );
            Box::new(super::rate_limit::RateLimitedStream::new(opened, limiter))
        }
        None => opened,
    };
    shared.metrics.record_stream_open();
    tracing::debug!(
        session = %session_id,
        route = %route_id,
        stream = %next_id,
        "tunnel data stream opened"
    );
    Ok(RelayedStream {
        inner: stream,
        metrics: shared.metrics.clone(),
        devices: shared.devices.clone(),
        device_id: shared.session_device(session_id),
        stream_id: next_id,
        route_id: route_id.to_string(),
        session_id: session_id.to_string(),
        _permit: permit,
        read_bytes: AtomicU64::new(0),
    })
}
