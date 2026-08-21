use std::{
    collections::HashSet,
    io,
    pin::Pin,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex, MutexGuard,
    },
    task::{Context, Poll, Waker},
};

use atomic_waker::AtomicWaker;
use bytes::Bytes;
use http::{
    header::{CONNECTION, CONTENT_LENGTH, HOST, TRAILER, TRANSFER_ENCODING},
    HeaderMap, HeaderName,
};
use http_body::{Body, Frame, SizeHint};
use sync_wrapper::SyncWrapper;

const FAILURE_NONE: u8 = 0;
const FAILURE_BODY_TOO_LARGE: u8 = 1;
const FAILURE_INVALID_BODY: u8 = 2;
const FAILURE_TIMEOUT: u8 = 3;
const BODY_ACTIVE: u8 = 0;
const BODY_COMPLETED: u8 = 1;
const BODY_PAUSED: u8 = 2;
const BODY_CANCELLED: u8 = 3;

#[derive(Clone, Default)]
pub(super) struct RequestBodyFailure {
    state: Arc<AtomicU8>,
}

impl RequestBodyFailure {
    pub(super) fn body_too_large(&self) -> bool {
        self.state.load(Ordering::Relaxed) == FAILURE_BODY_TOO_LARGE
    }

    pub(super) fn invalid_body(&self) -> bool {
        self.state.load(Ordering::Relaxed) == FAILURE_INVALID_BODY
    }

    pub(super) fn timed_out(&self) -> bool {
        self.state.load(Ordering::Relaxed) == FAILURE_TIMEOUT
    }

    pub(super) fn record_timeout(&self) {
        self.record(FAILURE_TIMEOUT);
    }

    fn record(&self, failure: u8) {
        let _ = self.state.compare_exchange(
            FAILURE_NONE,
            failure,
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
    }
}

#[derive(Clone, Default)]
pub(super) struct ProxyRequestBodyControl {
    inner: Arc<ProxyRequestBodyControlInner>,
}

#[derive(Default)]
struct ProxyRequestBodyControlInner {
    state: AtomicU8,
    waker: AtomicWaker,
    deferred_body: Mutex<Option<Box<dyn Send + Sync>>>,
}

impl ProxyRequestBodyControl {
    pub(super) fn completed() -> Self {
        let control = Self::default();
        control.record_completed();
        control
    }

    pub(super) fn pause_if_incomplete(&self) -> bool {
        if self
            .inner
            .state
            .compare_exchange(
                BODY_ACTIVE,
                BODY_PAUSED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            self.inner.waker.wake();
            true
        } else {
            false
        }
    }

    pub(super) fn cancel_if_incomplete(&self) -> bool {
        loop {
            let current = self.inner.state.load(Ordering::Acquire);
            if !matches!(current, BODY_ACTIVE | BODY_PAUSED) {
                return false;
            }
            if self
                .inner
                .state
                .compare_exchange(current, BODY_CANCELLED, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                self.inner.waker.wake();
                self.drop_deferred_body();
                return true;
            }
        }
    }

    fn cancelled(&self) -> bool {
        self.inner.state.load(Ordering::Acquire) == BODY_CANCELLED
    }

    fn paused(&self) -> bool {
        self.inner.state.load(Ordering::Acquire) == BODY_PAUSED
    }

    fn record_completed(&self) {
        loop {
            let current = self.inner.state.load(Ordering::Acquire);
            if !matches!(current, BODY_ACTIVE | BODY_PAUSED) {
                return;
            }
            if self
                .inner
                .state
                .compare_exchange(current, BODY_COMPLETED, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return;
            }
        }
    }

    fn register(&self, waker: &Waker) {
        self.inner.waker.register(waker);
    }

