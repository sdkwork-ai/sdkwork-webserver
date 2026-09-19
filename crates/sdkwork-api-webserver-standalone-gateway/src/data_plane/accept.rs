//! Shared accept-loop resilience policy for every TCP listener.
//!
//! A production edge listener must survive the transient `accept()` failures
//! that are routine under high connection churn and file-descriptor pressure
//! (aborted connections, fd exhaustion, transient network-stack errors).
//! Nginx's accept loop logs such failures and retries after a short pause;
//! only a closed or broken listener is fatal. Every accept loop in the data
//! plane shares this one policy so listener lifecycle behavior stays uniform.

use std::io;
use std::time::Duration;

/// Pause between accept retries so a persistent failure (for example an
/// fd-exhaustion storm) cannot spin the loop hot.
pub(crate) const ACCEPT_RETRY_PAUSE: Duration = Duration::from_millis(10);

/// Raw OS error codes whose `accept()` failures are transient. This is the
/// union of the Linux errno values and Windows Winsock error codes that nginx
/// treats as retryable; the numeric ranges do not collide.
const TRANSIENT_ACCEPT_OS_CODES: &[i32] = &[
    4,     // EINTR / WSAEINTR
    11,    // EAGAIN / EWOULDBLOCK
    12,    // ENOMEM
    23,    // ENFILE
    24,    // EMFILE
    71,    // EPROTO
    100,   // ENETDOWN
    101,   // ENETUNREACH
    103,   // ECONNABORTED
    105,   // ENOBUFS
    10024, // WSAEMFILE
    10035, // WSAEWOULDBLOCK
    10050, // WSAENETDOWN
    10051, // WSAENETUNREACH
    10053, // WSAECONNABORTED
    10054, // WSAECONNRESET
    10055, // WSAENOBUFS
    10060, // WSAETIMEDOUT
];

/// `true` when the accept error is a transient condition the listener must
/// survive, `false` when the listener itself is broken and must stop.
pub(crate) fn is_transient_accept_error(error: &io::Error) -> bool {
    if let Some(code) = error.raw_os_error() {
        return TRANSIENT_ACCEPT_OS_CODES.contains(&code);
    }
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionAborted
            | io::ErrorKind::Interrupted
            | io::ErrorKind::WouldBlock
            | io::ErrorKind::OutOfMemory
            | io::ErrorKind::TimedOut
    )
}

/// Tracks consecutive transient accept failures for one listener so the log
/// stays readable during a sustained failure storm.
pub(crate) struct AcceptRetryPolicy {
    consecutive_failures: u64,
}

impl AcceptRetryPolicy {
    pub(crate) const fn new() -> Self {
        Self {
            consecutive_failures: 0,
        }
    }

    /// Classify one accept failure. Returns `true` when the loop must pause
    /// and continue; `false` when the listener must stop.
    pub(crate) fn on_error(&mut self, error: &io::Error) -> bool {
        if !is_transient_accept_error(error) {
            return false;
        }
        self.consecutive_failures += 1;
        if self.consecutive_failures <= 3 || self.consecutive_failures % 100 == 0 {
            tracing::warn!(
                failures = self.consecutive_failures,
                error = %error,
                "transient accept error; listener continues after a short pause"
            );
        }
        true
    }

    pub(crate) fn on_success(&mut self) {
        self.consecutive_failures = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os_error(code: i32) -> io::Error {
        io::Error::from_raw_os_error(code)
    }

    #[test]
    fn transient_codes_are_retryable() {
        assert!(is_transient_accept_error(&os_error(103))); // ECONNABORTED
        assert!(is_transient_accept_error(&os_error(24))); // EMFILE
        assert!(is_transient_accept_error(&os_error(10054))); // WSAECONNRESET
    }

    #[test]
    fn broken_listener_is_fatal() {
        assert!(!is_transient_accept_error(&os_error(9))); // EBADF
        assert!(!is_transient_accept_error(&os_error(13))); // EACCES
        assert!(!is_transient_accept_error(&io::Error::other("custom")));
    }

    #[test]
    fn policy_resets_on_success_and_cap_logs() {
        let mut policy = AcceptRetryPolicy::new();
        assert!(policy.on_error(&os_error(103)));
        assert!(policy.on_error(&os_error(103)));
        policy.on_success();
        // After a success the consecutive counter restarts.
        assert!(policy.on_error(&os_error(103)));
        for _ in 0..200 {
            assert!(policy.on_error(&os_error(103)));
        }
        assert!(!policy.on_error(&os_error(13)));
    }
}
