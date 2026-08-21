use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use axum::{body::Body, http::Response};
use bytes::Bytes;
use http_body::{Body as HttpBody, Frame, SizeHint};
use sync_wrapper::SyncWrapper;
use tokio::sync::OwnedSemaphorePermit;

pub(super) fn hold_upstream_permit<L>(
    response: Response<Body>,
    permit: OwnedSemaphorePermit,
    lifetime: L,
) -> Response<Body>
where
    L: Send + Sync + Unpin + 'static,
{
    response.map(|body| Body::new(UpstreamPermitBody::new(body, permit, lifetime)))
}

struct UpstreamPermitBody<L> {
    inner: SyncWrapper<Pin<Box<Body>>>,
    permit: Option<OwnedSemaphorePermit>,
    lifetime: Option<L>,
    remaining_hint: SizeHint,
    ended: bool,
}

impl<L> UpstreamPermitBody<L> {
    fn new(inner: Body, permit: OwnedSemaphorePermit, lifetime: L) -> Self {
        let remaining_hint = inner.size_hint();
        let ended = inner.is_end_stream();
        Self {
            inner: SyncWrapper::new(Box::pin(inner)),
            permit: Some(permit),
            lifetime: Some(lifetime),
            remaining_hint,
            ended,
        }
    }
}

impl<L> HttpBody for UpstreamPermitBody<L>
where
    L: Send + Sync + Unpin + 'static,
{
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let (frame, ended, remaining_hint) = {
            let mut inner = self.inner.get_mut().as_mut();
            let frame = inner.as_mut().poll_frame(context);
            let ended = inner.as_ref().is_end_stream();
            let remaining_hint = inner.as_ref().size_hint();
            (frame, ended, remaining_hint)
        };
        self.remaining_hint = remaining_hint;
        match frame {
            Poll::Ready(None) => {
                self.ended = true;
                self.permit.take();
                self.lifetime.take();
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(error))) => {
                self.ended = true;
                self.permit.take();
                self.lifetime.take();
                Poll::Ready(Some(Err(io::Error::other(error))))
            }
            Poll::Ready(Some(Ok(frame))) => {
                self.ended = ended;
                if ended {
                    self.permit.take();
                    self.lifetime.take();
                }
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Pending => Poll::Pending,
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
    use std::sync::Arc;

    use bytes::Bytes;
    use http_body::Frame;
    use http_body_util::{channel::Channel, BodyExt};
    use tokio::sync::Semaphore;

    use super::UpstreamPermitBody;

    #[tokio::test]
    async fn permit_is_held_through_stream_completion_and_drop() {
        let permits = Arc::new(Semaphore::new(1));
        let permit = permits
            .clone()
            .try_acquire_owned()
            .expect("acquire upstream permit");
        let (mut sender, body) = Channel::<Bytes>::new(1);
        sender
            .try_send(Frame::data(Bytes::from_static(b"chunk")))
            .expect("queue upstream response frame");
        let lifetime = Arc::new(());
        let mut body =
            UpstreamPermitBody::new(axum::body::Body::new(body), permit, lifetime.clone());
        assert_eq!(Arc::strong_count(&lifetime), 2);

        body.frame()
            .await
            .expect("receive response frame")
            .expect("valid response frame");
        assert_eq!(permits.available_permits(), 0);
        drop(sender);
        assert!(body.frame().await.is_none());
        assert_eq!(permits.available_permits(), 1);
        assert_eq!(Arc::strong_count(&lifetime), 1);

        let permit = permits
            .clone()
            .try_acquire_owned()
            .expect("reacquire upstream permit");
        let (_sender, body) = Channel::<Bytes>::new(1);
        let body = UpstreamPermitBody::new(axum::body::Body::new(body), permit, lifetime.clone());
        assert_eq!(permits.available_permits(), 0);
        assert_eq!(Arc::strong_count(&lifetime), 2);
        drop(body);
        assert_eq!(permits.available_permits(), 1);
        assert_eq!(Arc::strong_count(&lifetime), 1);
    }
}
