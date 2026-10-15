//! Per-route bandwidth ceilings (FRP `transport.bandwidthLimit` parity).
//!
//! One token bucket per **route**, shared by every stream that route relays,
//! so the ceiling bounds the service rather than each connection: a route
//! limited to 1 MiB/s stays at 1 MiB/s whether it serves one visitor or fifty.
//!
//! The bucket is a leaky-bucket with a one-second (capped) burst, which is how
//! link shaping behaves in practice: a short request or a burst of small
//! frames is not stretched, while sustained throughput cannot exceed the
//! configured rate.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::time::Sleep;

/// Largest burst any route may accumulate, in bytes.
///
/// One second of a very fast limit would otherwise let a route dump a
/// multi-megabyte burst after an idle period, which reads as "the limit does
/// nothing" to everyone watching the link.
const MAX_BURST_BYTES: u64 = 1 << 20;

/// Token bucket for one route.
#[derive(Debug)]
pub(crate) struct RouteRateLimiter {
    bytes_per_second: u64,
    burst: u64,
    bucket: Mutex<Bucket>,
}

#[derive(Debug)]
struct Bucket {
    tokens: u64,
    /// When the token count was last brought up to date.
    last_refill: Instant,
}

impl RouteRateLimiter {
    /// Builds a limiter for `bytes_per_second` (validated non-zero upstream).
    pub(crate) fn new(bytes_per_second: u64) -> Self {
        let bytes_per_second = bytes_per_second.max(1);
        let burst = bytes_per_second.min(MAX_BURST_BYTES);
        Self {
            bytes_per_second,
            burst,
            bucket: Mutex::new(Bucket {
                // A fresh route starts with a full second of credit so the
                // first request is not artificially slowed.
                tokens: burst,
                last_refill: Instant::now(),
            }),
        }
    }

    /// The configured ceiling, for diagnostics.
    pub(crate) fn bytes_per_second(&self) -> u64 {
        self.bytes_per_second
    }

    /// Refills the bucket and reports the tokens available plus how long the
    /// caller must wait before a single byte becomes available.
    fn budget(&self) -> (u64, Duration) {
        let mut bucket = self.bucket.lock().expect("rate limiter lock");
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(bucket.last_refill);
        let gained =
            (elapsed.as_nanos() * u128::from(self.bytes_per_second) / 1_000_000_000) as u64;
        if gained > 0 {
            bucket.tokens = (bucket.tokens + gained).min(self.burst);
            // Advance the clock by exactly the time those tokens represent so
            // the fractional remainder is not lost to rounding.
            let consumed = Duration::from_nanos(
                (u128::from(gained) * 1_000_000_000 / u128::from(self.bytes_per_second)) as u64,
            );
            bucket.last_refill += consumed;
        }
        if bucket.tokens > 0 {
            return (bucket.tokens, Duration::ZERO);
        }
        let per_byte = Duration::from_nanos(
            (1_000_000_000_u128 / u128::from(self.bytes_per_second)).max(1) as u64,
        );
        let waited = now.saturating_duration_since(bucket.last_refill);
        (0, per_byte.saturating_sub(waited))
    }

    /// Consumes `bytes` tokens, saturating at the bucket's contents. Returns
    /// how many were consumed.
    fn consume(&self, bytes: u64) -> u64 {
        if bytes == 0 {
            return 0;
        }
        let mut bucket = self.bucket.lock().expect("rate limiter lock");
        let taken = bucket.tokens.min(bytes);
        bucket.tokens -= taken;
        taken
    }
}

/// Wraps a relayed stream with a route's ceiling.
///
/// Both directions are paced: a route's ceiling is a property of the service,
/// not of one direction, and an unpaced upload would still saturate the link.
#[derive(Debug)]
pub(crate) struct RateLimitedStream<S> {
    inner: S,
    limiter: Arc<RouteRateLimiter>,
    /// Timer armed while the bucket is empty; the poll that finds no tokens
    /// registers its waker here and the stream is polled again on expiry.
    sleep: Pin<Box<Sleep>>,
    sleep_deadline: Option<Instant>,
    /// Scratch for one paced read; the pacing wrapper copies through it
    /// because `ReadBuf` cannot be split into "read at most N" safely.
    scratch: Box<[u8]>,
}

/// Bytes read per paced poll: bounds the per-poll copy and keeps the pacing
/// responsive on a slow ceiling.
const READ_CHUNK: usize = 16 * 1024;

