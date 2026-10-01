//! Process-global observability counters.
//!
//! These counters are written by the management service (the component that
//! owns the failure) and read by the operations `/metrics` scrape (which may
//! run in the same process but holds no handle to the service), so they live
//! on the shared contract plane rather than behind a service handle.

use std::sync::atomic::{AtomicU64, Ordering};

static AUDIT_PERSISTENCE_FAILURES: AtomicU64 = AtomicU64::new(0);

/// Records one audit row that could not be persisted.
///
/// The audit insert is deliberately post-commit (the business effect is
/// durable before the audit row is attempted), so a persistence failure is a
/// permanent audit gap for that operation. It must be visible: the operations
/// scrape exports the total as
/// `sdkwork_webserver_audit_persistence_failures_total` and the shipped
/// Prometheus rules alert on it.
pub fn record_audit_persistence_failure() {
    AUDIT_PERSISTENCE_FAILURES.fetch_add(1, Ordering::Relaxed);
}

/// Total audit log persistence failures since process start. Operators must
/// alert on a nonzero value; a silent audit gap violates the commercial audit
/// contract.
pub fn audit_persistence_failures_total() -> u64 {
    AUDIT_PERSISTENCE_FAILURES.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_are_counted_and_readable() {
        let before = audit_persistence_failures_total();
        record_audit_persistence_failure();
        assert_eq!(audit_persistence_failures_total(), before + 1);
    }
}
