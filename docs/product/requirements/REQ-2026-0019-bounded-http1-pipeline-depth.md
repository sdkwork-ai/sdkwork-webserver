# REQ-2026-0019 Bounded HTTP/1 Pipeline Depth

```yaml
id: REQ-2026-0019
title: Bound fully read HTTP/1 request heads awaiting service dispatch
owner: SDKWork maintainers
status: accepted
source: security
problem: Hyper's finite parser buffer bounded unread Pipeline bytes, but the application did not bound the number of complete request heads already read from one connection and awaiting service dispatch. Tiny pipelined requests could therefore consume disproportionate per-connection scheduling work within the byte ceiling.
goals:
  - Add a finite http1MaxPipelineDepth control with enforced defaults and maximum.
  - Count complete HTTP/1 request heads after original-wire validation and before Hyper service dispatch.
  - Release a slot synchronously when Hyper calls the per-connection Service.
  - Close an over-depth connection without buffering request bodies or fabricating an out-of-order HTTP response.
  - Apply the policy after TLS decryption and bypass HTTP/2 selected by ALPN.
  - Prove a rejected connection releases its connection permit and a fresh connection remains healthy.
non_goals:
  - HTTP/2 concurrent Stream admission, which is governed separately.
  - A response queue, request-body buffer, disk spool, retry, fairness scheduler, or adaptive RSS/cgroup admission.
  - Reproducing Nginx's lack of an equivalent request-count limit.
acceptance_criteria:
  - Schema, Serde defaults, semantic validation, example config, and operator docs expose http1MaxPipelineDepth with default 16 and range 1 through 1024.
  - Original-wire parser unit evidence proves the pending-head counter rejects excess and releases on dispatch.
  - Plain and TLS HTTP/1 Socket evidence proves finite depth, connection close, ordered accepted requests, and one-connection recovery.
  - TLS H2 evidence proves the HTTP/1 counter is bypassed.
  - Watch changing the field retains the active generation as Restart-only.
  - Real Nginx 1.26.2 evidence records the intentional compatibility difference.
  - Full repository verification passes.
non_functional_requirements:
  security: Fail closed at connection scope before an excess request reaches routing or application handlers.
  performance: One Arc and one AtomicUsize per HTTP/1 connection; no task, timer, request collection, Body copy, waiter queue, or lock.
  memory: Hyper's parser byte limit remains the traffic-data bound; the new guard stores only a counter and maximum.
  compatibility: Accepted pipelines preserve order; excess-depth behavior is an explicit SDKWork hardening difference from Nginx 1.26.2.
affected_surfaces:
  - backend
  - composition
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - RUST_CODE_SPEC.md
    - CONFIG_SPEC.md
    - SECURITY_SPEC.md
    - NGINX_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
    - tests/nginx/http1-pipeline-depth/nginx.conf
verification:
  - cargo test -p sdkwork-webserver-core
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm verify
```

## Runtime Semantics

The original-wire parser increments one connection-local counter only after the request line and all Header fields have passed validation and the terminating CRLF is observed. It does not wait for, retain, or copy the request Body. The paired per-connection Service decrements the counter synchronously before it dispatches the request to the existing Hyper/Axum stack.

The counter therefore represents complete request heads already read by the Guard but not yet submitted to the application Service. When the configured maximum is reached, the next complete head fails the accepted stream with an invalid-data error. The connection closes because a parser-level synthetic response could overtake or conflict with already accepted Pipeline responses. No global capacity is consumed beyond the existing connection and request admission controls.

The field is Restart-only: every accepted connection owns an immutable maximum and counter created by its listener acceptor. A Watch candidate cannot partially alter existing connection semantics.

## Nginx 1.26.2 Comparison

The loopback fixture at `tests/nginx/http1-pipeline-depth/nginx.conf` accepted one write containing 64 HTTP/1.1 requests and returned 64 ordered `200` responses totaling 10,235 bytes. Nginx 1.26.2 exposes byte-buffer and Keep-Alive request controls but no equivalent pending Pipeline request-head count.

SDKWork deliberately rejects excess pending depth at the configured request-count ceiling while retaining its independent parser-byte ceiling. This lowers exact Behavioral compatibility for an over-depth Pipeline but provides a deterministic scheduling and OOM-defense boundary. Normal accepted Pipeline ordering remains compatible.

## Current Evidence

- Core Schema/model/default and semantic range validation are implemented.
- The HTTP/1 Guard/Service pair uses one `Arc<PipelineState>` and one `AtomicUsize`; H2 uses no state.
- Unit tests prove depth rejection and synchronous slot release.
- Plain and TLS Socket tests prove over-depth close and fresh one-connection recovery; TLS H2 remains usable.
- Nginx 1.26.2 differential evidence is recorded above.
- Watch changing the field retains the complete active generation as Restart-only.
- Gateway verification passes 31 unit tests, 38 data-plane integration tests, and 4 raw HTTP/1 connection tests. Core verification passes 4 unit tests and 27 configuration integration tests.
- Strict workspace Clippy, formatting, `pnpm verify`, configuration validation, SDKWork pagination/envelope/import/documentation checks, and diff checks pass.

## Acceptance

Accepted on 2026-07-16 for the declared HTTP/1 pending-head scope. PostgreSQL lifecycle execution remains ignored without `SDKWORK_DATABASE_TEST_POSTGRES_URL`; the unrelated existing `agent.sync` operation-pattern violation remains a repository-level commercial blocker. This acceptance does not establish adaptive memory-pressure admission, full HTTP/1 differential/fuzz conformance, proxy early-response request lifecycle, or commercial runtime-core acceptance.
