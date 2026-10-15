//! Tunnel metrics (PRD §48).
//!
//! Lock-free counters shared by the gateway and agent roles, rendered in
//! Prometheus text exposition for the webserver operations listener.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// Tunnel metric counters (PRD §48). Gauges are kept as signed values so a
/// decrement can never wrap below zero.
#[derive(Debug, Default)]
pub struct TunnelMetrics {
    connections_total: AtomicU64,
    connections_active: AtomicI64,
    sessions_total: AtomicU64,
    sessions_active: AtomicI64,
    streams_total: AtomicU64,
    streams_active: AtomicI64,
    bytes_in: AtomicU64,
    bytes_out: AtomicU64,
    reconnects: AtomicU64,
    errors: AtomicU64,
    auth_failures: AtomicU64,
    relay_denied: AtomicU64,
    health_failures: AtomicU64,
    no_healthy_replica: AtomicU64,
    visitor_streams: AtomicU64,
    visitor_streams_closed: AtomicU64,
    visitor_auth_failures: AtomicU64,
    /// Datagrams dropped because they exceeded the configured packet size.
    udp_datagrams_too_large: AtomicU64,
    /// Stream sequence a client-side opener (the visitor runtime) uses.
    visitor_stream_sequence: AtomicU64,
    routes_active: AtomicI64,
}

impl TunnelMetrics {
    /// A new empty counter set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one accepted transport connection.
    pub fn record_connection_open(&self) {
        self.connections_total.fetch_add(1, Ordering::Relaxed);
        self.bump_active(&self.connections_active, 1);
    }

    /// Records one transport connection closed.
    pub fn record_connection_close(&self) {
        self.bump_active(&self.connections_active, -1);
    }

    /// Records one authenticated session established.
    pub fn record_session_open(&self) {
        self.sessions_total.fetch_add(1, Ordering::Relaxed);
        self.bump_active(&self.sessions_active, 1);
    }

    /// Records one session torn down.
    pub fn record_session_close(&self) {
        self.bump_active(&self.sessions_active, -1);
    }

    /// Records one data stream opened.
    pub fn record_stream_open(&self) {
        self.streams_total.fetch_add(1, Ordering::Relaxed);
        self.bump_active(&self.streams_active, 1);
    }

