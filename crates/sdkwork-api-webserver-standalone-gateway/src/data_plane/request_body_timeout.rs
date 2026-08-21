use std::{
    future::Future,
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use bytes::Bytes;
use http_body::{Body, Frame, SizeHint};
use sync_wrapper::SyncWrapper;
use tokio::time::{Instant, Sleep};

use super::{
    metrics::{DataPlaneMetrics, ProtocolErrorKind},
    proxy_body::RequestBodyFailure,
};

pub(super) struct RequestBodyTimeout<B> {
    inner: SyncWrapper<Pin<Box<B>>>,
    remaining_hint: SizeHint,
    ended: bool,
    idle_timeout: Duration,
    deadline: Option<Pin<Box<Sleep>>>,
    failure: RequestBodyFailure,
    metrics: Option<Arc<DataPlaneMetrics>>,
}

impl<B> RequestBodyTimeout<B>
where
    B: Body,
{
    #[cfg(test)]
    pub(super) fn new(
        inner: B,
        start_timeout: Duration,
        idle_timeout: Duration,
        failure: RequestBodyFailure,
    ) -> Self {
        Self::build(inner, start_timeout, idle_timeout, failure, None)
    }

    pub(super) fn new_observed(
        inner: B,
        start_timeout: Duration,
        idle_timeout: Duration,
        failure: RequestBodyFailure,
        metrics: Arc<DataPlaneMetrics>,
    ) -> Self {
        Self::build(inner, start_timeout, idle_timeout, failure, Some(metrics))
    }

    fn build(
        inner: B,
        start_timeout: Duration,
        idle_timeout: Duration,
        failure: RequestBodyFailure,
        metrics: Option<Arc<DataPlaneMetrics>>,
    ) -> Self {
        let remaining_hint = inner.size_hint();
        let ended = inner.is_end_stream();
        let deadline = (!ended).then(|| Box::pin(tokio::time::sleep(start_timeout)));
        Self {
            inner: SyncWrapper::new(Box::pin(inner)),
            remaining_hint,
            ended,
            idle_timeout,
            deadline,
            failure,
            metrics,
        }
    }

    fn timeout(&mut self) -> Poll<Option<Result<Frame<Bytes>, io::Error>>> {
        self.ended = true;
        self.remaining_hint = SizeHint::with_exact(0);
        self.deadline = None;
        self.failure.record_timeout();
        if let Some(metrics) = &self.metrics {
            metrics.record_protocol_error(ProtocolErrorKind::RequestBodyTimeout);
        }
        Poll::Ready(Some(Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "request Body progress timeout exceeded",
        ))))
    }
}

