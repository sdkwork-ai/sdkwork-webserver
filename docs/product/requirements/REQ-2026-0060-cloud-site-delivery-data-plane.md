# REQ-2026-0060 Cloud Site Delivery Data Plane

```yaml
id: REQ-2026-0060
title: Deliver live Drive directory and Knowledgebase Wiki Sites from compiled runtime descriptors
owner: SDKWork Web Server maintainers
status: in-progress
source: platform
problem: The Web Server needs a bounded execution contract for cloud Sites without becoming a second writable publishing authority or requiring a release for every source change.
goals:
  - atomically consume website and TLS runtime snapshots
  - route domains, paths, Variants, and Mounts deterministically
  - stream eligible public content through typed Drive and Knowledgebase provider ports
  - preserve freshness, tenant isolation, last-known-good service, and commercial telemetry
non_goals:
  - own Site/domain/certificate business metadata in cloud mode
  - build or release source content per update
  - access source databases or object stores directly
affected_surfaces:
  - webserver-edge-runtime
  - webserver-acme-service
  - webserver-certificate-worker
  - runtime-bootstrap
  - observability
  - deployment
```

Specs: REQUIREMENTS_SPEC.md, ARCHITECTURE_DECISION_SPEC.md, API_SPEC.md, SDK_SPEC.md,
APP_SDK_INTEGRATION_SPEC.md, CONFIG_SPEC.md, DEPLOYMENT_SPEC.md, NGINX_SPEC.md,
SECURITY_SPEC.md, PRIVACY_SPEC.md, PERFORMANCE_SPEC.md, OBSERVABILITY_SPEC.md, TEST_SPEC.md

## Requirements

1. Validate and atomically activate a complete `WebsiteRuntimeDescriptor` with deterministic
   canonical hash and bounded indexes.
2. Validate and activate a separate node-scoped TLS snapshot; website and certificate changes shall
   not create revisions in each other's lifecycle.
3. Resolve exact/wildcard Host, longest Binding path, Variant precedence, and longest Mount path
   without scanning all tenants or Sites on the request path.
4. Implement STATIC, SPA, and WIKI handlers with root confinement, safe index/fallback behavior,
   streaming, conditional/range support, MIME and security policy.
5. Resolve content only through injected Drive/Knowledgebase generated SDK clients or typed service
   ports. Do not use raw HTTP/manual auth, provider DBs, arbitrary filesystem paths, or object keys.
6. Consume idempotent provider change events and use read-through revalidation so ordinary content
   changes do not require descriptor activation.
7. Prevent stale public cache from bypassing private/deleted/revoked transitions.
8. Reuse bounded ACME and certificate runtime primitives for typed Deploy jobs and report actual
   served observations.
9. Continue last-known-good snapshots during temporary control-plane failure and fail readiness when
   no valid assigned state exists.
10. Emit bounded observability and deduplicated usage facts without logging content or secrets.
11. Treat a Drive WebsiteRoot as an opaque provider resource whether its owner selected
    `SPACE_ROOT` or `FOLDER`. Mount `ROOT`/`ALIAS` translates URL paths only and shall never retarget
    the Drive source root from descriptor diagnostics.
12. Treat every Knowledgebase as potentially Wiki-capable but require provider validation of its one
    canonical WikiPublication and `ACTIVE` status on every activation/revalidation path. Multiple
    Site Resources referencing one provider UUID remain isolated by the full runtime/cache identity.
13. Declare and consume owner-generated `sdkwork-drive-internal-sdk` and
    `sdkwork-knowledgebase-internal-sdk` dependencies plus their AsyncAPI event authorities.
    Knowledgebase provider events flow directly to Web Server; Deploy is not a per-content-event
    relay. Provider endpoint/credentials are runtime configuration and never descriptor fields.
14. Retire writable Web Site/Domain/Deployment/Certificate app-api routes and `web_*` business
    authority after the approved Deploy single-writer cutover. Web persistence is limited to
    immutable snapshots, checkpoints, bounded cache metadata, observations, audit and usage spool.
15. Separate provider-wide generation, route page/static content version, navigation/search
    generation, and Deploy SiteRevision policy generation. Wiki private processing must not advance
    a public cache version or flush unrelated routes.