    /// Next stream sequence for a client-side opener (visitor runtime): the
    /// gateway assigns ids to the streams it opens, and a visitor must not
    /// guess one it did not create.
    pub fn next_stream_id(&self) -> u64 {
        self.visitor_stream_sequence.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Records one data stream closed.
    pub fn record_stream_close(&self) {
        self.bump_active(&self.streams_active, -1);
    }

    /// Adjusts the active route gauge (registration/removal).
    pub fn record_route_change(&self, delta: i64) {
        self.bump_active(&self.routes_active, delta);
    }

    /// Adds relayed ingress bytes (visitor to agent direction).
    pub fn add_bytes_in(&self, bytes: u64) {
        self.bytes_in.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Adds relayed egress bytes (agent to visitor direction).
    pub fn add_bytes_out(&self, bytes: u64) {
        self.bytes_out.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Records one agent reconnect.
    pub fn record_reconnect(&self) {
        self.reconnects.fetch_add(1, Ordering::Relaxed);
    }

    /// Records one tunnel error.
    pub fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Records one failed authentication attempt.
    pub fn record_auth_failure(&self) {
        self.auth_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Records one visitor turned away by a route's admission policy (network
    /// allow-list or missing/wrong visitor token). Kept apart from
    /// [`Self::record_auth_failure`], which counts *agent* credential
    /// failures: an operator alerting on agent auth failures must not be
    /// paged by visitors hitting a private route.
    pub fn record_relay_denied(&self) {
        self.relay_denied.fetch_add(1, Ordering::Relaxed);
    }

    /// Records one failed health probe reported by an agent, and one request
    /// turned away because a load-balancer group had no healthy replica.
    pub fn record_health_failure(&self) {
        self.health_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Records one request rejected because every replica was unhealthy.
    pub fn record_no_healthy_replica(&self) {
        self.no_healthy_replica.fetch_add(1, Ordering::Relaxed);
    }

    /// Records one visitor stream bridged to a provider (FRP `stcp`/`sudp`).
    pub fn record_visitor_stream_open(&self) {
        self.visitor_streams.fetch_add(1, Ordering::Relaxed);
    }

    /// Records one bridged visitor stream closing.
    pub fn record_visitor_stream_close(&self) {
        self.visitor_streams_closed.fetch_add(1, Ordering::Relaxed);
    }

    /// Records one visitor rejected for a wrong or missing secret key.
    ///
    /// Separate from [`Self::record_auth_failure`] (agent credentials) so an
    /// alert on agent authentication cannot be triggered by visitors guessing
    /// a secret-route key.
    pub fn record_visitor_auth_failure(&self) {
        self.visitor_auth_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Records one UDP datagram dropped for exceeding the configured packet
    /// size (FRP `transport.udpPacketSize`).
    ///
    /// Counted rather than logged per packet: a service that consistently
    /// emits oversized datagrams would otherwise flood the log, and the rate
    /// is exactly what tells an operator to raise the ceiling.
    pub fn record_udp_datagram_too_large(&self) {
        self.udp_datagrams_too_large.fetch_add(1, Ordering::Relaxed);
    }

    /// Datagrams dropped for exceeding the configured packet size.
    pub fn udp_datagrams_too_large(&self) -> u64 {
        self.udp_datagrams_too_large.load(Ordering::Relaxed)
    }

    /// Snapshot of every counter, used by status APIs and tests.
    pub fn snapshot(&self) -> TunnelMetricsSnapshot {
        TunnelMetricsSnapshot {
            connections_total: self.connections_total.load(Ordering::Relaxed),
            connections_active: u64::try_from(self.connections_active.load(Ordering::Relaxed))
                .unwrap_or(0),
            sessions_total: self.sessions_total.load(Ordering::Relaxed),
            sessions_active: u64::try_from(self.sessions_active.load(Ordering::Relaxed))
                .unwrap_or(0),
            streams_total: self.streams_total.load(Ordering::Relaxed),
            streams_active: u64::try_from(self.streams_active.load(Ordering::Relaxed)).unwrap_or(0),
            bytes_in: self.bytes_in.load(Ordering::Relaxed),
            bytes_out: self.bytes_out.load(Ordering::Relaxed),
            reconnects: self.reconnects.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            auth_failures: self.auth_failures.load(Ordering::Relaxed),
            relay_denied: self.relay_denied.load(Ordering::Relaxed),
            health_failures: self.health_failures.load(Ordering::Relaxed),
            no_healthy_replica: self.no_healthy_replica.load(Ordering::Relaxed),
            visitor_streams: self.visitor_streams.load(Ordering::Relaxed),
            udp_datagrams_too_large: self.udp_datagrams_too_large.load(Ordering::Relaxed),
            visitor_streams_closed: self.visitor_streams_closed.load(Ordering::Relaxed),
            visitor_auth_failures: self.visitor_auth_failures.load(Ordering::Relaxed),
            routes_active: u64::try_from(self.routes_active.load(Ordering::Relaxed)).unwrap_or(0),
        }
    }

    /// Renders the Prometheus text exposition of every tunnel metric.
    pub fn render_prometheus(&self) -> String {
        let snapshot = self.snapshot();
        let mut text = String::with_capacity(1024);
        let mut emit = |name: &str, help: &str, value: u64, kind: &str| {
            text.push_str(&format!(
                "# HELP {name} {help}\n# TYPE {name} {kind}\n{name} {value}\n"
            ));
        };
        emit(
            "sdkwork_tunnel_connections",
            "Tunnel transport connections",
            snapshot.connections_total,
            "counter",
        );
        emit(
            "sdkwork_tunnel_connections_active",
            "Currently open tunnel transport connections",
            snapshot.connections_active,
            "gauge",
        );
        emit(
            "sdkwork_tunnel_sessions",
            "Authenticated tunnel sessions",
            snapshot.sessions_total,
            "counter",
        );
        emit(
            "sdkwork_tunnel_sessions_active",
            "Currently live tunnel sessions",
            snapshot.sessions_active,
            "gauge",
        );
        emit(
            "sdkwork_tunnel_streams",
            "Tunnel data streams",
            snapshot.streams_total,
            "counter",
        );
        emit(
            "sdkwork_tunnel_streams_active",
            "Currently open tunnel data streams",
            snapshot.streams_active,
            "gauge",
        );
        emit(
            "sdkwork_tunnel_bytes_in",
            "Bytes relayed from visitors toward agents",
            snapshot.bytes_in,
            "counter",
        );
        emit(
            "sdkwork_tunnel_bytes_out",
            "Bytes relayed from agents toward visitors",
            snapshot.bytes_out,
            "counter",
        );
        emit(
            "sdkwork_tunnel_reconnects",
            "Agent reconnect attempts",
            snapshot.reconnects,
            "counter",
        );
        emit(
            "sdkwork_tunnel_errors",
            "Tunnel errors",
            snapshot.errors,
            "counter",
        );
        emit(
            "sdkwork_tunnel_auth_failures",
            "Failed tunnel authentication attempts",
            snapshot.auth_failures,
            "counter",
        );
        emit(
            "sdkwork_tunnel_relay_denied",
            "Visitor relays denied by route admission policy",
            snapshot.relay_denied,
            "counter",
        );
        emit(
            "sdkwork_tunnel_health_failures",
            "Failed route health probes reported by agents",
            snapshot.health_failures,
            "counter",
        );
        emit(
            "sdkwork_tunnel_no_healthy_replica",
            "Requests rejected because no replica of a load balancer group was healthy",
            snapshot.no_healthy_replica,
            "counter",
        );
        emit(
            "sdkwork_tunnel_visitor_streams",
            "Visitor streams bridged to a provider (secret routes)",
            snapshot.visitor_streams,
            "counter",
        );
        emit(
            "sdkwork_tunnel_visitor_auth_failures",
            "Visitors rejected for a wrong or missing secret key",
            snapshot.visitor_auth_failures,
            "counter",
        );
        emit(
            "sdkwork_tunnel_routes_active",
            "Currently registered tunnel routes",
            snapshot.routes_active,
            "gauge",
        );
        emit(
            "sdkwork_tunnel_udp_datagrams_too_large",
            "UDP datagrams dropped for exceeding the configured packet size",
            snapshot.udp_datagrams_too_large,
            "counter",
        );
        text
    }

    fn bump_active(&self, gauge: &AtomicI64, delta: i64) {
        let _ = gauge.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            Some((current + delta).max(0))
        });
    }
}

/// Point-in-time copy of [`TunnelMetrics`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelMetricsSnapshot {
    /// Total transport connections accepted.
    pub connections_total: u64,
    /// Currently open transport connections.
    pub connections_active: u64,
    /// Total authenticated sessions.
    pub sessions_total: u64,
    /// Currently live sessions.
    pub sessions_active: u64,
    /// Total data streams.
    pub streams_total: u64,
    /// Currently open data streams.
    pub streams_active: u64,
    /// Visitor to agent bytes.
    pub bytes_in: u64,
    /// Agent to visitor bytes.
    pub bytes_out: u64,
    /// Agent reconnect attempts.
    pub reconnects: u64,
    /// Tunnel errors.
    pub errors: u64,
    /// Failed authentications.
    pub auth_failures: u64,
    /// Visitor relays denied by route admission policy.
    pub relay_denied: u64,
    /// Failed route health probes reported by agents.
    pub health_failures: u64,
    /// Requests rejected because no replica of a load-balancer group was
    /// healthy.
    pub no_healthy_replica: u64,
    /// Visitor streams bridged to a provider (FRP `stcp`/`sudp`).
    pub visitor_streams: u64,
    /// UDP datagrams dropped for exceeding the configured packet size.
    pub udp_datagrams_too_large: u64,
    /// Bridged visitor streams that have closed.
    pub visitor_streams_closed: u64,
    /// Visitors rejected for a wrong or missing secret key.
    pub visitor_auth_failures: u64,
    /// Currently registered routes.
    pub routes_active: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_track_lifecycles() {
        let metrics = TunnelMetrics::new();
        metrics.record_connection_open();
        metrics.record_session_open();
        metrics.record_stream_open();
        metrics.record_route_change(1);
        metrics.add_bytes_in(10);
        metrics.add_bytes_out(20);
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.connections_active, 1);
        assert_eq!(snapshot.sessions_active, 1);
        assert_eq!(snapshot.streams_active, 1);
        assert_eq!(snapshot.routes_active, 1);
        assert_eq!(snapshot.bytes_in, 10);
        assert_eq!(snapshot.bytes_out, 20);
        metrics.record_stream_close();
        metrics.record_session_close();
        metrics.record_connection_close();
        metrics.record_route_change(-1);
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.connections_active, 0);
        assert_eq!(snapshot.streams_active, 0);
        assert_eq!(snapshot.routes_active, 0);
        // Gauges never go negative.
        metrics.record_route_change(-5);
        assert_eq!(metrics.snapshot().routes_active, 0);
    }

    #[test]
    fn prometheus_render_contains_canonical_names() {
        let metrics = TunnelMetrics::new();
        metrics.record_session_open();
        let text = metrics.render_prometheus();
        for name in [
            "sdkwork_tunnel_connections",
            "sdkwork_tunnel_sessions",
            "sdkwork_tunnel_streams",
            "sdkwork_tunnel_bytes_in",
            "sdkwork_tunnel_bytes_out",
            "sdkwork_tunnel_reconnects",
            "sdkwork_tunnel_errors",
            "sdkwork_tunnel_auth_failures",
            "sdkwork_tunnel_relay_denied",
            "sdkwork_tunnel_health_failures",
            "sdkwork_tunnel_no_healthy_replica",
            "sdkwork_tunnel_visitor_streams",
            "sdkwork_tunnel_visitor_auth_failures",
            "sdkwork_tunnel_routes_active",
        ] {
            assert!(text.contains(name), "missing {name}");
        }
    }

    #[test]
    fn visitor_denials_are_counted_separately_from_agent_auth_failures() {
        // An operator alerting on agent credential failures must not be paged
        // by visitors hitting a private route.
        let metrics = TunnelMetrics::new();
        metrics.record_relay_denied();
        metrics.record_relay_denied();
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.relay_denied, 2);
        assert_eq!(snapshot.auth_failures, 0);
    }
}
