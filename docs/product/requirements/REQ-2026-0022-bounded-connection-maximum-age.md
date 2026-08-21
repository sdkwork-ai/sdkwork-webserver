# REQ-2026-0022 Bounded Connection Maximum Age

```yaml
id: REQ-2026-0022
title: Gracefully retire HTTP connections after a finite maximum age
owner: sdkwork-webserver
status: accepted
source: reliability
problem: Healthy HTTP/1 keep-alive and HTTP/2 connections can remain attached to one process generation indefinitely. HTTP/2 PING proves peer liveness but does not rotate responsive connections, so old connection-scoped state can survive certificate, deployment, and load-balancing changes without a finite retirement boundary.
goals:
  - Add one finite maximum connection age shared by HTTP/1 and HTTP/2 listeners.
  - Retire aged HTTP/1 connections by disabling further reuse and retire aged HTTP/2 connections with GOAWAY(NO_ERROR).
  - Allow in-flight work to finish only within the existing finite drain deadline, then cancel the connection.
  - Keep accepted connection tasks supervised and reclaim all permits, timers, streams, and socket state on every exit path.
non_goals:
  - HTTP/1 request-count retirement, per-listener age overrides, randomized connection-age jitter, or HTTP/3.
  - Dynamic certificate replacement inside an already established TLS connection.
  - Replacing HTTP/1 idle timeout, HTTP/2 PING/ACK timeout, request timeout, or process shutdown drain.
users:
  - application operators
  - load balancers and HTTP clients
  - rolling deployment controllers
acceptance_criteria:
  - sdkwork.webserver.app limits expose maxConnectionAgeMs with a one-hour default and a finite validated range of 100 milliseconds through 24 hours.
  - Process/listener connection admission succeeds before a connection task, TLS state, HTTP state, or age timer is created; rejected sockets allocate none of them.
  - One connection-owned timer begins after transport acceptance; no per-stream age timer, payload buffer, queue, or detached retirement task is created.
  - At maximum age, Hyper graceful shutdown prevents a new HTTP/1 request or HTTP/2 Stream while allowing already accepted work to complete within drainTimeoutMs.
  - A real TLS HTTP/2 client observes GOAWAY with NO_ERROR at age expiry and can complete an in-flight Stream before the drain deadline.
  - A real HTTP/1 keep-alive connection completes its current response and closes instead of accepting another request after age expiry.
  - A connection that does not finish draining is canceled no later than maxConnectionAgeMs plus drainTimeoutMs, and a fresh connection remains healthy.
  - Watch reload classifies maxConnectionAgeMs as restart-only and never partially changes existing listener connection policy.
non_functional_requirements:
  security: Finite connection retirement limits stale connection-scoped policy retention and introduces no security exception.
  privacy: No new data is collected or persisted.
  performance: One reusable Tokio timer and one supervised task for each admitted connection; admission precedes task/TLS/HTTP allocation, and no per-request allocation or request-path lock is introduced.
  reliability: Listener accept loops and connection tasks have explicit ownership, bounded drain, cancellation, and join behavior with no detached tasks.
affected_surfaces:
  - backend
  - config
  - runtime
  - deployment
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - ARCHITECTURE_DECISION_SPEC.md
    - CODE_STYLE_SPEC.md
    - NAMING_SPEC.md
    - RUST_CODE_SPEC.md
    - CONFIG_SPEC.md
    - SECURITY_SPEC.md
    - TEST_SPEC.md
    - NGINX_SPEC.md
  components:
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
    - specs/sdkwork.webserver.config.schema.json
verification:
  - cargo test -p sdkwork-webserver-core
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Design Decision

`maxConnectionAgeMs` maps to Nginx `keepalive_time` at the application contract level and defaults to 3,600,000 milliseconds. The runtime applies it to every accepted HTTP connection rather than only to periods of inactivity. HTTP/1 idle retirement remains owned by `http1KeepAliveIdleTimeoutMs`; HTTP/2 liveness remains owned by `http2KeepAliveIntervalMs` and `http2KeepAliveTimeoutMs`.

The listener owns a bounded `JoinSet` of connection tasks. Each task owns the Hyper connection Future and selects between connection completion, process shutdown, and the maximum-age timer. Shutdown or age expiry calls Hyper's protocol-aware `graceful_shutdown`: HTTP/1 stops keep-alive reuse and HTTP/2 emits `GOAWAY(NO_ERROR)`. The connection Future remains polled for at most `drainTimeoutMs`; expiry drops the Future and its complete IO/Acceptor ownership chain, releasing connection admission and all protocol state.

The server does not inject raw GOAWAY frames beside Hyper, terminate TLS below HTTP, or approximate graceful retirement with a socket read/write error. Those alternatives can corrupt concurrent frame writes or turn a normal retirement into a protocol failure.

Nginx 1.26.2 `keepalive_time` accepts and completes the first request received after its deadline and then closes the HTTP/1 connection. SDKWork proactively initiates graceful retirement at the deadline, so an already idle HTTP/1 connection closes before another request is accepted. Both prevent indefinite reuse, but this is an intentional hardening and rolling-deployment difference rather than exact HTTP/1 timing compatibility. The pinned fixture under `tests/nginx/connection-maximum-age` preserves the comparison.

## Acceptance

Accepted on 2026-07-16 for the declared finite HTTP/1 and HTTP/2 connection-retirement boundary.

- The schema, Serde model, semantic validator, checked-in example, Core README, config README, runtime topology, PRD, and technical architecture expose one real `maxConnectionAgeMs` field with a 3,600,000ms default and validated 100ms..86,400,000ms range.
- The listener-owned accept loop acquires process/listener permits before spawning. Rejected sockets allocate no connection task, TLS state, HTTP state, or age Timer; the biased `JoinSet` loop reaps completed tasks before accepting more work.
- HTTP/1 real-socket evidence proves an aged Keep-Alive connection closes before reuse and a new connection succeeds.
- TLS/H2 evidence proves age expiry sends `GOAWAY(NO_ERROR)`, rejects a new Stream, permits an accepted Stream to complete within drain, forcibly retires a held Stream at the finite deadline, and preserves fresh-connection health.
- Watch evidence proves `maxConnectionAgeMs` is Restart-only. Existing shutdown, saturation, TLS, Wire Guard, PING, timeout, proxy, and Pipeline tests remain green after replacing the framework-owned connection loop with an explicitly supervised runtime loop.
- Core verification passed 4 unit tests and 29 configuration contract tests. Gateway verification passed 35 unit tests, 46 data-plane integration tests, and 4 raw HTTP/1 connection tests.
- `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm.cmd verify`, config validation, pagination, API envelope, API operation-pattern, route-collision, app-SDK import, repository-doc, formatting, and diff checks passed.
- Pinned Nginx 1.26.2 validation passed. Its probe observed `HTTP/1.1 200 OK` for both the pre-deadline and first post-deadline request, `Connection: close` on the second response, and EOF afterward. SDKWork's proactive close-at-deadline behavior is the documented hardening difference.

Acceptance is limited to this requirement. PostgreSQL lifecycle was not executed in this requirement's acceptance run; REQ-2026-0004/0049 own that evidence. The backend OpenAPI authority still has apparent encoding corruption and an unreviewed `agent.sync` to `agent.retrieve` public operation rename, so API/SDK commercial readiness requires human review. Dynamic DNS/rebinding defense, upstream TLS policy, retries/health/balancing/circuit breaking, full gRPC/WebSocket/SSE, adaptive RSS/cgroup admission, 100,000-connection and 24-hour soak evidence, HA, and commercial release readiness remain separate gates.