impl<S> RateLimitedStream<S> {
    pub(crate) fn new(inner: S, limiter: Arc<RouteRateLimiter>) -> Self {
        Self {
            inner,
            limiter,
            sleep: Box::pin(tokio::time::sleep(Duration::ZERO)),
            sleep_deadline: None,
            scratch: vec![0_u8; READ_CHUNK].into_boxed_slice(),
        }
    }

    /// How many bytes this poll may move, or `Pending` once the timer that
    /// refills the bucket is armed.
    fn poll_budget(&mut self, cx: &mut Context<'_>) -> Poll<usize> {
        let (tokens, wait) = self.limiter.budget();
        if tokens > 0 {
            self.sleep_deadline = None;
            return Poll::Ready(tokens as usize);
        }
        let now = Instant::now();
        let deadline = now + wait.max(Duration::from_millis(1));
        if self.sleep_deadline != Some(deadline) {
            self.sleep
                .as_mut()
                .reset(tokio::time::Instant::from_std(deadline));
            self.sleep_deadline = Some(deadline);
        }
        let _ = self.sleep.as_mut().poll(cx);
        Poll::Pending
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for RateLimitedStream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let allowed = match this.poll_budget(cx) {
            Poll::Ready(allowed) => allowed,
            Poll::Pending => return Poll::Pending,
        };
        let max = buf.remaining().min(allowed).min(READ_CHUNK);
        if max == 0 {
            // The destination has no room; let the caller's next poll decide.
            return Poll::Ready(Ok(()));
        }
        let mut scratch = ReadBuf::new(&mut this.scratch[..max]);
        match Pin::new(&mut this.inner).poll_read(cx, &mut scratch) {
            Poll::Ready(Ok(())) => {
                let filled = scratch.filled();
                if !filled.is_empty() {
                    buf.put_slice(filled);
                    this.limiter.consume(filled.len() as u64);
                }
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for RateLimitedStream<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        let allowed = match this.poll_budget(cx) {
            Poll::Ready(allowed) => allowed,
            Poll::Pending => return Poll::Pending,
        };
        let max = data.len().min(allowed);
        if max == 0 {
            return Poll::Pending;
        }
        match Pin::new(&mut this.inner).poll_write(cx, &data[..max]) {
            Poll::Ready(Ok(written)) => {
                this.limiter.consume(written as u64);
                Poll::Ready(Ok(written))
            }
            other => other,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin + Send> sdkwork_webserver_tunnel_transport::TunnelStream
    for RateLimitedStream<S>
{
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_bucket_grants_one_burst_and_then_waits() {
        let limiter = RouteRateLimiter::new(4_000);
        let (tokens, wait) = limiter.budget();
        assert_eq!(
            tokens, 4_000,
            "a fresh route starts with one second of credit"
        );
        assert_eq!(wait, Duration::ZERO);

        assert_eq!(limiter.consume(4_000), 4_000);
        let (tokens, wait) = limiter.budget();
        assert_eq!(tokens, 0, "the burst is spent");
        // 4000 B/s means one byte every 250 microseconds.
        assert!(
            wait <= Duration::from_micros(250),
            "the wait for one token must be the inverse rate, got {wait:?}"
        );
    }

    #[test]
    fn tokens_refill_at_the_configured_rate() {
        let limiter = RouteRateLimiter::new(10_000);
        let (burst, _) = limiter.budget();
        limiter.consume(burst);
        std::thread::sleep(Duration::from_millis(200));
        let (tokens, _) = limiter.budget();
        // 200 ms at 10 000 B/s is ~2000 bytes; allow scheduler slack.
        assert!(
            (1_200..=2_800).contains(&tokens),
            "expected roughly 2 KB of credit, got {tokens}"
        );
    }

    #[test]
    fn the_burst_is_capped_so_a_limit_cannot_dump_a_huge_burst() {
        let limiter = RouteRateLimiter::new(100 * 1024 * 1024);
        let (tokens, _) = limiter.budget();
        assert_eq!(tokens, MAX_BURST_BYTES);
        assert_eq!(limiter.bytes_per_second(), 100 * 1024 * 1024);
    }

    #[test]
    fn consumption_saturates_at_the_bucket_contents() {
        let limiter = RouteRateLimiter::new(1_000);
        let (burst, _) = limiter.budget();
        assert_eq!(limiter.consume(burst + 10_000), burst);
        assert_eq!(limiter.consume(1), 0);
    }
}
