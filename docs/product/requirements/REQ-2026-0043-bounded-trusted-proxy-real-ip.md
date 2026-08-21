# REQ-2026-0043 Bounded Trusted Proxy Real IP

```yaml
id: REQ-2026-0043
title: Add bounded Nginx-compatible trusted-proxy real-IP resolution
owner: sdkwork-webserver
status: accepted
source: nginx-realip-commercial-readiness
problem: Direct TCP peer identity is safe but loses the original client address behind an explicitly trusted load balancer or ingress. Blindly trusting forwarding Headers would let clients control affinity and downstream identity.
goals:
  - Add listener-local typed trusted-proxy CIDR and real-IP Header policy with no trust by default.
  - Align non-recursive and recursive X-Forwarded-For selection with Nginx realip semantics.
  - Bound Header bytes, hop count, trusted network count, parsing work, and diagnostics.
  - Use one effective client IP for IP-hash initial selection, safe retries, and canonical upstream X-Forwarded-For.
  - Preserve the accepted socket peer as the transport authority while keeping client IP out of metric labels and current access logs.
non_goals:
  - PROXY protocol, RFC 7239 Forwarded, hostname trust sources, Unix sockets, or dynamic cloud-provider CIDR discovery.
  - Per-client metrics, logging raw forwarding chains, geolocation, rate limiting, or authentication from client IP.
  - Nginx binary directive import or full ngx_http_realip_module compatibility.
users:
  - operators deploying behind trusted reverse proxies, load balancers, or ingress controllers
  - applications using bounded IP-hash affinity
acceptance_criteria:
  - Omitted trustedProxy ignores all forwarding metadata and uses the direct TCP peer.
  - Forwarding Headers are considered only when the direct peer matches one of at most 64 trusted, non-overbroad CIDRs.
  - Exactly x-forwarded-for is supported; unknown Header tokens and object fields fail schema validation.
  - A trusted entry rejects duplicate, non-text, empty, malformed, over-byte, and over-hop Headers with a bounded 400 response.
  - Non-recursive mode chooses the rightmost address. Recursive mode scans right-to-left through trusted hops and chooses the first non-trusted address; an all-trusted chain chooses the leftmost address.
  - Plain IP, IPv4 address with port, bracketed IPv6, and bracketed IPv6 with port follow deterministic parsing; ambiguous or zone-scoped forms fail closed.
  - Effective client IP drives IP-hash initial and safe retry selection and replaces inbound X-Forwarded-For with one canonical address upstream.
  - HTTP/1, HTTPS, HTTP/2, untrusted-peer, malformed-input, recursive-chain, and Watch-generation behavior have executable tests.
non_functional_requirements:
  security: Only the immediate trusted peer can assert forwarding identity; malformed trusted input is rejected and never silently accepted as a client identity.
  privacy: Neither the Header chain nor effective client IP becomes a metric label or a new access-log field.
  performance: Parsing is a bounded reverse scan over one bounded Header value with scalar stack state and no split collection, address Vec, per-client map, cache, queue, or lock.
  reliability: Every request uses one immutable runtime generation, so policy and routing change atomically under Watch.
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
  - cargo test -p sdkwork-api-webserver-standalone-gateway data_plane::real_ip
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test trusted_proxy_real_ip
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Compatibility Boundary

The policy corresponds to Nginx `set_real_ip_from`, `real_ip_header X-Forwarded-For`, and `real_ip_recursive`. SDKWork expresses those semantics as typed listener JSON and adds mandatory finite byte/hop/network limits plus strict duplicate and malformed-input rejection. These stricter failure rules are an SDKWork security profile, not a claim that every Nginx parser edge case or directive source is implemented.

## Acceptance Evidence

- The root JSON Schema and public Rust model add one optional listener-local `trustedProxy` object. Omission keeps direct-peer behavior. The object accepts exactly `x-forwarded-for`, 1..64 unique CIDRs, 1..64 hops, and 64..65,536 Header bytes; its byte ceiling cannot exceed `limits.maxHeaderValueBytes`. Model defaults are non-recursive, 16 hops, and 4 KiB. Core configuration tests reject empty/malformed/overbroad CIDRs, unknown Header tokens/fields, zero or excessive hops, byte bounds, IPv4-universal mapped IPv6 trust, and incoherent global budgets. The full current configuration suite passes.
- `resolve_client_ip` first canonicalizes IPv4-mapped peers and checks the immediate peer against the fixed configured CIDR slice. Untrusted peers return immediately without inspecting forwarding metadata. Trusted peers accept exactly one Header field and perform one `rsplit(',')` scan. Every token is validated even after recursive selection has found the first non-trusted address, preventing malformed left-side data from being silently ignored. No split collection, address Vec, per-client cache/map, request queue, async waiter, or lock is introduced.
- Unit evidence covers omitted policy, untrusted peer, Nginx non-recursive rightmost selection, recursive right-to-left trusted-hop selection, all-trusted leftmost selection, IPv4/IPv6 address-with-port forms, mapped IPv4 peers/CIDRs, duplicate fields, empty and non-text values, zone-scoped and malformed addresses, malformed tokens left of the selected identity, byte overflow, and hop overflow.
- The handler resolves one effective IP from the same immutable generation used for route and upstream selection. Invalid trusted input still passes the existing second-stage request/resource-pressure classification and returns a fixed `400`; HTTP/1 closes rather than preserving a connection with an unread request Body. Proxy initial selection, all timeout/transport/status safe retries, HTTP and WebSocket Header forwarding use the same effective address. Inbound XFF occurrences are replaced with one canonical value.
- Real sockets prove recursive XFF selection over HTTP/1 and HTTPS negotiated as HTTP/2, canonical upstream XFF, bounded malformed/duplicate/byte/hop rejection, non-recursive policy replacement under Watch, and a later Watch policy that removes loopback trust and restores direct `127.0.0.1` identity. No listener socket replacement is required for policy-only changes.
- Two real origins prove forwarded IPv4 values with adjacent Nginx hash tickets retain stable per-address IP-hash affinity and select different targets. The existing safe status-retry integration now enables trusted-proxy resolution and uses forwarded `192.0.3.1` to map to the failing first target before retrying the distinct successful target. Direct-peer spoof-isolation tests remain green for listeners without the policy.
- The complete current standalone gateway library and HTTP/HTTPS/H2/WebSocket/DNS/TLS/health/capacity/retry integration suites pass. Full-workspace `cargo clippy --workspace --all-targets -- -D warnings` passes.
- Isolated-target, environment-independent `pnpm.cmd verify` checks passed workspace tests, contract tests, API materialization, formatting, repository/application standards, topology, database-framework, and cloud-gateway validation. The first default-target attempt only encountered the pre-existing Windows lock on a running gateway executable; no process was terminated. PostgreSQL lifecycle was not executed or claimed by this requirement; REQ-2026-0004/0049 own that evidence.
- Pagination, API operation-pattern, response-envelope, app-SDK consumer-import, application-layering, Rust backend-composition, route-collision, and component-port validators pass. `cargo fmt --all -- --check` and `git diff --check` pass.

## Accepted Boundary

Acceptance covers bounded listener-local CIDR trust and one X-Forwarded-For real-IP profile, including HTTP/1, HTTPS/H2, IP-hash, safe retry, canonical upstream forwarding, and Watch composition. Trusted external request scheme is implemented and traced separately by `REQ-2026-0060`. This requirement does not claim PROXY protocol ownership, RFC 7239 `Forwarded`, hostname or Unix-socket trust sources, dynamic cloud-provider CIDR discovery, client-IP rate limiting/logging, Nginx directive import, or full `ngx_http_realip_module` parser equivalence. It also does not establish overall commercial release readiness, allocator/OOM immunity, 100,000 concurrent connections, 24-hour soak, or cluster-global behavior.
