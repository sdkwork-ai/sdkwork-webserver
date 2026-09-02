# SDKWork Web Server Technical Architecture

Status: active
Owner: SDKWork maintainers
Updated: 2026-07-31
Specs: ARCHITECTURE_DECISION_SPEC.md, DOCUMENTATION_SPEC.md, RUST_CODE_SPEC.md, WEB_FRAMEWORK_SPEC.md, WEB_BACKEND_SPEC.md, DATABASE_FRAMEWORK_SPEC.md, CONFIG_SPEC.md, SECURITY_SPEC.md, DEPLOYMENT_SPEC.md, NGINX_SPEC.md

## Document Map

- [TECH-cloud-site-delivery-data-plane.md](TECH-cloud-site-delivery-data-plane.md) - in-progress;
  descriptor v1 ingestion and immutable domain/path/Variant/Mount indexes are implemented, while
  generated-SDK Drive/Knowledgebase provider adapters, activation-time Provider validation, and the
  transport-neutral runtime-set delivery executor run through the dedicated
  `sdkwork-webserver-website-delivery-edge-runtime`. Authenticated assignment publication,
  conditional cloud pull, latest-observation reads, immutable Deploy evidence, and strict
  all-frozen-target `ACTIVE` quorum now run through the generated Web Internal SDK. Web performs an
  isolated node-local Binding/Variant `HEAD` probe before staging and activation; this does not
  replace external public-domain multi-vantage probes. True upstream SDK streaming, provider-aware
  cache consistency, TLS snapshot activation, and commercial runtime evidence remain open.
  Node-local dual-slot runtime-set recovery,
  owner-authenticated Drive/Knowledgebase event
  ingress, dual-slot per-stream checkpoints, ordering/gap handling, and generated-SDK reconciliation
  are implemented.

- [TECH-runtime-data-plane.md](TECH-runtime-data-plane.md) - target and implementation status for the Rust HTTP/HTTPS request data plane.
- [TECH-resolution-cache.md](TECH-resolution-cache.md) - multi-layer DNS/IP resolution cache layers, TTL policy, and negative caching.
- [TECH-app-domain-publishing-fallback.md](TECH-app-domain-publishing-fallback.md) - app publishing default-domain and custom-domain fallback behavior on the data plane.
- [TECH-standards-alignment.md](TECH-standards-alignment.md) - pointer to the repository standards-alignment matrix.
- [ADR-20260715-rust-webserver-data-plane.md](../decisions/ADR-20260715-rust-webserver-data-plane.md) - accepted data-plane component and technology decision.
- [PRD.md](../../product/prd/PRD.md) - product behavior and commercial release authority.

## 1. Architecture Overview

SDKWork Web Server is a Rust-native HTTP/HTTPS server with separate request, management, and
host-operations planes.

Current implemented baseline:

- app-api and backend-api management surfaces;
- `apps/sdkwork-webserver-pc`, with package-owned Console and backend-admin capabilities, one IAM/TokenManager bootstrap, generated SDK injection, and lazy backend-admin loading;
- site, domain, deployment, certificate, Nginx, health-check, audit, environment, and Web Node workflows;
- SQLx 0.9 persistence through the SDKWork database framework with compile-time dynamic-SQL
  injection audit (`audited_sql` wrapper) and PostgreSQL-only repository branches;
- ACME/self-signed certificate services;
- external Nginx artifact materialization and Web Node Daemon synchronization;
- durable bounded Web Node Daemon desired/observed apply checkpoints with crash replay;
- generated Rust backend SDK heartbeat/sync transport with AgentToken and bounded responses;
- machine-validated Web Server configuration and deterministic virtual-host/route compilation;
- bounded immutable website runtime-set compilation/activation and generated Drive/Knowledgebase
  Rust Internal SDK adapters behind transport-neutral resource/static/Wiki provider ports;
- immutable provider registry plus a runtime-set delivery executor for STATIC/explicit SPA
  fallback/WIKI outcomes with compiled scope, canonical URL reverse mapping, exact Range evidence,
  force-HTTPS, consumer-owned deadlines, and bounded streams;
- an independent single-tenant website delivery edge runtime that watches a verified runtime-set,
  constructs both generated Provider SDKs with secret-file ingress credentials, validates Provider
  resources before activation, registers both adapters, and maps typed outcomes and incremental
  chunks to public HTTP responses;
- a dedicated tenant-fleet Kubernetes topology whose per-fleet Website Service, Node-specific
  provider-event Services, Pod selectors, NetworkPolicy, and disruption budget cannot select Nodes
  assigned to another tenant scope or deliver a signed callback to the wrong Node;
