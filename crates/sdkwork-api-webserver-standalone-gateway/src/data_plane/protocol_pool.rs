//! Bounded upstream transport for the CGI family: TCP or unix sockets, source
//! address binding, `SO_KEEPALIVE`, and the per-generation FastCGI connection
//! pool (`fastcgi_keep_conn`).
//!
//! The pool is deliberately small and bounded in three independent ways:
//!
//! - one in-flight request per connection: a connection is only returned to the
//!   pool after a complete, clean response, so it is never shared;
//! - a global connection cap and a per-upstream idle cap (`maxIdleConnections`);
//! - an idle deadline per connection (`idleConnectionTimeoutMs`), checked when
//!   a connection is taken and when one is returned.
//!
//! The pool lives in the runtime generation: a reload drops the whole
//! generation, which drops every idle connection instead of carrying sockets
//! from an old configuration into a new one.
//!
//! Unix-socket targets are accepted here too. They are configured only through
//! the typed plane (`[http] unixSocketRoot`); the path confinement is validated
//! semantically and re-checked at connect time.

use std::{
    collections::VecDeque,
    net::{IpAddr, SocketAddr},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::{Duration, Instant},
};

use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
};
use tokio_rustls::{client::TlsStream as ClientTlsStream, TlsConnector};

#[cfg(unix)]
use tokio::net::UnixStream;

/// Hard process-wide bound on idle CGI-family connections held by one runtime
/// generation. `maxIdleConnections` per upstream bounds the same pool more
/// tightly per target.
pub(crate) const MAX_GENERATION_POOLED_CONNECTIONS: usize = 256;

/// One connected upstream stream.
pub(crate) enum ProtocolStream {
    Tcp(TcpStream),
    /// TLS over TCP for the CGI family when the upstream target is `https://`
    /// (`uwsgi_pass suwsgi://…` on the nginx compatibility plane, or a typed
    /// upstream TLS policy). Boxed to keep the enum small.
    Tls(Box<ClientTlsStream<TcpStream>>),
    #[cfg(unix)]
    Unix(UnixStream),
}

impl ProtocolStream {
    /// Whether an idle connection still looks usable.
    ///
    /// A FastCGI app that ignores the `keep_conn` flag closes the socket as soon
    /// as the response is written, so a pooled socket can be readable at EOF.
    /// Probing before reuse turns that into a fresh connect instead of a failed
    /// request. `Err(WouldBlock)` is the healthy "no bytes yet" case.
    pub(crate) fn is_idle_healthy(&mut self) -> bool {
        let mut probe = [0u8; 1];
        let result = match self {
            Self::Tcp(stream) => stream.try_read(&mut probe),
            // A TLS record stream cannot be probed for EOF without consuming
            // handshake/record bytes, so a pooled TLS connection is assumed
            // healthy and a stale one surfaces as a transport failure that the
            // bounded retry policy handles (nginx reuses uWSGI/SCGI connections
            // not at all; TLS pooling is FastCGI `keep_conn` only).
            Self::Tls(_) => return true,
            #[cfg(unix)]
            Self::Unix(stream) => stream.try_read(&mut probe),
        };
        matches!(
            result,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        )
    }
}

impl AsyncRead for ProtocolStream {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_read(context, buffer),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_read(context, buffer),
            #[cfg(unix)]
            Self::Unix(stream) => Pin::new(stream).poll_read(context, buffer),
        }
    }
}

impl AsyncWrite for ProtocolStream {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_write(context, buffer),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_write(context, buffer),
            #[cfg(unix)]
            Self::Unix(stream) => Pin::new(stream).poll_write(context, buffer),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_flush(context),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_flush(context),
            #[cfg(unix)]
            Self::Unix(stream) => Pin::new(stream).poll_flush(context),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_shutdown(context),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_shutdown(context),
            #[cfg(unix)]
            Self::Unix(stream) => Pin::new(stream).poll_shutdown(context),
        }
    }
}

/// A resolved CGI-family upstream destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ResolvedTarget {
    Tcp(SocketAddr),
    #[cfg(unix)]
    Unix(std::path::PathBuf),
}

/// Pool key: the selected target URL, the upstream identity (one TLS policy per
/// upstream), and the configured source bind (a bind change must never reuse a
/// socket opened from a different source address).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PoolKey {
    pub(crate) upstream: String,
    pub(crate) target: String,
    pub(crate) bind: Option<IpAddr>,
}

