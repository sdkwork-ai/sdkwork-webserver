# REQ-2026-0023 Bounded Upstream DNS And SSRF Policy

```yaml
id: REQ-2026-0023
title: Resolve upstream hostnames asynchronously and reject DNS rebinding destinations
owner: sdkwork-webserver
status: accepted
source: security
problem: Reverse-proxy clients use Reqwest's default resolver without an application-owned answer bound, concurrency limit, timeout, or post-resolution address policy. A configured public hostname can therefore resolve or rebind to loopback, private, link-local, ULA, cloud metadata, multicast, documentation, or reserved destinations without a fail-closed SDKWork check.
goals:
  - Resolve each new upstream connection through a bounded asynchronous system-resolver adapter.
  - Apply the upstream address policy to literal IP targets during configuration compilation and to every DNS answer before Reqwest receives it.
  - Default to public unicast destinations and require explicit narrow CIDR authorization for loopback, private, shared, link-local, and ULA targets.
  - Bound resolver concurrency, answer retention, lookup duration, and idle upstream connection lifetime.
  - Preserve the configured hostname for HTTP Host, TLS SNI, and certificate hostname verification while connecting to approved resolved addresses.
non_goals:
  - Custom DNS server transport, DNSSEC, authoritative TTL extraction, positive/negative cache, stale-on-error, CNAME depth inspection, or Happy Eyeballs tuning.
  - Active/passive health checks, target weights, retries, circuit breaking, or terminating healthy in-flight connections after a DNS address-set change.
  - Allowing request data, headers, paths, query values, or redirects to choose an upstream authority.
users:
  - application operators
  - security operators
  - reverse-proxy clients
acceptance_criteria:
  - Resolver profiles expose finite timeoutMs, maximumAnswers, and maxConcurrentQueries; non-empty custom servers fail compilation because custom resolver transport is not implemented.
  - Upstreams expose optional resolverRef, finite idleConnectionTimeoutMs, and addressPolicy.allowedCidrs with a bounded item count.
  - Unknown resolver references, broad/public allowlist CIDRs, malformed CIDRs, and private literal targets without an explicit permitted CIDR fail configuration compilation.
  - Resolution retains at most maximumAnswers plus one detection item, rejects oversized/empty/mixed-forbidden answer sets, deduplicates approved addresses, and never returns a forbidden answer to Reqwest.
  - Resolver saturation fails immediately without a waiter queue; timeout maps through the proxy timeout path; no resolver lock is held across lookup I/O.
  - A hostname that resolves to an approved public address and later to a forbidden private address succeeds once and fails on the next resolution, proving no application-side indefinite DNS pinning.
  - Real proxy tests prove localhost is denied by default and succeeds only with explicit loopback CIDRs, while existing local test upstreams remain explicit rather than bypassing policy.
non_functional_requirements:
  security: Fail closed for DNS rebinding and literal-IP SSRF; redirects remain disabled and response errors expose no resolved address.
  privacy: DNS names and answers are not persisted or added to high-cardinality metrics.
  performance: Retained answers and resolver concurrency are finite; new connections use asynchronous resolution and pooled connections have a finite idle lifetime.
  reliability: New connections observe current system resolver results while healthy in-flight connections are not terminated by an address-set change.
affected_surfaces:
  - config
  - backend
  - runtime
  - security
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

Reqwest's public `dns::Resolve` interface remains the transport integration point. One immutable resolver runtime is built for each authored resolver profile and shared by every upstream that references it; an additional shared implicit system profile serves upstreams without `resolverRef`. Each upstream wraps that runtime with its immutable address policy, so resolver concurrency is shared while authorization remains upstream-specific.

The system lookup is asynchronous from Tokio's perspective and retains no more than `maximumAnswers + 1` Socket addresses. The extra item detects overflow. Resolver admission uses `try_acquire_owned` and therefore creates no waiter queue. DNS timeout returns a timed-out I/O error so existing proxy timeout mapping can return `504`; policy, saturation, empty-answer, and overflow failures map to bounded `502` without exposing an answer.

The allowed-CIDR surface is deliberately narrow. Only subnets wholly contained in loopback, RFC1918, RFC6598 shared, IPv4 link-local, IPv6 ULA, or IPv6 link-local ranges are accepted. Unspecified, multicast, broadcast, documentation, benchmark, reserved, deprecated special-use, and embedded-private translation destinations remain forbidden even when an operator attempts a broad allowlist. IPv4-mapped IPv6, well-known NAT64, and 6to4 forms are evaluated using their embedded IPv4 address.

The selected system resolver does not expose authoritative DNS TTL or CNAME response metadata. This requirement therefore does not claim custom server, TTL cache, negative cache, or stale-answer support. Reqwest calls the guarded resolver whenever it creates an upstream connection; `idleConnectionTimeoutMs` provides a finite bound for unused pooled connections, while active in-flight connections are never killed merely because DNS changes.

## Acceptance

Accepted on 2026-07-16 for the declared bounded system-resolution and upstream address-policy boundary.

- The root JSON Schema, Serde model, semantic compiler, checked-in example, configuration guide, component READMEs, PRD, and technical architecture expose real `maxConcurrentQueries`, `resolverRef`, `addressPolicy.allowedCidrs`, and `idleConnectionTimeoutMs` behavior. Unknown resolver references, custom server lists, malformed/broad/public CIDRs, unauthorized private literal targets, and out-of-range limits fail before listener activation.
- One immutable `BoundedSystemResolver` is created per configured resolver profile and shared across its upstreams; one shared implicit profile serves upstreams without `resolverRef`. Admission uses `try_acquire_owned`, creates no waiter queue, holds no lock across lookup I/O, retains at most `maximumAnswers + 1` addresses, and enforces a finite Tokio timeout.
- Each upstream supplies an immutable `GuardedDnsResolver` policy to Reqwest. Empty, oversized, or mixed forbidden answer sets fail as a whole; approved addresses are deduplicated; the configured hostname remains the URL authority for Host, TLS SNI, and certificate hostname verification.
- Address classification defaults to public unicast and requires narrow explicit CIDRs for loopback, RFC1918, RFC6598 shared, IPv4 link-local, IPv6 ULA, and IPv6 link-local destinations. Cloud metadata and hard special-use destinations cannot be authorized. IPv4-mapped, deprecated IPv4-compatible, well-known NAT64, and 6to4 forms are normalized through the embedded IPv4 policy.
- Unit evidence proves public-to-private rebinding rejection, explicit loopback authorization, mixed-answer rejection, answer overflow, empty answers, lookup timeout, non-queuing saturation, unsupported-CIDR non-authorization, and embedded-address handling. A real system-DNS proxy test proves localhost returns `502` by default and succeeds only after explicit IPv4/IPv6 loopback authorization.
- Core verification passed 8 unit tests and 33 configuration contract tests. Gateway verification passed 38 unit tests, 47 data-plane integration tests, and 4 raw HTTP/1 connection tests.
- `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm.cmd verify`, example configuration validation, pagination, API envelope, API operation-pattern, route-collision, app-SDK import, repository-doc, formatting, and diff checks passed.

Acceptance is limited to this requirement. The system resolver does not expose authoritative TTL or CNAME metadata, so custom DNS server transport, DNSSEC, positive/negative application cache, stale answers, CNAME depth checks, resolver retries, active health checks, weighted balancing, circuit breaking, and application-owned Happy Eyeballs behavior remain separate gates. PostgreSQL lifecycle execution remains ignored because `SDKWORK_DATABASE_TEST_POSTGRES_URL` is not configured. Backend OpenAPI encoding corruption and the unreviewed public `agent.sync` to `agent.retrieve` operation rename still require human review. Upstream TLS policy, WebSocket/SSE/full gRPC, adaptive RSS/cgroup admission, 100,000-connection and 24-hour soak evidence, HA/failover/rolling upgrade, SBOM/signing/provenance, and commercial operations also remain unresolved.