    fn defer_body_drop<T>(&self, body: T)
    where
        T: Send + Sync + 'static,
    {
        let mut body = Some(Box::new(body) as Box<dyn Send + Sync>);
        {
            let mut deferred = self.deferred_body();
            if matches!(
                self.inner.state.load(Ordering::Acquire),
                BODY_ACTIVE | BODY_PAUSED
            ) {
                debug_assert!(deferred.is_none(), "one request has one deferred Body");
                if deferred.is_none() {
                    *deferred = body.take();
                }
            }
        }
        drop(body);
    }

    fn drop_deferred_body(&self) {
        let deferred = self.deferred_body().take();
        drop(deferred);
    }

    fn deferred_body(&self) -> MutexGuard<'_, Option<Box<dyn Send + Sync>>> {
        self.inner
            .deferred_body
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub(super) struct ProxyTrailerPolicy {
    maximum_bytes: usize,
    maximum_count: usize,
    declared: HashSet<HeaderName>,
    forbidden: HashSet<HeaderName>,
}

impl ProxyTrailerPolicy {
    pub(super) fn new(
        maximum_bytes: usize,
        maximum_count: usize,
        declared: HashSet<HeaderName>,
        forbidden: HashSet<HeaderName>,
    ) -> Self {
        Self {
            maximum_bytes,
            maximum_count,
            declared,
            forbidden,
        }
    }
}

pub(super) struct GuardedProxyBody<B>
where
    B: Body + Send + 'static,
{
    inner: Option<SyncWrapper<Pin<Box<B>>>>,
    remaining_hint: SizeHint,
    ended: bool,
    maximum_body_bytes: Option<u64>,
    observed_body_bytes: u64,
    trailer_policy: ProxyTrailerPolicy,
    request_failure: Option<RequestBodyFailure>,
    request_control: Option<ProxyRequestBodyControl>,
    cancel_on_response_end: Option<ProxyRequestBodyControl>,
}

impl<B> GuardedProxyBody<B>
where
    B: Body + Send + 'static,
{
    pub(super) fn request(
        inner: B,
        maximum_body_bytes: u64,
        trailer_policy: ProxyTrailerPolicy,
        failure: RequestBodyFailure,
        control: ProxyRequestBodyControl,
    ) -> Self {
        Self::new(
            inner,
            Some(maximum_body_bytes),
            trailer_policy,
            Some(failure),
            Some(control),
            None,
        )
    }

    pub(super) fn response(
        inner: B,
        trailer_policy: ProxyTrailerPolicy,
        maximum_body_bytes: Option<u64>,
    ) -> Self {
        Self::new(inner, maximum_body_bytes, trailer_policy, None, None, None)
    }

    pub(super) fn response_with_request_cancellation(
        inner: B,
        trailer_policy: ProxyTrailerPolicy,
        request_control: ProxyRequestBodyControl,
        maximum_body_bytes: Option<u64>,
    ) -> Self {
        Self::new(
            inner,
            maximum_body_bytes,
            trailer_policy,
            None,
            None,
            Some(request_control),
        )
    }

    fn new(
        inner: B,
        maximum_body_bytes: Option<u64>,
        trailer_policy: ProxyTrailerPolicy,
        request_failure: Option<RequestBodyFailure>,
        request_control: Option<ProxyRequestBodyControl>,
        cancel_on_response_end: Option<ProxyRequestBodyControl>,
    ) -> Self {
        let remaining_hint = inner.size_hint();
        let ended = inner.is_end_stream();
        if ended {
            if let Some(control) = &request_control {
                control.record_completed();
            }
        }
        Self {
            inner: Some(SyncWrapper::new(Box::pin(inner))),
            remaining_hint,
            ended,
            maximum_body_bytes,
            observed_body_bytes: 0,
            trailer_policy,
            request_failure,
            request_control,
            cancel_on_response_end,
        }
    }

    fn validate_trailers(&self, trailers: &HeaderMap) -> io::Result<()> {
        let mut count = 0_usize;
        let mut bytes = 0_usize;
        for (name, value) in trailers {
            count = count.saturating_add(1);
            bytes = bytes
                .saturating_add(name.as_str().len())
                .saturating_add(value.as_bytes().len())
                .saturating_add(4);
            if is_forbidden_trailer(name)
                || self.trailer_policy.forbidden.contains(name)
                || !self.trailer_policy.declared.contains(name)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "forbidden proxy trailer field",
                ));
            }
        }
        if count > self.trailer_policy.maximum_count || bytes > self.trailer_policy.maximum_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "proxy trailers exceed the configured limit",
            ));
        }
        Ok(())
    }

    fn record_invalid_body(&self) {
        if let Some(failure) = &self.request_failure {
            failure.record(FAILURE_INVALID_BODY);
        }
    }

    fn cancellation(&mut self) -> Poll<Option<Result<Frame<Bytes>, io::Error>>> {
        self.ended = true;
        self.remaining_hint = SizeHint::with_exact(0);
        Poll::Ready(Some(Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            "upstream accepted an early response",
        ))))
    }
}

