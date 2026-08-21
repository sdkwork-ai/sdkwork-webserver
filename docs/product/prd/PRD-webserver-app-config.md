# SDKWork Web Server Application Configuration PRD

Status: active
Owner: SDKWork maintainers
Application: sdkwork-web
Updated: 2026-08-04
Parent: [PRD.md](PRD.md)
Specs: APP_MANIFEST_SPEC.md, APPLICATION_SPEC.md, CONFIG_SPEC.md, ENVIRONMENT_SPEC.md, NGINX_SPEC.md, SECURITY_SPEC.md

## 1. Purpose

Define the complete application-owned Web Server configuration contract for Web applications under `apps/<app-root>/`. The contract must cover common Nginx-class Web Server behavior while remaining deterministic, portable, safe, versioned, and executable by the Rust data plane.

## 2. Source Of Truth And File Layout

Every independently hosted Web application root must contain:

```text
apps/<app-root>/
  sdkwork.app.config.json
  config/
    webserver/
      sdkwork.webserver.config.json
```

- `sdkwork.app.config.json` remains the application identity, release, platform, media, artifact, and publication authority.
- `sdkwork.webserver.config.json` is the authored traffic-serving authority.
- Generated Nginx files, compiled Rust snapshots, runtime secrets, live certificates, node state, logs, and databases must not be committed into the app root.
- The app manifest references the Web Server contract through a standardized `runtime.webServer` object. Adding this object requires coordinated changes to `APP_MANIFEST_SPEC.md`, its JSON Schema, validator, initializer, full example, and projection tooling.

Required manifest reference:

```json
{
  "runtime": {
    "webServer": {
      "enabled": true,
      "configRef": "config/webserver/sdkwork.webserver.config.json",
      "defaultProfile": "static-spa"
    }
  }
}
```

## 3. Top-Level Contract

```json
{
  "schemaVersion": 1,
  "kind": "sdkwork.webserver.app",
  "appKey": "sdkwork-example-pc",
  "nginx": {},
  "profiles": {},
  "listeners": [],
  "certificates": [],
  "tlsPolicies": [],
  "resolvers": [],
  "resources": [],
  "upstreams": [],
  "virtualHosts": [],
  "policies": {},
  "observability": {},
  "deployment": {},
  "metadata": {}
}
```

Rules:

- Unknown fields are rejected unless explicitly allowed inside a bounded metadata extension object.
- IDs are stable lower-kebab-case identifiers unique within the application configuration.
- `appKey` must exactly match the owning `sdkwork.app.config.json` application key.
- References must resolve inside the same compiled application configuration or through an approved external resource reference type.
- Secrets and secret values are forbidden. Only secret, KMS, certificate, Drive artifact, or discovery resource references are allowed.
- Configuration must declare explicit limits; an omitted limit resolves to a documented bounded default, never infinity.

This app-owned contract is not the server process configuration. Process binds, service account, worker/runtime sizing, runtime directories, administrative listener, global emergency reserve, and platform secret providers belong to the typed server runtime configuration governed by `CONFIG_SPEC.md` and `RUNTIME_DIRECTORY_SPEC.md`. The compiler combines app contracts, deployment overlays, resolved resources, and host runtime policy into one immutable node snapshot without allowing an application to weaken host-level limits.

## 4. Nginx Profile

`nginx` declares the intended Nginx profile behavior (deploy TOML `[nginx]` /
`nginx.enabled`). Deploy-time keys:

| Field | Requirement |
| --- | --- |
| `enabled` | Master switch for nginx profile activation; default `true`. |
| `profile` | Supported profile such as `http-core-v1`. |
| `unknownDirectivePolicy` | `error` (default), `warn`, or `allow` (requires `exceptionRef`). |
| `strict` | When `true`, present per-profile sidecars must match the TOML render (W16). |
| `confFile` | Sidecar base name; default `nginx.conf` → `nginx.<profile>.conf`. |
| `exceptionRef` | Required for `unknownDirectivePolicy = "allow"` or `strict = false`. |

The Rust runtime app model carries `enabled` / `profile` / `unknownDirectivePolicy`
only. `strict` and `confFile` are deploy-validator concerns.

Validators fail closed on the retired table `[compatibility]` and key
`nginxProfile` with a migration diagnostic. Dual-read is forbidden.

## 5. Default Profiles

The product must provide five complete profiles:

| Profile | Default behavior |
| --- | --- |
| `static-site` | Static files, index documents, conditional/range requests, MIME, safe traversal, bounded caching. |
| `static-spa` | Static site behavior plus client-route fallback to `index.html` and immutable fingerprinted asset caching. |
| `reverse-proxy` | One or more upstreams, forwarding headers, streaming, WebSocket, timeouts, health checks, and bounded retries. |
| `hybrid` | Static assets and SPA shell with selected API paths routed to upstreams. |
| `api-gateway` | Route-based upstream selection, streaming, authentication extension points, limits, audit, and high-volume observability. |

