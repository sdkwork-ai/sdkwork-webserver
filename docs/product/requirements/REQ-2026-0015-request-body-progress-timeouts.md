# REQ-2026-0015 Request Body Progress Timeouts

```yaml
id: REQ-2026-0015
title: Bound request Body start and progress gaps across every resource action
owner: SDKWork maintainers
status: accepted
source: security
problem: Request Headers and total Handler time were finite, but the application had no separately observable deadline for the first request Body byte or later Body progress. Slow clients could occupy process request admission and connection/Stream state until the broader request timeout, and a future longer total deadline could silently weaken slow-body protection.
goals:
  - Add separate finite requestBodyStartTimeoutMs and requestBodyIdleTimeoutMs controls with safe defaults and enforced ranges.
  - Apply one zero-copy timeout wrapper before fixed, redirect, static, and proxy action dispatch.
  - Start the first-byte deadline only for a Body that is not already end-of-stream.
  - Reset to the progress deadline only after non-empty Data or Trailer Frames; empty Data cannot keep a request alive.
  - Return bounded HTTP 408 for observed Body timeout, close HTTP/1, and isolate HTTP/2 failure to the timed-out Stream.
  - Preserve existing Body byte limits, Trailer validation, proxy streaming, Expect/Continue, cancellation, and request admission behavior.
  - Treat both controls as Restart-only under Watch reload and retain the active generation when either changes.
non_goals:
  - HTTP/1 Keep-Alive idle timeout; Hyper exposes only a Keep-Alive enable switch, and a generic Socket read timeout would incorrectly terminate connections during long responses.
  - HTTP/2 PING keepalive policy, TLS handshake timeout, response timeout, upstream timeout, total request timeout, WebSocket/SSE heartbeat, or gRPC deadline changes.
  - Per-route overrides, minimum byte-rate policy, adaptive timeouts, client-source aggregation, or commercial release acceptance.
users:
  - Platform operators
  - Site reliability engineers
  - Application traffic clients
acceptance_criteria:
  - Schema, Serde defaults, semantic validation, example configuration, and documentation expose both controls.
  - A Body that is not already complete and yields no meaningful Frame before requestBodyStartTimeoutMs returns 408 and releases request admission.
  - After first meaningful progress, every gap longer than requestBodyIdleTimeoutMs returns 408; shorter gaps reset the deadline and complete successfully.
  - Empty Data Frames never switch from start to progress phase and never reset either deadline.
  - Fixed, redirect, static, and proxy actions consume the same timeout-wrapped request Body.
  - HTTP/1 timeout responses include Connection: close; a fresh connection remains healthy.
  - Real TLS/H2 timeout terminates only the affected Stream and the same connection serves a later Stream.
  - Timeout classification is preserved through the proxy Body adapter and is not mislabeled as invalid framing or upstream failure.
  - Watch changing either control retains the active generation and reports restart required.
non_functional_requirements:
  security: Timeout responses use fixed safe text and disclose no Body, route, upstream, tenant, or peer detail.
  privacy: The wrapper retains only phase state, deadlines, and the existing shared failure classification; it stores no request content.
  performance: One reusable Timer per admitted non-empty request Body; no Body copy, collection, waiter queue, or task spawn.
  reliability: Timeout state is shared with downstream adapters, cancellation releases capacity, and healthy connections/Streams remain available at the correct protocol scope.
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

The timeout wrapper is created after Header/framing validation and request admission but before route action execution. If `Body::is_end_stream()` is already true, it allocates no Timer and passes completion through. Otherwise the start deadline begins immediately.

A non-empty Data Frame or Trailer Frame is meaningful progress. The first such Frame changes the phase to progress and resets the Timer to `requestBodyIdleTimeoutMs`; later meaningful Frames reset the same progress budget. Empty Data is forwarded unchanged but does not reset or change phase. Pending polls register the Hyper task Waker. Expiry records one shared timeout classification before returning `io::ErrorKind::TimedOut`.

All non-proxy drains and the proxy adapter inspect the shared classification before mapping generic Body errors. HTTP/1 timeout responses request connection close because unread framing bytes cannot be reused safely. HTTP/2 returns a Stream response without a connection-specific Header.

## Acceptance Evidence

The configuration contract exposes both finite controls with 30-second defaults and a 100-millisecond through 1-hour range. Core schema and semantic evidence rejects values outside those bounds.

The gateway uses one zero-copy `RequestBodyTimeout` wrapper for every selected resource action. Unit tests cover the start deadline, the transition to the idle deadline, repeated progress, later stalls, empty Data hardening, shared timeout classification, and the no-Timer already-ended path. Real socket tests prove HTTP/1 start and idle `408` responses with `Connection: close`, short-gap completion, proxy classification, one-permit recovery, TLS/H2 Stream isolation, and reuse of the same H2 connection after the timed-out Stream. Watch tests reject changes to either field while retaining the active generation.

Executed acceptance evidence:

- `cargo test -p sdkwork-webserver-core --test webserver_config`: 22 passed.
- `cargo test -p sdkwork-api-webserver-standalone-gateway`: 24 unit, 30 data-plane integration, and 3 raw HTTP/1 tests passed.
- `cargo clippy -p sdkwork-api-webserver-standalone-gateway -p sdkwork-webserver-core --all-targets -- -D warnings`: passed.
- `cargo fmt -- --check`: passed.
- `cargo run -p sdkwork-api-webserver-standalone-gateway -- validate etc/examples/sdkwork.webserver.config.json`: passed.
- Full-workspace Clippy, `pnpm verify`, the pagination/API-envelope/SDK-consumer checks applicable to this config/runtime change, documentation validation, and diff hygiene passed in the same acceptance run.

This acceptance closes only request Body start/progress deadlines. HTTP/1 Keep-Alive idle reaping was subsequently delivered by accepted [REQ-2026-0016](REQ-2026-0016-http1-keep-alive-idle-timeout.md); the other listed non-goals remain separate commercial release gates.

Repository-wide commercial release remains blocked independently: the disposable PostgreSQL lifecycle URL was not configured, and the pre-existing `GET /backend/v3/api/agent/sync` `operationId` fails the SDKWork operation-pattern check. Correcting that public generated SDK action requires human review and is outside this non-API requirement.
