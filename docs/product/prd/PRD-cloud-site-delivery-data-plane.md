# SDKWork Cloud Site Delivery Data Plane PRD

Status: in-progress
Owner: SDKWork Web Server maintainers
Application: sdkwork-web
Updated: 2026-07-22
Requirement: REQ-2026-0060
Parent: [PRD.md](PRD.md)
Specs: REQUIREMENTS_SPEC.md, DOCUMENTATION_SPEC.md, NGINX_SPEC.md, SECURITY_SPEC.md,
PRIVACY_SPEC.md, PERFORMANCE_SPEC.md, OBSERVABILITY_SPEC.md, DEPLOYMENT_SPEC.md, TEST_SPEC.md

## 1. Purpose

Define the Web Server runtime behavior required to deliver cloud Sites whose sources are opaque live
Drive WebsiteRoots (whole-Space or selected-folder) or canonical Knowledgebase Wiki publications.
The Web Server is the HTTP/TLS execution plane. It
does not become an independent writable authority for cloud Site, domain, Variant, Mount,
certificate, or commercial state.

The data plane consumes two independently activated inputs:

- an immutable `WebsiteRuntimeDescriptor` compiled by `sdkwork-deployments`;
- a node-scoped TLS runtime snapshot containing only approved certificate assignments/material.

File and page changes are resolved live from source providers and do not require a configuration
reload, Deploy Release, or SiteRevision.

## 2. Users And Jobs

| User/system | Job |
| --- | --- |
| Anonymous browser | Receive the correct secure representation for host, path, and client class. |
| Tenant operator | Trust that preview/activation/rollback has observable runtime evidence. |
| Platform SRE | Operate bounded, horizontally scalable, last-known-good dedicated tenant fleets. |
| Security operator | Verify TLS, routing, origin confinement, headers, and non-disclosure. |
| Drive provider | Resolve public directory content without exposing storage topology. |
| Knowledgebase provider | Resolve only eligible public Wiki routes and rendered representations. |
| Deploy control plane | Distribute desired immutable snapshots and receive observations. |

## 3. Goals

- Deterministically route exact and standards-compliant single-label wildcard hosts, longest Binding prefixes, client Variants, and
  longest Mount prefixes from an in-memory immutable snapshot.
- Deliver STATIC, SPA, and WIKI handlers with Nginx-compatible supported root/alias/index behavior.
- Treat Drive `SPACE_ROOT`/`FOLDER` selection as provider-owned WebsiteRoot metadata and keep it
  distinct from URL Mount `ROOT`/`ALIAS` translation.
- Use typed provider resource ports/SDKs; never use arbitrary filesystem paths, raw origin URLs,
  provider object keys, or handwritten HTTP/auth.
- Make ordinary provider changes visible within the publishing freshness target through events and
  read-through revalidation.
- Protect private/draft/revoked/deleted content across cache, origin, and failure paths.
- Activate website and TLS snapshots atomically and independently while preserving last-known-good
  service.
- Stream bodies and keep connections, queues, caches, descriptors, rendering, background work, and
  telemetry cardinality bounded.
- Emit request, cache, origin, revision, certificate, and usage evidence required for commercial
  operations.

## 4. Non-Goals

- Own cloud Site authoring, domain claims, certificate orders, entitlements, billing, or Wiki page
  state.
- Continue serving writable `/app/v3/api/applications...` control-plane operations or authoritative
  `web_site`, `web_domain`, `web_deployment`, and `web_certificate` state after cutover.
- Infer publication from Drive Space visibility or filesystem/object-store reachability.
- Reconstruct, override, or authorize a Drive root selector or Knowledgebase publication from
  diagnostic Space/folder fields in the descriptor.
- Build React applications or compile Markdown into immutable release bundles on each change.
- Execute customer server code, arbitrary plugins, arbitrary regular expressions, or untrusted
  templates.
- Expose bucket/object/presigned/private upstream identities through public errors or descriptors.
- Guarantee performance of customer-authored JavaScript after the server response is delivered.

## 5. Runtime Inputs And Truthfulness

The node accepts a website snapshot only after schema, size, canonical hash, signature/source,
compatibility, referential integrity, policy, and local safety validation. It stages the complete
snapshot, builds all indexes away from request threads, then swaps one immutable pointer. A partial
snapshot is never visible.