Development defaults bind to `127.0.0.1:8080`. Container defaults bind to `0.0.0.0:8080`. Public 80/443 listeners are created only by an explicit public-edge profile or deployment overlay.

## 6. Listener Contract

Each listener is an application-logical listener. The deployment compiler maps compatible logical listeners from multiple applications onto physical node sockets and proves that their protocol, TLS, Proxy Protocol, and default-host policies can coexist. Each listener supports:

- Stable `id`, bind address, port, socket family, and default-server selection.
- HTTP, HTTPS, HTTP/1.0 compatibility, HTTP/1.1, HTTP/2, and future-gated HTTP/3 protocol declarations.
- `reusePort`, accept backlog, keep-alive, header timeout, request timeout, idle timeout, graceful drain, and maximum connection limits.
- Optional Proxy Protocol with explicit trusted source networks.
- Optional `trustedProxy` forwarding policy with explicit bounded CIDRs, the exact `x-forwarded-for` identity Header token, strict single-value `X-Forwarded-Proto`, Nginx-compatible recursive IP selection, and finite Header/hop bounds.
- Optional TLS policy reference; HTTPS listeners require one.
- Platform-aware binding overlays without changing the app-owned logical listener id.

Validation must reject duplicate socket ownership, invalid ports, unsupported protocol combinations, unsafe wildcard exposure, missing TLS dependencies, and multiple defaults on the same effective address.

Forwarding identity and scheme are never trusted by default. When `trustedProxy` is omitted, the accepted TCP peer and listener transport are authoritative and inbound forwarding Headers are replaced before proxying. When configured, only an immediate peer in `trustedCidrs` may supply exactly one `X-Forwarded-For` field and one exact `X-Forwarded-Proto: http|https` field. `recursive: false` selects the rightmost address, matching Nginx's default realip behavior. `recursive: true` scans right-to-left through trusted hops and selects the first non-trusted address; an all-trusted chain resolves to its leftmost address. Native TLS always resolves to HTTPS and cannot be downgraded. The policy accepts at most 64 unique CIDRs and 64 hops, rejects IPv4 networks broader than `/8`, IPv6 networks broader than `/16`, IPv4-universal mapped IPv6 networks, and requires `maxHeaderBytes` not to exceed the global field-value limit. Duplicate, empty, malformed, chained protocol, non-text, over-byte, and over-hop trusted Headers fail with `400`; an untrusted peer's Header is ignored without parsing. Watch publishes policy changes atomically for new requests without replacing listener sockets.

`proxyProtocol` is a distinct listener transport contract for load balancers that emit HAProxy PROXY protocol. Omission means the first bytes are TLS or HTTP as usual. Presence means every accepted connection must begin with a valid enabled v1/v2 Header before TLS ClientHello or HTTP parsing; optional auto-detection is forbidden because an attacker could otherwise choose the identity authority. Only an immediate socket peer in `trustedSourceCidrs` may assert a source address. v1 supports `TCP4`, `TCP6`, and `UNKNOWN`; v2 supports `LOCAL` and `PROXY` for TCP over IPv4/IPv6. Parsing has a finite 100..10,000 ms timeout and 107..4,096 byte total Header ceiling, with defaults of 3 seconds and 536 bytes. v2 TLVs require exact structural framing and are streamed through fixed scratch memory. `crc32cPolicy` defaults to `ignore`, supports `validate-if-present` for rollout and `required` for strict v2 integrity, and rejects malformed or duplicate CRC TLVs under every mode. CRC32C is corruption detection rather than cryptographic sender authentication; no TLV is promoted to request identity or forwarded metadata. Missing, malformed, oversized, untrusted, disabled-version, or CRC-policy-failing input closes without starting TLS/HTTP or emitting an HTTP response. `proxyProtocol` and HTTP `trustedProxy` are mutually exclusive, the resolved transport source becomes the effective client identity, and every policy change is Restart-only.

Host policy may narrow bind addresses, ports, protocols, connection budgets, or public exposure. An app contract cannot force a privileged port, wildcard bind, Proxy Protocol trust, or public administrative endpoint when the host profile forbids it.

## 7. Certificate Contract

Certificates are logical references, never embedded PEM values. Supported sources:

- SDKWork managed certificate resource.
- ACME managed policy.
- Secret-manager/KMS reference.
- Standalone protected file reference.
- Development-only generated self-signed certificate.

Each certificate declaration includes server names, source, lifecycle policy, renewal window, deployment scope, and optional client-auth trust reference. Full HTTPS behavior is defined in [PRD-https-and-certificates.md](PRD-https-and-certificates.md).

Each `tlsPolicies` entry declares protocol versions, approved cipher/security profile, ALPN, session behavior, SNI/default-certificate behavior, OCSP, and optional client-auth trust references. Application TLS policy may select an approved host baseline or a stricter policy; it cannot weaken the host security minimum.

## 8. Resource Contract

Resource types:

