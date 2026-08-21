use std::{
    future::Future,
    io,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::Duration,
};

use axum_server::accept::Accept;
use futures_util::task::AtomicWaker;
use http::{Request, Response};
use http_body::{Body, Frame, SizeHint};
use sync_wrapper::SyncWrapper;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    time::{Instant, Sleep},
};
use tower::Service;

use super::http1_wire::NegotiatedHttpProtocol;

#[derive(Clone)]
pub(crate) struct Http1KeepAliveTimeoutAcceptor<A> {
    inner: A,
    timeout: Duration,
}

impl<A> Http1KeepAliveTimeoutAcceptor<A> {
    pub(crate) fn new(inner: A, timeout: Duration) -> Self {
        Self { inner, timeout }
    }
}

impl<A, I, S> Accept<I, S> for Http1KeepAliveTimeoutAcceptor<A>
where
    A: Accept<I, S> + Clone + Send + Sync + 'static,
    A::Future: Send + 'static,
    A::Stream: NegotiatedHttpProtocol + AsyncRead + AsyncWrite + Send + Unpin + 'static,
    A::Service: Send + 'static,
    I: Send + 'static,
    S: Send + 'static,
{
    type Stream = Http1KeepAliveTimeoutStream<A::Stream>;
    type Service = Http1KeepAliveTimeoutService<A::Service>;
    type Future =
        Pin<Box<dyn Future<Output = io::Result<(Self::Stream, Self::Service)>> + Send + 'static>>;

    fn accept(&self, stream: I, service: S) -> Self::Future {
        let inner = self.inner.clone();
        let timeout = self.timeout;
        Box::pin(async move {
            let (stream, service) = inner.accept(stream, service).await?;
            let activity = stream
                .is_http1()
                .then(|| Arc::new(ConnectionActivity::default()));
            Ok((
                Http1KeepAliveTimeoutStream::new(stream, activity.clone(), timeout),
                Http1KeepAliveTimeoutService {
                    inner: service,
                    activity,
                },
            ))
        })
    }
}

#[derive(Default)]
struct ConnectionActivity {
    saw_request: AtomicBool,
    active_responses: AtomicUsize,
    write_pending_flush: AtomicBool,
    read_waker: AtomicWaker,
}

impl ConnectionActivity {
    fn begin_request(self: &Arc<Self>) -> ResponseLease {
        self.saw_request.store(true, Ordering::Release);
        self.active_responses.fetch_add(1, Ordering::AcqRel);
        self.read_waker.wake();
        ResponseLease {
            activity: Some(self.clone()),
        }
    }

    fn end_response(&self) {
        let previous = self.active_responses.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "response activity count must not underflow");
        if previous == 1 {
            self.read_waker.wake();
        }
    }

    fn mark_write_pending_flush(&self) {
        self.write_pending_flush.store(true, Ordering::Release);
    }

    fn mark_write_flushed(&self) {
        if self.write_pending_flush.swap(false, Ordering::AcqRel) {
            self.read_waker.wake();
        }
    }

    fn is_keep_alive_idle(&self) -> bool {
        self.saw_request.load(Ordering::Acquire)
            && self.active_responses.load(Ordering::Acquire) == 0
            && !self.write_pending_flush.load(Ordering::Acquire)
    }
}

struct ResponseLease {
    activity: Option<Arc<ConnectionActivity>>,
}

impl Drop for ResponseLease {
    fn drop(&mut self) {
        if let Some(activity) = self.activity.take() {
            activity.end_response();
        }
    }
}

#[derive(Clone)]
pub(crate) struct Http1KeepAliveTimeoutService<S> {
    inner: S,
    activity: Option<Arc<ConnectionActivity>>,
}

impl<S, R, B> Service<Request<R>> for Http1KeepAliveTimeoutService<S>
where
    S: Service<Request<R>, Response = Response<B>>,
    S::Future: Send + 'static,
    B: Body,
{
    type Response = Response<KeepAliveResponseBody<B>>;
    type Error = S::Error;
    type Future = Pin<
        Box<dyn Future<Output = Result<Response<KeepAliveResponseBody<B>>, Self::Error>> + Send>,
    >;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(context)
    }

    fn call(&mut self, request: Request<R>) -> Self::Future {
        let lease = self
            .activity
            .as_ref()
            .map(|activity| activity.begin_request());
        let future = self.inner.call(request);
        Box::pin(async move {
            match future.await {
                Ok(response) => Ok(response.map(|body| KeepAliveResponseBody::new(body, lease))),
                Err(error) => {
                    drop(lease);
                    Err(error)
                }
            }
        })
    }
}

pub(crate) struct KeepAliveResponseBody<B> {
    body: SyncWrapper<Pin<Box<B>>>,
    remaining_hint: SizeHint,
    ended: bool,
    _lease: Option<ResponseLease>,
}

impl<B> KeepAliveResponseBody<B>
where
    B: Body,
{
    fn new(body: B, lease: Option<ResponseLease>) -> Self {
        let remaining_hint = body.size_hint();
        let ended = body.is_end_stream();
        Self {
            body: SyncWrapper::new(Box::pin(body)),
            remaining_hint,
            ended,
            _lease: lease,
        }
    }
}

