# REQ-2026-0028 Bounded Upstream Physical Connections

```yaml
id: REQ-2026-0028
title: Bound connecting, active, multiplexed, and idle upstream sockets by real connection lifetime
owner: sdkwork-webserver
status: accepted
source: reliability
problem: maxInFlightRequests bounds request cardinality and maxIdleConnections bounds only retained idle sockets. Reqwest exposes no hard active physical-connection ceiling, so HTTP/1 concurrency, target churn, health probes, and idle pools can still create more TCP/TLS connections than the operator's descriptor and memory budget permits.
goals:
  - Add one explicit finite maxConnections hard ceiling to every configured upstream generation.
  - Acquire connection capacity before DNS, TCP, or TLS work without creating a connector waiter queue.
  - Retain capacity for the complete connecting, TLS, active HTTP/1, multiplexed HTTP/2, and idle pooled connection lifetime.
  - Preserve guarded DNS/rebinding policy, TLS verification, custom trust, mTLS identity, request/response streaming, Trailer fidelity, passive/active health isolation, Watch pool isolation, and bounded timeouts while replacing the upstream HTTP client transport.
  - Classify local connection-capacity exhaustion as bounded 503/Retry-After without blaming the selected target or consuming retry budget.
non_goals:
  - Claiming Nginx per-server max_conns directive compatibility; this requirement is one SDKWork aggregate process-local ceiling across all targets in an upstream generation.
  - Adding per-target connection caps, cross-process zones, global cluster pools, request retries, connection racing, slow start, weighted/least-connection balancing, or circuit breaking.
  - Eliminating Hyper's internal pool checkout coordination; its request cardinality remains bounded by maxInFlightRequests and every admitted request retains a finite whole-request timeout.
users:
  - platform operators
  - site reliability engineers
  - high-concurrency reverse-proxy operators
acceptance_criteria:
  - upstreams[].maxConnections defaults to 256, accepts 1..100000, is strictly deserialized, and requires maxIdleConnections <= maxConnections.
  - One immutable upstream generation owns one connection Semaphore shared by business traffic and active health checks across every target.
  - Connector admission uses try_acquire_owned before DNS, TCP, and TLS, creates no waiter future, and returns a typed local saturation error immediately.
  - A successful permit is stored in the connected I/O object and survives plaintext/TLS wrapping, Hyper protocol ownership, HTTP/1 response completion, HTTP/2 stream multiplexing, and idle pool retention until the physical connection is dropped.
  - DNS lookup, address authorization, answer bounds, timeout, and rebinding checks remain identical to the accepted guarded resolver boundary.
  - HTTPS preserves system/custom/combined roots, hostname/SNI verification, TLS 1.2/1.3 bounds, mTLS client identity, ALPN, protected-file bounds, and immutable generation pool isolation.
  - Business connection saturation returns local 503 Service Unavailable plus Retry-After without polling the client Body, recording passive target failure, or starting a hidden connection attempt.
  - Active-health connection saturation does not mark a target unhealthy; it leaves target health unchanged because the failure is local capacity, not an upstream observation.
  - maxIdleConnections remains a pool-retention ceiling and cannot exceed maxConnections; idle sockets retain physical permits and idle expiry releases capacity.
  - Real HTTP/1 tests prove the physical ceiling during a held streaming response, immediate second-request rejection, no target-failure mutation, connection reuse, idle-socket accounting, expiry release, and recovery.
  - Real HTTPS/HTTP2 tests prove multiple concurrent streams reuse one physical connection under maxConnections=1 rather than being misclassified as connection saturation.
  - Watch builds a complete candidate connector/TLS/pool before publication, never reuses old security or connection capacity, and drops old idle pool ownership after generation replacement.
  - Shutdown and generation drop close idle connections and release every physical connection permit with no detached connector task.
non_functional_requirements:
  security: Connection errors and logs expose only bounded local classes; they never expose resolved addresses, trust paths, certificate content, credentials, request identity, or tenant data.
  privacy: The limiter stores only aggregate permit state and no request, route, host, tenant, user, or response content.
  performance: Healthy pooled requests add no request-path Semaphore operation; only actual new physical connections acquire one non-queuing permit, and HTTP/2 streams reuse the same permitted connection.
  reliability: Connecting plus established plus idle physical connections never exceed maxConnections for one upstream generation, and every failure/cancellation/drop path releases ownership exactly once.
affected_surfaces:
  - config
  - runtime
  - proxy
  - reliability
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - CODE_STYLE_SPEC.md
    - NAMING_SPEC.md
    - RUST_CODE_SPEC.md
    - CONFIG_SPEC.md
    - PERFORMANCE_SPEC.md
    - SECURITY_SPEC.md
    - TEST_SPEC.md
  components:
    - specs/sdkwork.webserver.config.schema.json
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-webserver-core
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Design Decision

Reqwest 0.12.28 exposes an idle-pool count but no active physical-connection hard limit. Its public `connector_layer` fixes the response type to Reqwest's private connection object, so an external Layer can bound concurrent connection-establishment futures but cannot retain a permit for the connection lifetime. A connector concurrency layer would therefore be a false implementation of this requirement.

The standalone gateway instead owns a Hyper legacy client and `hyper-rustls` connector. A bounded connector acquires one upstream-generation permit synchronously before resolution and returns a stream wrapper that owns the permit. `hyper-rustls` wraps that stream for HTTPS, and Hyper owns the resulting plaintext/TLS I/O through active use and idle pooling. Failed connection/TLS paths, normal close, and idle expiry release the permit through ordinary Rust ownership. Generation replacement and shutdown additionally issue `Shutdown::Both` through a non-owning `socket2::SockRef` registry of weak Tokio stream references, so Hyper-held idle I/O closes promptly without duplicating descriptors; the stream object remains the sole permit owner.

For multiple authorized addresses, connection attempts remain sequential so one logical connection never opens racing sockets under one permit. Every non-final address receives a bounded 250-millisecond fallback window inside the total `connectTimeoutMs`; the final address and TLS handshake remain governed by the outer connection deadline. This prevents a stalled first-family address from consuming the entire dual-stack budget while preserving the requirement's no-racing scope.

The hard cap is aggregate across all targets in one upstream generation. This is the correct resource-safety boundary for process descriptors and memory, but it is not declared compatible with Nginx `server ... max_conns=N`, whose scope and active/idle semantics differ. Per-target Nginx-compatible connection policy requires a later focused requirement and compatibility fixture.

Hyper may coordinate admitted requests while a connection is being established or returned to the pool. That coordination cannot grow beyond the already non-queuing `maxInFlightRequests` gate and every request is canceled by finite `requestTimeoutMs`; the connector itself never waits for capacity. This requirement does not label Hyper pool coordination as an unbounded application queue.

## Architecture Review

The client transport remains inside the existing standalone data-plane upstream adapter. No public API, database, process, SDK family, or cross-repository protocol changes. The switch from Reqwest to Hyper is internal but security-sensitive because it must reproduce DNS, TLS, mTLS, timeout, Body, Trailer, and Watch isolation evidence before acceptance. A separate ADR is unnecessary unless the transport becomes a shared cross-application framework component.

## Acceptance Evidence

Accepted on 2026-07-16.

- Core configuration suite: 8 unit tests plus 48 config/integration tests passed, including strict defaults/ranges, unknown fields, `maxIdleConnections <= maxConnections`, and the checked-in proxy example.
- Standalone gateway suite: 121 tests passed: 52 library, 55 real data-plane, 4 raw HTTP/1, 1 resource-pressure, 4 active-health, and 5 physical-connection tests.
- Real HTTP/1 socket counters prove held streaming saturation, immediate local `503`/`Retry-After`, no second TCP accept, reuse, idle-capacity retention, idle expiry, target-health preservation, recovery, and shutdown closure.
- Real HTTPS/H2 socket counters prove concurrent Streams share one TLS/H2 physical connection with `maxConnections=1`.
- Real active-health evidence proves local connection saturation creates no hidden probe connection and leaves target availability unchanged.
- Real Watch evidence proves a candidate owns a fresh pool, the listener no longer pins generation 1, retired idle sockets close after publication, in-flight old-generation Streams retain their generation until completion, and replacement sockets close on shutdown.
- Private CA, hostname mismatch, mTLS, TLS-version, malformed material, Trailer, streaming Body, early response, timeout, DNS/SSRF, passive health, and resource-pressure regression suites remain green after the transport migration.
- `cargo clippy --workspace --all-targets -- -D warnings`, isolated-target `pnpm.cmd verify`, pagination, API operation pattern, API response envelope, app SDK consumer imports, formatting, and `git diff --check` passed.
- PostgreSQL lifecycle remains environment-gated because `SDKWORK_DATABASE_TEST_POSTGRES_URL` is not configured; database lifecycle passed. This does not weaken this requirement's no-database runtime boundary, but remains a repository-level commercial release blocker.