The node accepts a TLS snapshot only after assignment, decrypt/secret resolution, key/certificate
match, chain, hostname, validity, algorithm, policy, and local load validation. Website activation
never waits for an unrelated certificate renewal, and certificate renewal never creates a website
revision.

Success observations mean the exact revision/hash or certificate fingerprint is loaded and probed.
Receiving a message, writing a file, or updating a database row is not success evidence.

## 6. Request Behavior

### 6.1 Ingress And Binding

1. Apply listener, connection, protocol, header, URI, and admission bounds.
2. Normalize SNI, Host, scheme, port, and path using one canonical policy.
3. Serve an active, exact ACME HTTP-01 token before ordinary Site routing.
4. Match exact host before an approved single-label wildcard and longest segment-aware Binding path.
5. Reject absent, ambiguous, paused, unverified, or incompatible bindings without tenant disclosure.

### 6.2 Variant And Mount

Apply forced Binding Variant, signed preference, path rule, Client Hints, bounded User-Agent, bot,
Binding default, then Site default. The selected reason is internally observable. It cannot grant
authorization.

Within the Variant, choose the longest segment-aware Mount prefix. Translate the remainder using
`ROOT` or `ALIAS`. Normalize provider-relative paths once, reject traversal/encoded ambiguity, and
keep them confined to the declared provider root.

Mount `ROOT`/`ALIAS` is URL translation only. It is unrelated to Drive WebsiteRoot
`SPACE_ROOT`/`FOLDER`: Web Server receives an opaque stable WebsiteRoot reference and starts every
provider-relative lookup at `/` of its current effective generation. It never prepends a Space path
or selected folder UUID from descriptor diagnostics.

### 6.3 Handler Behavior

| Handler | Required behavior |
| --- | --- |
| STATIC | directory-faithful files, bounded index lookup, conditional/range support, no listing by default |
| SPA | STATIC behavior plus an explicit resource-confined fallback for eligible navigation requests |
| WIKI | provider-owned route/page resolution, sanitized rendering, navigation/search/SEO metadata, visibility enforcement |

SPA fallback shall not turn a missing hashed asset into `index.html` merely because the Site is an
SPA. WIKI shall never use Drive visibility alone and shall never expose `okf/`, `output/`,
`.sdkwork/`, or paths outside `sources/raw`.

### 6.4 Response

Apply provider/version-aware `ETag`/`Last-Modified`, cache, MIME, range, compression, CSP,
`nosniff`, referrer, frame, robots, redirect, and error policies. Stream response bodies. Record a
bounded outcome and usage fact after response accounting without delaying the client on the billing
pipeline.

## 7. Freshness And Cache Requirements

- Cache identity includes tenant-safe Site/Binding/Variant/Mount/resource/path/public-version and
  renderer/template representation identity.
- Provider events are idempotent and invalidate exact keys or advance a provider-wide generation.
  Wiki private processing changes no public cache version; a page edit advances its route-scoped
  page public version instead of flushing the full Wiki.
- Event gaps and reconnects force provider revalidation; negative cache TTLs are short.
- A transition from public to private/deleted/revoked has priority invalidation and cannot be served
  indefinitely from stale cache.
- Allowed stale serving is explicit per policy, bounded by time, and applies only to representations
  previously verified public.
- Concurrent cache misses coalesce within a bounded single-flight mechanism; overload does not form
  an unbounded origin queue.
- Hashed immutable assets may use long cache policy only after the provider declares the content
  version immutable.

## 8. Provider Integration

The Drive provider port supports eligibility validation for both Space-root and folder-root
WebsiteRoots, path metadata resolution from provider `/`, conditional open, range/open stream,
version observation, and content change events. The Knowledgebase provider port resolves the one
canonical publication for a Knowledgebase resource and supports publication validation, public route
resolution, rendered page/navigation/search representations, asset open, state/version observation,
and change events. A DRAFT/PAUSED Wiki fails public eligibility even if it is present in a descriptor.
Knowledgebase events are consumed directly from the provider event authority; Deploy remains the
configuration/snapshot authority and is not a per-content-event proxy.

