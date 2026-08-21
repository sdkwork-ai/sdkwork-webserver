use std::{
    future::Future,
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use axum_server::accept::Accept;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    time::{Instant, Sleep},
};

use super::metrics::{DataPlaneMetrics, ProtocolErrorKind};

#[derive(Clone)]
pub(crate) struct WriteTimeoutAcceptor<A> {
    inner: A,
    timeout: Duration,
    metrics: Option<Arc<DataPlaneMetrics>>,
}

impl<A> WriteTimeoutAcceptor<A> {
    pub(crate) fn new_observed(
        inner: A,
        timeout: Duration,
        metrics: Arc<DataPlaneMetrics>,
    ) -> Self {
        Self {
            inner,
            timeout,
            metrics: Some(metrics),
        }
    }
}

impl<A, I, S> Accept<I, S> for WriteTimeoutAcceptor<A>
where
    A: Accept<I, S> + Clone + Send + Sync + 'static,
    A::Future: Send + 'static,
    A::Stream: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    A::Service: Send + 'static,
    I: Send + 'static,
    S: Send + 'static,
{
    type Stream = WriteTimeoutStream<A::Stream>;
    type Service = A::Service;
    type Future =
        Pin<Box<dyn Future<Output = io::Result<(Self::Stream, Self::Service)>> + Send + 'static>>;

    fn accept(&self, stream: I, service: S) -> Self::Future {
        let inner = self.inner.clone();
        let timeout = self.timeout;
        let metrics = self.metrics.clone();
        Box::pin(async move {
            let (stream, service) = inner.accept(stream, service).await?;
            Ok((
                WriteTimeoutStream::new_observed(stream, timeout, metrics),
                service,
            ))
        })
    }
}

pub(crate) struct WriteTimeoutStream<I> {
    inner: I,
    deadline: WriteDeadline,
    metrics: Option<Arc<DataPlaneMetrics>>,
    timeout_recorded: bool,
}

impl<I> WriteTimeoutStream<I> {
    fn new_observed(inner: I, timeout: Duration, metrics: Option<Arc<DataPlaneMetrics>>) -> Self {
        Self {
            inner,
            deadline: WriteDeadline::new(timeout),
            metrics,
            timeout_recorded: false,
        }
    }

    fn record_deadline_timeout<T>(&mut self, result: &Poll<io::Result<T>>) {
        if !self.timeout_recorded
            && matches!(result, Poll::Ready(Err(error)) if error.kind() == io::ErrorKind::TimedOut)
        {
            if let Some(metrics) = &self.metrics {
                metrics.record_protocol_error(ProtocolErrorKind::DownstreamWriteTimeout);
            }
            self.timeout_recorded = true;
        }
    }
}

impl<I: AsyncRead + Unpin> AsyncRead for WriteTimeoutStream<I> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(context, output)
    }
}

impl<I: AsyncWrite + Unpin> AsyncWrite for WriteTimeoutStream<I> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        match Pin::new(&mut self.inner).poll_write(context, bytes) {
            Poll::Ready(result) => {
                self.deadline.disarm();
                Poll::Ready(result)
            }
            Poll::Pending => {
                let result = self.deadline.poll_timeout(context);
                self.record_deadline_timeout(&result);
                result
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match Pin::new(&mut self.inner).poll_flush(context) {
            Poll::Ready(result) => {
                self.deadline.disarm();
                Poll::Ready(result)
            }
            Poll::Pending => {
                let result = self.deadline.poll_timeout(context);
                self.record_deadline_timeout(&result);
                result.map(|result| result.map(|_| ()))
            }
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match Pin::new(&mut self.inner).poll_shutdown(context) {
            Poll::Ready(result) => {
                self.deadline.disarm();
                Poll::Ready(result)
            }
            Poll::Pending => {
                let result = self.deadline.poll_timeout(context);
                self.record_deadline_timeout(&result);
                result.map(|result| result.map(|_| ()))
            }
        }
    }
}

struct WriteDeadline {
    timeout: Duration,
    sleep: Option<Pin<Box<Sleep>>>,
    armed: bool,
}

impl WriteDeadline {
    fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            sleep: None,
            armed: false,
        }
    }

    fn poll_timeout(&mut self, context: &mut Context<'_>) -> Poll<io::Result<usize>> {
        if !self.armed {
            let deadline = Instant::now() + self.timeout;
            if let Some(sleep) = &mut self.sleep {
                sleep.as_mut().reset(deadline);
            } else {
                self.sleep = Some(Box::pin(tokio::time::sleep_until(deadline)));
            }
            self.armed = true;
        }
        let sleep = self.sleep.as_mut().expect("armed write deadline has timer");
        if sleep.as_mut().poll(context).is_ready() {
            self.armed = false;
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "connection write timeout exceeded",
            )))
        } else {
            Poll::Pending
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

#[cfg(test)]
mod tests {
    use std::{io, pin::Pin, task::Context, task::Poll, time::Duration};

    use futures_util::future::poll_fn;
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

    use super::WriteTimeoutStream;
    use crate::{
        data_plane::metrics::{DataPlaneMetrics, ProtocolErrorKind},
        metric_dimensions::CanonicalMetricDimensions,
    };

    struct PendingWriter;

    impl AsyncRead for PendingWriter {
        fn poll_read(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
            _output: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Pending
        }
    }

    impl AsyncWrite for PendingWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
            _bytes: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Pending
        }

        fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Pending
        }

        fn poll_shutdown(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Pending
        }
    }

    #[tokio::test]
    async fn times_out_pending_write_flush_and_shutdown() {
        for operation in 0..3 {
            let metrics = DataPlaneMetrics::new(CanonicalMetricDimensions::default());
            let mut stream = WriteTimeoutStream::new_observed(
                PendingWriter,
                Duration::from_millis(50),
                Some(metrics.clone()),
            );
            let result = poll_fn(|context| match operation {
                0 => Pin::new(&mut stream)
                    .poll_write(context, b"blocked")
                    .map(|result| result.map(|_| ())),
                1 => Pin::new(&mut stream).poll_flush(context),
                _ => Pin::new(&mut stream).poll_shutdown(context),
            })
            .await;
            assert_eq!(
                result
                    .expect_err("pending write operation must time out")
                    .kind(),
                io::ErrorKind::TimedOut
            );
            assert_eq!(
                metrics.protocol_error_count(ProtocolErrorKind::DownstreamWriteTimeout),
                1
            );
        }
    }
}
