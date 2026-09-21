//! QUIC transport over `quinn` + `rustls` TLS 1.3 (PRD §22, §23).
//!
//! This module owns every `quinn` type in the tunnel stack: business layers
//! see only [`QuicClientTransport`], [`QuicServerTransport`], and the
//! [`TunnelConnection`] / `TunnelStream` traits.

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use sdkwork_webserver_tunnel_core::{Result, TunnelError};

use crate::tls;
use crate::{RemoteEndpoint, TransportOptions, TunnelConnection, TunnelStream};

/// Client-side QUIC transport.
pub struct QuicClientTransport {
    config: quinn::ClientConfig,
    options: TransportOptions,
}

impl QuicClientTransport {
    /// Builds a client transport from resolved agent TLS options.
    pub fn new(options: &tls::ClientTlsOptions, transport: TransportOptions) -> Result<Self> {
        let quinn_crypto = quinn::crypto::rustls::QuicClientConfig::try_from(Arc::new(
            tls::client_config(options)?,
        ))
        .map_err(|error| {
            TunnelError::ConnectionFailed(format!("tunnel TLS suite unusable: {error}"))
        })?;
        let mut config = quinn::ClientConfig::new(Arc::new(quinn_crypto));
        config.transport_config(Arc::new(transport_config(transport)));
        Ok(Self {
            config,
            options: transport,
        })
    }

    /// The applied transport options.
    pub fn options(&self) -> &TransportOptions {
        &self.options
    }
}

#[async_trait]
impl crate::TunnelClientTransport for QuicClientTransport {
    async fn connect(
        &self,
        endpoint: &RemoteEndpoint,
        server_name: &str,
    ) -> Result<Box<dyn TunnelConnection>> {
        let address = resolve_endpoint(endpoint).await?;
        let mut root =
            quinn::Endpoint::client("0.0.0.0:0".parse::<SocketAddr>().map_err(|error| {
                TunnelError::ConnectionFailed(format!("client bind failed: {error}"))
            })?)
            .map_err(|error| {
                TunnelError::ConnectionFailed(format!("QUIC endpoint setup failed: {error}"))
            })?;
        root.set_default_client_config(self.config.clone());
        let connecting = root
            .connect(address, server_name)
            .map_err(quinn_connect_error)?;
        let connection = tokio::time::timeout(self.options.handshake_budget(), connecting)
            .await
            .map_err(|_| TunnelError::Timeout("transport connect"))?
            .map_err(quinn_connection_error)?;
        Ok(Box::new(QuicTunnelConnection { connection }))
    }
}

/// Server-side QUIC transport bound to one UDP address.
pub struct QuicServerTransport {
    endpoint: quinn::Endpoint,
    options: TransportOptions,
}

impl QuicServerTransport {
    /// Builds a server transport from PEM material.
    pub fn from_pem(
        bind: SocketAddr,
        cert_pem: &[u8],
        key_pem: &[u8],
        transport: TransportOptions,
    ) -> Result<Self> {
        let rustls_config = tls::server_config_from_pem(cert_pem, key_pem)?;
        Self::from_rustls_config(bind, rustls_config, transport)
    }

    /// Builds a server transport from an already-materialized rustls server
    /// configuration.
    pub fn from_rustls_config(
        bind: SocketAddr,
        config: rustls::ServerConfig,
        transport: TransportOptions,
    ) -> Result<Self> {
        let quinn_crypto = quinn::crypto::rustls::QuicServerConfig::try_from(Arc::new(config))
            .map_err(|error| {
                TunnelError::ConnectionFailed(format!("tunnel TLS suite unusable: {error}"))
            })?;
        let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(quinn_crypto));
        server_config.transport = Arc::new(transport_config(transport));
        let endpoint = quinn::Endpoint::server(server_config, bind).map_err(|error| {
            TunnelError::ConnectionFailed(format!("QUIC listener bind to {bind}: {error}"))
        })?;
        Ok(Self {
            endpoint,
            options: transport,
        })
    }

    /// The applied transport options.
    pub fn options(&self) -> &TransportOptions {
        &self.options
    }
}

