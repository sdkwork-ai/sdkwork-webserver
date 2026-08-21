use std::{
    error::Error as StdError,
    future::Future,
    io,
    net::{Shutdown, SocketAddr},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
    task::{Context, Poll},
    time::Duration,
};

use axum::body::Body;
use bytes::Bytes;
use http::{HeaderMap, Request, Response, Uri};
use http_body::{Body as HttpBody, Frame, SizeHint};
use hyper::body::Incoming;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::{
    client::legacy::{
        connect::{Connected, Connection},
        Client, Error as HyperClientError,
    },
    rt::{TokioExecutor, TokioIo, TokioTimer},
};
use sdkwork_webserver_core::UpstreamConfig;
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
    sync::{OwnedSemaphorePermit, Semaphore},
    time::{timeout_at, Instant, Sleep},
};
use tower::Service;
use url::Url;

use super::{dns::GuardedDnsResolver, upstream_tls::build_upstream_tls_config, DataPlaneError};

type BoxError = Box<dyn StdError + Send + Sync>;
type InnerConnector = HttpsConnector<BoundedConnector>;
type InnerClient = Client<TimedConnector, Body>;

const MULTI_ADDRESS_FALLBACK_TIMEOUT: Duration = Duration::from_millis(250);

pub(crate) struct UpstreamClient {
    inner: InnerClient,
    request_timeout: Duration,
    max_response_header_bytes: usize,
    max_response_headers: usize,
    connection_shutdown: Arc<ConnectionShutdown>,
    connection_permits: Arc<Semaphore>,
    max_connections: usize,
    target_connection_capacities: Arc<[TargetConnectionCapacity]>,
}

impl UpstreamClient {
    pub(crate) fn build(
        app: &sdkwork_webserver_core::CompiledWebServerApp,
        config: &UpstreamConfig,
        resolver: Arc<GuardedDnsResolver>,
    ) -> Result<Self, DataPlaneError> {
        let connection_permits = Arc::new(Semaphore::new(config.max_connections));
        let target_connection_capacities = build_target_connection_capacities(config)?;
        let connection_shutdown = Arc::new(ConnectionShutdown::default());
        let tcp = BoundedConnector {
            resolver,
            permits: connection_permits.clone(),
            target_capacities: target_connection_capacities.clone(),
            shutdown: connection_shutdown.clone(),
            address_attempt_timeout: Duration::from_millis(config.connect_timeout_ms)
                .min(MULTI_ADDRESS_FALLBACK_TIMEOUT),
        };
        let tls = build_upstream_tls_config(app, config)?;
        let connector = HttpsConnectorBuilder::new()
            .with_tls_config(tls)
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .wrap_connector(tcp);
        let connector = TimedConnector {
            inner: connector,
            timeout: Duration::from_millis(config.connect_timeout_ms),
        };
        let mut builder = Client::builder(TokioExecutor::new());
        builder
            .pool_timer(TokioTimer::new())
            .timer(TokioTimer::new())
            .pool_idle_timeout(Duration::from_millis(config.idle_connection_timeout_ms))
            .pool_max_idle_per_host(config.max_idle_connections)
            .http1_max_buf_size(config.max_response_header_bytes)
            .http2_max_header_list_size(config.max_response_header_bytes as u32)
            .http2_adaptive_window(false);
        if config.max_response_headers != 100 {
            builder.http1_max_headers(config.max_response_headers);
        }
        Ok(Self {
            inner: builder.build(connector),
            request_timeout: Duration::from_millis(config.request_timeout_ms),
            max_response_header_bytes: config.max_response_header_bytes,
            max_response_headers: config.max_response_headers,
            connection_shutdown,
            connection_permits,
            max_connections: config.max_connections,
            target_connection_capacities,
        })
    }

    pub(crate) fn connection_capacity(&self) -> [u64; 3] {
        let configured = self.max_connections as u64;
        let available = self
            .connection_permits
            .available_permits()
            .min(self.max_connections) as u64;
        [configured, configured.saturating_sub(available), available]
    }