16. Deploy the current cloud runtime as one dedicated fleet per tenant scope. Every Website,
    provider-event, and headless Service plus every matching Pod selector, NetworkPolicy, and
    PodDisruptionBudget shall include one stable non-sensitive fleet label matching
    `^tf-[a-z2-7]{15}$`. Each provider-event
    Service shall additionally select exactly one Web Node because its Drive channel, HMAC secret,
    and checkpoint are Node-bound. Drive shall use the canonical Node-qualified callback
    `/nodes/{nodeUuid}/provider-events/drive-website-events`; the unqualified provider-event route
    shall accept Knowledgebase only. Tenant identity and tenant scope hashes shall not appear in
    object names or labels and remain Secret/runtime data.
17. Do not claim a shared multi-tenant edge fleet until owner contracts exist for tenant-aware
    assignment, credential brokerage and hot rotation, per-tenant generated Provider SDK client
    lifecycle, multi-tenant event subscriptions, bounded tenant cache/client eviction, and
    tenant-qualified operations evidence.
18. A highly available tenant fleet shall contain at least two Node identities scheduled on
    distinct Kubernetes hosts, prefer distinct labeled zones, and use a fleet-scoped disruption
    budget. Multiple Pods on one host do not satisfy high-availability acceptance.

## Implementation Status

Implemented foundation as of 2026-07-23:

- strict Draft 2020-12 consumer schema for `sdkwork.website-runtime.v1`;
- bounded JSON ingestion, canonical SHA-256 verification, collection-order and referential checks;
- opaque Drive/Knowledgebase provider references with no URL, object-key, database, or credential
  fields;
- immutable exact/wildcard Host, longest Binding prefix, Variant precedence, and longest Mount
  prefix indexes;
- strict node-scoped `sdkwork.website-runtime-set.v1` envelope with a 64 MiB/10,000-Site ceiling,
  canonical outer hash, monotonic JSON-safe generation, stable Site ordering, cross-Site Host/path
  conflict rejection, and node/environment scope enforcement;
- whole-set off-path compilation, serialized control-plane writers, one-pointer activation,
  unchanged-snapshot idempotency, stale/same-generation-conflict rejection, one complete in-memory
  rollback generation, and failed-candidate retention of the current set;
- segment-aware Binding-relative routing, `ROOT`/`ALIAS` URL translation, redirect structure, and
  fail-closed denied-path/dotfile policy;
- independent node-scoped `sdkwork.tls-runtime.v1` assignment schema, canonical hash, strictly
  increasing JSON-safe generation, stale/same-generation-conflict fencing, exact/wildcard SNI
  index, TLS policy bounds, and raw private-key rejection;
- native Rustls assignment consumption through an explicit `tlsRuntime: assignment` listener,
  protected `file:<opaque-version-id>` material provider, canonical root/symlink confinement,
  bounded PEM parsing, SAN/current-time/declared-time/fingerprint/key-match validation,
  listener-wide policy and ALPN compatibility checks, exact-before-single-label-wildcard SNI,
  enforced TLS 1.2/1.3 bounds, off-path complete-context construction, atomic hot activation,
  unchanged-hash fast path, failed-candidate last-known-good retention, and node-local A/B restart
  recovery required for staging/production native TLS;
- typed resource/static/Wiki provider ports with opaque redacted content handles, incremental body
  streams, conditions/range metadata, public/provider generations, and cursor page-size bounds;
- generated Knowledgebase Internal SDK adapter with ACTIVE publication checks, Wiki page/redirect
  resolution, exact content-handle revalidation, navigation/search generations, conditional
  requests, bounded content, tenant-scoped resolution, and non-disclosing error mapping;
- generated Drive Internal SDK adapter with ACTIVE WebsiteRoot checks, `SPACE_ROOT`/`FOLDER`,
  `LIVE_TREE`/`ATOMIC_GENERATION`, exact path/generation/NodeVersion revalidation, conditional and
  byte-range behavior, `If-Range`/`416`, bounded content, tenant isolation, and non-disclosing error
  mapping;
- immutable delivery executor for STATIC, explicit SPA fallback, and WIKI with complete compiled
  scope propagation, canonical route reverse mapping, exact Range response evidence,
  consumer-owned provider/chunk deadlines, and force-HTTPS redirects;