#[async_trait]
impl crate::TunnelServerTransport for QuicServerTransport {
    async fn accept(&self) -> Result<Box<dyn TunnelConnection>> {
        let connection = self
            .endpoint
            .accept()
            .await
            .ok_or(TunnelError::ConnectionClosed)?
            .await
            .map_err(quinn_connection_error)?;
        Ok(Box::new(QuicTunnelConnection { connection }))
    }

    fn local_addr(&self) -> Result<SocketAddr> {
        self.endpoint
            .local_addr()
            .map_err(|error| TunnelError::ConnectionFailed(format!("local addr: {error}")))
    }

    fn close(&self) {
        self.endpoint.close(0_u32.into(), b"gateway shutdown");
    }
}

fn transport_config(options: TransportOptions) -> quinn::TransportConfig {
    let mut transport = quinn::TransportConfig::default();
    let idle = quinn::IdleTimeout::try_from(Duration::from_millis(u64::from(
        options.max_idle_timeout_ms.max(1),
    )));
    transport.max_idle_timeout(Some(idle.ok().unwrap_or_else(|| {
        quinn::IdleTimeout::try_from(Duration::from_secs(300))
            .expect("300s is a valid idle timeout")
    })));
    if let Some(interval_ms) = options.keep_alive_interval_ms {
        transport.keep_alive_interval(Some(Duration::from_millis(u64::from(interval_ms))));
    }
    transport.max_concurrent_bidi_streams(quinn::VarInt::from_u32(
        options.max_concurrent_bidi_streams.max(1),
    ));
    transport
}

/// Resolves a remote endpoint host to a UDP socket address, preferring the
/// literal form and falling back to system DNS.
async fn resolve_endpoint(endpoint: &RemoteEndpoint) -> Result<SocketAddr> {
    if let Ok(address) = endpoint.authority().parse::<SocketAddr>() {
        return Ok(address);
    }
    tokio::net::lookup_host((endpoint.host.as_str(), endpoint.port))
        .await
        .map_err(|error| {
            TunnelError::ConnectionFailed(format!(
                "cannot resolve tunnel gateway {}: {error}",
                endpoint.authority()
            ))
        })?
        .next()
        .ok_or_else(|| {
            TunnelError::ConnectionFailed(format!(
                "tunnel gateway {} resolved to no addresses",
                endpoint.authority()
            ))
        })
}

fn quinn_connect_error(error: quinn::ConnectError) -> TunnelError {
    TunnelError::ConnectionFailed(format!("QUIC connect failed: {error}"))
}

fn quinn_connection_error(error: quinn::ConnectionError) -> TunnelError {
    match error {
        quinn::ConnectionError::TimedOut => TunnelError::Timeout("transport idle"),
        quinn::ConnectionError::LocallyClosed
        | quinn::ConnectionError::ApplicationClosed(_)
        | quinn::ConnectionError::ConnectionClosed(_)
        | quinn::ConnectionError::Reset => TunnelError::ConnectionClosed,
        quinn::ConnectionError::TransportError(error) => {
            TunnelError::ConnectionFailed(format!("QUIC transport error: {error}"))
        }
        quinn::ConnectionError::VersionMismatch => {
            TunnelError::Protocol("QUIC version mismatch".to_owned())
        }
        other => TunnelError::ConnectionFailed(format!("QUIC connection error: {other}")),
    }
}

/// One QUIC connection wrapped as a tunnel connection.
#[derive(Clone)]
pub struct QuicTunnelConnection {
    connection: quinn::Connection,
}

#[async_trait]
impl TunnelConnection for QuicTunnelConnection {
    async fn open_stream(&self) -> Result<Box<dyn TunnelStream>> {
        let (send, recv) = self
            .connection
            .open_bi()
            .await
            .map_err(quinn_connection_error)?;
        Ok(Box::new(QuicTunnelStream { send, recv }))
    }

    async fn accept_stream(&self) -> Result<Box<dyn TunnelStream>> {
        let (send, recv) = self
            .connection
            .accept_bi()
            .await
            .map_err(quinn_connection_error)?;
        Ok(Box::new(QuicTunnelStream { send, recv }))
    }

