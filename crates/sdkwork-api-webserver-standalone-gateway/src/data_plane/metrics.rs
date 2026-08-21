use std::{
    error::Error as StdError,
    fmt::Write as _,
    io,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::{Duration, Instant},
};

use axum::{body::Body, http::Response};
use bytes::Bytes;
use http_body::{Body as HttpBody, Frame, SizeHint};
use sdkwork_webserver_delivery_runtime::WebsiteProviderResolutionCacheSnapshot;
use sync_wrapper::SyncWrapper;

use crate::metric_dimensions::CanonicalMetricDimensions;

use super::{fixed_histogram::FixedHistogram, runtime::DataPlaneRuntime};

const STATUS_CLASSES: [&str; 6] = ["1xx", "2xx", "3xx", "4xx", "5xx", "other"];
const CONNECTION_REJECTION_REASONS: [&str; 3] = ["capacity", "resource_pressure", "proxy_protocol"];
const REQUEST_REJECTION_REASONS: [&str; 5] = [
    "capacity",
    "resource_pressure",
    "access_denied",
    "rate_limited",
    "auth_required",
];
const UPSTREAM_RESULTS: [&str; 5] = [
    "response",
    "timeout",
    "transport_failure",
    "invalid_response",
    "request_failure",
];
const UPSTREAM_REJECTION_REASONS: [&str; 4] = [
    "request_capacity",
    "connection_capacity",
    "no_eligible_target",
    "missing_upstream",
];
const UPSTREAM_RETRY_REASONS: [&str; 5] = [
    "transport_failure",
    "timeout",
    "http_502",
    "http_503",
    "http_504",
];
const RELOAD_RESULTS: [&str; 4] = ["published", "unchanged", "restart_required", "failed"];
const TARGET_HEALTH_STATES: [&str; 4] = [
    "healthy",
    "passive_ejected",
    "half_open",
    "active_unhealthy",
];
const RESOURCE_PRESSURE_REASONS: [&str; 5] = [
    "process_memory",
    "cgroup_memory",
    "open_handles",
    "event_loop_lag",
    "sample_failure",
];
const DURATION_BUCKET_LABELS: [&str; 12] = [
    "0.005", "0.01", "0.025", "0.05", "0.1", "0.25", "0.5", "1", "2.5", "5", "10", "+Inf",
];
const DURATION_BUCKET_MICROSECONDS: [u64; 11] = [
    5_000, 10_000, 25_000, 50_000, 100_000, 250_000, 500_000, 1_000_000, 2_500_000, 5_000_000,
    10_000_000,
];
const PROTOCOL_ERROR_KINDS: [&str; 7] = [
    "http1_wire",
    "http2_wire",
    "request_body_timeout",
    "request_body_io",
    "response_body_timeout",
    "response_body_io",
    "downstream_write_timeout",
];
const DNS_RESULTS: [&str; 8] = [
    "success",
    "saturated",
    "timeout",
    "answer_limit",
    "empty",
    "forbidden",
    "io_failure",
    "cancelled",
];
const CAPACITY_STATES: [&str; 3] = ["configured", "in_use", "available"];
const TUNNEL_BYTE_DIRECTIONS: [&str; 2] = ["downstream_to_upstream", "upstream_to_downstream"];
const PROVIDER_RESOLUTION_CACHE_LOOKUP_RESULTS: [&str; 6] = [
    "hit",
    "stale_hit",
    "negative_hit",
    "miss",
    "coalesced",
    "bypass",
];
const PROXY_CACHE_RESULTS: [&str; 3] = ["hit", "miss", "store"];

pub(super) struct DataPlaneMetrics {
    dimensions: CanonicalMetricDimensions,
    connections_accepted_total: AtomicU64,
    connections_active: AtomicU64,
    connection_rejections_total: [AtomicU64; CONNECTION_REJECTION_REASONS.len()],
    requests_started_total: AtomicU64,
    requests_active: AtomicU64,
    requests_total: [AtomicU64; STATUS_CLASSES.len()],
    request_rejections_total: [AtomicU64; REQUEST_REJECTION_REASONS.len()],
    request_duration: FixedHistogram<{ STATUS_CLASSES.len() }, { DURATION_BUCKET_LABELS.len() }>,
    request_body_bytes_total: AtomicU64,
    response_body_bytes_total: AtomicU64,
    upstream_attempts_total: AtomicU64,
    upstream_results_total: [AtomicU64; UPSTREAM_RESULTS.len()],
    upstream_rejections_total: [AtomicU64; UPSTREAM_REJECTION_REASONS.len()],
    upstream_retries_total: [AtomicU64; UPSTREAM_RETRY_REASONS.len()],
    upstream_selection_contentions_total: AtomicU64,
    upstream_duration: FixedHistogram<{ UPSTREAM_RESULTS.len() }, { DURATION_BUCKET_LABELS.len() }>,
    protocol_errors_total: [AtomicU64; PROTOCOL_ERROR_KINDS.len()],
    dns_lookups_active: AtomicU64,
    dns_results_total: [AtomicU64; DNS_RESULTS.len()],
    reloads_total: [AtomicU64; RELOAD_RESULTS.len()],
    tunnels_opened_total: AtomicU64,
    tunnels_active: AtomicU64,
    tunnels_closed_total: AtomicU64,
    tunnel_shutdowns_total: AtomicU64,
    tunnel_drain_timeouts_total: AtomicU64,
    tunnel_bytes_total: [AtomicU64; TUNNEL_BYTE_DIRECTIONS.len()],
    proxy_cache_total: [AtomicU64; PROXY_CACHE_RESULTS.len()],
}

