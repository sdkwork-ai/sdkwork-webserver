# REQ-2026-0030 Weighted Upstream Selection

```yaml
id: REQ-2026-0030
title: Make configured upstream target weights affect real traffic selection
owner: sdkwork-webserver
status: accepted
source: correctness-compatibility
problem: upstreams[].targets[].weight exists in the authored Schema and model, but semantic compilation deliberately rejects every value except 1 because runtime weighting was not implemented. The system therefore avoids a silent stub but cannot yet deliver the common relative-weight behavior operators expect from an Nginx-class upstream.
goals:
  - Apply every validated target weight to business request selection.
  - Preserve one-request half-open passive probes and independent active-health exclusion.
  - Keep request-path CPU and memory bounded by configured target count rather than the sum of weights.
  - Preserve equal round-robin behavior when every weight is one.
  - Publish weight changes only through a fully built immutable Watch generation.
non_goals:
  - Claiming byte-for-byte Nginx smooth weighted-round-robin request ordering.
  - Implementing slow_start, least_conn, ip_hash, hash, random, sticky sessions, shared zones, or cluster-global scheduling.
  - Adding per-target physical max_conns, retries, request replay, or circuit breaking.
users:
  - platform operators
  - site reliability engineers
acceptance_criteria:
  - Stable healthy targets receive requests in the configured relative weight ratio over each complete weight cycle.
  - Equal weights preserve deterministic round-robin selection.
  - Active-unhealthy and passively ejected targets contribute no weight while unavailable.
  - An expired passive target obtains at most one half-open probe regardless of its weight.
  - Concurrent eligibility changes cannot cause an unbounded retry loop, request queue, panic, or selection outside the configured target set.
  - Selection allocates no weight-expanded schedule or per-request collection and performs O(configured target count) work.
  - Real multi-origin traffic proves a non-default distribution, unhealthy-target exclusion, recovery, and Watch weight replacement.
  - Documentation states the supported relative-weight semantics and explicit Nginx compatibility limits.
non_functional_requirements:
  security: Target weight does not alter SSRF, TLS identity, request admission, response Header limits, or health failure policy.
  privacy: No user or request identity participates in weighted selection.
  performance: Selection uses bounded linear scans over at most 1000 targets, no request-path allocation, no weight-expanded vector, and no lock across I/O.
  reliability: If eligibility changes during selection, one bounded fallback scan may select another eligible target; the request never spins or queues.
affected_surfaces:
  - config
  - runtime
  - proxy
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - CODE_STYLE_SPEC.md
    - NAMING_SPEC.md
    - RUST_CODE_SPEC.md
    - CONFIG_SPEC.md
    - PERFORMANCE_SPEC.md
    - TEST_SPEC.md
  components:
    - specs/sdkwork.webserver.config.schema.json
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-webserver-core
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Design Decision

The scheduler maps one monotonically advancing atomic ticket into the cumulative weight range of the currently eligible target set. It scans the fixed target vector to compute a bounded eligible total and scans it again to map the ticket. A single bounded fallback scan handles a concurrent health transition between those passes. With stable eligibility, each complete cycle contains exactly each target's configured number of slots. No `weight`-expanded vector is stored.

An active-unavailable target, a passively ejected target before its deadline, or a target whose half-open probe is already in flight contributes no eligible weight. An expired passive target may participate in ticket mapping, but selection still requires the existing `probe_in_flight` compare-exchange, so weight cannot create parallel probes.

## Compatibility Boundary

Nginx upstream `weight` also represents relative traffic share, and the default is one. This requirement aligns that operator-visible meaning. It does not claim the same smooth ordering as Nginx's internal peer scheduler, state sharing through `zone`, `slow_start`, or exact behavior under concurrent health transitions. Those capabilities require separate requirements and conformance evidence.

## Acceptance Evidence

Accepted on 2026-07-16 with the following evidence:

- Core configuration: 8 unit tests and 50 integration/configuration tests passed. The target contract proves default 1, exact 1 and 1,000 boundaries, rejection of 0 and 1,001, and rejection of the unknown `ratio` alias.
- Standalone gateway: 56 library tests, 55 data-plane integration tests, 4 raw HTTP/1 tests, 1 resource-pressure test, 4 active-health tests, 5 physical-connection tests, 4 response-Header tests, and 2 weighted-selection tests passed, for 131 tests total.
- Unit selection evidence proves exact `3:1` complete-cycle slots, unchanged all-one round robin, exclusion of a high-weight active-unavailable target, and a single half-open probe even at weight 100.
- Real dual-origin evidence proves `3:1` produces 6/2 responses over two complete cycles, a configured `503` ejects only the failing high-weight target, all interim traffic reaches the healthy target, and one later half-open success restores weighted service.
- Watch evidence proves an atomic `3:1` to `1:3` replacement and proves a `weight: 0` candidate retains the active weighted generation.
- The selection path stores no expanded schedule, allocates no request collection, adds no waiter, and performs only bounded scans over the Schema-limited 1,000-target vector. Existing admission, DNS/SSRF, TLS, response Header, active health, passive health, and socket-capacity tests remained green.
- Full-workspace `cargo clippy --workspace --all-targets -- -D warnings` and the environment-independent `pnpm.cmd verify` checks passed with the isolated target directory. PostgreSQL lifecycle was not executed in this requirement's acceptance run; REQ-2026-0004 and REQ-2026-0049 own that evidence.
- Pagination, API operation patterns, API response envelope, app SDK consumer imports, application layering, Rust backend composition, `cargo fmt --all -- --check`, and `git diff --check` passed.