- a production `cloud` runtime assignment source that uses only the generated Web Internal Rust
  SDK, a protected Web Node token file, strict node/environment/hash validation, conditional pulls,
  durable last-known-good recovery, isolated candidate-only Binding/Variant `HEAD` probes, and
  resumable `RECEIVED -> VALIDATED -> STAGED -> ACTIVE` or terminal `REJECTED` observations;
- a Deploy convergence loop that publishes assignments and reads Web observations through the
  generated SDK, stores immutable per-target evidence, and transactionally advances the Site
  current revision only after every frozen target reports the exact assignment as `ACTIVE`;
- a node-local dual-slot website runtime-set recovery store that preserves the highest valid
  generation across restart, rejects scope/hash conflicts, and never substitutes for authenticated
  Deploy distribution;
- a separate loopback provider-event listener with subscription-bound Drive/Knowledgebase HMAC
  verification, nine strict owner event contracts, bounded stream-sharded processing, dual-slot
  checkpoints, gap/conflict uncertainty, and current-runtime Provider reconciliation;
- bounded HTTP/1, HTTP/2, TLS, static, redirect, reverse-proxy, WebSocket, health, retry, admission,
  pressure, DNS, and observability controls;
- `drive` and `knowledgebase` application-config resources that mix local static/proxy routes with
  Drive WebsiteRoot (or subdirectory) mounts and Knowledgebase WikiPublications in one
  `sdkwork.webserver.config.json`, assembled fail-closed from environment-owned provider SDK
  credentials and executed through the shared bounded provider registry and resolution cache;
- per-virtual-host security response headers (HSTS over HTTPS, X-Frame-Options, CSP,
  Referrer-Policy, `nosniff` default, bounded custom headers with hop-by-hop rejection) and a
  fail-closed plaintext policy that rejects public non-loopback listeners without TLS unless
  `allowPlaintextHttp` or `acmeHttp01` is declared;
- standalone and cloud development topology plans plus standalone/cloud production deployment
  templates.

The host synchronization process is named **Web Node Daemon** in all new
runtime and operational surfaces. The canonical packaged/development entry
point is `sdkwork-webserver-node-daemon`; `sdkwork-webserver-agent` is retained only as a
v3 compatibility binary. The v3 Agent API and generated DTO names remain wire
compatibility identifiers and are not new product terminology.

Commercial release approval remains separate from implementation. The PRD owns outstanding native
capacity, long-duration soak, managed PostgreSQL/PITR, external image publication, staged rollout,
and production monitoring evidence.

## 2. Technology Choices

| Layer | Choice | Status |
| --- | --- | --- |
| Language/runtime | Rust 2021 + Tokio | Implemented |
| Management HTTP | Axum through `sdkwork-web-framework` | Implemented |
| Request HTTP | Axum/Hyper with explicit HTTP/1 and HTTP/2 guards | Implemented bounded baseline |
| Request TLS | Rustls with bounded certificate material | Implemented bounded baseline |
| Static content | Compiled route/static-file service | Implemented bounded baseline |
| Reverse proxy transport | Hyper/Rustls with streamed bodies and bounded retries | Implemented bounded baseline |
| App Web Server config | JSON Schema authority + Serde + semantic compiler | Implemented |
| Database | `sdkwork-database` + SQLx; PostgreSQL authoritative-server profile only | Implemented baseline; managed HA, client failover, fencing, PITR, and production query-plan evidence remain open |
| IAM | `sdkwork-iam-web-adapter` for protected management surfaces | Implemented |
| Certificates | `instant-acme`, `rcgen`, encrypted persistence, durable accounts, revocation, ARI, self-hosted Rustls snapshot activation | Implemented bounded baseline |

## 3. System Boundaries

```text
sdkwork-api-webserver-standalone-gateway
  |-- one Web Framework + IAM authorization + readiness/metrics/OpenAPI boundary
  |-- Web Server owner assembly -> app/backend/internal route crates -> service -> repository -> database
  |-- IAM owner App API assembly contribution (same process, same application ingress)
  |-- Drive owner App API assembly contribution (same process, same application ingress)
  |-- data-plane bootstrap -> compiled Web Server config -> HTTP/HTTPS/static/proxy/Drive/Knowledgebase
  `-- host operations -> config, signals, readiness, drain, runtime paths

sdkwork-webserver-website-delivery-edge-runtime
  `-- management-disabled data-plane library -> host config + cloud assignment or local file
      -> delivery executor -> generated-SDK provider adapters -> public HTTP response

sdkwork-webserver-core
  `-- framework-independent environment and Web Server config/compiler logic