The implemented runtime accepts the four Drive WebsiteRoot and five Knowledgebase Wiki owner event
types on a separate loopback listener. Subscription identity is bound to provider, tenant scope,
tenant, organization and Drive channel as applicable; owner-specific HMAC signatures and replay
windows are verified before strict event parsing. Per-stream dual-slot checkpoints, bounded
deduplication, Drive contiguous-gap detection, Knowledgebase monotonic ordering, and generated-SDK
Provider reconciliation make event restart/replay behavior durable on each Web Node. The delivery
executor owns a bounded node-local resolution metadata cache with positive and short negative TTLs,
positive-only stale-while-revalidate, bounded same-key single-flight, and LRU eviction. The same
cache instance is injected into the event processor: route events evict exact Provider paths,
provider/navigation/search changes evict that Provider resource, and an uncertain stream clears the
affected Provider type. Provider epochs prevent an in-flight pre-invalidation result from being
reinserted. Response body bytes remain uncached.

Cloud runtime integration uses owner-generated `sdkwork-drive-internal-sdk` and
`sdkwork-knowledgebase-internal-sdk` through injected clients and
the approved authentication/runtime context. Same-process standalone composition may use an
equivalent typed Rust service port. Raw HTTP, manual auth headers, direct provider database access,
and direct object-storage SDK access are prohibited.

## 9. TLS Execution

Web Server reuses its bounded ACME provider and certificate activation crates as execution
capabilities. In cloud mode, Deploy owns account/order/challenge/certificate metadata and scheduling;
Web Server workers execute typed jobs and report redacted observations. In approved standalone mode,
the same runtime engine may be driven by local configuration without claiming cloud authority.

HTTP-01 token handling precedes Site routing only for an active authorized token. Certificate
versions are hot-loaded with SNI verification; existing connections retain the old context and new
connections use the verified current context. Failed renewal/load/probe keeps the previous valid
version active.

## 10. Operations And Admin Projection

Web Server supplies the data behind Deploy platform-admin views:

- node version, region, readiness, capacity, connection and queue pressure;
- desired/observed website revision and descriptor hash;
- desired/observed certificate version and served fingerprint;
- request/error/latency/bytes/cache/origin summaries;
- provider health, event lag, invalidation backlog, circuit state, and stale serving;
- rejected descriptors/TLS snapshots with bounded reason codes;
- rollout, probe, drain, restart, and rollback observations.

Nodes do not provide a competing tenant Site editor. Local diagnostics are operator-only, protected,
bounded, and do not reveal content, secrets, private endpoints, or uncontrolled host/path labels.

## 11. Security And Privacy

- Enforce tenant-qualified runtime/cache/provider identity even though the public request is
  anonymous.
- Reject Host/SNI mismatch according to listener policy, encoded traversal, null/control bytes,
  invalid ranges, header injection, unsafe redirects, and root escape.
- Deny dotfiles, source maps, manifests, active content types, and reserved paths according to the
  compiled policy; secure defaults win when the descriptor omits a field.
- Bound Wiki render input/output/time and sanitize HTML, links, images, SVG, embeds, and metadata.
- Redact tokens, cookies, query values, private paths, secret references, certificate material,
  provider payloads, and customer content from logs/support bundles.
- Apply trusted-proxy policy before using client IP or forwarded request scheme. Only a configured
  direct peer may provide a single exact `X-Forwarded-Proto: http|https`; malformed trusted values
  fail closed, untrusted values are ignored, and native TLS cannot be downgraded. IP and User-Agent
  telemetry follow retention, minimization, and consent requirements.
- Public not-found behavior does not distinguish missing Site, private page, wrong tenant, paused
  publication, or revoked resource.

## 12. Performance And Reliability Targets

| Target | Objective |
| --- | --- |
| Snapshot routing lookup | bounded indexed lookup; no scan across all Sites/certificates |
| Cached static p95 server latency | <= 100 ms in-region |
| Eligible uncached provider p95 | <= 500 ms in-region |
| Content freshness | p95 <= 5 seconds, p99 <= 30 seconds after provider commit |
| Website activation | p95 <= 30 seconds to target quorum |
| Graceful reload | no partial map; established connections drain on prior context |
| Availability | 99.95% standard target after production certification |

