use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use axum::{body::Body, http::Response};
use bytes::Bytes;
use http_body::{Body as HttpBody, Frame, SizeHint};
use sync_wrapper::SyncWrapper;
use tokio::time::{Instant, Sleep};

use super::request_gate::RequestAdmissionPermit;

pub(super) fn hold_request_permit(
    response: Response<Body>,
    permit: RequestAdmissionPermit,
    idle_timeout: Duration,
) -> Response<Body> {
    response.map(|body| Body::new(RequestPermitBody::new(body, permit, idle_timeout)))
}

struct RequestPermitBody {
    inner: SyncWrapper<Pin<Box<Body>>>,
    permit: Option<RequestAdmissionPermit>,
    remaining_hint: SizeHint,
    ended: bool,
    idle_timeout: Duration,
    idle_deadline: Pin<Box<Sleep>>,
}

impl RequestPermitBody {
    fn new(inner: Body, permit: RequestAdmissionPermit, idle_timeout: Duration) -> Self {
        let remaining_hint = inner.size_hint();
        let ended = inner.is_end_stream();
        Self {
            inner: SyncWrapper::new(Box::pin(inner)),
            permit: Some(permit),
            remaining_hint,
            ended,
            idle_timeout,
            idle_deadline: Box::pin(tokio::time::sleep(idle_timeout)),
        }
    }
}

impl HttpBody for RequestPermitBody {
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let polled = {
            let mut inner = self.inner.get_mut().as_mut();
            let frame = inner.as_mut().poll_frame(context);
            let ended = inner.as_ref().is_end_stream();
            let remaining_hint = inner.as_ref().size_hint();
            (frame, ended, remaining_hint)
        };
        match polled {
            (Poll::Pending, _, _) => {
                if self.idle_deadline.as_mut().poll(context).is_ready() {
                    self.ended = true;
                    self.remaining_hint = SizeHint::with_exact(0);
                    self.permit.take();
                    Poll::Ready(Some(Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "response Body idle timeout exceeded",
                    ))))
                } else {
                    Poll::Pending
                }
            }
            (Poll::Ready(None), _, remaining_hint) => {
                self.ended = true;
                self.remaining_hint = remaining_hint;
                self.permit.take();
                Poll::Ready(None)
            }
            (Poll::Ready(Some(Err(error))), _, remaining_hint) => {
                self.ended = true;
                self.remaining_hint = remaining_hint;
                self.permit.take();
                Poll::Ready(Some(Err(io::Error::other(error))))
            }
            (Poll::Ready(Some(Ok(frame))), ended, remaining_hint) => {
                let made_progress = frame.data_ref().is_some_and(|data| !data.is_empty())
                    || frame.trailers_ref().is_some();
                if !made_progress && Instant::now() >= self.idle_deadline.deadline() {
                    self.ended = true;
                    self.remaining_hint = SizeHint::with_exact(0);
                    self.permit.take();
                    return Poll::Ready(Some(Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "response Body idle timeout exceeded",
                    ))));
                }
                self.ended = ended;
                self.remaining_hint = remaining_hint;
                if made_progress {
                    let next_deadline = Instant::now() + self.idle_timeout;
                    self.idle_deadline.as_mut().reset(next_deadline);
                }
                if ended {
                    self.permit.take();
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
    use std::{sync::Arc, time::Duration};

    use bytes::Bytes;
    use http_body::Frame;
    use http_body_util::{channel::Channel, BodyExt};
    use tokio::sync::Semaphore;

    use super::RequestPermitBody;
    use crate::data_plane::request_gate::RequestAdmissionPermit;

    #[tokio::test]
    async fn holds_permit_until_response_body_end() {
        let permits = Arc::new(Semaphore::new(1));
        let permit = permits
            .clone()
            .try_acquire_owned()
            .expect("acquire request permit");
        let (mut sender, body) = Channel::<Bytes>::new(1);
        sender
            .try_send(Frame::data(Bytes::from_static(b"one")))
            .expect("queue response frame");
        drop(sender);
        let mut body = RequestPermitBody::new(
            axum::body::Body::new(body),
            RequestAdmissionPermit::single(permit),
            Duration::from_secs(1),
        );

        body.frame()
            .await
            .expect("receive response frame")
            .expect("valid response frame");
        assert_eq!(permits.available_permits(), 0);
        assert!(body.frame().await.is_none());
        assert_eq!(permits.available_permits(), 1);
    }

    #[tokio::test]
    async fn releases_permit_when_response_body_is_cancelled() {
        let permits = Arc::new(Semaphore::new(1));
        let permit = permits
            .clone()
            .try_acquire_owned()
            .expect("acquire request permit");
        let (_sender, body) = Channel::<Bytes>::new(1);
        let body = RequestPermitBody::new(
            axum::body::Body::new(body),
            RequestAdmissionPermit::single(permit),
            Duration::from_secs(1),
        );
        assert_eq!(permits.available_permits(), 0);

        drop(body);
        assert_eq!(permits.available_permits(), 1);
    }

    #[tokio::test]
    async fn response_body_idle_timeout_releases_permit() {
        let permits = Arc::new(Semaphore::new(1));
        let permit = permits
            .clone()
            .try_acquire_owned()
            .expect("acquire request permit");
        let (_sender, body) = Channel::<Bytes>::new(1);
        let mut body = RequestPermitBody::new(
            axum::body::Body::new(body),
            RequestAdmissionPermit::single(permit),
            Duration::from_millis(100),
        );

        let error = body
            .frame()
            .await
            .expect("receive timeout frame")
            .expect_err("idle response Body must time out");
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert_eq!(permits.available_permits(), 1);
    }

    #[tokio::test]
    async fn empty_data_frame_does_not_reset_elapsed_idle_deadline() {
        let permits = Arc::new(Semaphore::new(1));
        let permit = permits
            .clone()
            .try_acquire_owned()
            .expect("acquire request permit");
        let (mut sender, body) = Channel::<Bytes>::new(1);
        sender
            .try_send(Frame::data(Bytes::new()))
            .expect("queue empty response Frame");
        let mut body = RequestPermitBody::new(
            axum::body::Body::new(body),
            RequestAdmissionPermit::single(permit),
            Duration::from_millis(100),
        );
        tokio::time::sleep(Duration::from_millis(150)).await;

        let error = body
            .frame()
            .await
            .expect("receive timeout frame")
            .expect_err("empty Data must not reset idle deadline");
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert_eq!(permits.available_permits(), 1);
    }
}
