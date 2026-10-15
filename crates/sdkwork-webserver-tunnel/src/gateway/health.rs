//! Live health of a registered route (FRP `healthCheck` parity).
//!
//! Health is reported by the **agent**, which is the only party that can reach
//! the route's local target without consuming the tunnel it serves with. The
//! gateway stores the latest verdict here and uses it to steer a
//! load-balancer group away from a replica that cannot serve.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// One route's health state, shared between the registry and the control loop
/// that applies probe reports.
#[derive(Debug)]
pub struct RouteHealth {
    healthy: AtomicBool,
    consecutive_failures: AtomicU32,
    /// Successful transitions to unhealthy, for diagnostics.
    down_transitions: AtomicU64,
}

impl Default for RouteHealth {
    fn default() -> Self {
        Self {
            // A route starts healthy: it was just registered by an agent that
            // believes it can serve, and starting "down" would black-hole
            // traffic until the first probe came back.
            healthy: AtomicBool::new(true),
            consecutive_failures: AtomicU32::new(0),
            down_transitions: AtomicU64::new(0),
        }
    }
}

impl RouteHealth {
    /// True while the route may receive traffic.
    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Relaxed)
    }

    /// Consecutive failed probes since the last success.
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures.load(Ordering::Relaxed)
    }

    /// How often this route has been marked down.
    pub fn down_transitions(&self) -> u64 {
        self.down_transitions.load(Ordering::Relaxed)
    }

    /// Applies one probe verdict, returning `true` when the route's health
    /// *changed* (so the caller can log or count exactly the transitions).
    pub fn apply_probe(&self, healthy: bool, consecutive_failures: u32) -> bool {
        self.consecutive_failures
            .store(consecutive_failures, Ordering::Relaxed);
        let previous = self.healthy.swap(healthy, Ordering::Relaxed);
        if previous == healthy {
            return false;
        }
        if !healthy {
            self.down_transitions.fetch_add(1, Ordering::Relaxed);
        }
        true
    }

    /// Marks the route down because its owning session ended.
    ///
    /// A route whose agent is gone is not merely unhealthy, it is unreachable;
    /// the session teardown path unregisters routes, so this exists for the
    /// window where a session is being torn down while a request is in flight.
    pub fn mark_unreachable(&self) {
        self.healthy.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_route_is_healthy() {
        let health = RouteHealth::default();
        assert!(health.is_healthy());
        assert_eq!(health.consecutive_failures(), 0);
        assert_eq!(health.down_transitions(), 0);
    }

    #[test]
    fn transitions_are_reported_once_per_change() {
        let health = RouteHealth::default();
        assert!(health.apply_probe(false, 1), "going down is a transition");
        assert!(!health.is_healthy());
        assert_eq!(health.down_transitions(), 1);
        assert!(
            !health.apply_probe(false, 2),
            "staying down is not a transition"
        );
        assert_eq!(health.consecutive_failures(), 2);
        assert!(health.apply_probe(true, 0), "recovery is a transition");
        assert!(health.is_healthy());
        assert_eq!(health.down_transitions(), 1);
    }

    #[test]
    fn an_unreachable_route_is_marked_down() {
        let health = RouteHealth::default();
        health.mark_unreachable();
        assert!(!health.is_healthy());
    }
}
