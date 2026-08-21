# REQ-2026-0039 Bounded Upstream Slow Start

```yaml
id: REQ-2026-0039
title: Add bounded health-aware upstream slow start
owner: sdkwork-webserver
status: accepted
source: nginx-upstream-server-slow-start-commercial-readiness
problem: A target that recovers from active or passive health isolation immediately regains its full configured weight. A cold or only partially recovered instance can therefore receive a sudden traffic surge and fail again.
goals:
  - Add optional target-level `slowStartMs` aligned with the Nginx `server ... slow_start=time` recovery intent.
  - Ramp a recovered target's effective integer weight from one bounded selection slot to its configured nominal weight over finite monotonic time.
  - Start or restart the ramp only when a previously unavailable target becomes fully business-traffic eligible.
  - Apply the same effective weight to round-robin tickets and least-connections load normalization.
  - Preserve primary/backup tiering, active/passive health independence, one half-open probe, bounded retries, Watch generation ownership, and fixed memory cardinality.
non_goals:
  - Nginx shared `zone`, cross-worker or cross-node recovery state, sub-integer selection probability, exact Nginx timer ticks, or byte-for-byte request order.
  - Applying slow start at initial process startup or every configuration Watch publication when the target was not unhealthy.
  - Hash, IP-hash, random, sticky, latency, or cluster-global scheduling.
users:
  - web server application authors
  - node operators
acceptance_criteria:
  - Omitted `slowStartMs` preserves existing full nominal weight behavior.
  - Schema accepts only integer `slowStartMs` values from 100 through 3,600,000 milliseconds; unknown aliases, zero, negative, fractional, string, and out-of-range values fail closed.
  - A passive half-open success starts the ramp after clearing ejection; a failed probe restarts ejection and does not retain an obsolete ramp.
  - An active-health unavailable-to-available transition starts the ramp only when no passive ejection remains; later passive recovery starts it otherwise.
  - Repeated recovery restarts the ramp from its minimum effective weight.
  - Effective weight is monotonic, never zero, never above nominal, uses overflow-safe integer arithmetic, and reaches nominal at the configured deadline.
  - Round-robin and least-connections use effective rather than nominal weight while retaining eligible primary/backup tier and attempted-target rules.
  - A one-target eligible tier continues serving without a queue; nominal weight one remains behaviorally valid even though no finer integer ramp exists.
  - Watch creates fresh inactive ramp state and never transfers old-generation recovery state; invalid candidates retain the active generation.
non_functional_requirements:
  security: Slow start cannot bypass health, DNS/SSRF, TLS, admission, retry replay, Header/Body, connection, or shutdown policy.
  privacy: No target, URL, address, host, route, tenant, request, or trace metric label is introduced.
  performance: Add exactly one immutable duration and one atomic start timestamp per configured target; selection remains bounded scans over at most 1,000 targets with no request-level allocation or lock across I/O.
  reliability: Monotonic generation-relative time and compare-exchange cleanup prevent wall-clock jumps or stale completion from clearing a newer ramp.
affected_surfaces:
  - sdkwork-webserver-app-config
  - request-data-plane
trace:
  specs:
    - CONFIG_SPEC.md
    - NGINX_SPEC.md
    - PERFORMANCE_SPEC.md
    - SECURITY_SPEC.md
    - RUST_CODE_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-webserver-core --test webserver_config
  - cargo test -p sdkwork-api-webserver-standalone-gateway data_plane::proxy
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test upstream_weighted_selection
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test upstream_least_connections
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Compatibility Boundary

Nginx describes `slow_start=time` as the interval during which a server recovers its weight from zero to nominal after becoming healthy or available, with zero disabling the feature. This runtime uses finite integer weights, so an eligible recovered target begins at one slot rather than zero; otherwise it could never be selected without a separate timer queue. A single eligible target and nominal weight one therefore continue serving normally. This is a bounded discrete approximation of the Nginx recovery intent, not exact Nginx request ordering or shared-zone state.

## Recovery State

Each immutable target stores optional duration milliseconds and one atomic monotonic start offset. Initial value zero means no ramp. Passive half-open success or fully eligible active recovery stores `max(nowMs, 1)`. Selection derives effective weight from elapsed generation-relative time with checked `u128` multiplication and clears only the exact completed start value by compare-exchange. A concurrent newer recovery cannot be cleared by stale completion.

## Acceptance Evidence

- The Rust model, root JSON Schema, checked-in example, core/gateway documentation, PRD, and technical architecture define optional target `slowStartMs` from 100 through 3,600,000 milliseconds. Omission is disabled. Zero, negative, fractional, string, boolean, out-of-range, alias, and unknown values fail closed. `cargo test -p sdkwork-webserver-core --test webserver_config` passes 55 tests.
- Each immutable runtime target stores one duration and one `AtomicU64` generation-relative start. Initial and Watch-built targets start inactive at nominal weight. Passive half-open success and fully eligible active recovery start or restart at `max(nowMs, 1)`; passive ejection and active unavailability clear obsolete state.
- Effective weight is derived lazily with monotonic time and `u128` arithmetic, clamped to 1..nominal. Exact-start compare-exchange cleanup cannot clear a newer recovery. A discovered stale-completion edge was fixed so CAS failure recomputes against the newly observed start instead of serving one request at stale nominal weight.
- Unit tests cover start, midpoint, pre-deadline, deadline, restart, stale recomputation, weight-one-safe lower bound, active recovery, passive half-open recovery, combined active recovery while passive ejection remains, repeated failure clearing, and least-connections use of effective rather than nominal weight.
- A real dual-origin round-robin test proves nominal 4:1 traffic, passive failure/ejection, half-open recovery, immediate discrete 1:1 slow-start traffic, and restored 4:1 distribution after the configured deadline. Existing primary/backup, active/passive health, retry, and fixed target selection tests remain green.
- Watch tests prove invalid `slowStartMs` candidates retain the active weighted generation. Fresh generation construction is inactive and does not inherit recovery state; existing in-flight old-generation streaming and tunnel ownership remains unchanged.
- The complete standalone gateway suite passes 178 tests. Full-workspace strict Clippy, `pnpm.cmd verify`, `cargo fmt --all -- --check`, and `git diff --check` pass.
- SDKWork pagination, API operation-pattern, response-envelope, app-SDK consumer-import, application-layering, Rust backend-composition, route-collision, and database-framework validators passed. PostgreSQL lifecycle was not executed or claimed by this requirement; REQ-2026-0004/0049 own that evidence.

## Accepted Boundary

Acceptance covers a generation-local, monotonic, integer effective-weight ramp after fully eligible active or passive recovery for round-robin and least-connections. It does not claim sub-integer selection probability, exact Nginx timer ticks/request order, initial-start ramping, Nginx shared `zone`, cross-worker/cross-node recovery state, hash/IP-hash/random compatibility, or cluster-global scheduling.