    pub(crate) fn target_connection_capacity(&self) -> [u64; 3] {
        self.target_connection_capacities
            .iter()
            .fold([0_u64; 3], |mut total, capacity| {
                let Some(maximum) = capacity.maximum else {
                    return total;
                };
                let configured = maximum as u64;
                let available = capacity
                    .permits
                    .as_ref()
                    .map_or(0, |permits| permits.available_permits().min(maximum))
                    as u64;
                total[0] = total[0].saturating_add(configured);
                total[1] = total[1].saturating_add(configured.saturating_sub(available));
                total[2] = total[2].saturating_add(available);
                total
            })
    }

    pub(crate) async fn execute(
        &self,
        request: Request<Body>,
    ) -> Result<Response<UpstreamResponseBody>, UpstreamRequestError> {
        self.execute_with_timeout(request, self.request_timeout)
            .await
    }

    pub(crate) async fn execute_with_timeout(
        &self,
        request: Request<Body>,
        timeout: Duration,
    ) -> Result<Response<UpstreamResponseBody>, UpstreamRequestError> {
        let deadline = Instant::now() + timeout;
        let response = timeout_at(deadline, self.inner.request(request))
            .await
            .map_err(|_| UpstreamRequestError::Timeout)?
            .map_err(UpstreamRequestError::Transport)?;
        validate_response_headers(
            response.headers(),
            self.max_response_header_bytes,
            self.max_response_headers,
        )?;
        let (parts, body) = response.into_parts();
        Ok(Response::from_parts(
            parts,
            UpstreamResponseBody::new(body, deadline),
        ))
    }
}