#[derive(Clone, Copy)]
pub(super) enum ConnectionRejection {
    Capacity = 0,
    ResourcePressure = 1,
    ProxyProtocol = 2,
}

#[derive(Clone, Copy)]
pub(super) enum RequestRejection {
    Capacity = 0,
    ResourcePressure = 1,
    AccessDenied = 2,
    RateLimited = 3,
    AuthRequired = 4,
}

#[derive(Clone, Copy)]
pub(super) enum UpstreamResult {
    Response = 0,
    Timeout = 1,
    TransportFailure = 2,
    InvalidResponse = 3,
    RequestFailure = 4,
}

#[derive(Clone, Copy)]
pub(super) enum UpstreamRejection {
    RequestCapacity = 0,
    ConnectionCapacity = 1,
    NoEligibleTarget = 2,
    MissingUpstream = 3,
}

#[derive(Clone, Copy)]
pub(super) enum UpstreamRetryReason {
    TransportFailure = 0,
    Timeout = 1,
    Http502 = 2,
    Http503 = 3,
    Http504 = 4,
}

#[derive(Clone, Copy)]
pub(super) enum ReloadResult {
    Published = 0,
    Unchanged = 1,
    RestartRequired = 2,
    Failed = 3,
}

#[derive(Clone, Copy)]
pub(super) enum ProtocolErrorKind {
    Http1Wire = 0,
    Http2Wire = 1,
    RequestBodyTimeout = 2,
    RequestBodyIo = 3,
    ResponseBodyTimeout = 4,
    ResponseBodyIo = 5,
    DownstreamWriteTimeout = 6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DnsResult {
    Success = 0,
    Saturated = 1,
    Timeout = 2,
    AnswerLimit = 3,
    Empty = 4,
    Forbidden = 5,
    IoFailure = 6,
    Cancelled = 7,
}

impl DataPlaneMetrics {
    pub(crate) fn record_proxy_cache_hit(&self) {
        saturating_increment(&self.proxy_cache_total[0]);
    }

    pub(crate) fn record_proxy_cache_miss(&self) {
        saturating_increment(&self.proxy_cache_total[1]);
    }

    pub(crate) fn record_proxy_cache_store(&self) {
        saturating_increment(&self.proxy_cache_total[2]);
    }
    pub(super) fn new(dimensions: CanonicalMetricDimensions) -> Arc<Self> {
        Arc::new(Self {
            dimensions,
            connections_accepted_total: AtomicU64::new(0),
            connections_active: AtomicU64::new(0),
            connection_rejections_total: atomic_counters(),
            requests_started_total: AtomicU64::new(0),
            requests_active: AtomicU64::new(0),
            requests_total: atomic_counters(),
            request_rejections_total: atomic_counters(),
            request_duration: FixedHistogram::new(),
            request_body_bytes_total: AtomicU64::new(0),
            response_body_bytes_total: AtomicU64::new(0),
            upstream_attempts_total: AtomicU64::new(0),
            upstream_results_total: atomic_counters(),
            upstream_rejections_total: atomic_counters(),
            upstream_retries_total: atomic_counters(),
            upstream_selection_contentions_total: AtomicU64::new(0),
            upstream_duration: FixedHistogram::new(),
            protocol_errors_total: atomic_counters(),
            dns_lookups_active: AtomicU64::new(0),
            dns_results_total: atomic_counters(),
            reloads_total: atomic_counters(),
            tunnels_opened_total: AtomicU64::new(0),
            tunnels_active: AtomicU64::new(0),
            tunnels_closed_total: AtomicU64::new(0),
            tunnel_shutdowns_total: AtomicU64::new(0),
            tunnel_drain_timeouts_total: AtomicU64::new(0),
            tunnel_bytes_total: atomic_counters(),
            proxy_cache_total: atomic_counters(),
        })
    }

    pub(super) fn record_connection_accepted(&self) {
        saturating_increment(&self.connections_accepted_total);
    }

    pub(super) fn begin_connection(self: &Arc<Self>) -> ConnectionMetricLease {
        saturating_increment(&self.connections_active);
        ConnectionMetricLease {
            metrics: self.clone(),
        }
    }

    pub(super) fn record_connection_rejection(&self, reason: ConnectionRejection) {
        saturating_increment(&self.connection_rejections_total[reason as usize]);
    }

    pub(super) fn begin_request(self: &Arc<Self>) -> RequestMetricLease {
        saturating_increment(&self.requests_started_total);
        saturating_increment(&self.requests_active);
        RequestMetricLease {
            metrics: self.clone(),
            started: Instant::now(),
            status_index: STATUS_CLASSES.len() - 1,
        }
    }

    pub(super) fn observe_response(
        &self,
        response: Response<Body>,
        mut lease: RequestMetricLease,
    ) -> Response<Body> {
        let status_index = status_class_index(response.status().as_u16());
        lease.status_index = status_index;
        saturating_increment(&self.requests_total[status_index]);
        response.map(|body| Body::new(RequestMetricBody::new(body, lease)))
    }

    pub(super) fn record_request_rejection(&self, reason: RequestRejection) {
        saturating_increment(&self.request_rejections_total[reason as usize]);
    }

    pub(super) fn record_request_body_bytes(&self, bytes: usize) {
        saturating_add(&self.request_body_bytes_total, bytes as u64);
    }

