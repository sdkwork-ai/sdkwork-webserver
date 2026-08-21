# REQ-2026-0016 HTTP/1 Keep-Alive Idle Timeout

```yaml
id: REQ-2026-0016
title: Reap idle HTTP/1 Keep-Alive connections without interrupting active traffic
owner: SDKWork maintainers
status: accepted
source: security
problem: HTTP/1 Keep-Alive was enabled but had no request-between-request idle deadline. Idle clients could retain connection permits and file descriptors indefinitely, while a generic Socket read timeout would incorrectly terminate connections during active uploads, long responses, or ordered pipelines.
goals:
  - Add a finite http1KeepAliveIdleTimeoutMs control with Nginx-aligned default semantics and an enforced range.
  - Start the deadline only after at least one HTTP/1 request and after every active response Body lifecycle and pending write flush complete.
  - Suspend or disarm the deadline during request handling, Body upload, response streaming, downstream write/flush, and ordered Pipeline work.
  - Close an expired idle HTTP/1 connection silently and release its connection permits.
  - Keep HTTP/2 outside this HTTP/1-specific policy on mixed ALPN listeners.
  - Reuse one Timer per eligible connection without spawning tasks or retaining request/response content.
  - Treat the control as Restart-only under Watch reload.
non_goals:
  - TLS handshake, first-request Header, request Body, response Body, downstream write-stall, total request, upstream, or graceful-drain deadlines.
  - HTTP/2 PING keepalive, HTTP/2 connection-idle GOAWAY policy, WebSocket/SSE heartbeat, or gRPC deadlines.
  - Per-listener, per-virtual-host, per-route, or client-source overrides.
users:
  - Platform operators
  - Site reliability engineers
  - HTTP clients
acceptance_criteria:
  - Schema, Serde defaults, semantic validation, example configuration, and documentation expose http1KeepAliveIdleTimeoutMs.
  - The default is 75000 ms and the accepted range is 100 ms through 1 hour.
  - A completed HTTP/1.0 or HTTP/1.1 Keep-Alive connection closes after the configured idle interval and a fresh connection remains healthy.
  - Active request Body upload can exceed the Keep-Alive interval when its own Body deadlines permit.
  - A streaming response can exceed the Keep-Alive interval, complete, and reuse the same connection.
  - Ordered pipelined requests complete before the connection becomes idle.
  - TLS HTTP/1 follows the same policy after ALPN, while H2 on the same listener remains usable beyond the HTTP/1 interval.
  - Watch changing the control retains the active generation and reports restart required.
non_functional_requirements:
  security: Expiry closes without emitting a synthetic HTTP response because no request is active, and logs must not include peer request content.
  privacy: State retains booleans, active-response count, Waker, and Timer only; it stores no Header, URI, Body, tenant, or peer content.
  performance: At most one lazily allocated reusable Timer and one bounded activity state per negotiated HTTP/1 connection; no task spawn, queue, lock, or polling loop.
  reliability: Activity transitions wake the connection read task, response cancellation releases activity, and H2 is protocol-bypassed.
affected_surfaces:
  - backend
  - composition
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - RUST_CODE_SPEC.md
    - CONFIG_SPEC.md
    - SECURITY_SPEC.md
    - TEST_SPEC.md
    - NGINX_SPEC.md
  components:
    - specs/sdkwork.webserver.config.schema.json
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-webserver-core --test webserver_config
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - cargo fmt -- --check
  - pnpm verify
```

Product authority: [PRD-runtime-core.md](../prd/PRD-runtime-core.md) sections 5, 13, and 14. Runtime design: [TECH-runtime-data-plane.md](../../architecture/tech/TECH-runtime-data-plane.md).

## Phase Contract

The post-TLS Wire Guard identifies the negotiated protocol. HTTP/2 receives no HTTP/1 idle state or Timer. HTTP/1 receives one connection-local activity state shared by the accepted Stream and its per-connection Service.

Service entry marks a request active. A response lease remains held until Hyper drops the wrapped response Body, including cancellation. Async writes mark pending flush; successful flush clears that state. Only a connection that has served at least one request, has no active response lease, and has no pending write flush can arm the reusable idle Timer. Read progress disarms it. Activity completion wakes the registered connection read Waker so the deadline begins without a polling task.

The first request is not governed by this policy. TLS handshake and request Header deadlines remain separate controls. Expiry returns an I/O timeout to the connection driver, which closes the idle socket without constructing an HTTP response.

## Acceptance Evidence

The root Schema, Serde model/default, semantic validator, checked-in example, runtime topology, Listener composition, and operator documentation expose `http1KeepAliveIdleTimeoutMs` with a 75-second default and 100-millisecond through 1-hour range.

The gateway creates activity state only for negotiated HTTP/1. Its response lease, pending-flush marker, registered read Waker, and lazily allocated reusable Timer contain no traffic data. Real tests use a one-connection process/listener budget to prove idle expiry releases the connection permit. Pipelined responses complete in order before expiry. An upload and a held streaming proxy response each exceed the idle interval without interruption. TLS HTTP/1 closes at the deadline, while H2 negotiated on the same Listener remains usable beyond it. Watch changing the field retains the prior generation.

Executed acceptance evidence:

- `cargo test -p sdkwork-webserver-core --test webserver_config`: 23 passed.
- `cargo test -p sdkwork-api-webserver-standalone-gateway`: 26 unit, 33 data-plane integration, and 3 raw HTTP/1 tests passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt -- --check`: passed.
- `cargo run -p sdkwork-api-webserver-standalone-gateway -- validate etc/examples/sdkwork.webserver.config.json`: passed.
- `pnpm verify`: passed, including full-workspace Rust tests, database lifecycle, contract materialization, repository/docs/topology/database checks, and cloud gateway validation.

This acceptance closes only HTTP/1 request-between-request idle reaping. HTTP/1 Pipeline depth was subsequently delivered by accepted [REQ-2026-0019](REQ-2026-0019-bounded-http1-pipeline-depth.md), and H2 PING/ACK failure detection by accepted [REQ-2026-0020](REQ-2026-0020-http2-keep-alive-ping-timeout.md). Responsive-but-idle H2 maximum lifetime, load/soak evidence, and the other non-goals remain separate commercial gates. PostgreSQL lifecycle execution remained ignored because no disposable URL was configured. The pre-existing public API operation-pattern violation for `GET /backend/v3/api/agent/sync` remains subject to human review.
