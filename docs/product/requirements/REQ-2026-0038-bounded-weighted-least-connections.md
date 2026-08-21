# REQ-2026-0038 Bounded Weighted Least Connections

```yaml
id: REQ-2026-0038
title: Add bounded weighted least-connections upstream selection
owner: sdkwork-webserver
status: accepted
source: nginx-upstream-least-conn-commercial-readiness
problem: The runtime can distribute traffic by relative target weight, but it cannot prefer the currently least-loaded eligible target. Long-lived streaming, HTTP/2, or WebSocket requests can therefore accumulate behind a target even when another target has lower active request load.
goals:
  - Add typed `upstreams[].loadBalancing` selection with `round-robin` as the compatibility-preserving default and `least-connections` as an opt-in strategy.
  - Align least-connections with the Nginx `least_conn` intent by minimizing active business-request load divided by target weight without floating-point arithmetic.
  - Count each selected business request or Stream from target claim through terminal response Body, cancellation, error, or WebSocket tunnel completion.
  - Preserve primary/backup tiering, active/passive health, one half-open probe, bounded safe retries, target physical connection limits, and immutable Watch generation ownership.
  - Keep target state fixed at configuration cardinality with no request queue, expanded schedule, unbounded retry loop, or Body buffering.
non_goals:
  - Nginx shared `zone`, worker-shared or cluster-global active counts, exact Nginx internal tie order, slow start, drain, hash, random-two-choice, sticky sessions, or connection prewarming.
  - Treating physical sockets as active requests or treating HTTP/2 Streams as physical connections.
  - Queueing requests until a less-loaded target becomes available.
users:
  - web server application authors
  - node operators
acceptance_criteria:
  - Omitted `loadBalancing` and explicit `round-robin` preserve the accepted bounded weighted round-robin behavior.
  - Schema and typed deserialization accept only `round-robin` and `least-connections`; unknown values and non-string values fail closed.
  - Least-connections selects the minimum `activeRequests / weight` inside the already selected primary or backup tier, comparing ratios with overflow-safe integer cross multiplication.
  - Targets with equal weighted load use deterministic bounded weighted ticket selection without an expanded schedule.
  - Active-unavailable, passively ejected, already attempted, and wrong-tier targets do not participate; one expired primary half-open probe still precedes backup traffic.
  - Every selected HTTP attempt owns one RAII activity lease. A retry releases the completed or failed attempt lease and claims the next target exactly once.
  - A normal or rejected WebSocket response holds the lease through downstream Body completion or cancellation; an accepted WebSocket holds it through tunnel completion or shutdown.
  - HTTP/2 concurrent Streams are counted independently even when they share one physical connection.
  - Watch publishes strategy changes as a fresh generation with fresh counters while active old-generation response bodies and tunnels retain only their original generation state.
  - Counter increment saturates without wraparound and release cannot underflow, including cancellation and panic-safe Drop paths.
non_functional_requirements:
  security: Selection cannot bypass DNS/SSRF policy, TLS identity, admission, physical connection limits, Header/Body bounds, health, retry replay policy, or shutdown ownership.
  privacy: No target URL, address, host, upstream, route, tenant, user, request, or trace metric label is introduced.
  performance: Selection performs bounded scans over at most 1,000 fixed targets, uses atomics and `u128` cross multiplication, and allocates only one shared counter per configured target per generation; claiming a request only clones an Arc.
  reliability: Activity ownership is RAII-based and follows downstream streaming or tunnel ownership rather than response-Header arrival.
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
  - cargo test -p sdkwork-api-webserver-standalone-gateway data_plane::upstream_admission
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test upstream_weighted_selection
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test upstream_safe_retries
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test upstream_physical_connections
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test websocket_proxy
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Compatibility Boundary

`least-connections` implements process-local, immutable-generation weighted active-request selection compatible with the common Nginx `least_conn` intent. The active unit is a business request or HTTP/2 Stream, not a pooled physical socket. Equal weighted-load ties reuse this server's accepted bounded cumulative-weight ticket behavior; no claim is made for byte-for-byte Nginx internal tie order, shared `zone` state, or cluster-global scheduling.

## Ownership Model

Each immutable target owns one shared atomic active-request counter. Selection creates one RAII lease before upstream URL construction or I/O. Terminal setup failures drop it immediately. A retry replaces it only after the prior attempt has reached a terminal result. A final HTTP response moves it into the existing response-lifetime Body owner; an accepted WebSocket moves it into the supervised tunnel task. Drop performs a checked atomic decrement, so cancellation, Body errors, task abort, shutdown, and unwind cannot leak or underflow the counter.

## Acceptance Evidence

- The Rust model, public core exports, root JSON Schema, checked-in example, configuration documentation, PRD, and technical architecture define `loadBalancing` as a strict `round-robin` or `least-connections` enum with the compatibility-preserving `round-robin` default. Unknown tokens, non-string values, and aliases fail closed. `cargo test -p sdkwork-webserver-core --test webserver_config` passes 54 tests.
- Every immutable runtime target owns one generation-time `Arc<AtomicUsize>`. A request claim performs a checked atomic increment and returns one RAII `TargetActivityLease`; Drop performs a checked decrement. Saturation cannot wrap and defensive release cannot underflow. Request selection adds no queue, target-state copy, expanded schedule, request-level map, or lock across I/O.
- Least-connections first chooses the existing primary/backup and health/probe tier, then minimizes active requests divided by weight through overflow-safe `u128` cross multiplication. Exact ratio ties reuse the bounded cumulative-weight ticket. Unit evidence covers ratios, weighted ties, backup fallback, active-health exclusion, `usize::MAX` comparison, counter saturation, and defensive underflow.
- HTTP attempt setup, timeout, transport, response validation, status retry, and final streaming ownership carry exactly one target lease. Real retry tests pass with least-connections for 503, transport, and primary-to-backup failover while preserving one request permit, fixed attempted-target bitmap, replay-safe method/Body rules, and total deadlines.
- Real streaming HTTP/1 tests hold response Bodies to prove busy-target avoidance, 2:1 weighted load comparison, primary-tier authority, observable TCP-reset cancellation release, and fresh Watch generation counters while the old streaming generation remains owned.
- Real verified TLS/ALPN H2 tests hold five concurrent upstream Streams and observe `primary, secondary, primary, primary, secondary` at weights 2:1 while each target owns exactly one physical connection. This proves Stream activity is independent from aggregate/target socket permits.
- Real WebSocket tests keep the primary tunnel open, route the next tunnel to the secondary, close the primary, and route the following tunnel back to primary. The activity lease is owned by the supervised tunnel task through close and shutdown; non-`101` responses continue through the existing streaming Body owner.
- The complete standalone gateway suite passes 174 tests. Target and full-workspace strict Clippy pass. `pnpm.cmd verify`, `cargo fmt --all -- --check`, and `git diff --check` pass.
- SDKWork pagination, API operation-pattern, response-envelope, app-SDK consumer-import, application-layering, Rust backend-composition, route-collision, and database-framework validators passed. PostgreSQL lifecycle was not executed or claimed by this requirement; REQ-2026-0004/0049 own that evidence.

## Accepted Boundary

Acceptance covers process-local, immutable-generation weighted least-connections for business HTTP requests, HTTP/2 Streams, rejected upgrade responses, and classic WebSocket tunnels. It does not claim Nginx shared `zone`, exact Nginx internal tie order, cross-worker/cross-node active counts, slow start, drain, hash, random-two-choice, sticky sessions, connection prewarming, or cluster-global scheduling.