- dedicated `sdkwork-webserver-website-delivery-edge-runtime` bootstrap and process entrypoint with production cloud assignment pull through
  the generated Web Internal Rust SDK, protected Web Node credential injection, conditional
  generation/hash retrieval, strict node/environment/hash checks, resumable phased observations,
  and a local bounded file source limited to standalone/development; both sources retain monotonic
  activation, explicit single-tenant credential scope, and handler-aware Provider validation with
  bounded concurrency before every activation;
- node-local A/B persistence of complete activated runtime-sets, highest-generation restart/source
  recovery, corruption fallback, stale/conflict/scope rejection, and mandatory staging/production
  recovery-directory configuration without Web business-database ownership;
- independent loopback-only provider-event ingress with provider/tenant/organization/channel-bound
  subscriptions, secret-file credentials, bounded body/time-window/concurrency checks, Drive
  derived-key `v1=` HMAC verification, Knowledgebase `sha256=` HMAC/header verification, canonical
  Node-qualified Drive routing, and fail-closed wrong/missing-Node rejection;
- strict consumption of four Drive WebsiteRoot and five Knowledgebase Wiki owner events, including
  Drive contiguous and Knowledgebase monotonic-non-contiguous ordering, bounded deduplication,
  conflict fail-closed behavior, initial/gap/uncertain generated-SDK reconciliation, and node-local
  per-stream dual-slot durable checkpoints with corruption fallback and bounded cross-stream
  concurrency;
- public HTTP mapping for GET/HEAD, conditional and redirect outcomes, malformed Range rejection,
  mobile Client Hint Variant selection, typed non-disclosing failures, response security headers,
  exact content length, and incremental response-body chunks;
- trusted external-scheme resolution for TLS-terminated ingress: native TLS is authoritative,
  untrusted forwarding metadata is ignored, trusted `X-Forwarded-Proto` is single-valued and
  bounded, and Website delivery, proxy forwarding, and access logging consume one result;
- digest/fleet/Node-bound Kubernetes rendering with explicit non-universal direct-ingress CIDRs,
  real config-compiler validation, a hash-versioned immutable per-Node ConfigMap, and
  a tenant-fleet-qualified Website Service, Node-qualified provider-event Services, Pod selectors,
  NetworkPolicy, and PodDisruptionBudget;
- focused tests for hash tampering, schema rejection, provider capability coherence, device/path
  Variant selection, Mount/provider-root separation, cross-Site longest path/conflicts, atomic
  activation, failed-candidate retention, node scope, rollback, adapter HTTP behavior, provider
  deadline mapping, and HEAD-without-content-open behavior;
- isolated candidate-only activation probes that issue bounded `HEAD` requests for every Binding
  and reachable selectable device Variant with activation-purpose Provider calls, reject missing
  entrypoints or unresolved routes before persistence, and preserve both the live registry and
  recovery slot on failure;
- Deploy assignment publication and latest-observation reads through the generated Web Internal
  SDK, immutable per-target observation evidence, full assignment-identity validation, and a
  transactional all-frozen-target `ACTIVE` quorum that alone advances
  `deploy_site.current_revision_id`.
- A focused cross-repository contract compiles a real Deploy Site/runtime set, activates the exact
  output in Web, routes host/path desktop and mobile Wiki requests through the Knowledgebase
  adapter/fake generated-SDK boundary, fails private/unpublished routes closed, and reads updated
  content without changing the SiteRevision, runtime generation, or snapshot hash.