impl<B> Body for RequestBodyTimeout<B>
where
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        if self.ended {
            return Poll::Ready(None);
        }

        let polled = {
            let mut inner = self.inner.get_mut().as_mut();
            let frame = inner.as_mut().poll_frame(context);
            let ended = inner.as_ref().is_end_stream();
            let remaining_hint = inner.as_ref().size_hint();
            (frame, ended, remaining_hint)
        };
        match polled {
            (Poll::Pending, _, _) => {
                let deadline = self
                    .deadline
                    .as_mut()
                    .expect("active request Body has a progress deadline");
                if deadline.as_mut().poll(context).is_ready() {
                    self.timeout()
                } else {
                    Poll::Pending
                }
            }
            (Poll::Ready(None), _, remaining_hint) => {
                self.ended = true;
                self.remaining_hint = remaining_hint;
                self.deadline = None;
                Poll::Ready(None)
            }
            (Poll::Ready(Some(Err(error))), _, remaining_hint) => {
                self.ended = true;
                self.remaining_hint = remaining_hint;
                self.deadline = None;
                if let Some(metrics) = &self.metrics {
                    metrics.record_protocol_error(ProtocolErrorKind::RequestBodyIo);
                }
                Poll::Ready(Some(Err(io::Error::other(error.into()))))
            }
            (Poll::Ready(Some(Ok(frame))), ended, remaining_hint) => {
                if let Some(data) = frame.data_ref() {
                    if let Some(metrics) = &self.metrics {
                        metrics.record_request_body_bytes(data.len());
                    }
                }
                let made_progress = frame.data_ref().is_some_and(|data| !data.is_empty())
                    || frame.trailers_ref().is_some();
                let deadline_elapsed = self
                    .deadline
                    .as_ref()
                    .is_some_and(|deadline| Instant::now() >= deadline.deadline());
                if !made_progress && deadline_elapsed {
                    return self.timeout();
                }

                self.ended = ended;
                self.remaining_hint = remaining_hint;
                if ended {
                    self.deadline = None;
                } else if made_progress {
                    let next_deadline = Instant::now() + self.idle_timeout;
                    self.deadline
                        .as_mut()
                        .expect("active request Body has a progress deadline")
                        .as_mut()
                        .reset(next_deadline);
                }
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
    use std::{io, time::Duration};

    use bytes::Bytes;
    use http_body::Frame;
    use http_body_util::{channel::Channel, BodyExt, Empty};

    use super::RequestBodyTimeout;
    use crate::{
        data_plane::{
            metrics::{DataPlaneMetrics, ProtocolErrorKind},
            proxy_body::RequestBodyFailure,
        },
        metric_dimensions::CanonicalMetricDimensions,
    };

    #[tokio::test]
    async fn times_out_before_first_meaningful_frame() {
        let (_sender, body) = Channel::<Bytes>::new(1);
        let failure = RequestBodyFailure::default();
        let mut body = RequestBodyTimeout::new(
            body,
            Duration::from_millis(100),
            Duration::from_secs(1),
            failure.clone(),
        );

        let error = body
            .frame()
            .await
            .expect("receive timeout frame")
            .expect_err("missing first request Body frame must time out");
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(failure.timed_out());
    }

    #[tokio::test]
    async fn first_data_switches_to_idle_deadline_and_progress_resets_it() {
        let (mut sender, body) = Channel::<Bytes>::new(2);
        sender
            .try_send(Frame::data(Bytes::from_static(b"a")))
            .expect("queue first request frame");
        let failure = RequestBodyFailure::default();
        let mut body = RequestBodyTimeout::new(
            body,
            Duration::from_millis(500),
            Duration::from_millis(150),
            failure.clone(),
        );

        body.frame()
            .await
            .expect("receive first frame")
            .expect("first frame is valid");
        tokio::time::sleep(Duration::from_millis(100)).await;
        sender
            .try_send(Frame::data(Bytes::from_static(b"b")))
            .expect("queue second request frame");
        body.frame()
            .await
            .expect("receive second frame")
            .expect("second frame is valid");
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(sender);
        assert!(body.frame().await.is_none());
        assert!(!failure.timed_out());
    }

    #[tokio::test]
    async fn times_out_after_later_progress_gap() {
        let (mut sender, body) = Channel::<Bytes>::new(1);
        sender
            .try_send(Frame::data(Bytes::from_static(b"a")))
            .expect("queue first request frame");
        let failure = RequestBodyFailure::default();
        let mut body = RequestBodyTimeout::new(
            body,
            Duration::from_secs(1),
            Duration::from_millis(100),
            failure.clone(),
        );
        body.frame()
            .await
            .expect("receive first frame")
            .expect("first frame is valid");

        let error = body
            .frame()
            .await
            .expect("receive timeout frame")
            .expect_err("request Body idle gap must time out");
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(failure.timed_out());
    }

    #[tokio::test]
    async fn empty_data_does_not_start_progress_or_reset_elapsed_deadline() {
        let (mut sender, body) = Channel::<Bytes>::new(1);
        sender
            .try_send(Frame::data(Bytes::new()))
            .expect("queue empty request frame");
        let failure = RequestBodyFailure::default();
        let mut body = RequestBodyTimeout::new(
            body,
            Duration::from_millis(100),
            Duration::from_secs(1),
            failure.clone(),
        );
        tokio::time::sleep(Duration::from_millis(150)).await;

        let error = body
            .frame()
            .await
            .expect("receive timeout frame")
            .expect_err("empty Data must not reset the start deadline");
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(failure.timed_out());
    }

    #[test]
    fn ended_body_does_not_allocate_a_deadline() {
        let body = RequestBodyTimeout::new(
            Empty::<Bytes>::new(),
            Duration::from_secs(1),
            Duration::from_secs(1),
            RequestBodyFailure::default(),
        );
        assert!(body.ended);
        assert!(body.deadline.is_none());
    }

    #[tokio::test]
    async fn observed_body_counts_frames_and_fixed_terminal_error_kinds() {
        let metrics = DataPlaneMetrics::new(CanonicalMetricDimensions::default());
        let (mut sender, channel) = Channel::<Bytes, io::Error>::new(2);
        sender
            .try_send(Frame::data(Bytes::from_static(b"ab")))
            .expect("queue first request frame");
        sender
            .try_send(Frame::data(Bytes::from_static(b"c")))
            .expect("queue second request frame");
        drop(sender);
        let mut body = RequestBodyTimeout::new_observed(
            channel,
            Duration::from_secs(1),
            Duration::from_secs(1),
            RequestBodyFailure::default(),
            metrics.clone(),
        );
        while let Some(frame) = body.frame().await {
            frame.expect("valid request frame");
        }
        assert_eq!(metrics.request_body_bytes(), 3);

        let (sender, channel) = Channel::<Bytes, io::Error>::new(1);
        sender.abort(io::Error::new(
            io::ErrorKind::ConnectionReset,
            "test request reset",
        ));
        let mut body = RequestBodyTimeout::new_observed(
            channel,
            Duration::from_secs(1),
            Duration::from_secs(1),
            RequestBodyFailure::default(),
            metrics.clone(),
        );
        body.frame()
            .await
            .expect("request error frame")
            .expect_err("request body must fail");
        assert_eq!(
            metrics.protocol_error_count(ProtocolErrorKind::RequestBodyIo),
            1
        );

        let (_sender, channel) = Channel::<Bytes, io::Error>::new(1);
        let mut body = RequestBodyTimeout::new_observed(
            channel,
            Duration::from_millis(10),
            Duration::from_secs(1),
            RequestBodyFailure::default(),
            metrics.clone(),
        );
        body.frame()
            .await
            .expect("request timeout frame")
            .expect_err("request body must time out");
        assert_eq!(
            metrics.protocol_error_count(ProtocolErrorKind::RequestBodyTimeout),
            1
        );
    }
}
