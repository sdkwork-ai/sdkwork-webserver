# REQ-2026-0041 Bounded Random Two Least Connections

```yaml
id: REQ-2026-0041
title: Add bounded weighted random-two least-connections selection
owner: sdkwork-webserver
status: accepted
source: nginx-random-two-least-conn-commercial-readiness
problem: Full least-connections scans every configured target for every request. Nginx also provides weighted `random two least_conn`, which samples two different servers and chooses the lower weighted active load, reducing selection work while retaining the power-of-two-choices behavior.
goals:
  - Add strict `random-two-least-connections` configuration without changing the default strategy.
  - Sample two distinct eligible targets without replacement and in proportion to current effective weight.
  - Choose the sampled target with the lower `activeRequests / effectiveWeight` using overflow-safe integer comparison.
  - Preserve attempted-target exclusion, primary/backup tiers, active/passive health, half-open probes, slow start, retry, Watch, and request-lifecycle activity ownership.
  - Keep memory fixed per upstream generation and selection bounded without request allocation or retry loops.
non_goals:
  - Cryptographic randomness, affinity, hash/IP-hash, sticky sessions, latency selection, or deterministic request routing.
  - Nginx shared `zone`, worker-shared PRNG/current-load state, byte-for-byte random sequences, or cluster-global selection.
  - Nginx Plus `least_time`.
users:
  - web server application authors
  - high-concurrency node operators
acceptance_criteria:
  - Schema and Rust model accept exactly `random-two-least-connections`; aliases, Nginx directive fragments, unknown tokens, and non-string values fail closed.
  - Omitted `loadBalancing` remains smooth weighted `round-robin`; existing `least-connections` remains unchanged.
  - Eligibility and tier selection match the existing strategies, and weighted sampling uses current slow-start effective weights.
  - Two or more eligible targets produce two distinct weighted candidates; one eligible target remains selectable.
  - Candidate comparison uses `u128` cross multiplication and never floating point.
  - A half-open race tries each sampled candidate at most once and then performs at most one bounded least-connections fallback scan.
  - Retry exclusion cannot reuse an attempted target; backup targets remain unavailable while an unattempted eligible primary exists.
  - One atomic PRNG state is created per immutable upstream generation; Watch replacement does not share state.
non_functional_requirements:
  security: Scheduling randomness is explicitly non-cryptographic and cannot bypass SSRF, TLS, health, admission, replay, response, connection, or shutdown controls.
  privacy: No target, URL, address, route, tenant, request, trace, or random-state metric label is introduced.
  performance: Store one AtomicU64 per upstream and perform a fixed number of bounded scans with no request allocation, expanded schedule, waiter, lock, or unbounded resampling loop.
  reliability: Wrapping SplitMix64 state transition and multiply-high bounded mapping avoid panic, division bias, and shared mutable RNG locks.
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
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test upstream_least_connections
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test upstream_safe_retries
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Nginx Alignment

The official Nginx upstream module defines `random [two [method]]`: selection takes server weights into account, `two` samples two servers, and its default comparison method is `least_conn`. SDKWork exposes the same intent through one unambiguous typed value rather than parsing Nginx directive fragments. Activity is counted per business request or HTTP/2 Stream because that is the existing SDKWork least-connections contract; physical socket limits remain separate.

## Acceptance Evidence

- Rust and JSON Schema accept exactly `random-two-least-connections`; default smooth `round-robin` and existing `least-connections` remain unchanged. Tests reject `least_conn`, `random`, `random two least_conn`, `random-two`, aliases, booleans, and numbers.
- Every immutable upstream generation owns one `AtomicU64` scheduling state. SplitMix64 wrapping arithmetic produces lock-free words and `u128` multiply-high maps them below the finite effective-weight total. No external RNG object, request allocation, candidate collection, expanded schedule, waiter, or resampling loop exists.
- Selection samples two distinct currently eligible targets without replacement and by slow-start effective weight, then compares `activeRequests / effectiveWeight` with the existing overflow-safe `u128` cross multiplication. One eligible target remains valid. Unit tests cover distinct two-target comparison in both load directions, attempted exclusion, primary-to-backup exhaustion, no-target termination, slow-start versus nominal weight, and bounded mapping through one million.
- Active/passive eligibility and the existing half-open `try_select` remain authoritative. The winner and alternative are each claimed at most once; a race invokes at most one bounded least-connections fallback. Retry continues using the fixed attempted-target bitmap and the same request-lifecycle activity lease.
- A real streaming dual-origin test proves that after the first random target holds an active response, the second request selects the other distinct candidate and both upstreams observe exactly one request. The complete standalone gateway suite passes 188 tests across HTTP/HTTPS, H2, WebSocket, health, retry, Watch, admission, and physical-connection ownership.
- Full-workspace strict Clippy passed with `-D warnings`. Isolated, environment-independent `pnpm.cmd verify` checks passed workspace and contract tests, API materialization, API envelope, repository docs, scripts, agent/workflow, topology, database-framework, and cloud-gateway validation. PostgreSQL lifecycle was not executed or claimed by this requirement; REQ-2026-0004/0049 own that evidence.

## Accepted Boundary

Acceptance covers bounded process-local weighted random-two least-connections using SDKWork active request/H2-Stream counts. Randomness is scheduling-only and non-cryptographic. It does not claim Nginx random sequence identity, physical-connection `least_conn` identity, shared `zone`, cross-worker/cross-node load or PRNG state, `least_time`, affinity, hash/IP-hash/sticky routing, or overall commercial release readiness.