impl<B> Drop for GuardedProxyBody<B>
where
    B: Body + Send + 'static,
{
    fn drop(&mut self) {
        if let (Some(control), Some(inner)) = (&self.request_control, self.inner.take()) {
            control.defer_body_drop(inner);
        }
        if let Some(control) = &self.cancel_on_response_end {
            control.cancel_if_incomplete();
        }
    }
}

impl<B> Body for GuardedProxyBody<B>
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
        if let Some(control) = &self.request_control {
            if control.cancelled() {
                return self.cancellation();
            }
            control.register(context.waker());
            if control.cancelled() {
                return self.cancellation();
            }
            if control.paused() {
                return Poll::Pending;
            }
        }
        let (polled, inner_ended) = {
            let mut inner = self
                .inner
                .as_mut()
                .expect("active guarded Body retains its inner Body")
                .get_mut()
                .as_mut();
            let polled = inner.as_mut().poll_frame(context);
            let ended = inner.as_ref().is_end_stream();
            (polled, ended)
        };
        let Poll::Ready(frame) = polled else {
            return Poll::Pending;
        };
        let Some(frame) = frame else {
            self.ended = true;
            self.remaining_hint = SizeHint::with_exact(0);
            if let Some(control) = &self.request_control {
                control.record_completed();
            }
            return Poll::Ready(None);
        };
        let frame = match frame {
            Ok(frame) => frame,
            Err(error) => {
                self.record_invalid_body();
                return Poll::Ready(Some(Err(io::Error::other(error.into()))));
            }
        };

        let frame = match frame.into_data() {
            Ok(data) => {
                self.remaining_hint = subtract_size_hint(&self.remaining_hint, data.len() as u64);
                self.observed_body_bytes =
                    self.observed_body_bytes.saturating_add(data.len() as u64);
                if self
                    .maximum_body_bytes
                    .is_some_and(|maximum| self.observed_body_bytes > maximum)
                {
                    if let Some(failure) = &self.request_failure {
                        failure.record(FAILURE_BODY_TOO_LARGE);
                    }
                    return Poll::Ready(Some(Err(io::Error::other("request body limit exceeded"))));
                }
                Frame::data(data)
            }
            Err(frame) => match frame.into_trailers() {
                Ok(trailers) => {
                    if let Err(error) = self.validate_trailers(&trailers) {
                        self.record_invalid_body();
                        return Poll::Ready(Some(Err(error)));
                    }
                    Frame::trailers(trailers)
                }
                Err(_) => {
                    self.record_invalid_body();
                    return Poll::Ready(Some(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "unsupported HTTP body frame",
                    ))));
                }
            },
        };
        self.ended = inner_ended;
        if inner_ended {
            if let Some(control) = &self.request_control {
                control.record_completed();
            }
        }
        Poll::Ready(Some(Ok(frame)))
    }

    fn is_end_stream(&self) -> bool {
        self.ended
    }

    fn size_hint(&self) -> SizeHint {
        self.remaining_hint
    }
}

fn subtract_size_hint(hint: &SizeHint, length: u64) -> SizeHint {
    let mut remaining = SizeHint::new();
    remaining.set_lower(hint.lower().saturating_sub(length));
    if let Some(upper) = hint.upper() {
        remaining.set_upper(upper.saturating_sub(length));
    }
    remaining
}

