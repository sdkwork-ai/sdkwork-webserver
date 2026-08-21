# REQ-2026-0034 Bounded Data-Plane RED And Capacity Metrics

```yaml
id: REQ-2026-0034
title: Complete bounded data-plane RED, byte, protocol, DNS, and capacity metrics
owner: sdkwork-webserver
status: accepted
source: production-observability-correctness
problem: REQ-2026-0033 exposes real fixed-cardinality lifecycle counters, but operators still cannot measure full streaming request latency, upstream response-header latency, transferred Body/tunnel bytes, normalized protocol failures, DNS saturation/results, or current upstream request and physical-connection capacity. Reporting inferred idle-pool values or handler-only latency would be misleading.
goals:
  - Measure request duration through final response Body completion, error, cancellation, or drop using fixed SDKWork seconds buckets and fixed status-class labels.
  - Measure upstream attempt duration through response Headers or terminal pre-response failure using the same fixed seconds buckets and fixed result labels.
  - Count request Body, response Body, and WebSocket directional bytes on their streaming paths without collecting payloads.
  - Count HTTP/1 wire, HTTP/2 wire, request Body timeout/I/O, response Body timeout/I/O, and downstream write-timeout events with a fixed error-kind vocabulary.
  - Measure DNS active lookups and fixed terminal results, including saturation, timeout, answer overflow, empty answers, forbidden answers, I/O failure, cancellation, and success.
  - Expose aggregate configured, in-use, and available request-admission and physical-connection capacity from the current immutable upstream generation.
  - Preserve constant metric-series cardinality and saturating arithmetic under arbitrary request and failure inputs.
non_goals:
  - Per-route, per-host, per-listener, per-upstream, per-target, per-address, tenant, user, request, trace, certificate, revision, or error-message labels.
  - Claiming Hyper idle-pool occupancy, DNS cache freshness, authoritative TTL, connection handshake latency, kernel bytes, or packet-level delivery when no authoritative runtime signal exists.
  - OpenTelemetry export, exemplars, traces, dashboards, alerts, remote authenticated operations, long-term storage, or cluster aggregation.
  - Changing application Web Server configuration, API/SDK contracts, management HTTP metrics, or operations-listener exposure policy.
users:
  - node operators
  - site reliability engineers
acceptance_criteria:
  - Request histogram observations remain active until streamed response completion/cancellation and render cumulative fixed buckets, sum, and count by fixed status class.
  - Upstream histograms render cumulative fixed buckets, sum, and count by the existing fixed result vocabulary.
  - Request/response/tunnel bytes use saturating counters and match real multi-frame traffic without retaining payloads.
  - Malformed HTTP/1 and HTTP/2 wire input, Body timeout/I/O, and downstream write timeout increment only fixed documented error-kind series.
  - DNS lookup active gauge returns to zero for success, failure, timeout, and cancellation; every terminal path has one fixed result.
  - Current-generation upstream request and physical-connection capacity gauges satisfy configured = in_use + available at scrape time, subject only to concurrent atomic sampling skew documented by Prometheus gauge semantics.
  - No unbounded Map, label registry, waiter queue, per-observation allocation, or request-derived persistent state is added.
  - Existing REQ-2026-0033 operations isolation and 32-connection listener bound remain intact.
non_functional_requirements:
  security: Metrics expose only canonical dimensions and fixed aggregate labels; no request content, topology identity, raw error, or secret is emitted.
  privacy: No tenant, user, domain, path, IP, certificate, request, trace, or payload value is retained or rendered.
  performance: Hot-path observations perform fixed atomic operations and bounded bucket scans; Body/tunnel data is counted by existing frame/copy lengths without a second payload copy.
  reliability: Counter and duration sums saturate instead of wrapping; leases finalize on drop and cannot leave active gauges permanently elevated after cancellation.
affected_surfaces:
  - request-data-plane
  - host-operations-plane
  - observability
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - OBSERVABILITY_SPEC.md
    - PERFORMANCE_SPEC.md
    - HEALTH_CHECK_SPEC.md
    - SECURITY_SPEC.md
    - RUST_CODE_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-api-webserver-standalone-gateway data_plane::metrics
  - cargo test -p sdkwork-api-webserver-standalone-gateway data_plane::dns
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test data_plane_metrics
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Design Decision

Both histograms use the stable SDKWork seconds buckets `0.005`, `0.01`, `0.025`, `0.05`, `0.1`, `0.25`, `0.5`, `1`, `2.5`, `5`, and `10`, plus `+Inf`. Runtime storage keeps one exclusive bucket counter per observation and renders cumulative Prometheus buckets; sums are saturating integer microseconds rendered as seconds. Request histograms multiply only by the six existing status classes, and upstream histograms only by the five existing result values.

Protocol and DNS labels are finite enums. Upstream capacity is aggregated from current-generation semaphores at scrape time. Hyper idle-pool occupancy is deliberately omitted because the selected client API exposes configuration and physical permits but no authoritative idle/active split; inventing it would violate the evidence-first rule.

## Compatibility Boundary

This requirement adds Prometheus metric names and fixed labels. It does not rename REQ-2026-0032/0033 names, expose tenant routes, or change application configuration. After acceptance these names, types, bucket boundaries, and label vocabularies are operational contracts.

## Acceptance Evidence

Accepted on 2026-07-17 with the following evidence:

- Request duration is owned by an RAII lease from data-plane middleware entry through response Body completion, error, cancellation, or drop. Six fixed status-class series use 11 documented finite seconds buckets plus `+Inf`; exclusive atomic storage renders cumulative Prometheus buckets, saturating integer-microsecond sums, and counts.
- Upstream attempts use one RAII lease from client submission through response Headers or terminal pre-response failure. Cancellation defaults to the fixed `request_failure` outcome, so every started attempt records exactly one result and one histogram observation.
- Request Body, response Body, and successful bidirectional WebSocket copy paths add actual frame/copy lengths to saturating counters without collecting or copying payloads. A bidirectional in-memory tunnel test proves 10 downstream-to-upstream and 8 upstream-to-downstream bytes. Tokio does not return partial byte totals on copy error, so error-path partial tunnel bytes are deliberately not fabricated.
- HTTP/1 wire, HTTP/2 wire, request Body timeout/I/O, response Body timeout/I/O, and downstream write-deadline failures map only to seven fixed categories. Wire and write-timeout guards record at most once per connection; underlying socket I/O is not mislabeled as a parser or deadline violation.
- DNS tests cover success, non-queuing saturation, timeout, answer overflow, empty answer, forbidden answer, I/O failure, and cancellation. Active lookup capacity increments only after permit acquisition and returns to zero after every terminal path, including task abort.
- Current-generation request and physical-connection capacity is read from the same Semaphores that enforce `maxInFlightRequests` and `maxConnections`. Scrapes expose only aggregate `configured`, `in_use`, and `available` gauges; tests prove the capacity equation while a real streaming response retains both permits. No Hyper idle-pool value is inferred.
- The fixed histogram and registry tests force bucket, count, sum, and lifecycle counters to `u64::MAX` and prove saturation without wrap. Sub-microsecond excess above a bucket boundary is classified into the next bucket rather than rounded down.
- A real proxy plus separate operations-listener test proves upstream latency is recorded at response Headers while request latency remains open until Body completion. It also proves DNS, byte, protocol, and capacity metric names render with canonical dimensions and fixed labels, while tenant domain, route id, path, address, request, and trace values remain absent.
- Standalone gateway verification passes with 72 library tests, 55 primary data-plane tests, 1 focused metrics test, 4 raw HTTP/1 tests, 1 resource-pressure test, 4 active-health tests, 5 physical-connection tests, 4 response-Header tests, 2 weighted-selection tests, and 9 WebSocket tests: 157 tests total.
- `cargo clippy -p sdkwork-api-webserver-standalone-gateway --all-targets -- -D warnings`, full `pnpm.cmd verify`, full-workspace strict Clippy, formatting, whitespace, pagination, API operation/envelope, SDK import, layering, Rust composition, route collision, topology, documentation, database-framework, cloud-gateway, and database lifecycle checks pass with the isolated F-drive target.
- PostgreSQL lifecycle remains explicitly unverified because `SDKWORK_DATABASE_TEST_POSTGRES_URL` is not configured. This requirement changes no database contract and does not claim PostgreSQL evidence.