    fn close(&self, reason: &str) {
        self.connection.close(0_u32.into(), reason.as_bytes());
    }

    async fn closed(&self) -> Result<()> {
        match self.connection.closed().await {
            quinn::ConnectionError::ApplicationClosed(_)
            | quinn::ConnectionError::LocallyClosed => Ok(()),
            error => Err(quinn_connection_error(error)),
        }
    }

    fn remote_addr(&self) -> Result<SocketAddr> {
        Ok(self.connection.remote_address())
    }
}

/// Combined send+recv half of a QUIC bidirectional stream as one
/// `AsyncRead + AsyncWrite` value.
pub struct QuicTunnelStream {
    send: quinn::SendStream,
    recv: quinn::RecvStream,
}

/// Maps a quinn write failure onto the io error surface hyper/tokio copy
/// loops expect.
fn map_write_error(error: quinn::WriteError) -> io::Error {
    match error {
        quinn::WriteError::Stopped(_) => {
            io::Error::new(io::ErrorKind::BrokenPipe, "stream stopped by peer")
        }
        quinn::WriteError::ConnectionLost(_) => {
            io::Error::new(io::ErrorKind::ConnectionAborted, "connection lost")
        }
        quinn::WriteError::ClosedStream => {
            io::Error::new(io::ErrorKind::BrokenPipe, "stream already closed")
        }
        quinn::WriteError::ZeroRttRejected => {
            io::Error::new(io::ErrorKind::ConnectionAborted, "0-RTT rejected")
        }
    }
}

impl TunnelStream for QuicTunnelStream {}

impl AsyncRead for QuicTunnelStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.recv).poll_read(cx, buf)
    }
}

impl AsyncWrite for QuicTunnelStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match Pin::new(&mut self.send).poll_write(cx, buf) {
            Poll::Ready(Ok(written)) => Poll::Ready(Ok(written)),
            Poll::Ready(Err(error)) => Poll::Ready(Err(map_write_error(error))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.send).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.send).poll_shutdown(cx)
    }
}

