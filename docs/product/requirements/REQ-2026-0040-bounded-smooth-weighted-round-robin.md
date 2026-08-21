# REQ-2026-0040 Bounded Smooth Weighted Round Robin

```yaml
id: REQ-2026-0040
title: Replace burst-slot rotation with bounded smooth weighted round robin
owner: sdkwork-webserver
status: accepted
source: nginx-default-smooth-weighted-round-robin-commercial-readiness
problem: The previous default `round-robin` strategy mapped an atomic ticket into contiguous cumulative weight slots. It preserved long-run ratios but emitted bursts such as A,A,A,B for weights 3:1 instead of the smoother Nginx-style A,A,B,A sequence. Bursts could concentrate short requests and cold-cache work on one target.
goals:
  - Preserve the existing `round-robin` wire value and long-run weights while upgrading its runtime behavior to smooth weighted round robin.
  - Linearize all process-local concurrent selections for one immutable upstream generation so the smooth state transition is one coherent transaction.
  - Apply active/passive health, attempted-target exclusion, primary/backup tiering, half-open probes, and slow-start effective weights inside the same bounded selection transaction.
  - Use fixed target-cardinality state without an expanded schedule, request queue, request-level collection, or lock across asynchronous work.
  - Expose fixed-cardinality process-lifetime selection lock contention telemetry.
non_goals:
  - Nginx shared `zone`, cross-process or cross-node current-weight state, exact Nginx worker topology, or cluster-global scheduling.
  - Hash, IP-hash, random-two-choice, sticky sessions, latency balancing, or service-discovery priority sets.
  - Holding the smooth-state lock through DNS, TCP, TLS, upstream execution, response streaming, retry delay, or WebSocket tunnels.
users:
  - web server application authors
  - node operators
acceptance_criteria:
  - Omitted or explicit `round-robin` remains configuration-compatible and produces exact stable smooth sequences such as A,A,B,A for weights 3:1 and A,A,B,A,C,A,A for weights 5:1:1 with stable ordered tie breaking.
  - Equal weights preserve deterministic A,B,C rotation.
  - Every selection adds each eligible target's current effective weight, chooses the greatest current weight, and subtracts the eligible total from the selected target exactly once.
  - `least-connections` remains independent and unchanged except for sharing target eligibility and slow-start effective weights.
  - Active-unavailable and passively ejected targets do not accumulate smooth current weight; recovery begins from reset state and uses slow-start effective weight when configured.
  - Request-local attempted-target exclusion does not erase global smooth state; retries select a distinct eligible target without an expanded schedule or Set.
  - Primary and backup tiers maintain independent retained current weights, and backup state cannot influence primary selection.
  - A half-open compare-exchange race performs at most one bounded fallback selection without double-subtracting or looping.
  - Watch creates a fresh fixed state array; invalid candidates retain the complete active generation.
  - Poisoned synchronization state is recovered without panic, and no code path can leave the lock held through panic-sensitive user, parser, network, or asynchronous code.
  - Concurrent multi-thread selection completes without deadlock and preserves exact total weighted counts.
non_functional_requirements:
  security: Selection cannot bypass DNS/SSRF, TLS, admission, physical limits, health, replay, response bounds, or shutdown policy.
  privacy: Contention telemetry has no upstream, target, URL, address, route, tenant, request, or trace labels.
  performance: Store two fixed signed/current-generation scalar arrays over at most 1,000 total targets. The synchronous critical section performs bounded scans only, creates no request allocation, and is never held across `.await` or I/O.
  reliability: Saturating signed arithmetic prevents wraparound; stable config order resolves ties; poison recovery and one-lock ownership prevent lock-order deadlock.
affected_surfaces:
  - request-data-plane
  - data-plane-operations-metrics
trace:
  specs:
    - NGINX_SPEC.md
    - PERFORMANCE_SPEC.md
    - OBSERVABILITY_SPEC.md
    - SECURITY_SPEC.md
    - RUST_CODE_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-api-webserver-standalone-gateway data_plane::smooth_weighted
  - cargo test -p sdkwork-api-webserver-standalone-gateway data_plane::proxy
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test upstream_weighted_selection
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test upstream_safe_retries
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test data_plane_metrics
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Concurrency Model

One immutable upstream generation owns one `Mutex`-protected boxed state containing one signed current weight and one observed recovery marker per configured target. The lock is acquired only inside synchronous target selection. While held, code performs bounded in-memory scans and target atomics; it does not allocate, log, parse, poll a Body, call network code, or cross `.await`. Contention is counted before blocking through a process-lifetime fixed metric. This provides one process-local linearization order without an async waiter queue.

## State Transition

For the selected primary or backup tier, each currently eligible and unattempted target adds its current slow-start effective weight to signed current weight. The target with the greatest value wins; stable configuration order breaks ties. The selected target subtracts the sum of eligible effective weights. Health-unavailable state resets that target's current weight. A new non-zero slow-start recovery marker also resets it before adding the first recovery slot. Attempted exclusion skips but does not reset current weight because it is request-local.

## Acceptance Evidence

- Default `round-robin` now owns one fixed-cardinality `SmoothWeightedState` per immutable upstream generation. Exact unit sequences pass for 3:1 (`A,A,B,A`), 5:1:1 (`A,A,B,A,C,A,A`), and equal weights (`A,B,C`). No configuration wire value changed, and `least-connections` remains a separate strategy.
- Eight concurrent selector threads complete 80,000 linearized selections without deadlock and produce exact 60,000:20,000 totals for 3:1. The state is two boxed scalar arrays sized once at construction; selection allocates no target list, expanded schedule, queue, or request collection.
- Focused phase tests prove request-local attempted exclusion does not erase retained current weight, primary and backup tiers retain independent phases, passive recovery resets stale phase, and active recovery restarts with slow-start effective weight. Health eligibility compare-exchange and target phase reset share the same short lock only on threshold transitions.
- The half-open claim path performs one bounded fallback at most. Poisoned Mutex state is recovered with fixed array cardinality intact. The lock scope contains bounded scans and atomics only and never crosses `.await`, DNS, TCP, TLS, request execution, response Body polling, WebSocket tunneling, logging, or user parsing.
- `sdkwork_web_data_plane_upstream_selection_contentions_total` is a process-lifetime saturating counter with only canonical service/environment/deployment/runtime labels. Selection detects `WouldBlock` before the blocking lock acquisition and reports it through the real request/retry selection path; metric storage and rendering tests prove fixed cardinality.
- Real dual-origin integration proves the first 3:1 cycle is `primary,primary,secondary,primary`, health exclusion/recovery remains correct, backup fallback and Watch replacement remain correct, slow start moves from 1:1 back to 4:1, and invalid Watch candidates retain the active generation. Safe retry and operations-metric integrations also pass.
- The complete standalone gateway suite passes 184 tests. Full-workspace strict Clippy passes with `-D warnings`. `pnpm.cmd verify` passes from an isolated Cargo target directory, including workspace tests, contract tests, API materialization, API envelope, repository documentation, scripts, agent/workflow, topology, database framework, and cloud gateway validation. The first default-target verification attempt was blocked only by the already-running Windows gateway executable lock and was not treated as product evidence.
- SDKWork pagination, API operation-pattern, response-envelope, app-SDK consumer-import, application-layering, Rust backend-composition, route-collision, component-port, and database-framework validators passed. `cargo fmt --all -- --check` and `git diff --check` passed. PostgreSQL lifecycle was not executed or claimed by this requirement; REQ-2026-0004/0049 own that evidence.

## Accepted Boundary

Acceptance covers bounded, process-local, generation-local smooth weighted round robin with stable configuration-order ties, health and slow-start reset, retry exclusion, primary/backup phases, Watch isolation, fixed contention telemetry, and one bounded half-open fallback. It does not claim Nginx worker-shared `zone`, cross-process/cross-node current-weight state, Nginx-internal tie behavior, hash/IP-hash/random/sticky algorithms, cluster-global scheduling, production-scale contention benchmarks, or overall commercial release readiness.