pub(super) fn validate_trailer_declaration(
    headers: &HeaderMap,
    maximum_bytes: usize,
    maximum_count: usize,
    forbidden_trailers: &HashSet<HeaderName>,
) -> Result<HashSet<HeaderName>, ()> {
    if headers.contains_key(TRAILER) && forbidden_trailers.contains(&TRAILER) {
        return Err(());
    }
    let values = headers.get_all(TRAILER);
    let mut count = 0_usize;
    let mut bytes = 0_usize;
    let mut names = HashSet::new();
    let mut saw_value = false;
    for value in values {
        saw_value = true;
        bytes = bytes.saturating_add(value.as_bytes().len());
        let Ok(value) = value.to_str() else {
            return Err(());
        };
        for token in value.split(',').map(str::trim) {
            let Ok(name) = HeaderName::from_bytes(token.as_bytes()) else {
                return Err(());
            };
            count = count.saturating_add(1);
            if token.is_empty()
                || is_forbidden_trailer(&name)
                || forbidden_trailers.contains(&name)
                || !names.insert(name)
            {
                return Err(());
            }
        }
    }
    if saw_value && (count == 0 || count > maximum_count || bytes > maximum_bytes) {
        return Err(());
    }
    Ok(names)
}

fn is_forbidden_trailer(name: &HeaderName) -> bool {
    matches!(
        name,
        &CONTENT_LENGTH | &TRANSFER_ENCODING | &HOST | &CONNECTION | &TRAILER
    )
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        pin::Pin,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        task::{Context, Poll},
    };

    use bytes::Bytes;
    use http::{header::TRAILER, HeaderMap};
    use http_body::{Body as HttpBody, Frame, SizeHint};
    use http_body_util::{channel::Channel, BodyExt};

    use super::{
        validate_trailer_declaration, GuardedProxyBody, ProxyRequestBodyControl,
        ProxyTrailerPolicy, RequestBodyFailure,
    };

    struct DropObservedBody {
        dropped: Arc<AtomicBool>,
    }

    impl HttpBody for DropObservedBody {
        type Data = Bytes;
        type Error = std::io::Error;

        fn poll_frame(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            Poll::Pending
        }

        fn is_end_stream(&self) -> bool {
            false
        }

        fn size_hint(&self) -> SizeHint {
            SizeHint::new()
        }
    }

    impl Drop for DropObservedBody {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::Release);
        }
    }

    #[tokio::test]
    async fn rejects_over_budget_actual_trailer_frames() {
        let (mut sender, body) = Channel::<Bytes>::new(1);
        let mut trailers = HeaderMap::new();
        trailers.insert("x-one", "1".parse().expect("first trailer"));
        trailers.insert("x-two", "2".parse().expect("second trailer"));
        sender
            .try_send(Frame::trailers(trailers))
            .expect("queue trailers");
        drop(sender);

        let declared = HashSet::from([
            "x-one".parse().expect("first declared trailer"),
            "x-two".parse().expect("second declared trailer"),
        ]);
        let mut guarded = GuardedProxyBody::response(
            body,
            ProxyTrailerPolicy::new(64, 1, declared, HashSet::new()),
            None,
        );
        let error = guarded
            .frame()
            .await
            .expect("receive guarded frame")
            .expect_err("too many trailers must fail");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn records_request_body_limit_failures_without_collecting() {
        let (mut sender, body) = Channel::<Bytes>::new(1);
        sender
            .try_send(Frame::data(Bytes::from_static(b"large")))
            .expect("queue data");
        drop(sender);
        let failure = RequestBodyFailure::default();
        let mut guarded = GuardedProxyBody::request(
            body,
            4,
            ProxyTrailerPolicy::new(64, 1, HashSet::new(), HashSet::new()),
            failure.clone(),
            ProxyRequestBodyControl::default(),
        );

        guarded
            .frame()
            .await
            .expect("receive guarded frame")
            .expect_err("oversized body must fail");
        assert!(failure.body_too_large());
    }

    #[tokio::test]
    async fn cancellation_wakes_a_pending_proxy_request_body() {
        let (_sender, body) = Channel::<Bytes>::new(1);
        let control = ProxyRequestBodyControl::default();
        let mut guarded = GuardedProxyBody::request(
            body,
            64,
            ProxyTrailerPolicy::new(64, 1, HashSet::new(), HashSet::new()),
            RequestBodyFailure::default(),
            control.clone(),
        );

        let waiter = tokio::spawn(async move { guarded.frame().await });
        tokio::task::yield_now().await;
        assert!(control.cancel_if_incomplete());
        let error = tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .expect("cancellation wakes the Body poll")
            .expect("Body task joins")
            .expect("cancellation returns a frame result")
            .expect_err("cancellation terminates the request Body");
        assert_eq!(error.kind(), std::io::ErrorKind::ConnectionAborted);
    }

    #[tokio::test]
    async fn early_response_pauses_input_until_response_completion_cancels_it() {
        let (_sender, body) = Channel::<Bytes>::new(1);
        let control = ProxyRequestBodyControl::default();
        let mut guarded = GuardedProxyBody::request(
            body,
            64,
            ProxyTrailerPolicy::new(64, 1, HashSet::new(), HashSet::new()),
            RequestBodyFailure::default(),
            control.clone(),
        );
        let waiter = tokio::spawn(async move { guarded.frame().await });
        tokio::task::yield_now().await;

        assert!(control.pause_if_incomplete());
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert!(!waiter.is_finished(), "pause does not fabricate Body EOF");
        assert!(control.cancel_if_incomplete());

        let error = tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .expect("deferred cancellation wakes the Body poll")
            .expect("Body task joins")
            .expect("deferred cancellation returns a frame result")
            .expect_err("deferred cancellation terminates the request Body");
        assert_eq!(error.kind(), std::io::ErrorKind::ConnectionAborted);
    }

    #[test]
    fn retains_an_upstream_dropped_body_until_response_cancellation() {
        let dropped = Arc::new(AtomicBool::new(false));
        let control = ProxyRequestBodyControl::default();
        let guarded = GuardedProxyBody::request(
            DropObservedBody {
                dropped: dropped.clone(),
            },
            64,
            ProxyTrailerPolicy::new(64, 1, HashSet::new(), HashSet::new()),
            RequestBodyFailure::default(),
            control.clone(),
        );

        drop(guarded);
        assert!(
            !dropped.load(Ordering::Acquire),
            "an incomplete Body survives the upstream request driver's early drop"
        );
        assert!(control.cancel_if_incomplete());
        assert!(
            dropped.load(Ordering::Acquire),
            "response completion releases the deferred client Body"
        );
    }

    #[tokio::test]
    async fn completed_proxy_request_body_cannot_be_cancelled() {
        let control = ProxyRequestBodyControl::default();
        let mut guarded = GuardedProxyBody::request(
            http_body_util::Empty::<Bytes>::new(),
            64,
            ProxyTrailerPolicy::new(64, 1, HashSet::new(), HashSet::new()),
            RequestBodyFailure::default(),
            control.clone(),
        );

        assert!(guarded.frame().await.is_none());
        assert!(!control.cancel_if_incomplete());
    }

    #[test]
    fn rejects_forbidden_and_duplicate_trailer_declarations() {
        let mut forbidden = HeaderMap::new();
        forbidden.insert(TRAILER, "Content-Length".parse().expect("declaration"));
        assert!(validate_trailer_declaration(&forbidden, 64, 2, &HashSet::new()).is_err());

        let mut duplicate = HeaderMap::new();
        duplicate.insert(TRAILER, "X-One, x-one".parse().expect("declaration"));
        assert!(validate_trailer_declaration(&duplicate, 64, 2, &HashSet::new()).is_err());
    }
}