Still open and therefore release-blocking: detached distribution signature/source attestation where
required, external public-domain multi-vantage probes, production drift dashboards/alerts,
Deploy-owned TLS assignment publication and node observation, KMS/Vault/CSI authorization and
encrypted material distribution beyond the protected file-provider boundary, served-fingerprint
convergence, full public trust-chain/revocation policy, service credential hot rotation,
true upstream content streaming, provider-aware cache behavior and concrete event-driven cache
invalidation, sanitizer/rendition
and full-text search pipelines, single-writer migration, deployed-service browser-to-resource E2E,
load/soak, and production operations evidence. The local file watcher remains a
standalone/development mechanism; cloud pull and node recovery do not replace Deploy's immutable
fleet rollout evidence or recorded restart/backup-restore drills. The isolated node-local `HEAD`
probe does not prove public DNS, certificate/SNI, CDN, or Internet reachability. Until the
generated owner SDKs support streaming, activation enforces their 16 MiB Knowledgebase and 256 MiB
Drive object ceilings rather than claiming the descriptor schema's future 1 TiB capability. The
delivery executor additionally enforces one process-wide buffered-content admission budget before
content open. The budget defaults to 256 MiB, is configured within 16 MiB..2 GiB, reserves the
compiled route's `maximumObjectBytes` so metadata drift and Range fallback cannot bypass it, never
queues saturated requests, and is held until response
completion, failure, or cancellation. This bounds concurrent generated-SDK buffer retention but
does not remove the owner SDK's per-response full-buffer allocation.
The website request path uses a bounded node-local resolution metadata cache. It caches only public
static/Wiki resolutions, redirects, and a non-disclosing negative sentinel; it does not cache body
bytes, credentials, conditional responses, activation probes, or private/draft content. Descriptor
TTLs control positive, negative, and positive-only stale-while-revalidate windows. Same-key misses
coalesce through a bounded in-flight map; capacity saturation bypasses caching instead of creating
an origin waiter queue. Exact and Provider-scoped events invalidate the same cache instance used by
delivery, uncertainty clears the affected Provider type, and Provider epochs fence stale in-flight
reinsertion. Shared/edge body caching and production invalidation-storm/load evidence remain open.

Local filesystem delivery opens one capability handle for the configured root, validates request
and fallback paths, opens every directory component and final regular file with no-follow
semantics, and streams from that already-open stable handle. Path replacement cannot redirect the
active response. Windows and Linux tests cover replacement stability, final and intermediate
symlink rejection, directory index/redirect, SPA fallback, conditional requests, Range, HEAD, MIME,
and bounded streaming. Immutable read-only mounts remain defense in depth against untrusted
writers, hard links, and mount changes rather than the primary TOCTOU control.

## Acceptance Criteria

- Descriptor schema/hash/signature/limit/referential checks and atomic activation tests pass.
- Host/SNI/path/IDNA/wildcard/redirect/Variant/Mount routing property tests pass.
- Trusted/untrusted/duplicate/chained/non-text/oversized forwarded-scheme tests pass, including
  force-HTTPS behind a trusted TLS terminator and native TLS downgrade resistance.
- STATIC/SPA/WIKI provider contract and browser-to-resource E2E tests pass.
- Space-root/folder-root, reserved-root, Mount-vs-provider-root separation, canonical Wiki
  publication, inactive Wiki, and multi-Site provider reuse tests pass.
- Event replay/gap/out-of-order, public-to-private, negative-cache, stampede, and provider outage
  tests pass.
- Exact Drive/Knowledgebase AsyncAPI and generated internal SDK compatibility tests pass in
  standalone and cloud topology, including provider generation, route page version, move and
  priority revocation behavior.
- A single-writer migration test proves Web control-plane write routes/tables are non-authoritative
  and normal rollback cannot reactivate dual writers.
- TLS challenge/renewal/distribution/hot-load/SNI/last-valid tests pass with website snapshot
  independence.
- Tenant-qualified cache and provider authorization tests prove no cross-tenant disclosure.
- Kubernetes render tests prove every Service and workload selector is tenant-fleet-qualified,
  every provider-event Service is Node-qualified, missing/oversized fleet labels are rejected, and
  tenant scope hashes are never materialized into labels or object names.
- Load and soak evidence proves bounded memory, concurrency, queues, caches, rendering, descriptor
  count, certificate count, and telemetry labels.
- Last-known-good, rollout quorum, node drift, drain, rollback, and restart recovery drills pass.
- Multi-Node deployment evidence proves hard hostname spread, preferred zone spread, and one-Node
  voluntary-disruption tolerance for each tenant fleet.

## Trace

- PRD: `docs/product/prd/PRD-cloud-site-delivery-data-plane.md`
- Decision: `docs/architecture/decisions/ADR-20260721-compiled-website-runtime-descriptor.md`
- Architecture: `docs/architecture/tech/TECH-cloud-site-delivery-data-plane.md`
- Cross-repository authority: `sdkwork-deployments` REQ-2026-0001 and
  ADR-20260721-unified-cloud-site-publishing-control-plane
