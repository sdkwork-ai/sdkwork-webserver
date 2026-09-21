//! Gateway relay dispatch: stream-level admission and opening (PRD §20,
//! §30, §31, §44–§46).
//!
//! The tunnel crate stays free of HTTP semantics: the webserver integration
//! asks for a relayed byte stream by visitor host or gateway TCP port, and
//! this module performs route lookup, ACL evaluation, session resolution,
//! data-stream opening, and stream-header framing. The caller owns the
//! protocol spoken over the returned stream.

use std::net::IpAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};

use sdkwork_webserver_tunnel_core::{
    Result, RouteId, RoutePolicy, SessionId, StreamId, TunnelError,
};
use sdkwork_webserver_tunnel_protocol::DataStreamHeader;
use sdkwork_webserver_tunnel_transport::TunnelStream;

use super::GatewayShared;

/// Visitor identity inputs for relay admission.
#[derive(Debug, Clone, Copy)]
pub struct RelayVisitor<'a> {
    /// Client IP, after the caller's trusted-proxy resolution.
    pub ip: IpAddr,
    /// Visitor bearer token (HTTP `Authorization: Bearer …`), if any.
    pub bearer: Option<&'a str>,
}

/// A relayed data stream with byte accounting wired into tunnel metrics.
/// Dropping the stream releases the session's stream-admission permit.
pub struct RelayedStream {
    inner: Box<dyn TunnelStream>,
    metrics: Arc<crate::TunnelMetrics>,
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
            self.metrics
                .add_bytes_in(u64::try_from(*written).unwrap_or(u64::MAX));
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

impl Drop for RelayedStream {
    fn drop(&mut self) {
        self.metrics.record_stream_close();
    }
}

/// HTTP relay admission: matches `host` against registered domain routes
/// and opens a data stream toward the owning agent (PRD §30 flow).
pub async fn connect_http_stream(
    shared: &Arc<GatewayShared>,
    host: &str,
    visitor: RelayVisitor<'_>,
) -> Result<RelayedStream> {
    let lowered = host.trim().to_ascii_lowercase();
    let registered = shared
        .registry
        .match_domain(&lowered)
        .ok_or(TunnelError::RouteNotFound)?;
    admit(shared, &registered.route.policy, visitor.ip, visitor.bearer)?;
    let owner: SessionId = registered
        .session()
        .cloned()
        .ok_or(TunnelError::RouteNotFound)?;
    open_data_stream(shared, &owner, &registered.route.id).await
}

/// TCP relay admission: matches a gateway listener port against registered
/// TCP routes and opens a data stream toward the owning agent (PRD §31).
pub async fn connect_tcp_stream(
    shared: &Arc<GatewayShared>,
    port: u16,
    client_ip: IpAddr,
) -> Result<RelayedStream> {
    let registered = shared
        .registry
        .match_port(port)
        .or_else(|| shared.registry.match_udp_port(port))
        .ok_or(TunnelError::RouteNotFound)?;
    admit(shared, &registered.route.policy, client_ip, None)?;
    let owner: SessionId = registered
        .session()
        .cloned()
        .ok_or(TunnelError::RouteNotFound)?;
    open_data_stream(shared, &owner, &registered.route.id).await
}

fn admit(
    shared: &Arc<GatewayShared>,
    policy: &RoutePolicy,
    ip: IpAddr,
    bearer: Option<&str>,
) -> Result<()> {
    match shared.acl.evaluate(policy, ip, bearer) {
        crate::security::AclDecision::Admit => Ok(()),
        crate::security::AclDecision::DeniedByNetwork => {
            tracing::info!(%ip, "tunnel relay denied by network policy");
            Err(TunnelError::AuthorizationDenied)
        }
        crate::security::AclDecision::DeniedByAuth => {
            tracing::info!(%ip, "tunnel relay denied: visitor authentication required");
            Err(TunnelError::AuthorizationDenied)
        }
    }
}

async fn open_data_stream(
    shared: &Arc<GatewayShared>,
    session_id: &SessionId,
    route_id: &RouteId,
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
        let header = DataStreamHeader::new(route_id, next_id);
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
    let stream = match tokio::time::timeout(open_budget, open).await {
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
        stream_id: next_id,
        route_id: route_id.to_string(),
        session_id: session_id.to_string(),
        _permit: permit,
        read_bytes: AtomicU64::new(0),
    })
}
