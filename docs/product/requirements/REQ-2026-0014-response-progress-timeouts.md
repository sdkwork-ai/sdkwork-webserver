# REQ-2026-0014 Response Progress Timeouts

```yaml
id: REQ-2026-0014
title: Bound response producer idle time and downstream write stalls
owner: SDKWork maintainers
status: accepted
source: security
problem: The data plane bounded request handling and upstream total time, but a response Body could remain Pending after headers and a downstream Socket write could remain backpressured without an SDKWork phase deadline. Either condition could retain process request admission indefinitely relative to application policy and let slow peers exhaust active-request capacity.
goals:
  - Add separate finite responseBodyIdleTimeoutMs and connectionWriteTimeoutMs controls with safe defaults and enforced ranges.
  - Time the gap between response Body Frames without buffering or polling the Body outside Hyper backpressure.
  - Time continuous Pending periods for accepted-stream AsyncWrite write, flush, and shutdown operations.
  - Release process request admission when a response Body idle timeout terminates the Stream.
  - Close only the stalled HTTP/1 connection or HTTP/2 Stream/connection scope selected by Hyper after a Body error; keep unaffected listeners and connections healthy.
  - Apply write timeout after TLS and wire guards so plaintext and decrypted TLS protocol writes share one policy.
  - Treat both controls as Restart-only under Watch reload and retain the active generation when either changes.
non_goals:
  - Request Body start/progress, request Header, Keep-Alive idle, TLS handshake, upstream connect/read, or total request timeout changes.
  - Per-route timeout overrides, deadline propagation, gRPC deadline mapping, WebSocket/SSE heartbeat policy, or long-lived tunnel policy.
  - Detecting a peer FIN while no Body poll or Socket write is active; the configured producer idle deadline supplies the finite fallback for a completely quiescent response.
  - Adaptive timeout tuning, tenant priority, distributed admission, 100,000-connection, 24-hour soak, or commercial release acceptance.
users:
  - Platform operators
  - Site reliability engineers
  - Application traffic clients
acceptance_criteria:
  - Schema, Serde defaults, semantic validation, example configuration, and documentation expose both finite controls.
  - A response Body that yields no Frame for responseBodyIdleTimeoutMs terminates with a timeout error and releases its request permit.
  - Every non-empty Data Frame or Trailer Frame resets the Body idle deadline; empty Data Frames cannot keep a response alive, and no response data is copied or collected by the timeout wrapper.
  - A write, flush, or shutdown that stays Pending for connectionWriteTimeoutMs terminates with io::ErrorKind::TimedOut.
  - Successful write progress disarms the prior write deadline so separate Pending episodes receive independent budgets.
  - A real HTTP/1 streaming response stalls after a prefix, closes before a longer upstream timeout, releases admission, and leaves a fresh connection healthy.
  - A real TLS/H2 streaming response stalls after a prefix, terminates only that Stream, releases admission, and the same H2 connection serves a later Stream.
  - A real slow-reading HTTP/1 client causes the connection write deadline to close a large response while a fresh connection remains healthy.
  - Watch changing either new control retains the active generation and reports restart required.
non_functional_requirements:
  security: Timeout failures expose no upstream, route, Body, Header, or peer content and use fixed classified internal errors.
  privacy: Timers retain only deadlines and booleans; they store no request or response content.
  performance: One reusable timer exists per admitted response Body and at most one lazily allocated write timer per connection; wrappers add no Body copy or queue.
  reliability: Deadlines are progress-based, reset only by the phase they govern, and isolate failure to the stalled connection or Stream.
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

Product authority: [PRD-runtime-core.md](../prd/PRD-runtime-core.md) sections 5, 6, and 14. Runtime design: [TECH-runtime-data-plane.md](../../architecture/tech/TECH-runtime-data-plane.md).

## Phase Contract

`responseBodyIdleTimeoutMs` starts when Hyper first polls the admitted response Body and resets after each non-empty Data Frame or Trailer Frame. Empty Data Frames are forwarded but do not count as progress; an already elapsed deadline is checked even when the producer keeps returning empty Frames without entering Pending. While the wrapped Body is Pending, its timer is polled with the same task Waker. Expiry returns a bounded `TimedOut` Body error and drops the request permit. It is not a total response duration: a response may run longer while meaningful Frames arrive within the configured gap.

`connectionWriteTimeoutMs` starts only when the wrapped accepted stream returns Pending from `poll_write`, `poll_flush`, or `poll_shutdown`. Any Ready result disarms that episode. The same timer state is reused across operations, and a later stall receives a fresh full deadline. Reads pass through unchanged.

The response Body timer and connection write timer cannot substitute for each other. Body idle covers an upstream/static producer that stops yielding; write timeout covers a downstream peer that stops accepting bytes after the producer has yielded them.

## Acceptance Evidence

Accepted on 2026-07-16 with the following evidence:

- Root Schema, Core Serde defaults, semantic validation, checked-in example, Gateway reload topology, configuration docs, and runtime expose `responseBodyIdleTimeoutMs` and `connectionWriteTimeoutMs`, each defaulting to 30,000 and accepting 100..3,600,000 milliseconds.
- Core configuration tests passed 21 tests, including explicit rejection below/above the two timeout ranges.
- Gateway tests passed 19 unit tests, 27 real data-plane integration tests, and 3 raw HTTP/1 connection tests.
- Body unit tests prove idle expiry returns `io::ErrorKind::TimedOut`, releases the process request permit, meaningful Frame progress resets the deadline, and an empty Data Frame cannot reset an already elapsed deadline.
- Accepted-stream unit tests prove continuously Pending write, flush, and shutdown operations independently return `io::ErrorKind::TimedOut`.
- A real HTTP/1 proxy emits a response prefix and then stalls; the 200 ms Body idle deadline closes the partial response and releases admission in under two seconds, before the five-second upstream timeout, and a fresh request succeeds.
- A real Rustls/ALPN H2 proxy emits the same prefix; the 200 ms deadline terminates only that Stream, and a later Stream succeeds on the same H2 connection.
- A real slow-reading HTTP/1 client requests a 32 MiB sparse static file, stops after response headers, and is closed by the 200 ms connection write deadline before receiving the complete file; a fresh health request succeeds.
- Watch integration changes `responseBodyIdleTimeoutMs` and `connectionWriteTimeoutMs` in separate candidates, retains the active generation for both, and continues to accept the independently live-reloadable Body-limit candidate.
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt -- --check`, `pnpm verify`, example configuration compilation, repository documentation validation, pagination validation, API response-envelope validation, application SDK consumer-import validation, and `git diff --check` passed.

Acceptance is limited to this requirement. Request Body progress and HTTP/1 Keep-Alive idle were subsequently delivered by REQ-2026-0015 and REQ-2026-0016; TLS handshake, WebSocket/SSE heartbeat, gRPC deadlines, per-route overrides, adaptive timeout tuning, exact Nginx `send_timeout` differential compatibility, 100,000 connections, 24-hour soak, and commercial release readiness remain outside it. PostgreSQL lifecycle was not executed in this requirement's acceptance run; REQ-2026-0004/0049 own that evidence. The pre-existing public API operation-pattern blocker for `GET /backend/v3/api/agent/sync` remains subject to human review and was not changed by this runtime requirement.