fn build_target_connection_capacities(
    config: &UpstreamConfig,
) -> Result<Arc<[TargetConnectionCapacity]>, DataPlaneError> {
    if config
        .targets
        .iter()
        .all(|target| target.max_connections.is_none())
    {
        return Ok(Arc::from([]));
    }
    config
        .targets
        .iter()
        .map(|target| {
            let url =
                Url::parse(&target.url).map_err(|_| DataPlaneError::InvalidUpstreamTarget {
                    upstream_id: config.id.clone(),
                    target: target.url.clone(),
                })?;
            let Some(host) = url.host_str() else {
                return Err(DataPlaneError::InvalidUpstreamTarget {
                    upstream_id: config.id.clone(),
                    target: target.url.clone(),
                });
            };
            let Some(port) = url.port_or_known_default() else {
                return Err(DataPlaneError::InvalidUpstreamTarget {
                    upstream_id: config.id.clone(),
                    target: target.url.clone(),
                });
            };
            Ok(TargetConnectionCapacity {
                scheme: url.scheme().to_owned(),
                host: host.to_ascii_lowercase(),
                port,
                maximum: target.max_connections,
                permits: target
                    .max_connections
                    .map(|maximum| Arc::new(Semaphore::new(maximum))),
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Arc::from)
}

impl Drop for UpstreamClient {
    fn drop(&mut self) {
        self.connection_shutdown.close();
    }
}

#[derive(Default)]
struct ConnectionShutdown {
    closed: AtomicBool,
    streams: Mutex<Vec<Weak<TcpStream>>>,
}

impl ConnectionShutdown {
    fn register_connection(&self, stream: &Arc<TcpStream>) {
        let mut streams = self
            .streams
            .lock()
            .expect("connection shutdown stream registry is not poisoned");
        streams.retain(|registered| registered.strong_count() > 0);
        streams.push(Arc::downgrade(stream));
        drop(streams);
        if self.is_closed() {
            let _ = shutdown_stream(stream, Shutdown::Both);
        }
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        let mut streams = self
            .streams
            .lock()
            .expect("connection shutdown stream registry is not poisoned");
        streams.retain(|registered| {
            if let Some(stream) = registered.upgrade() {
                let _ = shutdown_stream(&stream, Shutdown::Both);
                true
            } else {
                false
            }
        });
    }
}

#[derive(Debug, Error)]
pub(crate) enum UpstreamRequestError {
    #[error("upstream whole-request timeout exceeded")]
    Timeout,
    #[error("upstream transport failed: {0}")]
    Transport(#[source] HyperClientError),
    #[error("upstream response Header byte limit exceeded")]
    ResponseHeaderBytesExceeded,
    #[error("upstream response Header field limit exceeded")]
    ResponseHeaderCountExceeded,
}

impl UpstreamRequestError {
    pub(crate) fn is_timeout(&self) -> bool {
        match self {
            Self::Timeout => true,
            Self::Transport(error) => find_connection_error(error).is_some_and(|error| {
                matches!(error, UpstreamConnectError::Timeout)
                    || matches!(error, UpstreamConnectError::Io(source) if source.kind() == io::ErrorKind::TimedOut)
            }),
            Self::ResponseHeaderBytesExceeded | Self::ResponseHeaderCountExceeded => false,
        }
    }

    pub(crate) fn is_connection_saturated(&self) -> bool {
        matches!(
            self,
            Self::Transport(error)
                if matches!(find_connection_error(error), Some(UpstreamConnectError::Saturated))
        )
    }
}

fn validate_response_headers(
    headers: &HeaderMap,
    maximum_bytes: usize,
    maximum_headers: usize,
) -> Result<(), UpstreamRequestError> {
    if headers.len() > maximum_headers {
        return Err(UpstreamRequestError::ResponseHeaderCountExceeded);
    }

    let mut bytes = 2usize;
    for (name, value) in headers {
        let field_bytes = name
            .as_str()
            .len()
            .checked_add(value.as_bytes().len())
            .and_then(|bytes| bytes.checked_add(4))
            .ok_or(UpstreamRequestError::ResponseHeaderBytesExceeded)?;
        bytes = add_response_header_bytes(bytes, field_bytes, maximum_bytes)?;
    }
    Ok(())
}

fn add_response_header_bytes(
    current: usize,
    field_bytes: usize,
    maximum_bytes: usize,
) -> Result<usize, UpstreamRequestError> {
    let bytes = current
        .checked_add(field_bytes)
        .ok_or(UpstreamRequestError::ResponseHeaderBytesExceeded)?;
    if bytes > maximum_bytes {
        return Err(UpstreamRequestError::ResponseHeaderBytesExceeded);
    }
    Ok(bytes)
}

fn find_connection_error<'a>(
    error: &'a (dyn StdError + 'static),
) -> Option<&'a UpstreamConnectError> {
    let mut current = Some(error);
    while let Some(error) = current {
        if let Some(connection_error) = error.downcast_ref::<UpstreamConnectError>() {
            return Some(connection_error);
        }
        current = error.source();
    }
    None
}

#[derive(Clone)]
struct BoundedConnector {
    resolver: Arc<GuardedDnsResolver>,
    permits: Arc<Semaphore>,
    target_capacities: Arc<[TargetConnectionCapacity]>,
    shutdown: Arc<ConnectionShutdown>,
    address_attempt_timeout: Duration,
}

impl Service<Uri> for BoundedConnector {
    type Response = TokioIo<PermitStream>;
    type Error = UpstreamConnectError;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let target_permits = if self.target_capacities.is_empty() {
            None
        } else {
            let Some(capacity) = self
                .target_capacities
                .iter()
                .find(|capacity| capacity.matches(&uri))
            else {
                return Box::pin(async { Err(UpstreamConnectError::InvalidUri) });
            };
            capacity.permits.clone()
        };
        let permit = match self.permits.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => return Box::pin(async { Err(UpstreamConnectError::Saturated) }),
        };
        let target_permit = match target_permits {
            Some(permits) => match permits.try_acquire_owned() {
                Ok(permit) => Some(permit),
                Err(_) => return Box::pin(async { Err(UpstreamConnectError::Saturated) }),
            },
            None => None,
        };
        let Some(host) = uri.host().map(str::to_owned) else {
            return Box::pin(async { Err(UpstreamConnectError::InvalidUri) });
        };
        let port = uri.port_u16().or_else(|| match uri.scheme_str() {
            Some("http") => Some(80),
            Some("https") => Some(443),
            _ => None,
        });
        let Some(port) = port else {
            return Box::pin(async { Err(UpstreamConnectError::InvalidUri) });
        };
        let resolver = self.resolver.clone();
        let shutdown = self.shutdown.clone();
        let address_attempt_timeout = self.address_attempt_timeout;
        Box::pin(async move {
            let addresses = resolver
                .resolve_host(host)
                .await
                .map_err(UpstreamConnectError::Io)?;
            let address_count = addresses.len();
            let mut last_error = None;
            for (index, address) in addresses.into_iter().enumerate() {
                let address = SocketAddr::new(address.ip(), port);
                let connect = TcpStream::connect(address);
                let result = if index + 1 == address_count {
                    connect.await
                } else {
                    match tokio::time::timeout(address_attempt_timeout, connect).await {
                        Ok(result) => result,
                        Err(_) => Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "upstream address attempt timed out",
                        )),
                    }
                };
                match result {
                    Ok(stream) => {
                        stream.set_nodelay(true).map_err(UpstreamConnectError::Io)?;
                        let stream = Arc::new(stream);
                        shutdown.register_connection(&stream);
                        return Ok(TokioIo::new(PermitStream {
                            stream,
                            _permit: permit,
                            _target_permit: target_permit,
                        }));
                    }
                    Err(error) => last_error = Some(error),
                }
            }
            Err(UpstreamConnectError::Io(last_error.unwrap_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "no upstream address was connectable",
                )
            })))
        })
    }
}

