# REQ-2026-0042 Bounded IP Hash Affinity

```yaml
id: REQ-2026-0042
title: Add bounded Nginx-compatible IP hash affinity
owner: sdkwork-webserver
status: accepted
source: nginx-ip-hash-commercial-readiness
problem: The runtime lacks a stable client-address affinity strategy. Applications that keep short-lived in-memory session or cache locality cannot request the common Nginx `ip_hash` behavior without an external proxy.
goals:
  - Add strict `ip-hash` configuration while preserving smooth round-robin as the default.
  - Match Nginx key bytes: first three IPv4 octets or all sixteen IPv6 bytes.
  - Match the Nginx hash recurrence and weighted target mapping with a finite retry boundary.
  - Preserve active/passive health, primary/backup roles, half-open probes, safe retry exclusion, Watch generations, and fixed request memory.
  - Use the accepted TCP peer address and never trust an unconfigured forwarding Header for affinity.
non_goals:
  - Trusted-proxy real-IP rewriting, arbitrary Nginx variables, cookie affinity, consistent hash rings, or cryptographic hashing.
  - Nginx shared `zone`, cross-process target state, or cluster-global affinity.
  - Combining `ip-hash` with `slowStartMs`; Nginx explicitly disallows `slow_start` with `ip_hash`.
users:
  - web server application authors
  - operators requiring bounded client affinity
acceptance_criteria:
  - Schema and Rust model accept exactly `ip-hash`; aliases, directive fragments, unknown tokens, and non-string values fail closed.
  - Any `slowStartMs` target under `ip-hash` fails semantic validation before listener activation.
  - IPv4 keys contain exactly the first three network-order octets and IPv6 keys contain all sixteen octets.
  - Hash starts at 89 and applies `(hash * 113 + byte) % 6271` for each key byte.
  - Nominal target weights determine the stable hash range; unavailable targets remain in the range so unrelated client mappings do not shift.
  - An unavailable, already-attempted, or lost half-open target advances the hash at most twenty additional times; exhaustion performs one bounded smooth round-robin fallback.
  - Initial attempts and safe retries use the same direct peer IP; retries never reuse an attempted target.
  - Primary targets remain authoritative while any unattempted primary is eligible; backup selection remains bounded when primaries are unavailable.
  - Watch creates a fresh runtime generation without changing deterministic mapping for unchanged configuration and peer IP.
non_functional_requirements:
  security: Direct peer IP is authoritative until an explicit trusted-proxy policy exists; untrusted X-Forwarded-For cannot influence affinity.
  privacy: Client IP and hash values are not logged or emitted as metric labels.
  performance: Selection uses scalar stack state and bounded scans only, with no request collection, ring, map, allocation, lock, waiter, or unbounded loop.
  reliability: Checked configuration bounds make nominal total weight finite; deterministic integer arithmetic is independent of wall clock and process randomness.
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
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test upstream_ip_hash
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test upstream_safe_retries
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Nginx Alignment

Nginx uses the first three bytes of an IPv4 client address or all sixteen IPv6 bytes, starts with hash 89, and repeatedly evaluates `(hash * 113 + byte) % 6271`. The resulting value selects a server from nominal weighted ranges. When the mapped server is unavailable or already tried, Nginx advances the hash and falls back to round robin after more than twenty retries. This requirement preserves that finite behavior while applying SDKWork active/passive eligibility and primary/backup ownership.

## Security Boundary

Affinity uses `SocketAddr::ip()` from the accepted connection. Existing forwarding Headers remain forwarding metadata only. Supporting a load balancer's original client IP requires a future typed trusted-proxy CIDR and real-IP policy; silently trusting a request Header would permit attacker-controlled affinity and is forbidden.

## Acceptance Evidence

- The Rust model and root JSON Schema accept exactly `ip-hash`; smooth `round-robin` remains the default. Tests reject `ip_hash`, `ip hash`, other directive fragments/aliases, booleans, and numbers. Semantic validation rejects any `ip-hash` upstream containing target `slowStartMs` and identifies `/loadBalancing`, matching Nginx's documented incompatibility.
- The data plane passes `ProxyRequestContext.peer.ip()` into initial selection and every safe retry through a stack `RetryTargetContext`. It does not read `X-Forwarded-For` or allocate a client identity object. The refactor eliminated an eight-argument retry function found by strict Clippy instead of suppressing the warning.
- Exact fixed vectors prove IPv4 `192.0.2.1` and `192.0.2.254` both hash to 6255, while IPv6 `2001:db8::1` and `::2` hash to 2600 and 2601. This proves the Nginx three-byte IPv4 and sixteen-byte IPv6 key boundary and recurrence.
- Nominal weighted mapping tests prove a known hash ticket selects the expected target, an active-unavailable target remains in the range but deterministically rehashes to a healthy target, attempted retry advances without reuse, primary targets remain authoritative, and backup becomes eligible only after primary unavailability.
- Selection performs at most twenty-one Nginx hash mappings over finite nominal target weights and then one existing smooth weighted fallback. Half-open `try_select`, active/passive health, primary/backup tiers, and the fixed 1,024-bit attempted bitmap remain authoritative. There is no client map, hash ring, request collection, allocation, lock, waiter, logging, or client-IP metric label.
- Two separately constructed upstream generations with identical target order/weights and the same IPv6 peer select the same target, proving Watch-stable stateless mapping. Existing generic Watch validation continues to retain invalid candidates atomically.
- A real socket integration sends multiple requests with changing spoofed IPv4, IPv6, and comma-separated `X-Forwarded-For` values; every request remains on the direct-peer affinity target. Passive 503 ejection moves traffic to the deterministic alternative, and half-open recovery returns it to the original target.
- The safe status-retry integration now runs with `ip-hash` and proves the same peer key rehashes past the attempted 503 target to a distinct successful origin. Non-idempotent and body-bearing requests remain non-replayable.
- The complete standalone gateway suite passed 192 tests before the final generation-stability test was added; that final test then passed separately, with no production implementation change afterward. Core config passes 55 tests. Full-workspace strict Clippy and isolated `pnpm.cmd verify` pass. The earlier combined verification timeout produced only a tool-level BrokenPipe and was replaced by a successful standalone verify run.
- SDKWork pagination, API operation-pattern, response-envelope, app-SDK consumer-import, application-layering, Rust backend-composition, route-collision, component-port, repository-doc, and database-framework validators passed. Formatting and diff checks passed. PostgreSQL lifecycle was not executed or claimed by this requirement; REQ-2026-0004/0049 own that evidence.

## Accepted Boundary

Acceptance covers bounded nominal-weight direct-peer IP affinity compatible with Nginx key bytes, recurrence, retry limit, and slow-start incompatibility. It does not claim trusted-proxy real-IP rewriting, arbitrary Nginx variable hash, consistent hash rings, cookie/sticky affinity, shared `zone`, cross-worker/cross-node health or affinity state, or overall commercial release readiness.