    fn record_response_body_bytes(&self, bytes: usize) {
        saturating_add(&self.response_body_bytes_total, bytes as u64);
    }

    pub(super) fn begin_upstream_attempt(self: &Arc<Self>) -> UpstreamMetricLease {
        saturating_increment(&self.upstream_attempts_total);
        UpstreamMetricLease {
            metrics: self.clone(),
            started: Instant::now(),
            result: UpstreamResult::RequestFailure,
        }
    }

    fn record_upstream_result(&self, result: UpstreamResult, duration: Duration) {
        saturating_increment(&self.upstream_results_total[result as usize]);
        self.upstream_duration
            .observe(result as usize, duration, &DURATION_BUCKET_MICROSECONDS);
    }

    pub(super) fn record_upstream_rejection(&self, reason: UpstreamRejection) {
        saturating_increment(&self.upstream_rejections_total[reason as usize]);
    }

    pub(super) fn record_upstream_retry(&self, reason: UpstreamRetryReason) {
        saturating_increment(&self.upstream_retries_total[reason as usize]);
    }

    pub(super) fn record_upstream_selection_contention(&self) {
        saturating_increment(&self.upstream_selection_contentions_total);
    }

    pub(super) fn record_protocol_error(&self, kind: ProtocolErrorKind) {
        saturating_increment(&self.protocol_errors_total[kind as usize]);
    }

    pub(super) fn begin_dns_lookup(self: &Arc<Self>) -> DnsMetricLease {
        saturating_increment(&self.dns_lookups_active);
        DnsMetricLease {
            metrics: self.clone(),
            result: DnsResult::Cancelled,
        }
    }

    pub(super) fn record_dns_result(&self, result: DnsResult) {
        saturating_increment(&self.dns_results_total[result as usize]);
    }

    pub(super) fn record_reload(&self, result: ReloadResult) {
        saturating_increment(&self.reloads_total[result as usize]);
    }

    pub(super) fn tunnel_opened(&self) {
        saturating_increment(&self.tunnels_opened_total);
        saturating_increment(&self.tunnels_active);
    }

    pub(super) fn tunnel_closed(&self) {
        saturating_decrement(&self.tunnels_active);
        saturating_increment(&self.tunnels_closed_total);
    }

    pub(super) fn tunnel_shutdown_started(&self) {
        saturating_increment(&self.tunnel_shutdowns_total);
    }

    pub(super) fn tunnel_drain_timed_out(&self) {
        saturating_increment(&self.tunnel_drain_timeouts_total);
    }

    pub(super) fn record_tunnel_bytes(
        &self,
        downstream_to_upstream: u64,
        upstream_to_downstream: u64,
    ) {
        saturating_add(&self.tunnel_bytes_total[0], downstream_to_upstream);
        saturating_add(&self.tunnel_bytes_total[1], upstream_to_downstream);
    }

    #[cfg(test)]
    pub(super) fn protocol_error_count(&self, kind: ProtocolErrorKind) -> u64 {
        load(&self.protocol_errors_total[kind as usize])
    }

    #[cfg(test)]
    pub(super) fn dns_active(&self) -> u64 {
        load(&self.dns_lookups_active)
    }

    #[cfg(test)]
    pub(super) fn dns_result_count(&self, result: DnsResult) -> u64 {
        load(&self.dns_results_total[result as usize])
    }

    #[cfg(test)]
    pub(super) fn request_body_bytes(&self) -> u64 {
        load(&self.request_body_bytes_total)
    }

    #[cfg(test)]
    pub(super) fn tunnel_bytes(&self, direction: usize) -> u64 {
        load(&self.tunnel_bytes_total[direction])
    }

    pub(super) fn render_prometheus(
        &self,
        runtime: &DataPlaneRuntime,
        provider_resolution_cache: Option<&WebsiteProviderResolutionCacheSnapshot>,
    ) -> String {
        let common = format!(
            "service=\"sdkwork-api-webserver-standalone-gateway\",environment=\"{}\",deployment_profile=\"{}\",runtime_target=\"{}\"",
            self.dimensions.environment,
            self.dimensions.deployment_profile,
            self.dimensions.runtime_target,
        );
        let mut output = String::with_capacity(16 * 1024);
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_health_status",
            "Whether the request data plane is serving.",
            "gauge",
        );
        metric(
            &mut output,
            "sdkwork_web_data_plane_health_status",
            &common,
            1,
        );

        metric_header(
            &mut output,
            "sdkwork_web_data_plane_connections_total",
            "Accepted downstream TCP connections.",
            "counter",
        );
        metric(
            &mut output,
            "sdkwork_web_data_plane_connections_total",
            &common,
            load(&self.connections_accepted_total),
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_connections_active",
            "Currently admitted downstream connections.",
            "gauge",
        );
        metric(
            &mut output,
            "sdkwork_web_data_plane_connections_active",
            &common,
            load(&self.connections_active),
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_connection_rejections_total",
            "Downstream connections rejected before protocol service.",
            "counter",
        );
        labeled_counters(
            &mut output,
            "sdkwork_web_data_plane_connection_rejections_total",
            &common,
            "reason",
            &CONNECTION_REJECTION_REASONS,
            &self.connection_rejections_total,
        );