The node continues last-known-good website and TLS snapshots through temporary control-plane
failure. Provider failure uses bounded timeout/retry/circuit behavior and policy-governed stale
cache. A node with invalid/missing assigned snapshots fails readiness for affected traffic rather
than inventing defaults.

## 13. Observability And Metering

Every request correlates trace, node, region, Site, Binding, revision, Variant, Mount, provider type,
resource, cache result, status, bytes, and latency. Metrics use bounded identifiers or aggregated
labels; arbitrary hostname/path/page title is not a metric label.

Usage events include stable deduplication/window identity and are delivered asynchronously. The
data plane reports facts; Deploy aggregates and Commerce prices them. Backpressure, spool capacity,
loss policy, replay, duplicate handling, and reconciliation are observable and tested.

## 14. Acceptance Criteria

- Golden tests prove deterministic descriptor validation/indexing/hash and atomic pointer swap.
- Property/fuzz tests cover Host, SNI, IDNA, path, prefix, alias/root, redirect, Variant, range,
  cache key, and traversal behavior.
- End-to-end tests route one domain to independent desktop/mobile Drive builds and explain the
  selected rule.
- STATIC/SPA tests preserve directory layout and prevent fallback from masking missing assets.
- STATIC/SPA provider tests serve equivalent trees from Drive `SPACE_ROOT` and `FOLDER`, reject
  reserved/cross-root access, and prove Mount `ROOT`/`ALIAS` never changes provider root identity.
- WIKI tests prove per-page state/visibility, sanitization, navigation/search/assets, and reserved
  root denial.
- WIKI tests prove every-Knowledgebase capability does not imply public access and that one canonical
  publication can safely serve multiple Site Resources/Mounts without cache-key collision.
- Provider event, loss, replay, out-of-order, public-to-private, atomic-root-switch, stampede, and
  outage tests satisfy freshness and non-disclosure requirements.
- Exact Drive/Knowledgebase AsyncAPI and internal SDK dependencies are declared by the Web Server
  app/component manifests and pass standalone/cloud compatibility fixtures.
- Native auto-public, explicit publish, and priority revocation p95/p99 tests prove route-scoped
  invalidation and no global cache churn from private processing.
- Deploy is the only writable Site/domain/TLS authority; overlapping Web app-api write routes and
  `web_*` business-table authority are retired with shadow-compare and rollback evidence.
- TLS tests prove challenge precedence, immutable versions, SNI selection, hot switch, served
  fingerprint, failed-renewal retention, and website/TLS snapshot independence.
- Load/soak tests prove bounded memory, queues, caches, descriptors, origins, rendering, events, and
  metrics cardinality.
- Node/control-plane/provider outage and last-known-good/rollback drills have recorded evidence.

## 15. Implementation Status

The first executable boundary is implemented in `sdkwork-webserver-core::website_runtime`:

- `specs/sdkwork.website-runtime.descriptor.schema.json` is the strict supported v1 consumer
  contract;
- the bounded loader verifies canonical payload SHA-256 before semantic activation;
- semantic validation rejects non-canonical identities/paths/order, broken references, conflicting
  Host/path or Variant/Mount ownership, unsafe redirects, handler/capability mismatch, provider
  topology leakage, and runtime limits above hard ceilings;
- compilation builds immutable Host, Binding path, Variant-rule, Mount path, Resource, and Variant
  indexes; request selection has no control-plane database dependency;
- `sdkwork.website-runtime-set.v1` groups a bounded, stable-ordered set of complete Site descriptors
  for one node/environment; its monotonic generation prevents delayed compilation or distribution
  from overwriting newer state. The registry compiles the entire candidate before serializing
  writers and swapping one read pointer, retains one prior complete set, and never exposes a
  partial, stale, conflicting, or otherwise rejected candidate;
- Binding-relative URL handling and Mount `ROOT`/`ALIAS` translation cannot mutate the opaque
  `providerResourceUuid`;
- `sdkwork.tls-runtime.v1` independently validates node assignment metadata, certificate material
  references, expected fingerprints, validity metadata, bounded TLS/ALPN policy, and exact/wildcard
  SNI ownership without carrying certificate or private-key PEM;