sdkwork-webserver-edge-runtime
  `-- existing external Nginx artifact operations only

apps/sdkwork-webserver-pc
  |-- console-* -> console-core -> @sdkwork/webserver-app-sdk -> app-api
  |-- admin-* -> lazy admin-core -> @sdkwork/webserver-backend-sdk -> backend-api
  `-- root bootstrap -> IAM + one TokenManager + typed browser runtime config
```

The request path does not call management services or repositories. Management route crates continue to use `sdkwork-web-framework`; application traffic routes are configuration-owned Web Server behavior and do not create a second SDKWork business API authority.

## 4. Configuration And Contract Ownership

- `sdkwork.app.config.json` remains application identity and release authority.
- `specs/sdkwork.webserver.config.schema.json` is the local machine contract for application Web
  Server configuration; the app manifest remains identity/release metadata rather than runtime
  configuration authority.
- Application-config `drive` and `knowledgebase` resources reuse the website provider contracts and
  bounded resolution cache through the shared provider executor. Provider SDK connections are
  environment-owned (base URL + ingress token file + tenant scope hash); every referenced resource
  is validated against its provider before the data plane starts, and watched reloads that would
  reference an unassembled provider type are rejected while the active generation is retained.
- Host process configuration follows `CONFIG_SPEC.md` and `RUNTIME_DIRECTORY_SPEC.md`.
- Node synchronization publishes bounded immutable `sv1:` snapshots through the Agent contract;
  mutable management DTOs do not enter the request path.
- OpenAPI authorities are app-api, backend-api, and the application-ingress Web Internal API. The
  Web Node consumes the generated `sdkwork-webserver-internal-sdk`; the internal route crate consumes the
  local `WebInternalApi` service port and never its own generated client.

## 5. API, SDK, And Data Ownership

- Management success/error responses follow SDKWork envelopes and Problem Details.
- Retriable management operations preserve one explicit idempotency contract from authority OpenAPI through route metadata and generated SDK inputs. The framework validates and scopes the Header; deployment repository deduplication receives only that framework-owned context value.
- SDK families are `sdkwork-webserver-app-sdk`, `sdkwork-webserver-backend-sdk`, and the machine-to-machine
  `sdkwork-webserver-internal-sdk` used for runtime assignment publication, retrieval, and observations.
- Request data-plane traffic preserves the configured upstream or static Web protocol; it does not wrap arbitrary application responses in SDKWork management envelopes.
- PostgreSQL is the only authoritative server database and lifecycle, recovery, and
  release-verification profile.
- List/search repositories and SDKs remain subject to store-level SDKWork pagination.
- The PC Console owns tenant workflows for sites, configuration, domains, certificates, and
  deployments. The backend-admin surface owns WEB/API application deployment, public domains,
  canonical certificate lifecycle and fleet convergence, Nginx, server inventory, diagnostics,
  and audit.
  UI packages never construct SDK clients or assemble authenticated HTTP requests.
- Application store-listing media is uploaded by the bootstrap-injected Drive App SDK. The Web
  service persists the canonical Drive-backed `MediaResource` snapshots under
  `web_site.metadata.storeListing`, projects only the typed `storeListing` API field, and atomically
  replaces that metadata member without exposing or overwriting unrelated system metadata.
- Application creation is a recoverable orchestration: validate source locally, create the draft,
  upload and attach bounded store media, persist ZIP/directory content through the Drive App SDK or
  import a public Git repository through the server-side Drive Uploader, create an immutable source
  version, then create the initial deployment with `sourceVersionId` and an explicit release version.
  Subsequent releases select a retained ready source version without re-uploading bytes. Deployment
  and activation enforce the icon invariant server-side, and activation also checks durable
  successful-deployment evidence.

The domain, certificate, and deployment relationship is normalized:

```text
web_root_domain 1 -> N web_domain
web_site 1 -> N web_site_binding N -> 1 web_domain
web_certificate N <-> N web_domain through web_certificate_identifier
web_certificate 1 -> N web_certificate_version
web_certificate 1 -> N web_certificate_operation
web_site_binding 1 -> N web_listener_certificate_binding
web_site 1 -> N web_deployment
```

Backend certificate listing and issuance use verified domain identifiers and remain available
before `web_site_binding` exists. Application identity enters only at
`web_listener_certificate_binding`, where hostname coverage, usable version, key algorithm, and
listener conflicts are checked before activation intent is accepted.

Issue and renew routes persist `web_certificate_operation` and return HTTP `202` standard async
data. App and backend generated SDKs retrieve operation status; the PC polls that generated method
with a bounded client deadline and can stop observing without cancelling server work. The
certificate worker schedules due renewals and claims both issue and renew operations with
`FOR UPDATE SKIP LOCKED`, expiring leases, bounded retries, and fencing tokens. Finalization updates
the immutable certificate version and canonical aggregate in the same PostgreSQL transaction;
stale or exhausted work cannot overwrite a newer result. Revocation (`POST .../revoke`) is
synchronous: the CA acknowledges before the aggregate is marked `status=3`, listener bindings are
archived, and auto-renewal stops. Renewal scheduling prefers the CA-suggested ARI window recorded on
the aggregate, falling back to the fixed `renew_before_days` window.

Only ACME certificate aggregates may enable automatic renewal or enter the scheduled scan.
Self-signed aggregates remain eligible for explicit manual reissuance. Every provider result passes
the common issuer boundary's SAN, algorithm, current-validity, leaf/private-key SPKI, and parsed
metadata checks before a worker can finalize an immutable version.

ACME accounts are persisted encrypted (AES-256-GCM under the process master key) per CA directory
URL and shared by issuance, renewal, revocation, and ARI lookups, so restarts never create new CA
accounts. HTTP-01 challenges are written atomically into the configured webroot by the worker and
served by the self-hosted data plane through the narrow `acmeHttp01.webroot` listener endpoint
(exact `/.well-known/acme-challenge/<token>` path only). After every successful certificate
operation the worker projects the node's listener certificate bindings into versioned TLS material
under `SDKWORK_WEBSERVER_TLS_MATERIAL_ROOT` and publishes a monotonic `tls-runtime.json` snapshot; the
data plane's `FileTlsRuntimeController` hot-loads the new Rustls configuration without dropping
existing connections. External Nginx artifact activation is a documented optional legacy path and
is not part of the certificate lifecycle.

`sdkwork-web` owns the mutable Zone, hostname, verification, route, certificate, listener, and Web
deployment intent. `sdkwork-deploy` is being aligned to immutable rollout, distribution, snapshot,
and observation evidence that references Web public ids and versions. `sdkwork-iam` owns identity,
application registration, and authorization; it must not own primary-domain or domain-configuration
state. The cross-repository boundary remains proposed pending human review in
`ADR-20260731-web-deploy-domain-certificate-authority`.

The composed Deployments domain matches the Web domain on the shared standards:
environment-variable secrets are AES-GCM encrypted at rest and masked in responses; audit logs
require a tenant context (no tenant-less enumeration); deployment rollback is transactional; site
writes use optimistic `version` concurrency; deployment creation deduplicates idempotency keys;
growing lists (`auditLogs.list`, `deployments.list`) use opaque keyset cursors with
`mode=cursor` pageInfo; `page_size` overflow and pagination aliases are rejected; query
parameters are `lower_snake_case`; and env/health collections are capped at 100 items per site
(PAGINATION_SPEC, SECURITY_SPEC, PRD-FR-011).

Web and Deploy deployment records are command intents: the deployment worker that advances
`status` beyond `PENDING` remains a separate authority (REQ-2026-0061/0062 gate). Until that
authority exists, `sites.activate` (which requires a successful deployment) and
`deployments.rollback` (which requires a successful source) honestly return `409`; the system
never invents a success state.

## 6. Security, Privacy, And Resource Boundaries

- Protected management surfaces use SDKWork IAM and typed request context; the internal
  distribution surface and Web Node agent routes are wrapped in machine-only framework layers
  so IAM user API keys can never impersonate node credentials on any composed gateway.
- Public application traffic uses explicit host/route policy and HTTPS requirements from the PRD.
- Private keys and credentials remain references to protected runtime sources and are never serialized into app config or logs.
- Static roots, upstream destinations, trusted proxy networks, headers, bodies, timeouts, connections, queues, and configuration size are validated and bounded.
- Request data-plane telemetry is redacted and low-cardinality.
- No lock may be held across asynchronous external I/O.

## 7. Deployment And Runtime Topology

- `standalone`: one packaged gateway runs the composed management and data plane and uses
  PostgreSQL for authoritative control-plane persistence.
  IAM and Drive browser dependency APIs are dependency-owned Rust assembly contributions linked
  into this gateway and use `application.public-ingress`; no dependency gateway or port 3900 is
  required. Development keeps Vite only as the browser-visible origin and proxies canonical API
  paths to the private gateway target. Production edge nginx (`expose.mode: api`) reverse-proxies
  all public paths to the packaged gateway; the process `AdaptiveAppShell` serves Adaptive Web
  (mobile → H5 → PC → static-fallback; desktop → PC → H5 → static-fallback) plus composed APIs
  from the application ingress. The archive carries IAM and Drive runtime assets under
  `share/sdkwork` and console roots under `share/sdkwork/webserver-{pc,h5,static}`; packaged
  relative application/static roots resolve from the parent of `bin/`, independent of the process
  working directory.
- `cloud`: the dedicated website delivery edge-runtime nodes consume node-scoped immutable
  configuration and secret assignments; management assemblies are hosted by the platform cloud
  gateway and the application standalone gateway is not started.
- `cloud.development` starts only the local Web Node Daemon client; application/API/database
  surfaces are explicit remote development URLs.
- `cloud.production` uses digest-bound Kubernetes templates with one Node Secret and recovery PVC
  per rendered edge-runtime StatefulSet plus a compiler-validated, hash-versioned immutable listener ConfigMap;
  trusted-proxy CIDRs are explicit deployment inputs and universal networks are rejected. At least
  two independently rendered Nodes are required for high availability. Published image existence
  is not claimed while release packages remain disabled.
- External Nginx remains an edge activation option and is not required for Rust request handling.

## 8. Architecture Decision Index

- [ADR-20260731 Web And Deploy Domain-Certificate Authority Boundary](../decisions/ADR-20260731-web-deploy-domain-certificate-authority.md) - proposed single mutable Web authority with immutable Deploy rollout evidence and no IAM domain configuration.
- [ADR-20260730 Domain, Certificate, And Listener Bindings](../decisions/ADR-20260730-detached-domain-assets-and-certificate-bindings.md) - accepted normalized site routes, multi-SAN certificate identifiers, immutable versions, and listener bindings.
- [ADR-20260730 Root Domain Zones And Hostname Deployments](../decisions/ADR-20260730-root-domain-zones-and-hostname-deployments.md) - accepted explicit root-domain Zones, store-paginated hostname children, and derived application deployment visibility.
- [ADR-20260730 Drive-Backed Application Source Versions](../decisions/ADR-20260730-drive-backed-application-source-versions.md) - accepted immutable Drive source catalog, five-version default retention, and release provenance through `sourceVersionId`.
- [ADR-20260728 Idempotency Contract Closure](../decisions/ADR-20260728-idempotency-contract-closure.md) - accepted strict marker/Header/route/SDK parity, stable action keys, bounded runtime validation, and Header-owned durable deduplication.
- [ADR-20260728 Standalone Browser Same-Origin Delivery](../decisions/ADR-20260728-standalone-browser-same-origin-delivery.md) - accepted topology-derived development proxy and production gateway-static delivery for one browser-visible origin.
- [ADR-20260728 Embedded Standalone Dependency Assemblies](../decisions/ADR-20260728-embedded-standalone-dependency-assemblies.md) - accepted one-process IAM/Drive owner assembly composition and one standalone browser API ingress.
- [ADR-20260721 Compiled Website Runtime Descriptor](../decisions/ADR-20260721-compiled-website-runtime-descriptor.md) - accepted cloud data-plane input and authority boundary.

| ADR | Topic | Status |
| --- | --- | --- |
| ADR-20260731-web-deploy-domain-certificate-authority | Web mutable authority and Deploy immutable rollout evidence | proposed; human review required |
| ADR-20260730-detached-domain-assets-and-certificate-bindings | Domain, certificate identifier/version, and listener binding relationships | accepted |
| ADR-20260730-drive-backed-application-source-versions | Immutable Drive source versions and release provenance | accepted |
| ADR-20260728-idempotency-contract-closure | Generated replay-safe API/SDK idempotency contract | accepted |
| ADR-20260716-canonical-uri-dual-representation | Raw request URI preservation and bounded canonical routing Path | proposed; human review required |
| ADR-20260715-rust-webserver-data-plane | Config authority, crate boundaries, HTTP/TLS/static/proxy stack | accepted |
| ADR-20260720-process-database-pool | One typed SDKWork lifecycle pool per process | accepted |
| ADR-20260623-acme-certificate-authority | ACME client, CA selection, key storage | accepted |
| ADR-20260623-cert-distribution-topology | Node synchronization and certificate distribution | accepted |
| ADR-20260726-admin-application-and-certificate-control-plane | Backend-admin deployment plus one-authority certificate renewal and fleet convergence | accepted |

## 9. Verification

```powershell
cargo fmt -- --check
cargo test --workspace
node ..\sdkwork-specs\tools\check-application-layering.mjs --root .
node ..\sdkwork-specs\tools\check-rust-backend-composition.mjs --root .
pnpm check
```

Commercial completion additionally requires the protocol, Nginx, HTTPS, performance, OOM, soak, failure, upgrade, backup/restore, and cluster evidence named by the PRD.
