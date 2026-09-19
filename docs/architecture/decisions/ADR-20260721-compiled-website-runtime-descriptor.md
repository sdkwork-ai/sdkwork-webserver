# ADR-20260721 Compiled Website Runtime Descriptor

Status: accepted
Requirement: REQ-2026-0060
Owner: SDKWork Web Server maintainers
Date: 2026-07-21
Specs: ARCHITECTURE_DECISION_SPEC.md, SDK_SPEC.md, APP_SDK_INTEGRATION_SPEC.md,
CONFIG_SPEC.md, DEPLOYMENT_SPEC.md, NGINX_SPEC.md, SECURITY_SPEC.md, PERFORMANCE_SPEC.md,
OBSERVABILITY_SPEC.md

## Context

Cloud Sites combine multiple domains, client Variants, path Mounts, live Drive directory resources,
live Knowledgebase Wiki resources, delivery policy, and certificates. Querying normalized control
plane tables or source services to reconstruct routing on every request would couple availability,
increase latency, and make atomic rollback difficult. Persisting provider URLs/keys in a Web Server
configuration would leak source topology and create a second authority.

Web Server already has Rust HTTP/TLS, bounded ACME, certificate activation, static serving, proxy,
and atomic runtime foundations. The missing boundary is a safe compiled input and typed provider
resolution model.

## Decision

1. Web Server consumes an immutable, bounded, hash-addressed `WebsiteRuntimeDescriptor` compiled by
   Deploy from its normalized source of truth.
2. The descriptor includes Bindings, Variants, Variant rules, Resources, Mounts, delivery/security
   policy, limits, and observability policy. It contains stable IDs/references only and no secrets,
   SDK/base URLs, buckets, object keys, presigned URLs, or database connections.
3. A node validates and stages the entire descriptor, builds immutable indexes, then atomically
   swaps one current pointer. Partial maps and in-place mutation are forbidden.
4. TLS assignments use a separate immutable snapshot and atomic pointer because certificate
   rotation is operationally independent from Site configuration.
5. Ordinary Drive/Wiki content changes are provider lifecycle events. They invalidate/revalidate
   content caches but do not activate a website descriptor.
6. Content is opened only through owner-generated SDK clients or typed same-process service ports.
   The provider returns typed public eligibility/version/metadata and a bounded stream or Wiki
   representation.
7. Exact host, approved wildcard, longest Binding prefix, deterministic Variant precedence, and
   longest Mount prefix are pre-indexed and evaluated in that order.
8. Last-known-good website and TLS snapshots remain active during temporary control-plane failure.
   Desired/observed revision and served fingerprint are reported back to Deploy.
9. In cloud mode, `webserver_site`, `webserver_domain`, `webserver_deployment`, and `webserver_certificate` are not runtime
   inputs or writable authority. They remain scoped to the explicit standalone local-management
   profile. The cloud artifact excludes management composition and accepts only Deploy-owned
   immutable assignments.
10. Existing ACME implementation choices remain reusable execution details. Upon acceptance, this
    decision narrows the cloud metadata/orchestration ownership portions of
    `ADR-20260623-acme-certificate-authority`; it does not discard its bounded Rust provider choice.
11. Drive `SPACE_ROOT`/`FOLDER` selection is resolved behind a stable WebsiteRoot by Drive; Web
    Server treats the reference as opaque. Mount `ROOT`/`ALIAS` remains URL translation and cannot
    retarget provider scope. Knowledgebase capability likewise never implies public access: the
    canonical WikiPublication must validate ACTIVE. One provider UUID may appear in multiple Site
    Resources, so cache/routing identity always includes the Site-local Resource and Mount.

## Alternatives

- Query Deploy/source databases per request: rejected for ownership, latency, availability, and
  tenant-isolation reasons.
- Copy normalized tables into every node and run joins: rejected because distribution and rollback
  would expose partial state and schema coupling.
- Embed origin URLs/secrets in descriptors: rejected because descriptors are distributed metadata
  and provider topology must remain private.
- Rebuild a frozen release for each content update: rejected because live directory/Wiki semantics
  and WYSIWYG freshness are product requirements.
- Let each provider register arbitrary executable handlers: rejected because runtime extension must
  remain typed, bounded, reviewed, and deterministic.

## Consequences

- A versioned descriptor schema, canonical serializer, compiler compatibility matrix, rollout
  protocol, and golden tests are required.
- Drive and Knowledgebase need stable provider service contracts and events.
- Web Server needs provider adapters, cache generation/invalidation, routing indexes, and per-resource
  circuit/timeout policy.
- The fleet can continue serving when Deploy is unavailable, but provider availability and stale
  policy become explicit SLO inputs.
- Cloud and standalone profiles require artifact-level process isolation; no compatibility import,
  cross-profile projection, or dual write is permitted.

## Implementation Status

The accepted Web Server consumer boundary is implemented:

- strict `sdkwork.website-runtime.v1` JSON Schema and typed Rust model;
- canonical payload SHA-256 verification and bounded semantic validation;
- immutable exact/wildcard Host, Binding path, Variant rule, Mount path, and resource indexes;
- node-scoped `sdkwork.website-runtime-set.v1` schema/hash, stable Site ordering, cross-Site route
  conflict checks, whole-candidate compilation, serialized activation/rollback writers, one atomic
  read pointer, scope enforcement, monotonic generation/replay protection, idempotency, and one
  previous in-memory generation;
- opaque Drive/Knowledgebase resource references and segment-aware `ROOT`/`ALIAS` translation;
- fail-closed provider-topology fields, path policy, reference, conflict, and capability checks;
- separate node-scoped TLS assignment schema/hash and immutable SNI assignment index without raw
  certificate/private-key material or Site revision coupling;
- transport-neutral resource/static/Wiki provider ports with typed public-state failures,
  generations, conditional/range inputs, cursor pagination, redacted handles, and body streaming.
- generated Drive and Knowledgebase Internal SDK provider adapters plus the standalone
  `website-data-plane` delivery executor and public HTTP mapping;
- a public-listener-independent, loopback-only provider-event ingress for four Drive and five
  Knowledgebase owner events, with owner HMAC verification, tenant/channel binding, bounded
  stream-sharded processing, dual-slot durable checkpoints, gap/conflict uncertainty, and
  generated-SDK Provider reconciliation.
- node-local A/B persistence of complete activated runtime-sets with corruption fallback,
  highest-generation restart/source recovery, replay protection, and staging/production
  configuration enforcement.

Producer/compiler conformance, authenticated runtime-set distribution, durable recovery,
provider adapters, provider-event reconciliation, cloud/standalone process isolation, and browser
delivery have executable evidence. Deploy-visible rollout observations, cloud TLS material
distribution/hot swap, any future concrete content-cache implementation, and production
security/load/availability evidence remain release gates rather than ADR acceptance blockers.

## Verification

- deterministic compiler/consumer golden fixtures and version compatibility tests;
- schema/hash/signature/size/reference failure tests;
- routing property/fuzz and Nginx-profile conformance tests;
- provider eligibility/path/state/version/event contract tests;
- atomic website/TLS activation, last-known-good, drift, and rollback tests;
- cross-tenant cache/origin/security tests and bounded load/soak evidence.

## Supersedes / Superseded By

This ADR supersedes only the cloud control-plane ownership assumptions of older Web
Server management designs. Existing protocol safety and ACME runtime library decisions remain in
force unless a separate ADR replaces them.