        metric_header(
            &mut output,
            "sdkwork_web_data_plane_requests_started_total",
            "Requests entering the data-plane handler.",
            "counter",
        );
        metric(
            &mut output,
            "sdkwork_web_data_plane_requests_started_total",
            &common,
            load(&self.requests_started_total),
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_requests_active",
            "Requests retaining response-stream ownership.",
            "gauge",
        );
        metric(
            &mut output,
            "sdkwork_web_data_plane_requests_active",
            &common,
            load(&self.requests_active),
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_requests_total",
            "Responses produced by HTTP status class.",
            "counter",
        );
        labeled_counters(
            &mut output,
            "sdkwork_web_data_plane_requests_total",
            &common,
            "status_class",
            &STATUS_CLASSES,
            &self.requests_total,
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_request_rejections_total",
            "Requests rejected by bounded admission.",
            "counter",
        );
        labeled_counters(
            &mut output,
            "sdkwork_web_data_plane_request_rejections_total",
            &common,
            "reason",
            &REQUEST_REJECTION_REASONS,
            &self.request_rejections_total,
        );
        histogram(
            &mut output,
            "sdkwork_web_data_plane_request_duration_seconds",
            "Full response-stream request duration by HTTP status class.",
            &common,
            "status_class",
            &STATUS_CLASSES,
            &self.request_duration,
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_request_body_bytes_total",
            "Request Body bytes read from downstream clients.",
            "counter",
        );
        metric(
            &mut output,
            "sdkwork_web_data_plane_request_body_bytes_total",
            &common,
            load(&self.request_body_bytes_total),
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_response_body_bytes_total",
            "Response Body bytes yielded to downstream clients.",
            "counter",
        );
        metric(
            &mut output,
            "sdkwork_web_data_plane_response_body_bytes_total",
            &common,
            load(&self.response_body_bytes_total),
        );

        metric_header(
            &mut output,
            "sdkwork_web_data_plane_upstream_attempts_total",
            "Proxy requests submitted to an upstream transport.",
            "counter",
        );
        metric(
            &mut output,
            "sdkwork_web_data_plane_upstream_attempts_total",
            &common,
            load(&self.upstream_attempts_total),
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_upstream_results_total",
            "Upstream attempt outcomes before downstream streaming.",
            "counter",
        );
        labeled_counters(
            &mut output,
            "sdkwork_web_data_plane_upstream_results_total",
            &common,
            "result",
            &UPSTREAM_RESULTS,
            &self.upstream_results_total,
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_upstream_rejections_total",
            "Proxy work rejected before an upstream attempt.",
            "counter",
        );
        labeled_counters(
            &mut output,
            "sdkwork_web_data_plane_upstream_rejections_total",
            &common,
            "reason",
            &UPSTREAM_REJECTION_REASONS,
            &self.upstream_rejections_total,
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_upstream_retries_total",
            "Sequential upstream retries by fixed triggering condition.",
            "counter",
        );
        labeled_counters(
            &mut output,
            "sdkwork_web_data_plane_upstream_retries_total",
            &common,
            "reason",
            &UPSTREAM_RETRY_REASONS,
            &self.upstream_retries_total,
        );
        histogram(
            &mut output,
            "sdkwork_web_data_plane_upstream_duration_seconds",
            "Upstream attempt duration through response Headers or terminal failure.",
            &common,
            "result",
            &UPSTREAM_RESULTS,
            &self.upstream_duration,
        );

        metric_header(
            &mut output,
            "sdkwork_web_data_plane_protocol_errors_total",
            "Normalized downstream protocol and streaming failures.",
            "counter",
        );
        labeled_counters(
            &mut output,
            "sdkwork_web_data_plane_protocol_errors_total",
            &common,
            "kind",
            &PROTOCOL_ERROR_KINDS,
            &self.protocol_errors_total,
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_dns_lookups_active",
            "DNS lookups currently holding resolver capacity.",
            "gauge",
        );
        metric(
            &mut output,
            "sdkwork_web_data_plane_dns_lookups_active",
            &common,
            load(&self.dns_lookups_active),
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_dns_results_total",
            "Terminal DNS lookup outcomes.",
            "counter",
        );
        labeled_counters(
            &mut output,
            "sdkwork_web_data_plane_dns_results_total",
            &common,
            "result",
            &DNS_RESULTS,
            &self.dns_results_total,
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_upstream_selection_contentions_total",
            "Process-lifetime smooth weighted selection lock contentions.",
            "counter",
        );
        metric(
            &mut output,
            "sdkwork_web_data_plane_upstream_selection_contentions_total",
            &common,
            load(&self.upstream_selection_contentions_total),
        );

