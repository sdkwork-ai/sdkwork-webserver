use std::{
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use tokio::net::TcpStream;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::{OwnedSemaphorePermit, Semaphore},
};

use super::metrics::{ConnectionMetricLease, ConnectionRejection, DataPlaneMetrics};

pub struct ConnectionLimiter {
    global_permits: Arc<Semaphore>,
    listener_permits: Arc<Semaphore>,
    metrics: Arc<DataPlaneMetrics>,
}

impl ConnectionLimiter {
    pub fn new(
        global_permits: Arc<Semaphore>,
        maximum_listener_connections: usize,
        metrics: Arc<DataPlaneMetrics>,
    ) -> Self {
        Self {
            global_permits,
            listener_permits: Arc::new(Semaphore::new(maximum_listener_connections)),
            metrics,
        }
    }

    pub fn try_admit<I>(&self, stream: I) -> io::Result<ConnectionLimitedStream<I>> {
        let global_permit = match self.global_permits.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                self.metrics
                    .record_connection_rejection(ConnectionRejection::Capacity);
                return Err(connection_limit_error());
            }
        };
        let listener_permit = match self.listener_permits.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                self.metrics
                    .record_connection_rejection(ConnectionRejection::Capacity);
                return Err(connection_limit_error());
            }
        };
        Ok(ConnectionLimitedStream {
            inner: stream,
            _global_permit: global_permit,
            _listener_permit: listener_permit,
            _metrics_lease: self.metrics.begin_connection(),
        })
    }
}

pub struct ConnectionLimitedStream<I> {
    inner: I,
    _global_permit: OwnedSemaphorePermit,
    _listener_permit: OwnedSemaphorePermit,
    _metrics_lease: ConnectionMetricLease,
}

impl ConnectionLimitedStream<TcpStream> {
    pub async fn peek(&self, buffer: &mut [u8]) -> io::Result<usize> {
        self.inner.peek(buffer).await
    }
}

impl<I: AsyncRead + Unpin> AsyncRead for ConnectionLimitedStream<I> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(context, buffer)
    }
}

impl<I: AsyncWrite + Unpin> AsyncWrite for ConnectionLimitedStream<I> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        Pin::new(&mut self.inner).poll_write(context, buffer)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.inner).poll_flush(context)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(context)
    }
}

fn connection_limit_error() -> io::Error {
    io::Error::new(io::ErrorKind::ConnectionAborted, "connection limit reached")
}
