//! Process-wide graceful-shutdown trigger.
//!
//! Two owners can end a standalone gateway process: the operator (`SIGINT` /
//! `SIGTERM`, handled by the binary) and the cluster registry, which asks an
//! instance to drain through the heartbeat's `ops.drainRequested` directive
//! (`docs/architecture/tech/TECH-cluster-management.md`:179,183). Both must run
//! the *same* drain path — the data plane retires in-flight work within
//! `drainTimeoutMs` — so they feed one trigger instead of racing two shutdown
//! paths.
//!
//! The trigger is deliberately process-wide. Its producer is
//! [`crate::cluster_self_report`], which the assembly spawns with no other
//! handle to the process, and its consumer is the data-plane server owned by
//! the gateway binary. Nothing else in the process may request a shutdown.

use std::sync::OnceLock;

use tokio::sync::watch;

/// Why this process is stopping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShutdownReason {
    /// The cluster registry asked this instance to drain: the operator marked
    /// it draining, so it finishes in-flight work and stops.
    ClusterDrain,
}

/// The trigger mechanism, kept separate from the process-wide instance below so
/// its behaviour can be exercised without touching a one-way global.
struct ShutdownSignal {
    sender: watch::Sender<Option<ShutdownReason>>,
    /// A receiver that lives as long as its owner. `watch::Sender` does not
    /// apply a request when every receiver is gone, and this trigger has no
    /// fixed owner — either side may be first — so one receiver is kept alive
    /// from the start and cloned by each waiter.
    keepalive: watch::Receiver<Option<ShutdownReason>>,
}

impl ShutdownSignal {
    fn new() -> Self {
        let (sender, keepalive) = watch::channel(None);
        Self { sender, keepalive }
    }

    /// Records `reason`, returning `false` when a reason was already recorded
    /// (the first one wins) or when nothing was listening.
    fn request(&self, reason: ShutdownReason) -> bool {
        self.sender.send_if_modified(|current| {
            if current.is_some() {
                return false;
            }
            *current = Some(reason);
            true
        })
    }

    fn reason(&self) -> Option<ShutdownReason> {
        *self.sender.borrow()
    }

    /// Resolves once a shutdown has been requested, with its reason.
    ///
    /// A request that lands before the first poll is never missed: the receiver
    /// is subscribed before the current value is read, and the value is re-read
    /// on every iteration.
    async fn wait(&self) -> ShutdownReason {
        let mut receiver = self.keepalive.clone();
        loop {
            if let Some(reason) = *receiver.borrow_and_update() {
                return reason;
            }
            if receiver.changed().await.is_err() {
                // Unreachable while the keepalive receiver lives; parking beats
                // spinning if that ever changes.
                std::future::pending::<()>().await;
            }
        }
    }
}

fn trigger() -> &'static ShutdownSignal {
    static TRIGGER: OnceLock<ShutdownSignal> = OnceLock::new();
    TRIGGER.get_or_init(ShutdownSignal::new)
}

/// Requests a process shutdown.
///
/// Idempotent, and the first reason wins: a drain directive must not relabel a
/// shutdown an operator already asked for.
pub fn request_shutdown(reason: ShutdownReason) {
    if trigger().request(reason) {
        tracing::info!(?reason, "process shutdown requested");
    }
}

/// The reason a shutdown was requested, if any.
#[must_use]
pub fn shutdown_reason() -> Option<ShutdownReason> {
    trigger().reason()
}

/// Resolves once a shutdown has been requested, with its reason.
pub async fn wait_for_shutdown() -> ShutdownReason {
    trigger().wait().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn a_waiter_parks_until_a_request_arrives() {
        let signal = ShutdownSignal::new();
        // No request yet: the waiter must stay parked rather than resolve.
        assert!(
            tokio::time::timeout(Duration::from_millis(50), signal.wait())
                .await
                .is_err(),
            "an unrequested shutdown must not resolve"
        );
        assert!(signal.request(ShutdownReason::ClusterDrain));
        assert_eq!(
            tokio::time::timeout(Duration::from_millis(50), signal.wait())
                .await
                .expect("a requested shutdown resolves"),
            ShutdownReason::ClusterDrain
        );
    }

    #[tokio::test]
    async fn a_request_that_lands_before_the_first_poll_is_not_missed() {
        let signal = ShutdownSignal::new();
        assert!(signal.request(ShutdownReason::ClusterDrain));
        assert_eq!(signal.reason(), Some(ShutdownReason::ClusterDrain));
        assert_eq!(signal.wait().await, ShutdownReason::ClusterDrain);
    }

    #[tokio::test]
    async fn a_second_request_is_not_a_new_transition() {
        let signal = ShutdownSignal::new();
        assert!(signal.request(ShutdownReason::ClusterDrain));
        assert!(
            !signal.request(ShutdownReason::ClusterDrain),
            "an already-requested shutdown must not report a second transition"
        );
        assert_eq!(signal.reason(), Some(ShutdownReason::ClusterDrain));
    }
}