        let generation = runtime.current();
        let target_health = generation.aggregate_target_health();
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_upstream_targets",
            "Current aggregate upstream target health states.",
            "gauge",
        );
        for (state, value) in TARGET_HEALTH_STATES.iter().zip(target_health) {
            metric_with_label(
                &mut output,
                "sdkwork_web_data_plane_upstream_targets",
                &common,
                "state",
                state,
                value,
            );
        }
        let request_capacity = generation.aggregate_upstream_request_capacity();
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_upstream_request_capacity",
            "Current-generation aggregate upstream request capacity.",
            "gauge",
        );
        for (state, value) in CAPACITY_STATES.iter().zip(request_capacity) {
            metric_with_label(
                &mut output,
                "sdkwork_web_data_plane_upstream_request_capacity",
                &common,
                "state",
                state,
                value,
            );
        }
        let connection_capacity = generation.aggregate_upstream_connection_capacity();
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_upstream_connection_capacity",
            "Current-generation aggregate physical upstream connection capacity.",
            "gauge",
        );
        for (state, value) in CAPACITY_STATES.iter().zip(connection_capacity) {
            metric_with_label(
                &mut output,
                "sdkwork_web_data_plane_upstream_connection_capacity",
                &common,
                "state",
                state,
                value,
            );
        }
        let target_connection_capacity = generation.aggregate_upstream_target_connection_capacity();
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_upstream_target_connection_capacity",
            "Current-generation aggregate explicitly configured target physical connection capacity.",
            "gauge",
        );
        for (state, value) in CAPACITY_STATES.iter().zip(target_connection_capacity) {
            metric_with_label(
                &mut output,
                "sdkwork_web_data_plane_upstream_target_connection_capacity",
                &common,
                "state",
                state,
                value,
            );
        }

        let pressure = runtime.resource_pressure.snapshot();
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_resource_pressure",
            "Whether resource-pressure admission is active.",
            "gauge",
        );
        metric_with_label(
            &mut output,
            "sdkwork_web_data_plane_resource_pressure",
            &common,
            "state",
            "enabled",
            u64::from(pressure.enabled),
        );
        metric_with_label(
            &mut output,
            "sdkwork_web_data_plane_resource_pressure",
            &common,
            "state",
            "active",
            u64::from(pressure.pressured),
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_resource_pressure_reason",
            "Active resource-pressure reasons.",
            "gauge",
        );
        for (reason, active) in RESOURCE_PRESSURE_REASONS.iter().zip(pressure.reasons) {
            metric_with_label(
                &mut output,
                "sdkwork_web_data_plane_resource_pressure_reason",
                &common,
                "reason",
                reason,
                u64::from(active),
            );
        }

        metric_header(
            &mut output,
            "sdkwork_web_data_plane_reloads_total",
            "Runtime configuration publication outcomes.",
            "counter",
        );
        labeled_counters(
            &mut output,
            "sdkwork_web_data_plane_reloads_total",
            &common,
            "result",
            &RELOAD_RESULTS,
            &self.reloads_total,
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_websocket_tunnels_opened_total",
            "WebSocket tunnels accepted by the runtime supervisor.",
            "counter",
        );
        metric(
            &mut output,
            "sdkwork_web_data_plane_websocket_tunnels_opened_total",
            &common,
            load(&self.tunnels_opened_total),
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_websocket_tunnels_active",
            "WebSocket tunnels currently owned by the runtime supervisor.",
            "gauge",
        );
        metric(
            &mut output,
            "sdkwork_web_data_plane_websocket_tunnels_active",
            &common,
            load(&self.tunnels_active),
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_websocket_tunnels_closed_total",
            "WebSocket tunnel tasks that released runtime ownership.",
            "counter",
        );
        metric(
            &mut output,
            "sdkwork_web_data_plane_websocket_tunnels_closed_total",
            &common,
            load(&self.tunnels_closed_total),
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_websocket_shutdowns_total",
            "Runtime shutdown sequences that signalled WebSocket tunnels.",
            "counter",
        );
        metric(
            &mut output,
            "sdkwork_web_data_plane_websocket_shutdowns_total",
            &common,
            load(&self.tunnel_shutdowns_total),
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_websocket_drain_timeouts_total",
            "WebSocket drain deadlines exceeded.",
            "counter",
        );
        metric(
            &mut output,
            "sdkwork_web_data_plane_websocket_drain_timeouts_total",
            &common,
            load(&self.tunnel_drain_timeouts_total),
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_websocket_bytes_total",
            "WebSocket tunnel bytes copied to successful tunnel completion.",
            "counter",
        );
        labeled_counters(
            &mut output,
            "sdkwork_web_data_plane_websocket_bytes_total",
            &common,
            "direction",
            &TUNNEL_BYTE_DIRECTIONS,
            &self.tunnel_bytes_total,
        );
        metric_header(
            &mut output,
            "sdkwork_web_data_plane_proxy_cache_total",
            "HTTP proxy response cache lookups and stores (nginx proxy_cache).",
            "counter",
        );
        labeled_counters(
            &mut output,
            "sdkwork_web_data_plane_proxy_cache_total",
            &common,
            "result",
            &PROXY_CACHE_RESULTS,
            &self.proxy_cache_total,
        );
        if let Some(snapshot) = provider_resolution_cache {
            append_provider_resolution_cache_metrics(&mut output, &common, snapshot);
        }
        output
    }
}