struct TargetConnectionCapacity {
    scheme: String,
    host: String,
    port: u16,
    maximum: Option<usize>,
    permits: Option<Arc<Semaphore>>,
}

impl TargetConnectionCapacity {
    fn matches(&self, uri: &Uri) -> bool {
        self.scheme == uri.scheme_str().unwrap_or_default()
            && uri
                .host()
                .is_some_and(|host| self.host.eq_ignore_ascii_case(host))
            && effective_uri_port(uri) == Some(self.port)
    }
}

fn effective_uri_port(uri: &Uri) -> Option<u16> {
    uri.port_u16().or_else(|| match uri.scheme_str() {
        Some("http") => Some(80),
        Some("https") => Some(443),
        _ => None,
    })
}

#[derive(Debug, Error)]
enum UpstreamConnectError {
    #[error("upstream physical connection capacity is saturated")]
    Saturated,
    #[error("upstream connection establishment timed out")]
    Timeout,
    #[error("upstream URI has no supported authority")]
    InvalidUri,
    #[error("upstream connection I/O failed: {0}")]
    Io(#[source] io::Error),
}

struct PermitStream {
    stream: Arc<TcpStream>,
    _permit: OwnedSemaphorePermit,
    _target_permit: Option<OwnedSemaphorePermit>,
}

fn shutdown_stream(stream: &TcpStream, how: Shutdown) -> io::Result<()> {
    socket2::SockRef::from(stream).shutdown(how)
}

impl AsyncRead for PermitStream {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            match self.stream.poll_read_ready(context) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {}
            }
            match self.stream.try_read(buffer.initialize_unfilled()) {
                Ok(read) => {
                    buffer.advance(read);
                    return Poll::Ready(Ok(()));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
    }
}

impl AsyncWrite for PermitStream {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        loop {
            match self.stream.poll_write_ready(context) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {}
            }
            match self.stream.try_write(buffer) {
                Ok(written) => return Poll::Ready(Ok(written)),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        Poll::Ready(shutdown_stream(&self.stream, Shutdown::Write))
    }

