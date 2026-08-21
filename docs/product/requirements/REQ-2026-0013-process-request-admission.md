# REQ-2026-0013 Process Request Admission

```yaml
id: REQ-2026-0013
title: Bound process-wide active requests through response Body completion
owner: SDKWork maintainers
status: accepted
source: reliability
problem: The data plane bounded accepted connections and per-connection HTTP/2 Streams, but it had no process-wide request admission gate. Multiplexed Streams across many connections could therefore create an unbounded number of Handler, upstream, and response-Body tasks relative to one node. A Handler-only guard would be insufficient because proxy and static response Bodies continue after the Handler Future returns.
goals:
  - Add a finite maxConcurrentRequests application limit with a safe default and enforced maximum.
  - Share one non-queuing request Semaphore across every listener and protocol in one data-plane process.
  - Acquire admission before routing, request-Body work, static access, or upstream I/O.
  - Hold admission until the response Body reaches end-of-stream or is cancelled/dropped, including streaming proxy and static responses.
  - Return a bounded 503 response with Retry-After when no permit is immediately available; never wait in an application admission queue.
  - Validate process-level HTTP/2 decoded-header/send-buffer and connection-level encoded-header products against finite aggregate ceilings.
  - Treat maxConcurrentRequests as Restart-only under Watch reload and retain the active generation when a candidate changes it.
non_goals:
  - Tenant, application, host, route, source-IP, or priority-specific fairness and quotas.
  - Distributed admission, global rate limiting, Redis coordination, WAF, or edge DDoS mitigation.
  - Adaptive RSS/cgroup pressure control, allocator reserve enforcement, CPU/event-loop/FD load shedding, or built-in health-route priority reserves.
  - Replacing finite connection, HTTP parser, Body, upstream, timeout, or HTTP/2 abuse controls.
  - Proving 100,000 connections, 24-hour soak, OOM immunity under every host profile, or commercial release readiness.
users:
  - Platform operators
  - Site reliability engineers
  - Application traffic clients
acceptance_criteria:
  - Schema, Serde defaults, semantic validation, example configuration, and documentation expose maxConcurrentRequests.
  - The runtime constructs exactly one process-wide request Semaphore, shared across listeners and immutable for the process lifetime.
  - A saturated request is rejected immediately with HTTP 503 and Retry-After rather than queued.
  - HTTP/1 overload responses request connection close when an unread request Body could otherwise retain connection work; HTTP/2 rejects only the excess Stream.
  - A real streaming response retains its permit after response headers and the Handler Future complete.
  - Response end-of-stream and client cancellation/drop each release the permit so later requests recover.
  - Real TLS/HTTP2 multiplexing proves one held Stream causes an excess Stream to receive 503 while the connection and accepted Stream remain usable.
  - Watch changing maxConcurrentRequests retains the active generation and reports restart required.
  - No request-path mutex is held across await and the admission path allocates no waiter queue.
non_functional_requirements:
  security: Overload responses disclose no route, tenant, upstream, or resource state and use a fixed bounded body.
  privacy: The gate stores only a process-wide permit count and no request identity or content.
  performance: Admission is one non-blocking Semaphore operation per request; a permit wrapper adds no Body buffering or data copy.
  reliability: Saturation is isolated to excess work, active streaming responses can finish, cancellation releases capacity, and reload cannot replace the gate partially.
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

Product authority: [PRD-runtime-core.md](../prd/PRD-runtime-core.md) section 14 and [PRD-production-operations.md](../prd/PRD-production-operations.md) section 11. Runtime design: [TECH-runtime-data-plane.md](../../architecture/tech/TECH-runtime-data-plane.md).

## Admission Contract

`maxConcurrentRequests` counts admitted requests across every plaintext/TLS listener and HTTP/1/HTTP/2 connection owned by one `DataPlaneRuntime`. Acquisition uses `try_acquire_owned`; it never registers a Semaphore waiter. An admitted permit moves into the returned response Body and is released only when that Body reaches end-of-stream or is dropped because the client disconnects, the Stream resets, a timeout cancels work, or shutdown terminates the connection.

The overload response is `503 Service Unavailable`, a fixed text Body, and `Retry-After: 1`. HTTP/1 responses also use `Connection: close` so an unconsumed request Body cannot keep the saturated connection reusable. HTTP/2 must not emit the connection-specific Header and continues serving already admitted and later recovered Streams on the same connection.

This gate bounds active application work; it does not reserve all possible bytes up front and is not evidence of a process RSS hard limit. Aggregate validation is a conservative configuration guard, while runtime load/soak and cgroup-aware memory admission remain required commercial gates.

`maxConcurrentRequests` defaults to `4096` and accepts `1..100000`. Semantic validation additionally caps `maxConcurrentRequests * http2MaxHeaderListBytes`, `maxConcurrentRequests * http2MaxSendBufferBytes`, and `maxConnections * http2MaxEncodedHeaderBlockBytes` at 1 GiB each. These are independent conservative ceilings, not a statement that the process will reserve or consume those amounts.

An HTTP/2 `RST_STREAM` makes cancellation observable immediately and drops the response Body. For HTTP/1, a peer FIN during a completely quiescent streaming response may be observed only on the next downstream write or when the existing request/upstream timeout terminates the Body. This requirement proves the wrapper's cancellation release and real H2 reset propagation; a distinct response-write/idle-progress timeout remains required before claiming prompt HTTP/1 quiet-stream disconnect detection.

## Acceptance Evidence

Accepted on 2026-07-16 with the following evidence:

- The root Schema, Core Serde model/default, semantic validator, checked-in example, Gateway runtime, reload topology, and configuration documentation expose `maxConcurrentRequests` with default `4096` and range `1..100000`.
- Core validation passed 20 tests. The new negative test proves aggregate active HTTP/2 Header List/send-buffer and connection encoded-header products above 1 GiB fail compilation.
- Gateway tests passed 16 unit tests, 24 real data-plane integration tests, and 3 raw HTTP/1 connection tests. Two Body-level tests prove permits remain through end-of-stream and release on Body drop without buffering.
- A real TLS/ALPN H2 streaming-proxy test proves response headers and Handler completion do not release admission: an excess Stream receives `503` plus `Retry-After`, no HTTP/1 `Connection` Header is emitted, the accepted Stream completes, and a later Stream on the same connection recovers.
- A second real TLS/H2 test sends `RST_STREAM(CANCEL)` and proves the same connection recovers admission in under two seconds, before the five-second upstream timeout.
- A raw HTTP/1 streaming-proxy test proves saturation returns `503`, `Retry-After: 1`, and `Connection: close`, then recovers after the accepted response Body completes.
- Watch integration changes `maxConcurrentRequests`, retains the active generation, and separately continues to accept live-reloadable Body-limit candidates.
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt -- --check`, `pnpm verify`, example configuration compilation, repository documentation validation, pagination validation, API response-envelope validation, application SDK consumer-import validation, and `git diff --check` passed.

Acceptance is limited to this requirement. It does not claim a hard process RSS/cgroup ceiling, health/priority reserve, per-tenant/route fairness, CPU/event-loop/FD adaptive shedding, prompt quiet HTTP/1 disconnect detection, 100,000 connections, 24-hour soak, or commercial release readiness. PostgreSQL lifecycle was not executed in this requirement's acceptance run; REQ-2026-0004/0049 own that evidence. The pre-existing public API operation-pattern blocker for `GET /backend/v3/api/agent/sync` remains subject to human review and was not changed by this runtime requirement.