fn append_provider_resolution_cache_metrics(
    output: &mut String,
    common: &str,
    snapshot: &WebsiteProviderResolutionCacheSnapshot,
) {
    metric_header(
        output,
        "sdkwork_web_data_plane_provider_resolution_cache_capacity_entries",
        "Configured maximum Provider resolution cache entries and in-flight slots.",
        "gauge",
    );
    metric(
        output,
        "sdkwork_web_data_plane_provider_resolution_cache_capacity_entries",
        common,
        usize_metric(snapshot.maximum_entries),
    );
    metric_header(
        output,
        "sdkwork_web_data_plane_provider_resolution_cache_entries",
        "Provider resolution metadata entries currently retained.",
        "gauge",
    );
    metric(
        output,
        "sdkwork_web_data_plane_provider_resolution_cache_entries",
        common,
        usize_metric(snapshot.entries),
    );
    metric_header(
        output,
        "sdkwork_web_data_plane_provider_resolution_cache_in_flight",
        "Provider resolution single-flight operations currently active.",
        "gauge",
    );
    metric(
        output,
        "sdkwork_web_data_plane_provider_resolution_cache_in_flight",
        common,
        usize_metric(snapshot.in_flight),
    );
    metric_header(
        output,
        "sdkwork_web_data_plane_provider_resolution_cache_lookups_total",
        "Provider resolution cache lookup outcomes.",
        "counter",
    );
    for (result, value) in PROVIDER_RESOLUTION_CACHE_LOOKUP_RESULTS.iter().zip([
        snapshot.hits,
        snapshot.stale_hits,
        snapshot.negative_hits,
        snapshot.misses,
        snapshot.coalesced,
        snapshot.bypasses,
    ]) {
        metric_with_label(
            output,
            "sdkwork_web_data_plane_provider_resolution_cache_lookups_total",
            common,
            "result",
            result,
            value,
        );
    }
    for (name, help, value) in [
        (
            "sdkwork_web_data_plane_provider_resolution_cache_writes_total",
            "Provider resolution metadata and negative entries written.",
            snapshot.writes,
        ),
        (
            "sdkwork_web_data_plane_provider_resolution_cache_evictions_total",
            "Provider resolution entries evicted by the LRU capacity bound.",
            snapshot.evictions,
        ),
        (
            "sdkwork_web_data_plane_provider_resolution_cache_revalidations_total",
            "Positive stale Provider resolutions revalidated in the background.",
            snapshot.revalidations,
        ),
        (
            "sdkwork_web_data_plane_provider_resolution_cache_invalidations_total",
            "Provider resolution entries or scopes processed by event invalidation.",
            snapshot.invalidations,
        ),
    ] {
        metric_header(output, name, help, "counter");
        metric(output, name, common, value);
    }
}

fn usize_metric(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

pub(super) struct ConnectionMetricLease {
    metrics: Arc<DataPlaneMetrics>,
}

impl Drop for ConnectionMetricLease {
    fn drop(&mut self) {
        saturating_decrement(&self.metrics.connections_active);
    }
}

pub(super) struct RequestMetricLease {
    metrics: Arc<DataPlaneMetrics>,
    started: Instant,
    status_index: usize,
}

impl Drop for RequestMetricLease {
    fn drop(&mut self) {
        saturating_decrement(&self.metrics.requests_active);
        self.metrics.request_duration.observe(
            self.status_index,
            self.started.elapsed(),
            &DURATION_BUCKET_MICROSECONDS,
        );
    }
}

pub(super) struct UpstreamMetricLease {
    metrics: Arc<DataPlaneMetrics>,
    started: Instant,
    result: UpstreamResult,
}

impl UpstreamMetricLease {
    pub(super) fn finish(mut self, result: UpstreamResult) {
        self.result = result;
    }
}

impl Drop for UpstreamMetricLease {
    fn drop(&mut self) {
        self.metrics
            .record_upstream_result(self.result, self.started.elapsed());
    }
}

pub(super) struct DnsMetricLease {
    metrics: Arc<DataPlaneMetrics>,
    result: DnsResult,
}

impl DnsMetricLease {
    pub(super) fn finish(mut self, result: DnsResult) {
        self.result = result;
    }
}

impl Drop for DnsMetricLease {
    fn drop(&mut self) {
        saturating_decrement(&self.metrics.dns_lookups_active);
        self.metrics.record_dns_result(self.result);
    }
}

struct RequestMetricBody {
    inner: SyncWrapper<Pin<Box<Body>>>,
    lease: Option<RequestMetricLease>,
    remaining_hint: SizeHint,
    ended: bool,
}

impl RequestMetricBody {
    fn new(inner: Body, lease: RequestMetricLease) -> Self {
        let remaining_hint = inner.size_hint();
        let ended = inner.is_end_stream();
        Self {
            inner: SyncWrapper::new(Box::pin(inner)),
            lease: Some(lease),
            remaining_hint,
            ended,
        }
    }
}

impl HttpBody for RequestMetricBody {
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let polled = {
            let mut inner = self.inner.get_mut().as_mut();
            let frame = inner.as_mut().poll_frame(context);
            let ended = inner.as_ref().is_end_stream();
            let remaining_hint = inner.as_ref().size_hint();
            (frame, ended, remaining_hint)
        };
        match polled {
            (Poll::Pending, _, _) => Poll::Pending,
            (Poll::Ready(None), _, _) => {
                self.ended = true;
                self.remaining_hint = SizeHint::with_exact(0);
                self.lease.take();
                Poll::Ready(None)
            }
            (Poll::Ready(Some(Err(error))), _, _) => {
                self.ended = true;
                self.remaining_hint = SizeHint::with_exact(0);
                let error = io::Error::other(error);
                let kind = if error_chain_has_io_kind(&error, io::ErrorKind::TimedOut) {
                    ProtocolErrorKind::ResponseBodyTimeout
                } else {
                    ProtocolErrorKind::ResponseBodyIo
                };
                if let Some(lease) = self.lease.as_ref() {
                    lease.metrics.record_protocol_error(kind);
                }
                self.lease.take();
                Poll::Ready(Some(Err(error)))
            }
            (Poll::Ready(Some(Ok(frame))), ended, remaining_hint) => {
                if let Some(data) = frame.data_ref() {
                    if let Some(lease) = self.lease.as_ref() {
                        lease.metrics.record_response_body_bytes(data.len());
                    }
                }
                self.ended = ended;
                self.remaining_hint = remaining_hint;
                if self.ended {
                    self.lease.take();
                }
                Poll::Ready(Some(Ok(frame)))
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.ended
    }

    fn size_hint(&self) -> SizeHint {
        self.remaining_hint
    }
}

fn atomic_counters<const N: usize>() -> [AtomicU64; N] {
    std::array::from_fn(|_| AtomicU64::new(0))
}

fn saturating_increment(counter: &AtomicU64) {
    saturating_add(counter, 1);
}

fn saturating_add(counter: &AtomicU64, value: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(value))
    });
}