- `sdkwork-webserver-contract::provider` defines injected resource eligibility, static resolve/open,
  Wiki route/open/navigation/search, cursor pagination, conditional/range metadata, typed failures,
  redacted content handles, and incremental stream ports without selecting a transport.
- `sdkwork-webserver-knowledgebase-provider` consumes the generated Knowledgebase Rust Internal SDK
  through an injected tenant-bound resolver and implements ACTIVE-publication validation,
  PAGE/REDIRECT resolution, exact content revalidation, navigation/search generations, conditional
  requests, versioned ETag/Last-Modified metadata, bounded content, deadlines, and non-disclosing
  error mapping. Its focused provider suite passes.
- `sdkwork-webserver-drive-provider` consumes the generated Drive Rust Internal SDK and implements
  WebsiteRoot validation plus static resolve/open for both `SPACE_ROOT` and `FOLDER`, generation and
  NodeVersion revalidation, conditional and range requests, `If-Range`/`416`, tenant isolation,
  traversal confinement, bounded content, deadlines, and non-disclosing SDK error mapping. Its
  focused provider suite passes.
- `sdkwork-webserver-delivery-runtime` owns an immutable provider registry and a transport-neutral
  executor that routes from the active runtime-set to STATIC/explicit SPA fallback/WIKI providers,
  preserves the complete compiled scope, handles HEAD/conditional/redirect/Range outcomes, and
  bounds streams again at the consumer boundary. It also validates each unique logical resource on
  the handler-specific Provider port before activation using bounded concurrency and descriptor
  deadlines, rejecting missing ports, invalid Provider evidence, and unsupported object limits;
- after Provider validation, an isolated candidate-only registry executes bounded `HEAD` delivery
  probes for every Binding and every reachable selectable device Variant with
  `WebsiteProviderPurpose::Activation`. A failed candidate reports `ACTIVATION_PROBE_FAILED`, is not
  persisted into the recovery slot, and cannot replace the live last-known-good registry. STATIC
  and SPA Variants require a resolvable entrypoint; WIKI probes its publication root. Focused
  executor, provider-validation, activation-probe, and last-known-good retention suites pass.
- the dedicated `sdkwork-web-server-website-delivery-edge-runtime` process loads and watches a bounded, hash-verified
  runtime-set, binds the process and both generated Provider clients to one configured tenant scope,
  loads Drive and Knowledgebase ingress tokens from secret files, registers both adapters, validates
  every candidate before atomic activation, and calls the executor directly from the existing
  bounded HTTP/HTTPS listener;
- production `cloud` assignment mode consumes the generated Web Internal Rust SDK with a protected
  Web Node token, performs conditional node/environment assignment pulls, rejects response identity
  or hash mismatch, continues from durable last-known-good state during temporary control-plane
  loss, and resumes the persisted observation phase through `RECEIVED`, `VALIDATED`, `STAGED`,
  `ACTIVE`, or terminal `REJECTED`; local `file` mode remains a standalone/development source;
- node-local A/B slots durably retain complete activated runtime-sets, recover the highest valid
  generation after restart or temporary source loss, reject stale/same-generation-conflicting and
  cross-node/environment candidates, and are mandatory in staging and production;
- a separate loopback-only provider-event ingress authenticates provider/tenant/channel-bound Drive
  and Knowledgebase deliveries, strictly consumes all nine owner event types, and drives bounded
  per-stream ordering, deduplication, dual-slot checkpoints, uncertainty, generated-SDK Provider
  reconciliation, and invalidation without activating a Site revision;
- HTTP mapping now covers GET/HEAD, `200`/`304`/`308`/`404`/`412`/`416`/`429`/`502`/`503`, exact
  content/range metadata, canonical locations, query-safe redirects, client-hint Variant selection,
  force-HTTPS, security headers, and incremental response-body chunks. Focused browser-adapter tests
  pass through the real Drive and Knowledgebase provider adapters and injected SDK port fakes.