- `static`: protected filesystem or packaged artifact root.
- `drive`: live Drive WebsiteRoot mount (a space root or a folder inside a space,
  declared through `providerResourceUuid` and optional `resourceSubpath`), served with
  index files, SPA fallback, conditional requests, byte ranges, and bounded resolution caching.
- `knowledgebase`: live Knowledgebase WikiPublication site, served through wiki route
  resolution, canonical redirects, navigation, and search with an optional resource-level
  `locale` default.
- `drive-artifact`: immutable SDKWork Drive-backed Web artifact (application release
  artifacts; distinct from the live `drive` WebsiteRoot mount).
- `proxy`: reference to an upstream pool.
- `redirect`: status and safe location template.
- `respond`: bounded fixed response.
- `acme-http-01`: reserved managed challenge responder.

Provider-backed `drive` and `knowledgebase` resources reuse the shared website provider
contracts and bounded resolution cache. Provider SDK connections are environment-owned
(base URL, ingress token file, tenant scope hash); every referenced resource is validated
against its provider before the data plane starts, and watched reloads that would reference
an unassembled provider type are rejected while the active generation is retained.

Static resources support index files, `tryFiles`, SPA fallback, directory listing policy, MIME mapping, charset, ETag, Last-Modified, byte ranges, precompressed variants, cache control, hidden-file policy, symlink policy, dot-file policy, and maximum file size policy.

All filesystem paths are resolved relative to an approved application artifact root. Normalization, canonicalization, symlink escape, device files, alternate data streams, encoded traversal, and platform separator variants must be tested and fail closed.

## 9. Virtual Host Contract

Each virtual host includes:

- Stable `id` and listener references.
- Exact, leading-wildcard, trailing-wildcard, and regex server names.
- Default-host designation and canonical-domain redirect policy.
- Optional TLS policy and certificate references.
- Ordered route declarations.
- Host-level security, compression, cache, access, and observability policy references.

Server-name selection follows the declared Nginx compatibility profile. Ambiguous or conflicting ownership must be reported before publication.

## 10. Route Contract

Route matches may include:

- Exact, prefix, Nginx `^~` prefix, or PCRE2 regex path.
- HTTP methods.
- Header, query, cookie, source network, protocol, or host conditions.
- Explicit priority only for match combinations that cannot be represented by Nginx location order.

Route actions are exactly one of static resource, Drive WebsiteRoot mount, Knowledgebase WikiPublication, proxy upstream, redirect, fixed response, or managed extension. Rewrites, header transforms, body limits, timeout, cache, compression, rate, access, authentication, and observability are policies around the action rather than hidden side effects.

Each virtual host may declare `securityHeaders` applied to every selected response: HSTS (HTTPS only), `X-Frame-Options`, `Content-Security-Policy`, `Referrer-Policy`, `X-Content-Type-Options` (default `nosniff`), and up to sixteen bounded custom headers. Custom headers reject hop-by-hop and framework-owned names at compile time. Public non-loopback listeners without TLS fail closed unless the operator explicitly declares `allowPlaintextHttp` or serves ACME HTTP-01 challenges through `acmeHttp01`.

The compiler must reject ambiguous exact matches, unreachable routes, unsafe rewrites, rewrite loops, invalid regex, conflicting actions, missing references, and route counts above the configured application quota.

## 11. Upstream Contract

Each upstream supports:

- Static targets or an approved discovery reference.
- IPv4, IPv6, DNS, and host-approved Unix domain socket targets where supported by the deployment platform.
- HTTP/HTTPS origin protocol, SNI, hostname verification, and optional mTLS.
- Target weight, backup status, drain state, and maximum connections.
- Round-robin, weighted round-robin, least-connections, IP hash, random-two-choice, and consistent-hash policy where implemented.
- Connection pooling, keepalive, connect/read/write timeouts, and queue limits.
- Active and passive health checks.
- Bounded retry budget, retryable error/status policy, circuit breaker, outlier detection, and recovery window.

Retries are allowed only before a non-replayable request body has been committed unless an explicit safe replay buffer and body-size bound exists.

## 11.1 Resolver Contract

The commercial target supports approved DNS servers or a platform resolver profile, query timeout, retry bound, positive TTL floor/ceiling, negative TTL, stale-on-error window, IPv4/IPv6 policy, maximum answers, and cache budget.

The currently implemented foundation profile is deliberately smaller. A resolver declares `timeoutMs`, `maximumAnswers`, and `maxConcurrentQueries`; its `servers` list must be empty because custom DNS transport is not implemented. An upstream may select `resolverRef`, set finite `idleConnectionTimeoutMs`, and authorize narrow restricted networks through `addressPolicy.allowedCidrs`. The runtime performs bounded asynchronous system lookup, admits without a waiter queue, rechecks every answer, rejects a mixed forbidden answer set, and preserves the configured hostname for Host, TLS SNI, and certificate verification. Public unicast is allowed by default; loopback/private/shared/link-local/ULA requires explicit narrow authorization; metadata and hard special-use destinations remain forbidden.

