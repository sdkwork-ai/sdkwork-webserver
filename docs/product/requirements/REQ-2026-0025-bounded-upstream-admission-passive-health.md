# REQ-2026-0025 Bounded Upstream Admission And Passive Health

```yaml
id: REQ-2026-0025
title: Bound upstream request lifetimes and eject repeatedly failing targets
owner: sdkwork-webserver
status: accepted
source: reliability
problem: Reverse-proxy upstreams currently admit every process-admitted request and select targets with unconditional round-robin. A slow or failing dependency can consume the process request budget, grow connection attempts and streaming state, and continue receiving traffic without a bounded per-upstream gate or passive failure isolation.
goals:
  - Add a non-queuing per-upstream in-flight request limit held through complete streaming response lifetime or cancellation.
  - Track bounded per-target consecutive transport and configured HTTP status failures without request-path locks or background tasks.
  - Temporarily eject failing targets, skip them during selection, and permit at most one half-open probe after the ejection deadline.
  - Reset target failure state on a successful response and return a bounded local 503 when admission or all-target availability is exhausted.
  - Keep health and admission state immutable-generation-local so Watch publication never mixes policies or pools.
non_goals:
  - Claiming a physical Reqwest connection hard limit; Reqwest does not expose that control.
  - Retrying, hedging, replaying request bodies, active health checks, slow-start, weighted selection, least-connections, circuit-breaker budgets, or cluster-global health.
  - Persisting passive health across process restart or configuration generation replacement.
users:
  - application operators
  - reliability operators
  - upstream services protected from overload
acceptance_criteria:
  - Upstreams expose finite maxInFlightRequests and passiveHealth.failureThreshold, ejectionTimeMs, and failureStatuses with strict Schema/Serde bounds.
  - Admission uses try_acquire_owned and returns 503 plus Retry-After without a waiter queue, spawned task, request-body poll, DNS lookup, TLS handshake, or upstream connection attempt when saturated.
  - An admitted permit remains held after response headers and through streaming Data/Trailer completion, error, downstream cancellation, or Body drop.
  - Target state cardinality is exactly the configured target count and uses atomics only; no lock is held across external I/O and no health background task exists.
  - Transport errors and configured 5xx statuses increment consecutive failures; other completed responses reset failures.
  - Reaching failureThreshold ejects only that target for ejectionTimeMs; selection skips it while healthy alternatives continue serving.
  - After ejection expiry, one request obtains a half-open probe and concurrent selectors skip that target until the probe succeeds or fails.
  - Probe success restores normal selection; probe failure starts a new full ejection interval.
  - When every target is ejected or already probing, the runtime returns local 503 plus Retry-After and performs no hidden retry.
  - Watch policy changes construct fresh bounded admission and target-health state before atomic generation publication.
non_functional_requirements:
  security: Health input is limited to transport outcomes and bounded configured status codes; no request-controlled target authority or health key is introduced.
  privacy: Target URLs and health transitions are not added to unbounded labels or persisted state.
  performance: Selection is allocation-free after target URL construction and O(configured targets) worst case; state is fixed cardinality with no queue or background task.
  reliability: Failure isolation is process-local and deterministic; overload and all-ejected states fail immediately without retry amplification.
affected_surfaces:
  - config
  - backend
  - runtime
  - reliability
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - CODE_STYLE_SPEC.md
    - NAMING_SPEC.md
    - RUST_CODE_SPEC.md
    - CONFIG_SPEC.md
    - SECURITY_SPEC.md
    - TEST_SPEC.md
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

`maxInFlightRequests` is deliberately named for the resource that can be enforced through Reqwest's public API. One non-queuing Semaphore belongs to each immutable upstream runtime. Its permit is acquired before target selection and is transferred into the downstream response Body so streaming work remains counted after upstream response headers arrive. It is not reported as a physical connection limit; HTTP/2 multiplexing and connector behavior remain transport-owned.

Each configured target owns three atomics: consecutive failures, ejected-until milliseconds relative to the immutable upstream runtime epoch, and a half-open probe flag. Normal selection advances a round-robin cursor and scans for a target whose ejection deadline is clear. Once a deadline expires, compare-exchange grants exactly one probe. Selection never waits for a target or probe.

Transport failure and configured `5xx` response status are passive failures. This slice does not consume or buffer a response Body to decide health and does not retry the original request. A failure crossing the threshold or a failed probe starts one finite ejection interval. Any non-failure response headers reset target state. When no target is selectable, a local bounded `503 Service Unavailable` with `Retry-After: 1` is returned.

## Acceptance

Accepted on 2026-07-16 for the declared non-queuing upstream request-lifecycle admission and process-local passive-health boundary.

- The root Schema and Core model expose finite `maxInFlightRequests` and `passiveHealth.failureThreshold`, `ejectionTimeMs`, and bounded unique `failureStatuses`. Defaults are 1,024 in-flight requests, three consecutive failures, 30-second ejection, and statuses `502`/`503`/`504`; invalid zero/oversized values and non-`5xx` statuses fail Schema validation.
- Each immutable upstream runtime owns one Semaphore and exactly one fixed atomic health record per configured target. Admission uses `try_acquire_owned`; saturation returns local `503` with `Retry-After: 1` before target selection, DNS, TLS, connection, or client-Body polling, with no waiter queue or task.
- The upstream permit transfers into a dedicated downstream Body wrapper and remains held through response Data/Trailer completion, Body error, downstream reset/cancellation, or drop. Unit and real held-stream tests prove headers alone do not release capacity and a dropped stream restores admission.
- Target selection uses round-robin start plus a bounded scan. Configured status and connection/header transport failures increment consecutive failure state; threshold ejection skips only the failed target while healthy alternatives continue. No original request is retried or replayed.
- Ejection expiry grants one half-open probe per target through compare-exchange. Concurrent selectors skip a probing target; probe failure starts a new full interval and success restores normal selection. Compare-exchange against the selected ejection deadline proves a stale concurrent success cannot clear newer ejection state.
- Real proxy evidence proves first failure reaches the failed target exactly once, subsequent traffic uses the healthy alternative, an expired target receives one recovery probe, all-ejected state returns local `503`/`Retry-After` without a second upstream attempt, and a valid Watch generation creates fresh health/admission state.
- Core verification passed 8 unit tests and 39 configuration contract tests. Gateway verification passed 43 unit tests, 55 data-plane integration tests, and 4 raw HTTP/1 connection tests.
- `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm.cmd verify`, example configuration validation, pagination, API envelope, API operation-pattern, route-collision, app-SDK import, repository-doc, formatting, and diff checks passed.

Acceptance is limited to request-lifecycle admission and process-local passive health. Reqwest physical connection hard limits, bounded wait queues, active checks, weighted/least-connection/hash selection, request retries/hedging/replay, retry budgets, slow start, outlier ejection algorithms, circuit breakers, health metrics/administration, and cluster-global health remain separate gates. PostgreSQL lifecycle execution remains ignored because `SDKWORK_DATABASE_TEST_POSTGRES_URL` is not configured. Backend OpenAPI encoding corruption and the unreviewed public `agent.sync` to `agent.retrieve` operation rename still require human review. Adaptive RSS/cgroup admission, 100,000-connection and 24-hour soak evidence, HA/failover/rolling upgrade, signed SBOM/provenance, and commercial operations remain unresolved.