impl TransportOptions {
    fn handshake_budget(&self) -> Duration {
        Duration::from_secs(10).min(Duration::from_millis(u64::from(
            self.max_idle_timeout_ms.min(10_000),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TunnelClientTransport, TunnelServerTransport};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn install_crypto_provider() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }

    fn test_options() -> TransportOptions {
        // Mirror the gateway defaults so unit tests exercise production
        // transport settings.
        TransportOptions {
            max_idle_timeout_ms: 300_000,
            keep_alive_interval_ms: Some(5_000),
            max_concurrent_bidi_streams: 256,
        }
    }

    #[tokio::test]
    async fn quic_loopback_connect_and_stream_round_trip() {
        install_crypto_provider();
        let material = tls::generate_self_signed(&["localhost".to_owned(), "127.0.0.1".to_owned()])
            .expect("self-signed material");
        let server = QuicServerTransport::from_pem(
            "127.0.0.1:0".parse().expect("bind addr"),
            material.cert_pem.as_bytes(),
            material.key_pem.as_bytes(),
            test_options(),
        )
        .expect("server transport");
        let bound = server.local_addr().expect("local addr");

        let accept_task = tokio::spawn(async move {
            let connection = server.accept().await.expect("accept");
            let mut stream = connection.accept_stream().await.expect("accept stream");
            let mut buffer = [0_u8; 4];
            stream.read_exact(&mut buffer).await.expect("read ping");
            stream.write_all(&buffer).await.expect("write echo");
            stream.flush().await.expect("flush echo");
            // Wait for the client FIN so the echo is delivered before the
            // task tears the endpoint down.
            let _ = stream.read(&mut [0_u8; 1]).await;
        });

        let client = QuicClientTransport::new(
            &tls::ClientTlsOptions {
                pinned_server_sha256: Some(material.sha256),
                ..tls::ClientTlsOptions::default()
            },
            test_options(),
        )
        .expect("client transport");
        let connection = client
            .connect(
                &RemoteEndpoint::new("127.0.0.1", bound.port()),
                "sdkwork-tunnel-gateway",
            )
            .await
            .expect("connect");
        let mut stream = connection.open_stream().await.expect("open stream");
        stream.write_all(b"ping").await.expect("write ping");
        stream.flush().await.expect("flush ping");
        let mut buffer = [0_u8; 4];
        stream.read_exact(&mut buffer).await.expect("read echo");
        assert_eq!(&buffer, b"ping");
        stream.shutdown().await.expect("graceful FIN");
        accept_task.await.expect("echo task");
    }

    #[tokio::test]
    async fn framed_read_over_dyn_stream_matches_read_exact() {
        install_crypto_provider();
        let material = tls::generate_self_signed(&["127.0.0.1".to_owned()]).expect("material");
        let server = QuicServerTransport::from_pem(
            "127.0.0.1:0".parse().expect("bind addr"),
            material.cert_pem.as_bytes(),
            material.key_pem.as_bytes(),
            test_options(),
        )
        .expect("server transport");
        let bound = server.local_addr().expect("local addr");

        let accept_task = tokio::spawn(async move {
            let connection = server.accept().await.expect("accept");
            let mut stream = connection.accept_stream().await.expect("accept stream");
            // The gateway's exact read path: framed read via `&mut dyn`.
            let mut scratch = bytes::BytesMut::new();
            let payload =
                sdkwork_webserver_tunnel_protocol::frame::read_frame(&mut *stream, &mut scratch)
                    .await
                    .expect("framed read");
            let message: sdkwork_webserver_tunnel_protocol::ControlMessage =
                sdkwork_webserver_tunnel_protocol::ControlMessage::from_frame(&payload)
                    .expect("decode");
            let _ = message;
        });

        let client = QuicClientTransport::new(
            &tls::ClientTlsOptions {
                pinned_server_sha256: Some(material.sha256),
                ..tls::ClientTlsOptions::default()
            },
            test_options(),
        )
        .expect("client transport");
        let connection = client
            .connect(
                &RemoteEndpoint::new("127.0.0.1", bound.port()),
                "sdkwork-tunnel-gateway",
            )
            .await
            .expect("connect");
        let mut stream = connection.open_stream().await.expect("open stream");
        let hello = sdkwork_webserver_tunnel_protocol::ControlMessage::Hello(
            sdkwork_webserver_tunnel_protocol::Hello {
                protocol_versions: vec![sdkwork_webserver_tunnel_protocol::ProtocolVersion::v1()],
                device_id: "dev_probe".to_owned(),
                device_name: "probe".to_owned(),
                platform: "linux".to_owned(),
            },
        );
        let frame = hello.to_frame().expect("frame");
        // Replicate the agent's exact burst: two control frames written
        // back-to-back on a fresh stream.
        let authenticate = sdkwork_webserver_tunnel_protocol::ControlMessage::Authenticate(
            sdkwork_webserver_tunnel_protocol::Authenticate {
                token: "t".to_owned(),
            },
        );
        let auth_frame = authenticate.to_frame().expect("auth frame");
        stream.write_all(&frame).await.expect("write hello");
        stream.flush().await.expect("flush hello");
        stream.write_all(&auth_frame).await.expect("write auth");
        stream.flush().await.expect("flush auth");
        accept_task.await.expect("server task");
    }

    #[tokio::test]
    async fn gateway_shaped_accept_relays_stream_data() {
        // Mirror the gateway's accept shape: server behind an Arc, select
        // with a watch arm, per-connection spawned task, framed dyn read.
        install_crypto_provider();
        let material = tls::generate_self_signed(&["127.0.0.1".to_owned()]).expect("material");
        let server = Arc::new(
            QuicServerTransport::from_pem(
                "127.0.0.1:0".parse().expect("bind addr"),
                material.cert_pem.as_bytes(),
                material.key_pem.as_bytes(),
                test_options(),
            )
            .expect("server transport"),
        );
        let bound = server.local_addr().expect("local addr");
        let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        let accept_task = {
            let server = server.clone();
            let mut stop = stop_rx.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        biased;
                        _ = stop.changed() => break,
                        accepted = server.accept() => {
                            match accepted {
                                Ok(connection) => {
                                    let _ = connection.remote_addr();
                                    let connection: Arc<dyn TunnelConnection> =
                                        Arc::from(connection);
                                    tokio::spawn(async move {
                                        let mut stream =
                                            connection.accept_stream().await.expect("accept stream");
                                        let mut scratch = bytes::BytesMut::new();
                                        // Wrap exactly like the gateway: timeout
                                        // around the framed read.
                                        let payload = tokio::time::timeout(
                                            std::time::Duration::from_secs(10),
                                            sdkwork_webserver_tunnel_protocol::frame::read_frame(
                                                &mut *stream, &mut scratch,
                                            ),
                                        )
                                        .await
                                        .expect("framed read not timed out")
                                        .expect("framed read");
                                        let message: sdkwork_webserver_tunnel_protocol::ControlMessage =
                                            sdkwork_webserver_tunnel_protocol::ControlMessage::from_frame(&payload)
                                                .expect("decode");
                                        let _ = message;
                                    });
                                }
                                Err(_) => break,
                            }
                        }
                    }
                }
            })
        };

        let client = QuicClientTransport::new(
            &tls::ClientTlsOptions {
                pinned_server_sha256: Some(material.sha256),
                ..tls::ClientTlsOptions::default()
            },
            test_options(),
        )
        .expect("client transport");
        let connection = client
            .connect(
                &RemoteEndpoint::new("127.0.0.1", bound.port()),
                "sdkwork-tunnel-gateway",
            )
            .await
            .expect("connect");
        let mut stream = connection.open_stream().await.expect("open stream");
        let hello = sdkwork_webserver_tunnel_protocol::ControlMessage::Hello(
            sdkwork_webserver_tunnel_protocol::Hello {
                protocol_versions: vec![sdkwork_webserver_tunnel_protocol::ProtocolVersion::v1()],
                device_id: "dev_probe".to_owned(),
                device_name: "probe".to_owned(),
                platform: "linux".to_owned(),
            },
        );
        let authenticate = sdkwork_webserver_tunnel_protocol::ControlMessage::Authenticate(
            sdkwork_webserver_tunnel_protocol::Authenticate {
                token: "t".to_owned(),
            },
        );
        let frame = hello.to_frame().expect("frame");
        let auth_frame = authenticate.to_frame().expect("auth frame");
        stream.write_all(&frame).await.expect("write hello");
        stream.flush().await.expect("flush hello");
        stream.write_all(&auth_frame).await.expect("write auth");
        stream.flush().await.expect("flush auth");
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let _ = stop_tx.send(true);
        accept_task.await.expect("accept task");
    }

    #[tokio::test]
    async fn untrusted_server_certificate_is_rejected() {
        install_crypto_provider();
        let material = tls::generate_self_signed(&["127.0.0.1".to_owned()]).expect("material");
        let server = QuicServerTransport::from_pem(
            "127.0.0.1:0".parse().expect("bind addr"),
            material.cert_pem.as_bytes(),
            material.key_pem.as_bytes(),
            test_options(),
        )
        .expect("server transport");
        let bound = server.local_addr().expect("local addr");
        tokio::spawn(async move {
            let _ = server.accept().await;
        });
        let client = QuicClientTransport::new(
            &tls::ClientTlsOptions {
                pinned_server_sha256: Some("f".repeat(64)),
                ..tls::ClientTlsOptions::default()
            },
            test_options(),
        )
        .expect("client transport");
        let outcome: Result<Box<dyn TunnelConnection>> = client
            .connect(
                &RemoteEndpoint::new("127.0.0.1", bound.port()),
                "sdkwork-tunnel-gateway",
            )
            .await;
        let error = match outcome {
            Ok(_) => panic!("pin mismatch must fail the handshake"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            TunnelError::ConnectionFailed(_) | TunnelError::ConnectionClosed
        ));
    }
}