Authoritative TTL retention, application positive/negative cache, stale-on-error, CNAME depth inspection, custom DNS server transport, deterministic address racing, resolver retries, and health-aware dynamic target selection are not implemented in this profile and remain release gates. An address change affects later connections; healthy in-flight work is not terminated solely because DNS changes.

## 11.2 Upstream TLS Contract

The implemented foundation profile verifies HTTPS upstream certificates and hostnames with Rustls. System WebPKI trust is the default. `tls.trustMode` may select system roots, replace them with bounded custom CA files, or combine both. A paired client certificate/private key enables mTLS, and minimum/maximum versions constrain negotiation to TLS 1.2 and/or TLS 1.3. TLS policy is invalid on a target set containing HTTP; the configured URL hostname remains Host authority, SNI, and the certificate-verification name.

TLS files are protected relative configuration resources, bounded to 1 MiB each and contained beneath the configuration directory after canonicalization. At most eight CA files and 64 parsed custom roots are accepted. Each upstream owns an independent immutable client/pool security context, and Watch publication occurs only after the complete candidate context builds successfully. No field disables certificate or hostname verification.

Certificate/SPKI pinning, CRL/OCSP and certificate-transparency enforcement, PKCS#11/KMS/Vault providers, automatic secret-file watching, upstream HTTP/2 policy controls, and live credential rotation remain commercial release gates.

## 11.3 Upstream Admission And Passive Health Contract

The implemented foundation profile bounds each upstream with non-queuing `maxInFlightRequests`. Capacity is acquired before upstream work and held through the downstream streaming response Body lifetime; saturation returns local `503` with retry guidance. This is an enforceable request-lifecycle limit independent of the physical connection boundary.

Each target has fixed process-local passive state. Consecutive transport or configured `5xx` failures trigger finite ejection; selection skips ejected targets while healthy alternatives continue. At deadline expiry one half-open request probes a target, success restores it, and failure restarts ejection. When every target is unavailable or probing, the runtime performs no queueing or hidden replay and returns local `503`. Configuration generation replacement creates fresh target state and upstream capacity before atomic publication.

Bounded wait queues, hedging, request-Body buffering/replay, arbitrary/consistent hash selection, outlier algorithms, shared circuit budgets, and cluster-global health are not implemented by this profile. The explicit bodyless-idempotent retry subset is defined in section 11.3.4; process-local weighted least-connections, random-two, IP-hash, and bounded recovery slow start are defined in section 11.3.3.

## 11.3.1 Upstream Physical Connection Contract

The implemented foundation profile adds `upstreams[].maxConnections`, default 256 and range 1..100,000. One immutable upstream generation owns one aggregate non-queuing Semaphore across every target and both business and active-health traffic. Optional `targets[].maxConnections` accepts 1..100,000, cannot exceed the upstream maximum, and adds one authority-owned Semaphore. When any target cap is present, normalized scheme/host/effective-port authorities must be unique because Hyper pools by origin. Connector admission acquires both applicable limits before DNS, TCP, or TLS; the permits are owned by the connected I/O through HTTP/1 use, HTTPS, HTTP/2 multiplexing, and idle-pool retention. `maxIdleConnections` cannot exceed the aggregate hard cap and remains Hyper's per-origin idle-retention parameter.

Business saturation returns local `503` plus retry guidance without reading the request Body, opening a hidden socket, retrying, or recording passive failure. Active-health saturation leaves target state unchanged. Multi-address TCP attempts are sequential and use a bounded non-final-address fallback within `connectTimeoutMs`; there is no connection racing. Watch builds a fresh candidate pool, publishes it atomically, drains existing generation references, and then closes retired idle sockets. Orderly shutdown closes current idle sockets without detached connector tasks.

The target cap implements the common safety intent of Nginx `server ... max_conns=N`, but counts idle pooled sockets and is therefore deliberately stricter than worker-local Nginx behavior. Worker-shared Nginx zones, cross-process/cluster-global pools, connection racing, hedging, advanced balancing, and shared circuit breaking remain separate commercial requirements.

## 11.3.2 Upstream Response Header Contract

The implemented profile defines `upstreams[].maxResponseHeaderBytes` with a 65,536-byte default and 8,192..1,048,576 range, plus `upstreams[].maxResponseHeaders` with a 100-field default and 1..1,024 range. HTTP/1 parser growth and field count are bounded before a response is materialized; H2 Header List decoding uses the same byte ceiling. The default HTTP/1 count is left on Hyper's optimized stack path, while non-default values select its finite heap-backed field array.

Every decoded response is checked once more before proxy handoff with a protocol-independent, allocation-free formula: every field occurrence contributes name bytes, value bytes, and four wire-separator bytes, followed by the terminating CRLF. Checked-arithmetic failure, count overflow, or byte overflow drops the response Body without polling it, records an upstream target failure for business or active-health traffic, and exposes only a generic local `502`. Accepted response Bodies and Trailers remain streaming.