This is not production completion. Authenticated assignment publication and convergence are
implemented across Deploy and Web: Deploy uses the generated Web Internal SDK for publication and
latest-observation reads; Web performs Provider validation plus an isolated node-local activation
probe before staging and atomic website activation; Deploy persists immutable per-target evidence
and advances `deploy_site.current_revision_id` only after strict all-frozen-target `ACTIVE` quorum.
Detached source attestation where required, external public-domain multi-vantage probes, atomic TLS
pointer activation, TLS material/key/chain validation, credential rotation/reload, provider-aware
cache and concrete event-driven cache invalidation, production drift dashboards/alerts, fleet
telemetry, single-writer cutover, and deployed browser E2E remain mandatory P0 work. The
legacy `data-plane` operation intentionally continues to use `ResourceConfig`; website delivery
uses the independent edge runtime with the management feature disabled. True upstream streaming is still absent
because both generated owner SDKs return bounded `Vec<u8>` content. Candidate activation therefore
enforces a 16 MiB Knowledgebase or 256 MiB Drive object ceiling; supporting the schema's future
1 TiB ceiling requires generated streaming APIs. The sanitizer/rendition chain, rendition-backed
full-text search, negative-cache/single-flight/stampede controls, and invalidation-storm evidence
also remain open. Node-local runtime-set recovery and event checkpoints are implemented; they are
not Deploy business authority and do not replace immutable rollout evidence or production
restart/backup-restore drill evidence. The node-local `HEAD` probe proves only candidate
route/provider resolution on one Node and does not replace public DNS, certificate/SNI, CDN, or
multi-region Internet reachability probes.

The cloud topology, image entrypoint, release archive, and Kubernetes workload now select the
dedicated edge-runtime binary. The production-deployable baseline is one dedicated fleet per
tenant scope. A required non-sensitive `tf-` plus 15-symbol random Base32 fleet label partitions
the Website Service, Pod selectors, NetworkPolicy, and the PodDisruptionBudget; each Node receives
a separate provider-event Service, and tenant scope hashes and provider credentials remain only in
per-Node Secrets. Each rendered StatefulSet is single-replica by design
and binds one Node Secret to one recovery PVC; production high availability requires at least two
independently rendered Node instances in the same tenant fleet and on distinct Kubernetes workers.
Hostname topology spread is mandatory and availability-zone spread is preferred. Built-in exec
probes reach only the loopback operations listener. A bounded
relay sidecar preserves provider callback bytes and NetworkPolicy separates website traffic from
the callback ingress through namespace-and-Pod selectors. Rendering requires explicit direct
ingress CIDRs, runs the real listener compiler, and emits a hash-versioned immutable per-Node
ConfigMap; Website
delivery, reverse-proxy forwarding, and logs share the resulting trusted external scheme. Internal
HTTPS/mTLS termination and source identity, plus multi-node rollout evidence, remain P0 deployment
gates rather than implicit operator steps.

Every Node requires an independent owner event subscription and exact callback route to
`sdkwork-web-events-<tenant-fleet-name>-<node-name>`. Kubernetes Service load balancing is not
event fan-out: sharing one callback Service across a fleet could deliver a signed event to a Node
with another subscription secret and would leave other Node-local checkpoints uninformed. A
provider without independent Node subscriptions requires an owner-approved durable fleet fan-out
contract before event-driven cache can be enabled.

A shared multi-tenant edge fleet remains out of scope for the current production baseline. It may
be productized only after the owning services publish tenant-aware assignment and credential
broker contracts, per-tenant generated Provider SDK client lifecycle and hot rotation,
multi-tenant event subscription authority, bounded tenant cache/client eviction, and
tenant-qualified readiness, drift, usage, rollout, and rollback evidence. Local token maps, tenant
headers, raw HTTP, and direct provider storage access are prohibited substitutes.

## 16. Dependencies

- Deploy product authority: `sdkwork-deployments/docs/product/prd/PRD-cloud-site-publishing-platform.md`
- Deploy descriptor/data authority: `sdkwork-deployments/docs/architecture/tech/TECH-cloud-site-publishing-control-plane.md`
- Local architecture: [TECH-cloud-site-delivery-data-plane.md](../../architecture/tech/TECH-cloud-site-delivery-data-plane.md)
- Local decision: [ADR-20260721 Compiled Website Runtime Descriptor](../../architecture/decisions/ADR-20260721-compiled-website-runtime-descriptor.md)
