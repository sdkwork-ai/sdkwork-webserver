//! Certificate-expiry metrics for the operations scrape endpoint.
//!
//! The data plane's fixed registry (REQ-2026-0033/0034) has a documented
//! charter — connection/request lifetimes, upstream outcomes, resource
//! pressure, reload, tunnel state — and certificate health is not in it. The
//! expiry series therefore live in their own process-wide snapshot, updated
//! by a management-side sampler (`data-plane` command, where the shared
//! database pool is initialized) and rendered by the operations `/metrics`
//! handler next to the tunnel registry, exactly like the tunnel precedent.
//!
//! Unknown state is honest: until the first successful sample the gauges
//! report `-1` seconds / `0` expiring, and a sampler that cannot reach the
//! database leaves the last value but logs — the values are observations,
//! never fabricated.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

const UNKNOWN_EXPIRY_SECONDS: i64 = -1;

static MINIMUM_EXPIRY_SECONDS: AtomicI64 = AtomicI64::new(UNKNOWN_EXPIRY_SECONDS);
static EXPIRING_WITHIN_30_DAYS: AtomicU64 = AtomicU64::new(0);

/// Records one sampler observation.
pub fn record_certificate_expiry(minimum_seconds: i64, expiring_within_30_days: i64) {
    MINIMUM_EXPIRY_SECONDS.store(minimum_seconds, Ordering::Release);
    EXPIRING_WITHIN_30_DAYS.store(expiring_within_30_days.max(0) as u64, Ordering::Release);
}

/// Renders the two certificate-health gauges in Prometheus text format.
pub fn render_prometheus() -> String {
    let minimum = MINIMUM_EXPIRY_SECONDS.load(Ordering::Acquire);
    let expiring = EXPIRING_WITHIN_30_DAYS.load(Ordering::Acquire);
    let mut text = String::with_capacity(256);
    text.push_str("# HELP sdkwork_webserver_certificate_expiry_seconds_min Smallest seconds to expiry over active, non-revoked certificates; -1 when no observation exists.\n");
    text.push_str("# TYPE sdkwork_webserver_certificate_expiry_seconds_min gauge\n");
    text.push_str(&format!(
        "sdkwork_webserver_certificate_expiry_seconds_min {minimum}\n"
    ));
    text.push_str("# HELP sdkwork_webserver_certificate_expiring_soon Active, non-revoked certificates expiring within 30 days.\n");
    text.push_str("# TYPE sdkwork_webserver_certificate_expiring_soon gauge\n");
    text.push_str(&format!(
        "sdkwork_webserver_certificate_expiring_soon {expiring}\n"
    ));
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One sequential test on purpose: the snapshot is process-global, so the
    /// tests would order-depend on each other if they asserted different
    /// states independently.
    #[test]
    fn the_snapshot_moves_from_unobserved_through_recorded_states() {
        // Before any sample the scrape says so instead of inventing a value
        // (PRD §12 - no fake telemetry).
        let text = render_prometheus();
        assert!(text.contains("sdkwork_webserver_certificate_expiry_seconds_min -1"));
        assert!(text.contains("sdkwork_webserver_certificate_expiring_soon 0"));

        // A recorded observation lands verbatim.
        record_certificate_expiry(86_400, 3);
        let text = render_prometheus();
        assert!(text.contains("sdkwork_webserver_certificate_expiry_seconds_min 86400"));
        assert!(text.contains("sdkwork_webserver_certificate_expiring_soon 3"));

        // A negative count cannot exist; it clamps to zero.
        record_certificate_expiry(60, -2);
        let text = render_prometheus();
        assert!(text.contains("sdkwork_webserver_certificate_expiring_soon 0"));
    }
}