The budgets are part of the immutable upstream client generation, so Watch must construct the complete candidate client before publication and retain the previous generation on invalid input. The profile does not define response Body buffering and does not claim Nginx `proxy_buffer_size`, `proxy_buffers`, or `proxy_busy_buffers_size` directive compatibility.

## 11.3.3 Weighted Round-Robin And Least-Connections Contract

`upstreams[].loadBalancing` accepts `round-robin`, `least-connections`, `random-two-least-connections`, and `ip-hash` and defaults to `round-robin`. Unknown tokens, non-string values, Nginx directive fragments, and aliases fail closed. Omission therefore preserves the accepted smooth weighted round-robin behavior.

Every target `weight` defaults to 1 and accepts 1..1,000. For default `round-robin` traffic, every eligible and unattempted target in the selected tier adds its current effective weight, the greatest current weight wins with stable configuration-order tie breaking, and the selected target subtracts the eligible total exactly once. Stable 3:1 weights therefore repeat `A,A,B,A`, stable 5:1:1 weights repeat `A,A,B,A,C,A,A`, and all-one target sets preserve deterministic configuration order.

Target `backup` defaults to false, is strictly boolean, and an upstream must contain at least one primary. Selection uses the primary tier while any unattempted primary is active/passively eligible; only then can it use the backup tier. Weighted selection applies independently inside the selected tier. A primary whose passive ejection expired is eligible for the single half-open claim before backup traffic, while an already claimed probe lets a backup continue serving other requests.

Each immutable upstream generation stores one signed current weight and one recovery marker per configured target in two boxed arrays behind one standard Mutex. The selection critical section performs bounded linear work over at most 1,000 targets and target atomics only; it never allocates, logs, calls I/O, or crosses `.await`. The request path allocates no tier vector, weight-expanded vector, or request-level collection. A half-open compare-exchange race may trigger one bounded fallback scan but never a loop or async waiter. Active-unavailable or ejected targets do not accumulate weight. Their eligibility transition and smooth-state reset share the same short lock, closing the stale-phase insertion window. Lock contention increments one process-lifetime fixed-label operations metric before blocking.

Request-local attempted-target exclusion skips the target without clearing its retained current weight. Primary and backup targets retain independent phases inside the fixed arrays, so selecting backup traffic cannot advance or reset the primary schedule. A Watch generation owns fresh arrays and publishes weight/backup changes atomically; an invalid or all-backup candidate retains the complete active generation.

`least-connections` first resolves the same primary/backup tier and health/probe eligibility, then selects the minimum `activeRequests / weight`. It compares ratios as `leftActive * rightWeight` versus `rightActive * leftWeight` in `u128`, avoiding division, floating point, and overflow for every representable counter value. Exact weighted-load ties reuse the bounded cumulative-weight ticket. A concurrent selector may observe the same atomic snapshot; selection never introduces a blocking lock, waiter, or retry spin.

`random-two-least-connections` implements the Nginx `random two least_conn` intent through a typed SDKWork token. One non-cryptographic generation-local atomic SplitMix64 sequence and multiply-high bounded mapping sample two distinct currently eligible targets without replacement in proportion to slow-start effective weight. The lower `activeRequests / effectiveWeight` candidate wins using the same `u128` comparison. A single eligible target still serves; a half-open race tries the two candidates once and then invokes at most one bounded least-connections fallback. There is no request allocation, candidate Vec, expanded schedule, resampling loop, RNG lock, or waiter. Activity remains SDKWork request/H2-Stream activity rather than Nginx physical connections, and no shared-zone or cross-node random/load state is claimed. Nginx forbids `slow_start` with `random`; accepting SDKWork effective-weight composition is an explicit extension, not directive-level compatibility for that combination.

`ip-hash` follows the Nginx key and recurrence exactly: the first three network-order IPv4 bytes or all sixteen IPv6 bytes update an initial value of 89 through `(hash * 113 + byte) % 6271`. Nominal weights define the stable target range, including targets that are currently health-unavailable, so failure does not remap unrelated clients. An unavailable, attempted, or concurrently lost half-open target advances the same hash at most twenty additional times and then performs one bounded smooth round-robin fallback. Primary/backup eligibility remains authoritative. Initial and safe retry attempts use one effective client IP: the direct accepted peer unless that listener's explicit trusted-proxy policy successfully resolves a bounded forwarding chain. The same address replaces inbound `X-Forwarded-For` upstream. No client map, ring, allocation, lock, log field, or metric label is created. `ip-hash` with any target `slowStartMs` fails before activation, matching Nginx's explicit incompatibility. Arbitrary-variable hash, consistent hash, and cookie affinity remain outside this profile.

Each immutable target owns one `Arc<AtomicUsize>` active-request counter allocated at generation construction. A saturating RAII claim begins before URL construction and upstream I/O. Terminal setup/transport/timeout/Header failures drop it; a safe retry replaces the failed attempt lease exactly once. Final HTTP and rejected-WebSocket responses move it into the downstream Body owner, while accepted WebSockets move it into the supervised tunnel task. Release uses checked atomic decrement and cannot underflow. HTTP/2 requests count per Stream even when Hyper reuses one physical connection; `maxConnections` remains socket ownership.