struct IdleConnection {
    key: PoolKey,
    stream: ProtocolStream,
    expires_at: Instant,
}

/// Bounded idle-connection cache for one runtime generation.
pub(crate) struct ProtocolPool {
    idle: Mutex<VecDeque<IdleConnection>>,
    capacity: usize,
}

impl ProtocolPool {
    pub(crate) fn new() -> Self {
        Self {
            idle: Mutex::new(VecDeque::new()),
            capacity: MAX_GENERATION_POOLED_CONNECTIONS,
        }
    }

    /// Take a healthy idle connection for `key`.
    pub(crate) fn take(&self, key: &PoolKey) -> Option<ProtocolStream> {
        let now = Instant::now();
        let mut idle = self
            .idle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut index = 0;
        while index < idle.len() {
            if idle[index].expires_at <= now {
                idle.remove(index);
                continue;
            }
            if &idle[index].key == key {
                let mut connection = idle.remove(index).expect("checked index");
                if connection.stream.is_idle_healthy() {
                    return Some(connection.stream);
                }
                // Closed by the app: drop it and look for another idle socket.
                continue;
            }
            index += 1;
        }
        None
    }

    /// Return a clean connection to the pool, honoring the per-upstream idle
    /// bound and the global cap (oldest idle connection is evicted first).
    pub(crate) fn put(
        &self,
        key: PoolKey,
        stream: ProtocolStream,
        per_key_max: usize,
        idle_timeout: Duration,
    ) {
        if per_key_max == 0 || idle_timeout.is_zero() {
            return;
        }
        let now = Instant::now();
        let mut idle = self
            .idle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        idle.retain(|connection| connection.expires_at > now);
        let for_key = idle
            .iter()
            .filter(|connection| connection.key == key)
            .count();
        if for_key >= per_key_max {
            return;
        }
        if idle.len() >= self.capacity {
            idle.pop_front();
        }
        idle.push_back(IdleConnection {
            key,
            stream,
            expires_at: now + idle_timeout,
        });
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.idle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }
}

/// Connect one upstream stream, applying the configured source bind address and
/// `SO_KEEPALIVE` for TCP targets.
pub(crate) async fn connect_stream(
    target: &ResolvedTarget,
    bind: Option<IpAddr>,
    socket_keepalive: bool,
) -> std::io::Result<ProtocolStream> {
    match target {
        ResolvedTarget::Tcp(address) => {
            let socket = if address.is_ipv4() {
                tokio::net::TcpSocket::new_v4()?
            } else {
                tokio::net::TcpSocket::new_v6()?
            };
            if let Some(bind) = bind {
                if bind.is_ipv4() != address.is_ipv4() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "upstream bind address family differs from the target address family",
                    ));
                }
                socket.bind(SocketAddr::new(bind, 0))?;
            }
            if socket_keepalive {
                socket2::SockRef::from(&socket).set_keepalive(true)?;
            }
            let stream = socket.connect(*address).await?;
            Ok(ProtocolStream::Tcp(stream))
        }
        #[cfg(unix)]
        ResolvedTarget::Unix(path) => Ok(ProtocolStream::Unix(UnixStream::connect(path).await?)),
        #[cfg(not(unix))]
        ResolvedTarget::Unix(_) => Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "unix upstream sockets are not supported on this platform",
        )),
    }
}

