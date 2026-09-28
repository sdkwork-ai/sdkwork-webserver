use std::{future::Future, io, pin::Pin, sync::Arc, time::Duration};

use axum_server::accept::Accept;
use tokio::time::timeout;

use super::metrics::{DataPlaneMetrics, ProtocolErrorKind};

/// Bounds the whole pre-serve accept phase of one accepted connection.
///
/// The chain this wraps ends in the rustls acceptor on TLS listeners and in
/// the HTTP/1+HTTP/2 wire guards on plaintext listeners; every deadline those
/// layers arm starts only once their own read begins, so a peer that trickles
/// ClientHello (or preread) bytes could otherwise hold its two admission
/// permits and one connection task forever. The deadline covers the entire
/// accept future instead — nginx `ssl_handshake_timeout` parity for TLS and
/// the same bound for the plaintext preread.
#[derive(Clone)]
pub(crate) struct HandshakeTimeoutAcceptor<A> {
    inner: A,
    timeout: Duration,
    metrics: Option<Arc<DataPlaneMetrics>>,
}

impl<A> HandshakeTimeoutAcceptor<A> {
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

impl<A, I, S> Accept<I, S> for HandshakeTimeoutAcceptor<A>
where
    A: Accept<I, S> + Clone + Send + Sync + 'static,
    A::Future: Send + 'static,
    I: Send + 'static,
    S: Send + 'static,
{
    type Stream = A::Stream;
    type Service = A::Service;
    type Future =
        Pin<Box<dyn Future<Output = io::Result<(Self::Stream, Self::Service)>> + Send + 'static>>;

    fn accept(&self, stream: I, service: S) -> Self::Future {
        let inner = self.inner.clone();
        let timeout_duration = self.timeout;
        let metrics = self.metrics.clone();
        Box::pin(async move {
            let handshake = timeout(timeout_duration, inner.accept(stream, service));
            match handshake.await {
                Ok(result) => result,
                Err(_) => {
                    if let Some(metrics) = &metrics {
                        metrics.record_protocol_error(ProtocolErrorKind::HandshakeTimeout);
                    }
                    Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "connection handshake timeout exceeded",
                    ))
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{future::Future, io, pin::Pin, time::Duration};

    use axum_server::accept::Accept;

    use super::HandshakeTimeoutAcceptor;
    use crate::{
        data_plane::metrics::{DataPlaneMetrics, ProtocolErrorKind},
        metric_dimensions::CanonicalMetricDimensions,
    };

    /// An acceptor whose future never resolves, mirroring a peer that never
    /// finishes its ClientHello.
    #[derive(Clone)]
    struct HangingAcceptor;

    impl<I: Send, S: Send> Accept<I, S> for HangingAcceptor {
        type Stream = I;
        type Service = S;
        type Future = Pin<Box<dyn Future<Output = io::Result<(I, S)>> + Send + 'static>>;

        fn accept(&self, stream: I, service: S) -> Self::Future {
            Box::pin(async {
                std::future::pending::<()>().await;
                unreachable!("pending never resolves")
            })
        }
    }

    #[derive(Clone)]
    struct ImmediateAcceptor;

    impl<I: Send + 'static, S: Send + 'static> Accept<I, S> for ImmediateAcceptor {
        type Stream = I;
        type Service = S;
        type Future = Pin<Box<dyn Future<Output = io::Result<(I, S)>> + Send + 'static>>;

        fn accept(&self, stream: I, service: S) -> Self::Future {
            Box::pin(async { Ok((stream, service)) })
        }
    }

    #[tokio::test]
    async fn times_out_a_hanging_handshake_and_records_the_error() {
        let metrics = DataPlaneMetrics::new(CanonicalMetricDimensions::default());
        let acceptor = HandshakeTimeoutAcceptor::new_observed(
            HangingAcceptor,
            Duration::from_millis(50),
            metrics.clone(),
        );
        let error = Accept::<&'static str, ()>::accept(&acceptor, "stream", ())
            .await
            .expect_err("a hanging handshake must time out");
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert_eq!(
            metrics.protocol_error_count(ProtocolErrorKind::HandshakeTimeout),
            1
        );
    }

    #[tokio::test]
    async fn passes_a_finished_handshake_through() {
        let metrics = DataPlaneMetrics::new(CanonicalMetricDimensions::default());
        let acceptor = HandshakeTimeoutAcceptor::new_observed(
            ImmediateAcceptor,
            Duration::from_secs(60),
            metrics.clone(),
        );
        let (stream, ()) = Accept::<&'static str, ()>::accept(&acceptor, "stream", ())
            .await
            .expect("a finished handshake must pass through");
        assert_eq!(stream, "stream");
        assert_eq!(
            metrics.protocol_error_count(ProtocolErrorKind::HandshakeTimeout),
            0
        );
    }
}