Watch creates fresh least-connections counters, its independent tie cursor, fresh random state, and fresh smooth arrays in every candidate generation. IP-hash itself stores no mutable affinity state, so unchanged target order/weights and peer IP preserve deterministic mapping. Old streaming responses and tunnels retain their original generation until end, error, cancellation, shutdown, or drop. This closes process-local weighted least-connections, Nginx-intent random-two least-connections, direct-peer IP-hash, and stable smooth weighted round-robin sequences. It does not claim exact Nginx internal tie behavior, shared `zone`, cross-worker or cross-node current-weight/active-count/random state, multi-priority discovery, arbitrary/consistent hash, sticky sessions, or cross-node scheduling.

Optional `targets[].slowStartMs` accepts 100..3,600,000 milliseconds and omission disables recovery ramping. After a passive half-open success, or an active-health unavailable-to-available transition with no remaining passive ejection, the target's effective integer weight starts at one slot and advances monotonically to nominal over generation-relative monotonic time. Repeated unavailability clears obsolete state; repeated recovery restarts the interval. Initial startup and Watch publication use nominal weight because no health recovery occurred.

Both smooth round-robin current-weight additions and least-connections load normalization consume the same effective weight. The calculation uses overflow-safe `u128` arithmetic, never yields zero or exceeds nominal, and compare-exchange cleanup cannot clear or serve one request at stale nominal weight through a newer concurrent ramp. Recovery updates the marker and clears the target current weight in the same short lock as the eligibility transition. One eligible target continues serving without an async waiter, and nominal weight one has no sub-integer observable ramp. Each target adds one immutable duration and one atomic timestamp; there is no timer task, request allocation, queue, or lock across I/O.

This implements a bounded discrete approximation of Nginx `server ... slow_start=time`. It does not claim sub-integer selection probability, exact Nginx timer ticks or request order, shared `zone`, cross-worker/cross-node recovery state, initial-start ramping, arbitrary/consistent hash, sticky sessions, or cluster-global scheduling.

## 11.3.4 Bounded Safe Retry Contract

`upstreams[].retry` is optional; omission preserves one attempt and the original per-request timeout semantics. When present, `maxAttempts` accepts 2..8 and cannot exceed the target count, `timeoutMs` accepts 100..3,600,000 and cannot exceed `requestTimeoutMs * maxAttempts`, and `retryOn` is a non-empty unique subset of the exact Nginx tokens `error`, `timeout`, `http_502`, `http_503`, and `http_504`.

Retry eligibility is fail-closed. Only Body-end-of-stream `GET`, `HEAD`, `OPTIONS`, `TRACE`, `PUT`, and `DELETE` requests may retry. POST, PATCH, any request whose Body or Trailers can still produce Frames, and every WebSocket handshake perform one attempt even when a retry policy exists. No request payload is collected, cloned, buffered, or spooled. Each sequential attempt uses the smaller of the per-attempt request timeout and remaining total retry budget.

One upstream request-admission permit owns the complete attempt sequence. Each actual attempt independently consumes normal DNS/TCP/TLS/physical-connection capacity, validates response Headers, records fixed result/latency metrics, and updates the selected target's existing passive state exactly once. Local request or connection capacity saturation, client Body failure, and invalid response metadata are not retry triggers. A fixed 1,024-bit stack bitmap prevents reuse of any of the at most 1,000 configured targets; selection continues to apply active/passive health and bounded weighted scans without a request-level Set, queue, or expanded schedule.

This is the safe operational subset of Nginx `proxy_next_upstream error timeout http_502 http_503 http_504` semantics. It does not implement `non_idempotent`, request buffering/replay, parallel hedging, response retry after downstream commitment, shared-zone/cluster budgets, or directive-level Nginx config import.

## 11.3.5 Classic WebSocket Proxy Contract

Every `proxy` route is eligible for classic WebSocket without a decorative enable flag. The runtime enters the upgrade path only for an HTTP/1.1 `GET` whose `Connection` fields contain the exact `upgrade` token, whose single `Upgrade` field is exactly `websocket` case-insensitively, and whose request has no Content-Length, Transfer-Encoding, or Expect Body framing. Other well-formed upgrade protocols return `501`; malformed attempts return `400`. HTTP/2-only traffic never enters this path.

The normal proxy route, target selection, DNS/SSRF policy, upstream TLS identity, response Header budgets, active/passive health, `maxInFlightRequests`, and physical `maxConnections` remain authoritative. The runtime removes hop-by-hop request fields, regenerates canonical `Connection: upgrade` and `Upgrade: websocket`, and preserves `Sec-WebSocket-*` fields. An upstream non-`101` response, including authentication rejection, follows the normal bounded streaming response path and then closes the downstream connection. An upstream `101` is accepted only with matching Connection/Upgrade tokens and no response Body framing; invalid metadata records target failure and exposes only generic local `502` output.