/// Connect one upstream stream over TLS, reusing the plain transport (bind,
/// `SO_KEEPALIVE`) and completing the rustls handshake with the policy built by
/// `upstream_tls::build_upstream_tls_config`. ALPN is left empty (the CGI
/// protocols have no ALPN identifier, matching nginx).
pub(crate) async fn connect_tls_stream(
    target: &ResolvedTarget,
    bind: Option<IpAddr>,
    socket_keepalive: bool,
    client_config: Arc<rustls::ClientConfig>,
    server_name: rustls::pki_types::ServerName<'static>,
) -> std::io::Result<ProtocolStream> {
    let ResolvedTarget::Tcp(_) = target else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "TLS is only supported for TCP upstream targets",
        ));
    };
    let stream = connect_stream(target, bind, socket_keepalive).await?;
    let ProtocolStream::Tcp(tcp) = stream else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "TLS is only supported for TCP upstream targets",
        ));
    };
    let connector = TlsConnector::from(client_config);
    let tls = connector
        .connect(server_name, tcp)
        .await
        .map_err(std::io::Error::other)?;
    Ok(ProtocolStream::Tls(Box::new(tls)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn key(target: &str, bind: Option<IpAddr>) -> PoolKey {
        PoolKey {
            upstream: "test-upstream".to_owned(),
            target: target.to_owned(),
            bind,
        }
    }

    /// A loopback echo server that keeps one connection open per accepted
    /// socket until the peer closes, so the pool can be exercised on real
    /// sockets.
    async fn echo_listener() -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind echo listener");
        let address = listener.local_addr().expect("listener address");
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut connection, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buffer = [0u8; 64];
                    while let Ok(read) = connection.read(&mut buffer).await {
                        if read == 0 {
                            break;
                        }
                        if connection.write_all(&buffer[..read]).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });
        (address, task)
    }

    #[tokio::test]
    async fn a_clean_connection_is_handed_back_to_the_same_key_only() {
        let (address, _task) = echo_listener().await;
        let pool = ProtocolPool::new();
        let first = connect_stream(&ResolvedTarget::Tcp(address), None, false)
            .await
            .expect("connect");
        pool.put(key("tcp://a", None), first, 4, Duration::from_secs(10));
        assert_eq!(pool.len(), 1);
        assert!(pool.take(&key("tcp://b", None)).is_none());
        let mut taken = pool
            .take(&key("tcp://a", None))
            .expect("same key takes the connection");
        taken.write_all(b"ping").await.expect("write");
        let mut buffer = [0u8; 4];
        taken.read_exact(&mut buffer).await.expect("read echo");
        assert_eq!(&buffer, b"ping");
        assert_eq!(pool.len(), 0);
    }

    #[tokio::test]
    async fn a_pooled_connection_is_handed_out_at_most_once() {
        let (address, _task) = echo_listener().await;
        let pool = ProtocolPool::new();
        let stream = connect_stream(&ResolvedTarget::Tcp(address), None, false)
            .await
            .expect("connect");
        pool.put(key("tcp://once", None), stream, 4, Duration::from_secs(10));
        assert!(pool.take(&key("tcp://once", None)).is_some());
        assert!(
            pool.take(&key("tcp://once", None)).is_none(),
            "one in-flight request per connection: a handed-out socket is never shared"
        );
    }

    #[tokio::test]
    async fn idle_deadline_and_per_key_cap_bound_the_pool() {
        let (address, _task) = echo_listener().await;
        let pool = ProtocolPool::new();
        for _ in 0..3 {
            let stream = connect_stream(&ResolvedTarget::Tcp(address), None, false)
                .await
                .expect("connect");
            pool.put(key("tcp://a", None), stream, 2, Duration::from_millis(50));
        }
        assert_eq!(pool.len(), 2, "per-key cap keeps at most two");
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(pool.take(&key("tcp://a", None)).is_none(), "expired");
        assert_eq!(pool.len(), 0);
    }

    #[tokio::test]
    async fn a_bind_address_changes_the_pool_key() {
        let pool = ProtocolPool::new();
        let (address, _task) = echo_listener().await;
        let stream = connect_stream(
            &ResolvedTarget::Tcp(address),
            Some(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
            false,
        )
        .await
        .expect("bind loopback and connect");
        pool.put(key("tcp://a", None), stream, 4, Duration::from_secs(10));
        assert!(
            pool.take(&key(
                "tcp://a",
                Some(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
            ))
            .is_none(),
            "a connection opened from a bind source must not be reused for an unbound target"
        );
        assert!(pool.take(&key("tcp://a", None)).is_some());
    }

    #[tokio::test]
    async fn session_keepalive_does_not_change_the_connected_stream() {
        let (address, _task) = echo_listener().await;
        let mut stream = connect_stream(&ResolvedTarget::Tcp(address), None, true)
            .await
            .expect("SO_KEEPALIVE connect");
        stream.write_all(b"ok").await.expect("write");
        let mut buffer = [0u8; 2];
        stream.read_exact(&mut buffer).await.expect("read");
        assert_eq!(&buffer, b"ok");
    }
}
