# REQ-2026-0035 Bounded Safe Upstream Retries

```yaml
id: REQ-2026-0035
title: Add explicit bounded upstream retries without unsafe request replay
owner: sdkwork-webserver
status: accepted
source: nginx-proxy-next-upstream-commercial-readiness
problem: The proxy has bounded admission and passive/active target health but always performs one upstream attempt. Transient connect, timeout, or selected gateway-status failures cannot fail over to another healthy target, while naive retry would duplicate non-idempotent or streaming requests and violate OOM/concurrency guarantees.
goals:
  - Add an opt-in per-upstream retry policy with a maximum of eight attempts and a finite total retry timeout.
  - Retry only fixed configured transport, timeout, 502, 503, and 504 conditions.
  - Retry only bodyless idempotent HTTP methods and never replay POST, PATCH, unknown/streaming Bodies, Trailers, or WebSocket upgrades.
  - Select a different currently eligible target for each attempt using a fixed-size attempted-target bitmap.
  - Reuse one upstream request-admission permit across all attempts and preserve physical connection, DNS, TLS, response-Header, passive-health, and whole-request bounds per attempt.
  - Expose retry decisions through fixed-cardinality saturating metrics without target, route, host, or error labels.
non_goals:
  - Retrying non-idempotent requests, buffered request replay, disk spooling, hedging, parallel attempts, HTTP/2 extended CONNECT, or WebSocket retry.
  - Exact support for every Nginx `proxy_next_upstream` token, shared-zone retry budgets, cluster-global circuit state, or cross-node retry coordination.
  - Retrying after downstream response Headers have been committed.
users:
  - web server application authors
  - node operators
acceptance_criteria:
  - Omitted retry config preserves exactly one upstream attempt.
  - Config validation rejects zero/unbounded attempts, incoherent timeout, empty/duplicate conditions, and attempts greater than configured target count.
  - A bodyless idempotent request can move from one failed target to one different healthy target and return the successful response.
  - POST, PATCH, requests with any Body framing/content, and WebSocket handshakes perform one attempt even when retry is configured.
  - Each failed attempt updates the existing passive-health state exactly once; local request/connection saturation is never treated as a retryable target failure.
  - Total retry time and attempt count are finite; exhaustion returns the final upstream response or the existing bounded local gateway failure.
  - Attempted-target tracking uses fixed stack storage and no queue, dynamic set, weight-expanded schedule, Body collection, or payload copy.
  - Metrics use only fixed retry-reason labels and saturating counters.
non_functional_requirements:
  security: Retry cannot bypass SSRF address policy, TLS identity verification, Header validation, or request method/Body replay restrictions.
  privacy: Retry telemetry contains no target, host, path, tenant, user, request, trace, or raw error values.
  performance: Retry work is bounded by eight attempts and two bounded target scans per attempt; no waiter queue or expanded weighted schedule is introduced.
  reliability: One request permit owns the complete retry sequence; cancellation drops the active attempt lease and all target probe claims are released.
affected_surfaces:
  - sdkwork-webserver-app-config
  - request-data-plane
  - host-operations-plane
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - CONFIG_SPEC.md
    - NGINX_SPEC.md
    - PERFORMANCE_SPEC.md
    - OBSERVABILITY_SPEC.md
    - SECURITY_SPEC.md
    - RUST_CODE_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-webserver-core --test webserver_config
  - cargo test -p sdkwork-api-webserver-standalone-gateway data_plane::proxy
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test upstream_safe_retries
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Compatibility Boundary

Retry is disabled unless `upstream.retry` is present. The initial profile intentionally accepts the exact Nginx `proxy_next_upstream` tokens `error`, `timeout`, `http_502`, `http_503`, and `http_504`. It does not expose Nginx `non_idempotent`, because replaying non-idempotent requests is forbidden in this profile.

## Acceptance Evidence

- The root configuration model, JSON Schema, semantic compiler, public exports, checked-in example, and configuration documentation define one opt-in policy with exact supported Nginx tokens. Validation rejects attempts outside 2..8, attempts above target count, total time outside 100..3,600,000 milliseconds, total time above `requestTimeoutMs * maxAttempts`, and empty or duplicate conditions. `cargo test -p sdkwork-webserver-core --test webserver_config` passes 51 tests.
- The request path retains one non-queuing upstream permit across the complete sequence, uses a fixed `[u64; 16]` attempted-target bitmap, independently executes every selected target through guarded DNS/TCP/TLS/physical-capacity/Header validation, updates passive health once per failed attempt, and emits only five fixed saturating retry reasons. No request Body, Trailer, target label, error value, queue, weight-expanded schedule, or retry inventory is collected.
- Replay eligibility fails closed to Body-end-of-stream GET, HEAD, OPTIONS, TRACE, PUT, and DELETE. POST, PATCH, Body-bearing GET, pending Body/Trailer Frames, WebSocket upgrades, request Body failures, and local connection saturation remain terminal. Omitted policy uses the original single-attempt client timeout path.
- A stack-owned probe lease releases half-open ownership on every branch and future cancellation. A separate completed-Body control state prevents replay-safe empty requests from being misclassified as early upstream responses and preserves downstream HTTP/1 Keep-Alive reuse.
- Real dual-origin tests prove configured 503 failover, transport-close failover, per-attempt timeout failover while total budget remains, final 503 Body forwarding on exhaustion, omitted-policy single-attempt behavior, POST and Body-bearing GET refusal, and total-deadline exhaustion without contacting the second target. The complete standalone gateway suite passes 161 tests, including the HTTP/1 Keep-Alive regression.
- `cargo clippy -p sdkwork-api-webserver-standalone-gateway --all-targets -- -D warnings`, `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm.cmd verify`, `cargo fmt --all -- --check`, and `git diff --check` pass.
- SDKWork pagination, API operation-pattern, response-envelope, app-SDK consumer-import, application-layering, Rust backend-composition, route-collision, and database-framework validators passed. PostgreSQL lifecycle was not executed or claimed by this requirement; REQ-2026-0004/0049 own that evidence.

## Accepted Boundary

Acceptance covers bounded node-local sequential failover only. It does not claim request payload replay or spooling, `non_idempotent`, idempotency-key policy, hedging, per-target/shared-zone retry budgets, cluster-global circuit state, or full Nginx directive import compatibility.