    fn is_write_vectored(&self) -> bool {
        true
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffers: &[io::IoSlice<'_>],
    ) -> Poll<Result<usize, io::Error>> {
        loop {
            match self.stream.poll_write_ready(context) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {}
            }
            match self.stream.try_write_vectored(buffers) {
                Ok(written) => return Poll::Ready(Ok(written)),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
    }
}

impl Connection for PermitStream {
    fn connected(&self) -> Connected {
        Connected::new()
    }
}

#[derive(Clone)]
struct TimedConnector {
    inner: InnerConnector,
    timeout: Duration,
}

impl Service<Uri> for TimedConnector {
    type Response = <InnerConnector as Service<Uri>>::Response;
    type Error = BoxError;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(context)
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let future = self.inner.call(uri);
        let timeout = self.timeout;
        Box::pin(async move {
            tokio::time::timeout(timeout, future)
                .await
                .map_err(|_| Box::new(UpstreamConnectError::Timeout) as BoxError)?
        })
    }
}

pub(crate) struct UpstreamResponseBody {
    inner: Pin<Box<Incoming>>,
    remaining_hint: SizeHint,
    deadline: Pin<Box<Sleep>>,
    ended: bool,
}

impl UpstreamResponseBody {
    fn new(inner: Incoming, deadline: Instant) -> Self {
        let remaining_hint = inner.size_hint();
        let ended = inner.is_end_stream();
        Self {
            inner: Box::pin(inner),
            remaining_hint,
            deadline: Box::pin(tokio::time::sleep_until(deadline)),
            ended,
        }
    }
}

impl HttpBody for UpstreamResponseBody {
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        if self.ended {
            return Poll::Ready(None);
        }
        if self.deadline.as_mut().poll(context).is_ready() {
            self.ended = true;
            self.remaining_hint = SizeHint::with_exact(0);
            return Poll::Ready(Some(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "upstream whole-request timeout exceeded",
            ))));
        }
        match self.inner.as_mut().poll_frame(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => {
                self.ended = true;
                self.remaining_hint = SizeHint::with_exact(0);
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(error))) => {
                self.ended = true;
                self.remaining_hint = SizeHint::with_exact(0);
                Poll::Ready(Some(Err(io::Error::other(error))))
            }
            Poll::Ready(Some(Ok(frame))) => {
                self.remaining_hint = self.inner.as_ref().size_hint();
                self.ended = self.inner.as_ref().is_end_stream();
                Poll::Ready(Some(Ok(frame)))
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.ended
    }

    fn size_hint(&self) -> SizeHint {
        self.remaining_hint
    }
}

#[cfg(test)]
mod tests {
    use http::{header::HeaderValue, HeaderMap};

    use super::{add_response_header_bytes, validate_response_headers, UpstreamRequestError};

    #[test]
    fn response_header_budget_counts_every_field_and_wire_separator() {
        let mut headers = HeaderMap::new();
        headers.append("x-test", HeaderValue::from_static("abc"));

        validate_response_headers(&headers, 15, 1)
            .expect("six name bytes, three value bytes, separators, and terminator fit exactly");
        assert!(matches!(
            validate_response_headers(&headers, 14, 1),
            Err(UpstreamRequestError::ResponseHeaderBytesExceeded)
        ));

        headers.append("x-test", HeaderValue::from_static("d"));
        validate_response_headers(&headers, 26, 2)
            .expect("repeated Header field occurrence is counted independently");
        assert!(matches!(
            validate_response_headers(&headers, 26, 1),
            Err(UpstreamRequestError::ResponseHeaderCountExceeded)
        ));
    }

    #[test]
    fn response_header_budget_classifies_arithmetic_overflow_as_exceeded() {
        assert!(matches!(
            add_response_header_bytes(usize::MAX, 1, usize::MAX),
            Err(UpstreamRequestError::ResponseHeaderBytesExceeded)
        ));
    }
}