fn saturating_decrement(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_sub(1))
    });
}

fn load(counter: &AtomicU64) -> u64 {
    counter.load(Ordering::Relaxed)
}

fn metric_header(output: &mut String, name: &str, help: &str, metric_type: &str) {
    let _ = writeln!(output, "# HELP {name} {help}");
    let _ = writeln!(output, "# TYPE {name} {metric_type}");
}

fn metric(output: &mut String, name: &str, common: &str, value: u64) {
    let _ = writeln!(output, "{name}{{{common}}} {value}");
}

fn metric_with_label(
    output: &mut String,
    name: &str,
    common: &str,
    label_name: &str,
    label_value: &str,
    value: u64,
) {
    let _ = writeln!(
        output,
        "{name}{{{common},{label_name}=\"{label_value}\"}} {value}"
    );
}

fn labeled_counters<const N: usize>(
    output: &mut String,
    name: &str,
    common: &str,
    label_name: &str,
    label_values: &[&str; N],
    counters: &[AtomicU64; N],
) {
    for (label_value, counter) in label_values.iter().zip(counters) {
        metric_with_label(output, name, common, label_name, label_value, load(counter));
    }
}

fn histogram<const SERIES: usize, const BUCKETS: usize>(
    output: &mut String,
    name: &str,
    help: &str,
    common: &str,
    series_label_name: &str,
    series_label_values: &[&str; SERIES],
    histogram: &FixedHistogram<SERIES, BUCKETS>,
) {
    debug_assert_eq!(BUCKETS, DURATION_BUCKET_LABELS.len());
    metric_header(output, name, help, "histogram");
    for (series, series_label_value) in series_label_values.iter().enumerate() {
        for (bucket, upper_bound) in DURATION_BUCKET_LABELS.iter().enumerate() {
            let _ = writeln!(
                output,
                "{name}_bucket{{{common},{series_label_name}=\"{series_label_value}\",le=\"{upper_bound}\"}} {}",
                histogram.cumulative_bucket(series, bucket),
            );
        }
        let sum = format_microseconds_as_seconds(histogram.sum_microseconds(series));
        let _ = writeln!(
            output,
            "{name}_sum{{{common},{series_label_name}=\"{series_label_value}\"}} {sum}"
        );
        let _ = writeln!(
            output,
            "{name}_count{{{common},{series_label_name}=\"{series_label_value}\"}} {}",
            histogram.count(series),
        );
    }
}

fn format_microseconds_as_seconds(microseconds: u64) -> String {
    format!(
        "{}.{:06}",
        microseconds / 1_000_000,
        microseconds % 1_000_000
    )
}

fn status_class_index(status: u16) -> usize {
    match status / 100 {
        1..=5 => usize::from(status / 100 - 1),
        _ => STATUS_CLASSES.len() - 1,
    }
}

