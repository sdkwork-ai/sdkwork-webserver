# REQ-2026-0026 Supervised Active Upstream Health

```yaml
id: REQ-2026-0026
title: Detect and recover unhealthy upstream targets without unbounded background work
owner: sdkwork-webserver
status: accepted
source: reliability
problem: Passive health reacts only after user traffic fails. Operators need proactive dependency detection, but detached per-target tasks, overlapping probes, unbounded response reads, or reload leaks would create availability and OOM risks of their own.
goals:
  - Add optional HTTP active health checks with bounded URI, method, interval, timeout, status, threshold, and response-body controls.
  - Run one supervised scheduler per immutable runtime generation with fixed-cardinality schedule state and a process-configured concurrency ceiling.
  - Compose active availability with passive ejection without allowing either mechanism to erase the other's state.
  - Start a complete candidate scheduler before Watch publication, then explicitly cancel and join the previous generation.
  - Cancel and join all health work during orderly data-plane shutdown.
non_goals:
  - Cluster-global health consensus, persisted target health, cross-process gossip, or control-plane health aggregation.
  - Request retries, hedging, replay, circuit-breaker budgets, slow start, weighted selection, least connections, or consistent hashing.
  - Body-content matching, custom probe headers, gRPC health protocol, TCP-only probes, DNS-specific probes, or health metrics/admin APIs.
  - Making process readiness depend on every configured upstream target; listener/runtime readiness and dependency target health remain separate signals.
users:
  - application operators
  - reliability operators
  - upstream service owners
acceptance_criteria:
  - Optional upstream activeHealth exposes GET/HEAD method, origin-form uri, intervalMs, timeoutMs, unhealthyThreshold, healthyThreshold, successStatusMin, successStatusMax, and maxResponseBodyBytes with strict Schema/Serde bounds.
  - limits.maxConcurrentHealthChecks is finite and bounds the total number of active probe operations in one process generation; a target never has overlapping probes.
  - Scheduler memory is O(configured targets), probe futures never exceed the global limit, missed capacity never creates waiter tasks, and response Bodies are consumed only through the configured byte ceiling.
  - Checks reuse the upstream-specific DNS, SSRF, TLS identity, hostname-verification, timeout, and pool security context without using the business maxInFlightRequests permits.
  - Targets begin eligible for traffic; unhealthyThreshold consecutive timeout, transport, oversized-body, or out-of-range-status outcomes mark only that target actively unavailable.
  - Actively unavailable targets continue receiving bounded probes and require healthyThreshold consecutive successful probes before business traffic resumes.
  - Business target selection requires both active availability and passive availability; active success never clears passive ejection and business success never clears active unavailability.
  - When every target is actively unavailable, passively ejected, or already in a passive half-open probe, the request fails locally with 503 and Retry-After without an upstream attempt.
  - One scheduler task owns all probe futures for an immutable generation; no probe is detached and no lock is held across DNS, TLS, connection, or response I/O.
  - Watch starts the complete candidate scheduler before atomic publication, explicitly cancels and joins the previous scheduler after publication, and a failed candidate leaves the active generation unchanged.
  - Data-plane shutdown explicitly cancels and joins the current scheduler and all in-flight probes within their cancellation path.
  - Real tests cover healthy checks, wrong status, timeout, oversized response, threshold recovery, global concurrency, Watch replacement, and shutdown cancellation.
non_functional_requirements:
  security: Probe authority always comes from the validated target URL; activeHealth.uri cannot carry a scheme or authority and probes reuse the upstream SSRF and TLS policy.
  privacy: Probe logs use bounded upstream id, target index, and generation id; target URLs and response Bodies are not logged or retained.
  performance: One fixed scheduler entry exists per actively checked target, exactly one Tokio supervisor task exists per active generation, and concurrent probe futures cannot exceed limits.maxConcurrentHealthChecks.
  reliability: Active and passive state are generation-local and independent; reload and shutdown use explicit cancellation plus join with no detached health worker.
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

The presence of `activeHealth` enables checks for every target in that upstream. A single generation-owned Tokio task keeps one deadline entry per checked target and a bounded set of in-flight probe futures that it polls directly. It starts work only while the process-wide `limits.maxConcurrentHealthChecks` budget has room, so delayed checks remain fixed schedule entries rather than spawned waiters. Each target has at most one scheduled or running probe, and no per-target Tokio task exists.

Probe requests reuse the immutable upstream Reqwest client and therefore the same guarded DNS resolver, SSRF address policy, TLS roots/client identity, SNI/hostname verification, and pool isolation as user traffic. Health capacity is separate from `maxInFlightRequests` so saturated user traffic cannot starve the bounded control reserve. A whole-request timeout covers response headers and Body progress. Successful response Bodies are drained only up to `maxResponseBodyBytes`; an advertised or observed excess is a failed check.

Active state and passive state are independent target-local atomics. Targets start actively available to avoid a startup outage before the first scheduled cycle. Consecutive active failures remove a target from business selection; consecutive successful active probes restore it. Active outcomes never modify passive failure counters or ejection deadlines, and business traffic never modifies active thresholds.

The data-plane runtime owns the scheduler handle. A Watch candidate is fully compiled, its upstream clients and fixed health state are built, and its scheduler is started before the generation pointer changes. After publication, the old scheduler receives cancellation and is joined. Orderly shutdown first stops the Watch worker, then cancels and joins current health work. Process readiness continues to mean that the configured runtime and listeners can serve; route-level dependency exhaustion is represented by bounded local `503` responses rather than making unrelated routes unready.

## Architecture Review

No new process, API surface, persistence owner, deployment profile, or cross-component dependency is introduced, so a separate ADR is not required. This requirement narrows lifecycle behavior inside the existing standalone data-plane runtime and records the generation-ownership decision here. Cluster-global health or a control-plane health service would cross that boundary and requires a separate requirement and ADR.

## Acceptance

Accepted on 2026-07-16 for the declared process-local HTTP active-health and supervised-generation lifecycle boundary.

- The root Schema and Core Serde model expose optional `activeHealth` with bounded `GET`/`HEAD`, origin-form URI, interval/timeout, failure/recovery thresholds, inclusive success-status range, and response Body bytes. `limits.maxConcurrentHealthChecks` defaults to 64 and is bounded to 1..1,024. Unknown fields, authority/fragment/backslash/control URI input, timeout greater than interval, reversed status ranges, invalid methods, zero/excessive thresholds, oversized Body limits, and excessive concurrency fail before runtime activation.
- Each immutable generation creates exactly one Tokio scheduler task when active checks exist. It owns one fixed deadline entry per checked target and directly polls a `FuturesUnordered` set capped by `maxConcurrentHealthChecks`; there are no per-target tasks, waiter futures, overlapping target probes, unbounded target maps, or locks across external I/O.
- Probe requests use the same immutable upstream Reqwest client as business traffic, preserving guarded DNS resolution, SSRF address authorization, verified TLS roots/client identity, SNI/hostname verification, version bounds, and pool isolation. Health concurrency is a separate bounded control reserve and does not consume or bypass `maxInFlightRequests` business permits.
- The whole probe, including response Body progress, has a finite timeout. Successful Bodies are drained only through the configured byte ceiling; advertised or streamed excess fails the check. Wrong status, timeout, transport failure, and oversized Body all feed only the fixed target-local active counters.
- Targets begin eligible. Real tests prove one failure threshold removes the target, two configured consecutive successes are required for recovery, one target never overlaps with itself, all-active-unavailable traffic fails locally without a business upstream request, and active/passive success paths cannot clear each other's isolation state.
- One four-target real TCP test observes no more than two concurrent probes under a configured global limit of two. Shutdown is awaited and no later health request arrives, proving there is no detached scheduler or per-target task.
- Watch evidence first publishes an invalid authority-bearing candidate and proves the active target plus scheduler continue unchanged. A later valid target replacement starts the new scheduler, atomically changes business traffic, cancels and joins the old generation, and leaves only the new target receiving checks. Orderly shutdown then stops the Watch worker and joins the new scheduler.
- Core verification passed 8 unit tests and 42 configuration contract tests. Gateway verification passed 46 unit tests, 55 data-plane integration tests, 4 raw HTTP/1 tests, and 4 focused active-health network tests.
- `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm.cmd verify`, example config validation, pagination, API operation-pattern, API response-envelope, app SDK consumer-import, repository documentation, formatting, and diff checks passed.

Acceptance is intentionally narrower than commercial Web Server completion. PostgreSQL lifecycle execution remains ignored because `SDKWORK_DATABASE_TEST_POSTGRES_URL` is absent. Backend OpenAPI encoding corruption and the unreviewed public `agent.sync` to `agent.retrieve` operation rename still require human review. Reqwest physical active-connection hard limits, bounded wait queues, retries/hedging/replay budgets, weighted/least-connection/hash selection, slow start, advanced outlier ejection, circuit breakers, health metrics/admin APIs, and cluster-global health remain unresolved. REQ-2026-0027 now supplies process-local RSS/Working Set, finite-cgroup-v2, FD/HANDLE, and event-loop-lag admission, but hard allocator/CPU/PSI/disk governance, WebSocket/SSE/full gRPC, 100,000-connection and 24-hour soak evidence, HA/failover/rolling upgrade, signed SBOM/provenance, and commercial operations remain release blockers.