A successful tunnel retains the upstream request permit, physical socket permit, and immutable Watch generation until close. It copies raw bytes with fixed 16 KiB buffers per direction, preserves Hyper read-ahead and TCP half-close behavior, stops at `maxConnectionAgeMs`, and observes runtime shutdown within the remaining shared drain budget. No WebSocket frame/message parsing, compression policy, heartbeat, idle timer, RFC 8441 extended CONNECT, generic CONNECT tunnel, or exact Nginx `proxy_read_timeout`/`proxy_send_timeout` compatibility is claimed.

## 11.4 Supervised Active Health Contract

The implemented foundation profile enables active checks when an upstream declares `activeHealth`. The policy defines `GET`/`HEAD`, an origin-form `uri`, finite interval and whole-response timeout, consecutive unhealthy and healthy thresholds, an inclusive success-status range, and a response Body byte ceiling. `limits.maxConcurrentHealthChecks` bounds all concurrent probe operations in one process generation. The URI cannot supply a scheme or authority, and the probe uses the same target hostname, guarded resolver, SSRF address policy, trust roots, mTLS identity, TLS versions, hostname verification, immutable Hyper pool, and physical connection capacity as business traffic.

The contract is an SDKWork-native extension aligned with the operational purpose of Nginx Plus active health checks. It is not represented as an open-source Nginx 1.26 directive and does not establish Nginx Plus config-import compatibility.

One generation-owned Tokio task polls a fixed schedule and a bounded set of probe futures. There is no per-target task, waiter future, overlapping target probe, unbounded Body read, or lock across network I/O. Targets start eligible; consecutive active failures remove one target and consecutive successes restore it. Active availability and passive ejection remain independent, so neither success path erases the other mechanism's isolation state.

Watch starts the complete candidate scheduler before publication and explicitly cancels and joins the old scheduler afterward. Orderly shutdown stops Watch and joins current health work. Process readiness does not become false solely because one dependency is unhealthy; affected upstream routes fail locally with `503` and retry guidance when no target passes both active and passive selection.

Body-content matching, custom headers, TCP/gRPC checks, persisted health, health administration, cross-node consensus, discovery health aggregation, shared retry budgets, shared-zone/cross-process/cluster connection policy, trust or forwarding of non-CRC PROXY v2 TLVs, cryptographic PROXY Header authentication, PROXY AF_UNIX/UDP and outbound `send-proxy`, RFC 7239 `Forwarded`, dynamic proxy-network discovery, arbitrary/consistent-hash, sticky/latency balancing, outlier detection, and shared circuit breaking remain commercial release gates.

## 12. Policy Contract

Policies include:

- Request header, URI, query, cookie, and body limits.
- Response header limits and security headers.
- IP/network access, CORS, method restrictions, and authentication extension references.
- Per-tenant/app/host/route rate and concurrent connection limits.
- Gzip and Brotli negotiation with minimum size and MIME allowlist.
- Static and proxy cache with explicit size, entry, TTL, stale, key, vary, and purge policies.
- Proxy buffering and streaming behavior.
- CSP, HSTS, `nosniff`, frame protection, referrer policy, and permissions policy.

Every queue, cache, buffer, rate bucket, connection pool, and concurrency gate has a finite default and an enforced maximum.

The runtime-wide `maxConcurrentRequests` gate is shared by every listener in one process and holds capacity through response Body completion or cancellation. It is distinct from per-listener connection limits and per-connection HTTP/2 Stream limits.

Optional `deployment.resourcePressure` declares one process-scoped, Restart-only adaptive governor for process Working Set/RSS, finite Linux cgroup v2 memory, Windows HANDLE/Linux FD count, and event-loop wake lag. Absolute byte/handle reserves combine with distinct admission/recovery percentages and consecutive samples. One total request ceiling remains `maxConcurrentRequests`; `operationsReserveRequests` reduces the capacity available to ordinary routes. Only exact fixed-response `GET`/`HEAD` health paths may use the reserve on established connections. This app-level policy cannot weaken a future host/container ceiling and does not represent a hard allocator cap, distributed admission, or complete OOM proof.

Response policy distinguishes `responseBodyIdleTimeoutMs`, which measures meaningful Frame production gaps, from `connectionWriteTimeoutMs`, which measures downstream Socket backpressure. Both have finite defaults and are not aliases for total request, upstream, or Keep-Alive timeouts.

Request ingress separately declares `requestBodyStartTimeoutMs` for the first non-empty Data or Trailer Frame and `requestBodyIdleTimeoutMs` for later meaningful Frame gaps. Both are finite, apply before every selected resource action, ignore empty Data as progress, return `408` on expiry, close HTTP/1, and remain Stream-scoped for HTTP/2. They are not HTTP/1 Keep-Alive idle controls.