fn error_chain_has_io_kind(error: &(dyn StdError + 'static), kind: io::ErrorKind) -> bool {
    let mut current = Some(error);
    while let Some(error) = current {
        if error
            .downcast_ref::<io::Error>()
            .is_some_and(|error| error.kind() == kind)
        {
            return true;
        }
        current = error.source();
    }
    false
}

#[cfg(test)]
mod tests {
    use std::{io, sync::atomic::Ordering};

    use axum::{
        body::Body,
        http::{Response, StatusCode},
    };
    use bytes::Bytes;
    use http_body::Frame;
    use http_body_util::{channel::Channel, BodyExt};
    use sdkwork_webserver_delivery_runtime::WebsiteProviderResolutionCacheSnapshot;

    use super::{
        append_provider_resolution_cache_metrics, DataPlaneMetrics, ProtocolErrorKind,
        ReloadResult, RequestRejection, UpstreamRejection, UpstreamResult, UpstreamRetryReason,
        PROVIDER_RESOLUTION_CACHE_LOOKUP_RESULTS,
    };
    use crate::metric_dimensions::CanonicalMetricDimensions;

    #[test]
    fn provider_resolution_cache_metrics_have_fixed_labels_and_complete_counters() {
        let mut output = String::new();
        append_provider_resolution_cache_metrics(
            &mut output,
            "service=\"test\"",
            &WebsiteProviderResolutionCacheSnapshot {
                maximum_entries: 16_384,
                entries: 12,
                in_flight: 2,
                hits: 11,
                stale_hits: 3,
                negative_hits: 5,
                misses: 7,
                writes: 9,
                evictions: 1,
                coalesced: 4,
                bypasses: 2,
                revalidations: 3,
                invalidations: 6,
            },
        );

        assert!(output.contains(
            "sdkwork_web_data_plane_provider_resolution_cache_capacity_entries{service=\"test\"} 16384"
        ));
        assert!(output.contains(
            "sdkwork_web_data_plane_provider_resolution_cache_lookups_total{service=\"test\",result=\"negative_hit\"} 5"
        ));
        assert!(output.contains(
            "sdkwork_web_data_plane_provider_resolution_cache_invalidations_total{service=\"test\"} 6"
        ));
        assert_eq!(
            output
                .lines()
                .filter(|line| line
                    .starts_with("sdkwork_web_data_plane_provider_resolution_cache_lookups_total{"))
                .count(),
            PROVIDER_RESOLUTION_CACHE_LOOKUP_RESULTS.len()
        );
    }

    #[test]
    fn request_and_rejection_storage_has_fixed_atomic_cardinality() {
        let metrics = DataPlaneMetrics::new(CanonicalMetricDimensions {
            environment: "test".to_owned(),
            deployment_profile: "standalone".to_owned(),
            runtime_target: "test-runner".to_owned(),
        });
        let lease = metrics.begin_request();
        let response = metrics.observe_response(Response::new(Body::empty()), lease);
        assert_eq!(metrics.requests_active.load(Ordering::Relaxed), 1);
        drop(response);
        assert_eq!(metrics.requests_active.load(Ordering::Relaxed), 0);
        metrics.record_request_rejection(RequestRejection::Capacity);
        metrics.record_upstream_rejection(UpstreamRejection::NoEligibleTarget);
        assert_eq!(metrics.requests_total.len(), 6);
        assert_eq!(metrics.request_rejections_total.len(), 5);
        assert_eq!(metrics.upstream_rejections_total.len(), 4);
    }

    #[test]
    fn lifecycle_counters_saturate_and_fixed_outcomes_do_not_allocate_series() {
        let metrics = DataPlaneMetrics::new(CanonicalMetricDimensions::default());
        metrics
            .connections_accepted_total
            .store(u64::MAX, Ordering::Relaxed);
        metrics.record_connection_accepted();
        assert_eq!(
            metrics.connections_accepted_total.load(Ordering::Relaxed),
            u64::MAX
        );

        metrics.record_reload(ReloadResult::Published);
        metrics.record_reload(ReloadResult::Unchanged);
        metrics.record_reload(ReloadResult::RestartRequired);
        metrics.record_reload(ReloadResult::Failed);
        metrics
            .begin_upstream_attempt()
            .finish(UpstreamResult::Timeout);
        for reason in [
            UpstreamRetryReason::TransportFailure,
            UpstreamRetryReason::Timeout,
            UpstreamRetryReason::Http502,
            UpstreamRetryReason::Http503,
            UpstreamRetryReason::Http504,
        ] {
            metrics.record_upstream_retry(reason);
        }
        metrics.record_upstream_selection_contention();
        metrics.record_upstream_selection_contention();
        metrics.tunnel_opened();
        metrics.tunnel_shutdown_started();
        metrics.tunnel_closed();
        metrics.tunnel_drain_timed_out();

        assert!(metrics
            .reloads_total
            .iter()
            .all(|value| value.load(Ordering::Relaxed) == 1));
        assert_eq!(metrics.upstream_attempts_total.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.upstream_results_total[1].load(Ordering::Relaxed), 1);
        assert!(metrics
            .upstream_retries_total
            .iter()
            .all(|value| value.load(Ordering::Relaxed) == 1));
        assert_eq!(
            metrics
                .upstream_selection_contentions_total
                .load(Ordering::Relaxed),
            2
        );
        assert_eq!(metrics.tunnels_opened_total.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.tunnels_active.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.tunnels_closed_total.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.tunnel_shutdowns_total.load(Ordering::Relaxed), 1);
        assert_eq!(
            metrics.tunnel_drain_timeouts_total.load(Ordering::Relaxed),
            1
        );
    }

    #[tokio::test]
    async fn response_stream_owns_request_lifetime_and_counts_each_data_frame() {
        let metrics = DataPlaneMetrics::new(CanonicalMetricDimensions::default());
        let (mut sender, channel) = Channel::<Bytes, io::Error>::new(2);
        sender
            .try_send(Frame::data(Bytes::from_static(b"ab")))
            .expect("queue first response frame");
        sender
            .try_send(Frame::data(Bytes::from_static(b"cde")))
            .expect("queue second response frame");
        drop(sender);

        let lease = metrics.begin_request();
        let response = metrics.observe_response(
            Response::builder()
                .status(StatusCode::CREATED)
                .body(Body::new(channel))
                .expect("response"),
            lease,
        );
        assert_eq!(metrics.requests_active.load(Ordering::Relaxed), 1);

        let mut body = response.into_body();
        let mut frames = 0;
        while let Some(frame) = body.frame().await {
            frame.expect("valid response frame");
            frames += 1;
        }
        assert_eq!(frames, 2);
        assert_eq!(metrics.response_body_bytes_total.load(Ordering::Relaxed), 5);
        assert_eq!(metrics.requests_active.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.request_duration.count(1), 1);
    }

    #[tokio::test]
    async fn response_stream_errors_use_fixed_timeout_and_io_categories() {
        let metrics = DataPlaneMetrics::new(CanonicalMetricDimensions::default());
        for (error, expected) in [
            (
                io::Error::new(io::ErrorKind::TimedOut, "test timeout"),
                ProtocolErrorKind::ResponseBodyTimeout,
            ),
            (
                io::Error::new(io::ErrorKind::ConnectionReset, "test reset"),
                ProtocolErrorKind::ResponseBodyIo,
            ),
        ] {
            let (sender, channel) = Channel::<Bytes, io::Error>::new(1);
            sender.abort(error);
            let lease = metrics.begin_request();
            let response = metrics.observe_response(Response::new(Body::new(channel)), lease);
            response
                .into_body()
                .frame()
                .await
                .expect("error frame")
                .expect_err("response body must fail");
            assert_eq!(metrics.protocol_error_count(expected), 1);
        }
        assert_eq!(metrics.requests_active.load(Ordering::Relaxed), 0);
    }
}