impl<B> Body for KeepAliveResponseBody<B>
where
    B: Body,
{
    type Data = B::Data;
    type Error = B::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let polled = {
            let mut body = self.body.get_mut().as_mut();
            let frame = body.as_mut().poll_frame(context);
            let ended = body.as_ref().is_end_stream();
            let remaining_hint = body.as_ref().size_hint();
            (frame, ended, remaining_hint)
        };
        if !matches!(polled.0, Poll::Pending) {
            self.ended = polled.1;
            self.remaining_hint = polled.2;
        }
        polled.0
    }

    fn is_end_stream(&self) -> bool {
        self.ended
    }

    fn size_hint(&self) -> SizeHint {
        self.remaining_hint
    }
}

pub(crate) struct Http1KeepAliveTimeoutStream<I> {
    inner: I,
    activity: Option<Arc<ConnectionActivity>>,
    timeout: Duration,
    deadline: Option<Pin<Box<Sleep>>>,
    armed: bool,
}

impl<I> Http1KeepAliveTimeoutStream<I> {
    fn new(inner: I, activity: Option<Arc<ConnectionActivity>>, timeout: Duration) -> Self {
        Self {
            inner,
            activity,
            timeout,
            deadline: None,
            armed: false,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }

    fn poll_idle_deadline(&mut self, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        let Some(activity) = &self.activity else {
            return Poll::Pending;
        };
        activity.read_waker.register(context.waker());
        if !activity.is_keep_alive_idle() {
            self.disarm();
            return Poll::Pending;
        }
        if !self.armed {
            let deadline = Instant::now() + self.timeout;
            if let Some(sleep) = &mut self.deadline {
                sleep.as_mut().reset(deadline);
            } else {
                self.deadline = Some(Box::pin(tokio::time::sleep_until(deadline)));
            }
            self.armed = true;
        }
        let deadline = self
            .deadline
            .as_mut()
            .expect("armed Keep-Alive timeout has a Timer");
        if deadline.as_mut().poll(context).is_ready() {
            self.armed = false;
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "HTTP/1 Keep-Alive idle timeout exceeded",
            )))
        } else {
            Poll::Pending
        }
    }
}

impl<I: AsyncRead + Unpin> AsyncRead for Http1KeepAliveTimeoutStream<I> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match Pin::new(&mut self.inner).poll_read(context, output) {
            Poll::Ready(result) => {
                self.disarm();
                Poll::Ready(result)
            }
            Poll::Pending => self.poll_idle_deadline(context),
        }
    }
}

impl<I: AsyncWrite + Unpin> AsyncWrite for Http1KeepAliveTimeoutStream<I> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        if let Some(activity) = &self.activity {
            activity.mark_write_pending_flush();
        }
        let result = Pin::new(&mut self.inner).poll_write(context, bytes);
        if matches!(result, Poll::Ready(Err(_))) {
            if let Some(activity) = &self.activity {
                activity.mark_write_flushed();
            }
        }
        result
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        let result = Pin::new(&mut self.inner).poll_flush(context);
        if matches!(result, Poll::Ready(_)) {
            if let Some(activity) = &self.activity {
                activity.mark_write_flushed();
            }
        }
        result
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(context)
    }
}

impl<I: NegotiatedHttpProtocol> NegotiatedHttpProtocol for Http1KeepAliveTimeoutStream<I> {
    fn is_http1(&self) -> bool {
        self.inner.is_http1()
    }
}

#[cfg(test)]
mod tests {
    use std::{io, sync::Arc, time::Duration};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::{ConnectionActivity, Http1KeepAliveTimeoutStream};

    #[tokio::test]
    async fn starts_only_after_a_response_finishes() {
        let (inner, mut peer) = tokio::io::duplex(64);
        let activity = Arc::new(ConnectionActivity::default());
        let lease = activity.begin_request();
        let mut stream =
            Http1KeepAliveTimeoutStream::new(inner, Some(activity), Duration::from_millis(100));
        let mut byte = [0_u8; 1];

        assert!(
            tokio::time::timeout(Duration::from_millis(150), stream.read_exact(&mut byte))
                .await
                .is_err()
        );
        drop(lease);
        let error = stream
            .read_exact(&mut byte)
            .await
            .expect_err("idle Keep-Alive connection must close");
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);

        peer.shutdown().await.expect("close peer");
    }

    #[tokio::test]
    async fn protocol_bypass_has_no_timer() {
        let (inner, mut peer) = tokio::io::duplex(64);
        let mut stream = Http1KeepAliveTimeoutStream::new(inner, None, Duration::from_millis(50));
        let mut byte = [0_u8; 1];
        assert!(
            tokio::time::timeout(Duration::from_millis(100), stream.read_exact(&mut byte))
                .await
                .is_err()
        );
        peer.write_all(b"x").await.expect("write bypass byte");
        stream
            .read_exact(&mut byte)
            .await
            .expect("read bypass byte");
    }
}