`http1KeepAliveIdleTimeoutMs` controls only the gap between completed HTTP/1 response lifecycles and a subsequent request. Its default is 75 seconds for Nginx-profile compatibility. It does not run during request upload, response streaming, pending downstream flush, TLS handshake, initial Header input, or H2 traffic.

`http1MaxPipelineDepth` independently bounds complete HTTP/1 request heads read ahead of Service dispatch. Its default is 16, it stores only a connection-local atomic count, and over-depth connections close without queuing or buffering request Bodies. The field is applied after TLS decryption, bypassed by H2, and classified Restart-only.

HTTP/2 liveness uses `http2KeepAliveIntervalMs` for inbound-Frame inactivity and `http2KeepAliveTimeoutMs` for the corresponding PING ACK. Hyper/H2 owns Frame generation, ACK matching, `GOAWAY(NO_ERROR)`, and close. Responsive idle connections remain alive, so these controls are failure detection rather than a maximum idle-age policy. Both fields are Restart-only and scoped away from HTTP/1.

`maxConnectionAgeMs` provides the separate total-lifetime boundary for HTTP/1, HTTP/2, and successful classic WebSocket tunnels and defaults to one hour, aligned with Nginx `keepalive_time`. Its connection/tunnel-owned timer does not reset on traffic. Expiry stops HTTP/1 reuse, sends H2 `GOAWAY(NO_ERROR)` through Hyper, or drops both upgraded tunnel halves, then bounds accepted work with the shared `drainTimeoutMs` budget. The field is Restart-only and has no per-Stream timer, request Body buffer, WebSocket frame parser, or heartbeat behavior.

URI resource governance declares raw Path bytes, once-decoded Path bytes, Path segment count, Query string bytes, Query parameter count, and Query name/value component bytes. Zero disables all Query input only when all three Query budgets are zero. This validation precedes routing and does not define canonical Nginx normalization or rewrite semantics.

Cache policy additionally declares eligibility, canonical key inputs, `Vary` handling, authorization/cookie behavior, maximum object size, memory and disk budgets, stale behavior, revalidation, collapsed forwarding, purge authorization, and cache-poisoning defenses. Disk spooling and cache writes have per-app and process quotas and must fail without exhausting the runtime volume.

## 13. Observability Contract

Configuration declares:

- Structured access and error log profiles.
- Redaction policy.
- Metrics profile and low-cardinality dimensions.
- Trace sampling and propagation.
- Slow request/upstream thresholds.
- Health, readiness, and metrics exposure policy.

Raw tokens, private keys, authorization headers, cookies, query secrets, request bodies, absolute private paths, and unbounded user values must not be logged or used as metric labels.

## 14. Deployment Contract

Deployment declares supported standalone/cloud profiles, node selectors, revision strategy, canary size, health gates, drain timeout, convergence timeout, automatic rollback policy, and offline-node behavior.

The application config does not contain node credentials or mutable node inventories. Runtime assignments are control-plane state bound to the immutable app revision.

The app contract cannot configure process service accounts, global worker/thread counts, file descriptor limits, runtime directories, crash-dump policy, profiling exposure, or the node administrative listener. Those are host-owned controls so one tenant cannot affect the isolation or availability of other applications.

## 15. Configuration Precedence

Precedence from lowest to highest:

1. Product profile defaults.
2. App-owned `sdkwork.webserver.config.json`.
3. Source-controlled non-secret environment/profile overlay.
4. Published control-plane deployment overlay.
5. Secret/KMS/certificate/discovery resolution.
6. Operator emergency override with expiry, audit, and explicit rollback.

String interpolation is forbidden for security-sensitive values. Typed references are resolved at compile or activation time. A missing required binding is a blocking error.

## 16. Versioning And Lifecycle

- `schemaVersion` changes only when the machine contract changes.
- Additive optional fields may remain within the same schema version when old consumers reject or safely ignore them according to the schema contract.
- Published configurations are canonicalized, checksummed, immutable, and content-addressable.
- Every change supports validate, explain, diff, plan, publish, status, and rollback.
- Breaking schema or behavior changes require migration tooling, compatibility notes, and human review.

## 17. Acceptance Criteria

- Every active Web application root contains a valid app manifest and referenced Web Server configuration.
- All five default profiles validate and execute on standalone and cloud test topologies where applicable.
- Schema, semantic, security, resource-budget, Nginx-compatibility, and deployment validation produce deterministic diagnostics.
- Unknown fields, missing references, secrets, unsafe paths, listener conflicts, route ambiguity, and unbounded settings are rejected.
- The same canonical configuration produces equivalent normalized IR on Windows, Linux, and macOS tooling.
- Generated Nginx output and Rust execution pass the declared compatibility conformance suite.
- Logical listeners from multiple applications compile into conflict-free physical sockets under host policy.
- Resolver, cache, disk spool, connection, body, header, regex, route, and observability budgets remain finite after every precedence layer is applied.
- Application configuration examples contain no live credentials, private keys, environment-specific machine paths, or generated runtime state.
