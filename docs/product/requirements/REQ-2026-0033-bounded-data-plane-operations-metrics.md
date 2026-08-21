# REQ-2026-0033 Bounded Data-Plane Operations Metrics

```yaml
id: REQ-2026-0033
title: Expose real bounded data-plane runtime metrics on an isolated operations listener
owner: sdkwork-webserver
status: accepted
source: production-observability-correctness
problem: The Rust request data plane has no scrapeable runtime-owned telemetry. Management HTTP metrics cannot report accepted or rejected sockets, streaming request lifetime, proxy saturation, target health, resource pressure, reload outcomes, or WebSocket tunnel lifecycle, and mounting these diagnostics on tenant virtual hosts would violate the operations-plane boundary.
goals:
  - Add one runtime-owned metric registry whose storage is a fixed set of saturating atomics with no request-derived map or queue.
  - Measure accepted, active, and rejected connections; started, active, and response-classified requests; upstream attempts/results/rejections; aggregate target health; resource-pressure state/reasons; reload outcomes; and WebSocket tunnel lifecycle.
  - Expose the registry through a separate host operations listener using canonical /healthz, /livez, /readyz, and /metrics routes.
  - Keep the operations listener disabled unless an operator explicitly configures a loopback socket.
  - Prove that tenant HTTP/HTTPS virtual hosts never acquire a /metrics route from this feature.
non_goals:
  - Public, non-loopback, mTLS, IAM, Unix-domain-socket, or named-pipe operations exposure.
  - Per-tenant, per-domain, raw path, client IP, upstream address, revision hash, certificate, request id, or trace id metric labels.
  - Histograms, OpenTelemetry export, access logs, traces, dashboards, alert rules, autoscaling policy, or long-term metric storage.
  - Claiming 100000-connection capacity, 24-hour soak stability, allocator-enforced OOM immunity, or cluster-global metric aggregation.
  - Changing the public management HTTP metric contract introduced by REQ-2026-0032.
users:
  - node operators
  - site reliability engineers
acceptance_criteria:
  - A real data-plane request changes runtime request and connection metrics exposed by the operations listener.
  - Active request and connection gauges retain ownership through streaming response and upgraded connection lifetimes and return to zero on completion or cancellation.
  - Capacity and resource-pressure rejection counters use fixed reason labels and never allocate a value derived from a request.
  - Upstream attempt, result, and rejection labels are finite enums; target health is aggregated into a fixed state vocabulary without upstream URL or id labels.
  - Resource-pressure gauges expose only a fixed enabled/active state and fixed reason vocabulary.
  - Reload and tunnel metrics use fixed outcome/state vocabularies and saturating counters.
  - Metric rendering has constant series cardinality and does not retain scrape history.
  - Operations configuration rejects non-loopback binds and non-canonical environment, deployment_profile, or runtime_target values before binding.
  - /healthz, /livez, /readyz, and /metrics are available on the operations listener, while /metrics on an application listener follows only authored application routing and returns no implicit telemetry.
non_functional_requirements:
  security: The unauthenticated initial operations profile is loopback-only and fail-closed; public or remote exposure requires a later reviewed mTLS/IAM design.
  privacy: Metrics contain only canonical process dimensions and fixed aggregate states, with no tenant, user, host, path, address, credential, certificate, request, trace, or payload values.
  performance: Request and connection hot paths perform only fixed saturating atomic updates; rendering emits a constant number of series and aggregates target health directly from the current immutable generation.
  reliability: Metrics failure or saturation cannot reject business traffic, allocate an unbounded series, block on an asynchronous exporter, or retain a configuration generation after a scrape.
affected_surfaces:
  - request-data-plane
  - host-operations-plane
  - observability
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - HEALTH_CHECK_SPEC.md
    - OBSERVABILITY_SPEC.md
    - PERFORMANCE_SPEC.md
    - SECURITY_SPEC.md
    - RUST_CODE_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-api-webserver-standalone-gateway data_plane::metrics
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test data_plane_metrics
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Design Decision

The data-plane registry is not the management `HttpMetricsRegistry`. It owns a fixed metric vocabulary and uses only saturating atomics; no request, route, listener, upstream, target, revision, host, or error string is inserted into persistent metric state. Target health is counted from the current immutable generation during a scrape and emitted only as aggregate fixed states.

The host operations listener is an explicit runtime option, not application-owned Web Server configuration. It uses `sdkwork-web-bootstrap::service_router` for the canonical health/readiness routes and its approved custom-metrics escape hatch for the runtime registry. The initial unauthenticated profile accepts loopback addresses only and remains disabled when no bind is configured.

## Compatibility Boundary

This requirement adds new Prometheus names. Their names, types, and labels become operational contracts after acceptance. It does not rename or merge the management metrics from REQ-2026-0032, expose metrics through tenant routes, or claim cluster-wide aggregation.

## Acceptance Evidence

Accepted on 2026-07-16 with the following evidence:

- The runtime registry stores only a fixed set of saturating `AtomicU64` counters and gauges. Unit tests force a counter to `u64::MAX`, prove it does not wrap, prove request leases release active ownership on Body drop, and cover every fixed reload, upstream, and tunnel outcome slot.
- One real HTTP proxy test starts separate application and loopback operations listeners. `/healthz`, `/livez`, `/readyz`, and `/metrics` succeed only on operations; application `/metrics` remains an authored-route `404` and contains no runtime telemetry.
- The real upstream sends response Headers and pauses its Body. During the pause the scrape reports one active request, one upstream attempt, one response outcome, and one healthy aggregate target; after Body consumption the active request gauge returns to zero.
- The same test holds two application sockets under a limit of two and proves the next accepted socket increments the fixed `reason="capacity"` rejection counter without creating a task. It then holds one streaming request under a request limit of one and proves the excess request returns bounded `503` and increments the fixed request-capacity counter.
- The operations listener itself uses a non-queuing 32-connection hard limit, 16 KiB HTTP/1 Header buffer, five-second Header/request deadlines, 60-second connection lifetime, one-second supervised drain, and no HTTP/2 multiplexing. A real 33rd loopback socket is closed promptly; releasing capacity restores `/healthz`.
- Scrape assertions prove the output does not contain the application domain, loopback address, route id, or any request-derived label. Target health is aggregated into four fixed states directly from the current immutable generation, and resource pressure uses two fixed state gauges plus five fixed reason gauges.
- Standalone gateway verification passes with 62 library tests, 55 primary data-plane tests, 1 focused metrics test, 4 raw HTTP/1 tests, 1 resource-pressure test, 4 active-health tests, 5 physical-connection tests, 4 response-Header tests, 2 weighted-selection tests, and 9 WebSocket tests: 147 tests total.
- The environment-independent `pnpm.cmd verify` checks and `cargo clippy --workspace --all-targets -- -D warnings` passed with the isolated F-drive target. PostgreSQL lifecycle was not executed in this requirement's acceptance run; this requirement changes no database behavior, and REQ-2026-0004/0049 own the database evidence.
- SDKWork pagination, API operation pattern, API response envelope, app SDK consumer import, application layering, Rust backend composition, repository documentation, topology, cloud-gateway, formatting, and whitespace checks pass.
