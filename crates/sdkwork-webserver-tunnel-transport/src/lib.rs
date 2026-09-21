//! SDKWork Web Server tunnel transport layer.
//!
//! Transport abstraction (PRD §10.3, §22) plus the V1 QUIC implementation.
//! Business layers ([`sdkwork_webserver_tunnel`] agent/gateway) depend only
//! on the traits in this crate's root; every `quinn`/`rustls` type stays
//! inside the [`quic`] and [`tls`] modules so a future TCP or WebSocket
//! transport can slot in without touching callers (PRD §132).
//!
//! TLS policy (PRD §24): the tunnel speaks QUIC over TLS 1.3 only, via the
//! process-level rustls crypto provider. The transport crate never disables
//! verification implicitly; dev skews (pinned fingerprint, skip-verify) are
//! explicit configuration and loudly logged at runtime.

pub mod quic;
pub mod tls;

use std::fmt;

use async_trait::async_trait;

use sdkwork_webserver_tunnel_core::Result;

pub use quic::{QuicClientTransport, QuicServerTransport, QuicTunnelStream};

/// ALPN identifier negotiated on every tunnel QUIC connection.
pub const TUNNEL_ALPN: &[u8] = b"sdkwork-tunnel/1";

/// Where a client transport dials: host (DNS name or IP literal) plus UDP
/// port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEndpoint {
    /// Gateway host: DNS name or IP literal.
    pub host: String,
    /// Gateway QUIC/UDP port.
    pub port: u16,
}

impl RemoteEndpoint {
    /// Builds an endpoint from parts.
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    /// Renders `host:port` for logs and display.
    pub fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

impl fmt::Display for RemoteEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.authority())
    }
}

/// Tunable transport behavior, applied by the composition layer from tunnel
/// configuration. Transport implementations must treat every field as an
/// upper bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportOptions {
    /// Idle timeout: a connection with no traffic for this long closes.
    pub max_idle_timeout_ms: u32,
    /// Keep-alive probing interval; `None` disables probing.
    pub keep_alive_interval_ms: Option<u32>,
    /// Maximum concurrent bidirectional streams per connection.
    pub max_concurrent_bidi_streams: u32,
}

impl Default for TransportOptions {
    fn default() -> Self {
        Self {
            max_idle_timeout_ms: 300_000,
            keep_alive_interval_ms: Some(5_000),
            max_concurrent_bidi_streams: 512,
        }
    }
}

/// A bidirectional byte stream inside a tunnel connection.
pub trait TunnelStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}

/// One established tunnel connection carrying a control stream and data
/// streams.
#[async_trait]
pub trait TunnelConnection: Send + Sync {
    /// Opens a new bidirectional stream (gateway: data stream; agent: unused
    /// in V1 but required by the QUIC symmetry).
    async fn open_stream(&self) -> Result<Box<dyn TunnelStream>>;

    /// Accepts an incoming bidirectional stream.
    async fn accept_stream(&self) -> Result<Box<dyn TunnelStream>>;

    /// Closes the connection with a reason string; further streams fail.
    fn close(&self, reason: &str);

    /// Resolves when the connection has closed, with the cause.
    async fn closed(&self) -> Result<()>;

    /// The remote peer address, when the transport exposes one.
    fn remote_addr(&self) -> Result<std::net::SocketAddr>;
}

/// Client-side transport: dials a gateway.
#[async_trait]
pub trait TunnelClientTransport: Send + Sync {
    /// Establishes one connection to `endpoint`.
    async fn connect(
        &self,
        endpoint: &RemoteEndpoint,
        server_name: &str,
    ) -> Result<Box<dyn TunnelConnection>>;
}

/// Server-side transport: accepts agent connections.
#[async_trait]
pub trait TunnelServerTransport: Send + Sync {
    /// Accepts the next agent connection.
    async fn accept(&self) -> Result<Box<dyn TunnelConnection>>;

    /// Local bind address of the listener.
    fn local_addr(&self) -> Result<std::net::SocketAddr>;

    /// Closes the listener and every live connection (graceful gateway
    /// shutdown, PRD §105).
    fn close(&self);
}
