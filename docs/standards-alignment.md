# SDKWork Standards Alignment

Application: `sdkwork-web-server`

Updated: 2026-08-03

This document records current implementation and evidence. It does not declare production release
approval. Normative requirements are owned by `../sdkwork-specs`.

## Framework And Capability Matrix

| Capability | Current state | Evidence |
| --- | --- | --- |
| `sdkwork-web-framework` | Integrated | App/backend route manifests, `WebRequestContext`, IAM resolver, framework `service_router`, health/readiness/metrics |
| `sdkwork-database` | Integrated | PostgreSQL-only authoritative manifest/baseline, lifecycle host, one process-shared typed pool, repository parity and recovery tests |
| `sdkwork-utils-rust` | Integrated | API envelopes, pagination, crypto, SHA-256, validation, serde helpers, platform helpers |
| `sdkwork-id-core` | Integrated through database ID support | Snowflake internal IDs and UUID resource identities |
| Backend SDK | Integrated | Generated TypeScript/Rust family, AgentToken auth, bounded response reads, Node Daemon consumption without handwritten HTTP |
| `sdkwork-drive` | Gated, no current upload capability | No business upload/presign/provider ownership; contract test rejects future bypasses |
| `sdkwork-discovery` | Gated, no current RPC transport | No tonic/prost service; contract test requires RPC framework and discovery together if RPC is introduced |

## Architecture State

- OpenAPI YAML is authored under `apis/`; materialized JSON, route manifests, and generated SDK
  inputs are deterministic derivatives.
- App and backend operations use the SDKWork v3 success envelopes and Problem Details error shape.
- The standalone gateway composes framework management routes with the bounded HTTP/HTTPS data
  plane. Management routes call services through ports; SQLx stays in repository modules.
- Database bootstrap returns one SDKWork lifecycle-owned PostgreSQL pool; production source
  contains no alternative-engine repository, `AnyPool` bridge, or second pool. The former
  `SDKWORK_DATABASE_TEMPORARY_*` exception flags were removed from both standalone topology
  profiles because no production code path consumes the temporary AnyPool mechanism.
- Agent sync byte-count projections use engine-specific SQL and cast PostgreSQL `OCTET_LENGTH`
  results to `BIGINT`, matching the shared Rust `i64` repository contract.
- The Web Node Daemon uses the application-root generated Rust backend SDK for heartbeat and sync,
  with typed AgentToken configuration, canonical envelope decoding, and finite response limits.
  Its sync loop applies exponential backoff on failure and per-cycle jitter so a fleet never
  hammers the control plane in lockstep.
- The application-ingress internal API resolves credentials through a machine-only resolver;
  `wagent_`-prefixed credentials never fall back to user API-key resolution on any surface.
  The standalone gateway composes the internal surface and the Web Node agent routes in their
  own machine-only framework layers, so IAM user API keys can never reach machine surfaces.
- Growing collections (`auditLogs.list`, `deployments.list`, `sourceVersions.list`, and
  `servers.list`) use keyset cursor pagination with opaque `(created_at|updated_at, id)`
  cursors; deep OFFSET is rejected at the store, the shared pagination middleware rejects
  `cursor` on non-keyset endpoints, and the final cursor page keeps `mode=cursor` with exact
  `hasMore` instead of falling back to a misleading offset `pageInfo` (PAGINATION_SPEC §6/§12,
  PRD-FR-011).
- The workspace dependency graph was upgraded to sqlx 0.9 (aligned with sdkwork-database and
  sdkwork-drive), enabling the compile-time SQL injection audit (`SqlSafeStr`): every
  dynamically assembled repository statement passes through the audited `audited_sql` wrapper
  that requires fixed-clause-only assembly with `$N` bind parameters for all request input.
- Direct dependencies track current releases: `jsonschema` 0.49, `hashlink` 0.12,
  `tower-http` 0.7, `cap-std`/`cap-fs-ext` 4.0.2, `rcgen` 0.14, `x509-parser` 0.18,
  `reqwest` 0.13 (generated SDK client templates included), `hickory-resolver` 0.26,
  axum 0.8.9, tokio 1.53, sqlx 0.9, rustls 0.23. Remaining version forks are transitive
  ecosystem pins (for example `opentelemetry-otlp` → `reqwest` 0.12) and are not under
  application control.
- SQLite engine branches were removed from the repository (the repository is instantiated
  exclusively with PostgreSQL); the `database_engine` field, engine-parameterized write
  expressions, and the SQLite JSON `json_set` branch no longer exist as dead code.
- Proxy orchestration, upstream selection/health, request-body controls, metrics, TLS, DNS, admission,
  and protocol guards are separated into focused private modules.
- TLS certificate and private-key parsing uses the maintained
  `rustls-pki-types::pem::PemObject` surface; the unmaintained `rustls-pemfile` dependency is not
  present.
- IAM provider callbacks consume `quick-xml` 0.41+, preserve XML entity and CDATA values, and
  reject DTDs, nested callback fields, unknown entities, and incomplete documents.
- Wildcard host matching (`*.suffix`) is a constant-time map lookup on the request host's
  first-label remainder in the request, website-runtime, and TLS compiled indexes, replacing
  per-request linear scans; listener compilation is a single pass over virtual-host
  declarations instead of an O(listeners x hosts) nested scan.
- End-to-end provider streaming is implemented: the generated Rust SDK clients expose a
  `BinaryResponseStream` (bounded chunk reads, byte budget, declared `Content-Length`) and
  every binary operation gains a `_stream` variant; the Drive and Knowledgebase adapters
  forward chunks instead of materializing `Vec<u8>` payloads, so request memory is O(chunk)
  instead of O(object size). The stream adapters enforce the expected length (Drive) or the
  configured ceiling (Knowledgebase) while consuming, failing closed on contract drift.
- The repository layer owns no Nginx validation/reload stubs: Nginx syntax validation and reload
  run exclusively through the edge runtime, `retrieve_nginx_status` probes the real edge
  configuration (`nginx -t`) for its `running` flag, and removed port methods can no longer
  misreport a fake result.
- Nginx subprocess capture (`nginx -T`) is bounded (4 MiB ceiling), provider-event checkpoint
  recovery enforces an aggregate byte budget and retains only the highest generation per stream,
  and TLS material distribution runs its file-system work on the blocking thread pool with
  fsync failures surfaced instead of swallowed.
- The certificate operation worker wraps each cycle in a watchdog timeout so a stalled ACME or
  database call can never block renewal scheduling or graceful shutdown.
- The PC Console pagination state machine pages both modes correctly: offset lists increment
  `page`, cursor lists navigate an explicit back-stack (`cursorHistory`), and a page load no
  longer re-triggers itself through the effect dependency (the previous `nextCursor`-driven
  effect could auto-advance through every page).

## API And SDK Guarantees

- Authored Agent routes explicitly declare `x-sdkwork-route-auth: agent-token` and require the
  `AgentToken` security scheme.
- Generated SDK methods return domain payloads after SDKWork v3 envelope unwrapping and reject
  nonzero business codes.
- Backend SDK generation defaults to TypeScript and Rust, retains generator control-plane manifests,
  removes stale owned files, and is idempotent on an unchanged contract.
- `sdk-manifest.json` and `specs/component.spec.json` agree on IAM SDK dependencies.
- Generated files under `generated/server-openapi` are generator-owned and are never hand-edited.
- HTTP GET query parameters use `lower_snake_case` exclusively (`page`, `page_size`, `cursor`,
  `application_type`, `site_type`, `site_id`, `domain_id`, `target_type`, `operator_id`,
  `start_date`, `end_date`, `config_type`, `is_active`, `if_sync_version`); no camelCase or
  pagination aliases are accepted.
- The standalone gateway wires the PostgreSQL-backed `SqlxIdempotencyStore` (web store
  migrations run at bootstrap) so Idempotency-Key replay deduplication survives restarts and
  multi-replica deployments, and applies a 60 s business handler deadline so stuck handlers
  fail bounded instead of occupying workers indefinitely.
- The internal API OpenAPI envelope matches the runtime: `SdkWorkApiResponse` requires only
  `code`/`data`/`traceId` (no invented `message`), and `ProblemDetail.detail` is optional,
  consistent with the app/backend envelopes.
- Unassembled runtime capabilities (for example Git source import without a configured
  importer) fail with a dedicated `Unavailable` error mapped to HTTP 503 instead of a
  misleading 500 internal-server fault.
- Audit writes fail closed when the IAM tenant subject cannot be resolved to a positive numeric
  id: no audit row is persisted with a fabricated `tenant_id = 0`.
- The management listener (unauthenticated `/healthz`, `/readyz`, `/livez`, `/metrics`) fails
  closed on a non-loopback bind unless `SDKWORK_WEBSERVER_MANAGEMENT_EXPOSE_ALLOWED=true` is set by
  the operator; the standalone production profile declares it explicitly, and the standalone
  data-plane operations listener (`127.0.0.1:3901`) keeps the profile observable.
- Cross-tenant runtime-assignment publication records a durable
  `web.runtime_assignment.publish_cross_tenant` audit marker with source/target tenant ids.

## Persistence And Data Lifecycle

- `database/ddl/baseline/postgres/0001_web_baseline.sql` is the authoritative schema snapshot;
  `database/contract/schema.yaml` is fully materialized for every table including
  `web_certificate_operation` with constraints, partial unique indexes, and predicates.
- `database/migrations/postgres/` carries expand-only migrations (`0001_web_schema_hardening`,
  `0002_web_env_variable_rotation`, `0003_web_certificate_lifecycle_completion`,
  `0004_web_list_index_hardening`) with standard metadata headers, paired down migrations, and
  idempotent statements for already-installed databases. `0004` adds tenant-prefixed
  `(tenant_id, site_id, created_at)` indexes for deployment/source-version keyset listing, a
  `(tenant_id, updated_at)` Web Node keyset index, and the listener-binding
  `(site_binding_id, is_default, priority, id)` sort index; the baseline snapshot and
  `database/contract/schema.yaml` are kept in sync with the same index set.
- Soft-deleted sites archive their bindings, TLS policies, listener certificate bindings, and
  deactivate environment variables and health checks in one transaction; active slugs are unique
  per tenant through a partial unique index.
- Certificates support soft deletion (`DELETE /backend/v3/api/certificates/{certificateId}`) that
  releases domain identifiers; terminal-failed ISSUE certificates are auto-archived by the worker
  reaper, so failed issuance never blocks domain removal.
- Managed-domain responses project `latestDeployment` from the same `web_deployment` join used by
  root-domain hostname responses, so the two listing paths return consistent deployment
  visibility.
- Environment variables support in-place rotation and soft deletion
  (`PATCH`/`DELETE /app/v3/api/applications/{siteId}/env_variables/{variableId}`); the active-key unique
  index releases keys on deactivation.
- Optimistic concurrency: `version` columns are enforced with compare-and-swap updates on site,
  Nginx configuration, and environment-variable writes, returning `409` on concurrent
  modification instead of silent last-write-wins. Certificate renewal-policy updates use a
  row-locked transaction, and agent heartbeats use atomic JSONB merges so concurrent heartbeats
  never drop fields.
- Certificate operation leases are renewed by a heartbeat while issuer work runs, so slow ACME
  issuance is no longer reaped mid-flight.
- Let's Encrypt account credentials are persisted encrypted (AES-256-GCM under the process
  secret key, atomic file commit, `0600`) and reused across issuances and restarts, avoiding
  per-operation CA account creation and its rate limits. The runtime requires
  `SDKWORK_WEBSERVER_ACME_ACCOUNT_ROOT` in production-like environments and falls back to process
  memory only for development.
- Certificate revocation (`POST /backend/v3/api/certificates/{certificateId}/revoke`) is
  synchronous and CA-acknowledged: the ACME account restores from the durable store, the CA
  confirms the RFC 5280 reason before the aggregate is marked `status=3`, listener bindings are
  archived, node observations are cleared, and auto-renewal stops. Self-signed certificates are
  marked revoked locally without a CA.
- The due-renewal scheduler prefers the CA-suggested ARI window (RFC 9773) recorded on the
  certificate aggregate after each successful issuance, falling back to the fixed
  `renew_before_days` window.
- Self-hosted TLS activation: after each successful certificate operation the worker projects the
  node's listener certificate bindings into versioned material files
  (`SDKWORK_WEBSERVER_TLS_MATERIAL_ROOT/<version-uuid>/fullchain.pem + privkey.pem`, atomic, `0600`
  private keys) and publishes a monotonic `tls-runtime.json` snapshot; the data plane's
  `FileTlsRuntimeController` hot-loads the revision with fingerprint, validity, SNI, and ALPN
  verification and A/B recovery slots. HTTP-01 challenges are served by the data plane listener
  that configures `acmeHttp01.webroot` (narrow exact-path endpoint only). External Nginx artifact
  activation remains a documented optional legacy path outside the certificate lifecycle.
- Certificate listener convergence processes each candidate in its own short row-locked
  transaction with a status guard (idempotent under concurrent workers) instead of one long
  transaction spanning hundreds of statements; agent certificate observations are batch-bounded.
  Every transition branch (ACTIVE, DEPLOYING, FAILED) commits its short transaction explicitly,
  so a node-reported failure durably terminates the rollout instead of being rolled back with a
  dropped transaction.
- `web_nginx_config` activation is edge-first: the site is deployed and reloaded before the
  control-plane state commits, and a failed commit rolls the edge back to the previously
  active configuration.
- Audit log persistence failures increment an observable `audit_persistence_failures` counter
  (surfaced through the service API) instead of remaining a silent log-only gap.
- HTTP health-check targets must be credentialed-free HTTP(S) URLs; Nginx site content is scanned
  before activation and rejects `include`, `alias`, localhost proxies, and literal private/metadata
  upstream addresses; `reload_nginx` rejects machine principals.
- Nginx reload convergence is proven before activation success is reported (PRD-FR-020):
  `nginx -T` dumps the loaded configuration (includes expanded), so the deployed site's
  `server_name` fragment must be present after `deploy + reload`; a reload that failed validation
  keeps the previous revision serving and the activation fails instead of reporting a false success.

## Deployments Domain Alignment (composed dependency assembly)

- Environment-variable secrets are encrypted at rest with AES-GCM under the process secret key
  (`SDKWORK_DEPLOY_SECRET_ENCRYPTION_KEY`, development fallback derived with a warning) and masked
  as `***` in every response; the `value_encrypted` column never returns plaintext, matching the
  Web domain contract.
- Audit log listing requires a tenant context at both the service and repository layers;
  tenant-less tokens are rejected (`Forbidden`) instead of enumerating the whole table.
- Deployment rollback marks the source record and inserts the rollback record in one transaction;
  a failed insert no longer leaves the source marked rolled back.
- Site update and status transitions enforce optimistic concurrency (`version` compare-and-swap)
  and return `409 conflict` on concurrent modification instead of silent last-write-wins.
- Deployment creation deduplicates concurrent idempotency-key submissions: the unique-violation
  path re-reads and returns the existing record instead of surfacing `409`.
- Growing collections (`auditLogs.list`, `deployments.list`) use opaque keyset cursor pagination
  on `(created_at, id)` with `mode=cursor` pageInfo, `nextCursor`, and exact `hasMore`
  (PAGINATION_SPEC §6/§12); offset lists carry stable `id` tie-breakers and exact `hasMore`;
  `page_size` beyond 200 and pagination aliases are rejected with 400 at the shared middleware,
  `cursor` fails closed on non-keyset endpoints, and audit log filters (`target_type`, `action`,
  `operator_id`, `start_date`, `end_date`) declared in OpenAPI are implemented with bound
  parameters.
- HTTP GET query parameters across the Deployments API use `lower_snake_case` exclusively
  (`site_type`, `site_id`, `config_type`, `is_active`, `cluster_id`, `target_type`,
  `operator_id`, `start_date`, `end_date`, `cursor`); no camelCase query aliases remain.
- Environment-variable and health-check collections are capped at 100 active items per site with a
  row-locked cardinality check on create and a cardinality invariant on list, bounding list memory
  to O(capacity) (PAGINATION_SPEC §2).
- The legacy top-level `migrations/` directory (deprecated pre-framework SQL) and the obsolete
  `postgres_composition_matches_sqlite_transaction_semantics` spec reference were removed; the
  deploy module remains PostgreSQL-only with a fail-closed engine gate.

## Deployment And Release State

`standalone.production` is a host-package profile. `cloud.production` is a Kubernetes/container-image
profile with digest-bound templates, a bounded migration Job, StatefulSet identity, probes,
PodDisruptionBudget, non-root execution, read-only root filesystem, dropped capabilities, and
secret-manager references.

The four Linux server package declarations in `sdkwork.app.config.json` are disabled and carry
`releaseBuildDeferred: true`. Archive packaging, checksum, Sigstore, CycloneDX, x64/arm64 smoke,
database recovery, and HA workflow steps are implemented, but no container registry publication
authority or production release approval is declared. Docker/Kubernetes files are deployment
templates, not evidence that an image has been published or deployed.

## Verification

Primary local gates:

```powershell
pnpm check
pnpm verify
pnpm db:validate
pnpm topology:validate
pnpm test:postgres:required
pnpm test:database:recovery
pnpm test:postgres:ha
node ..\sdkwork-specs\tools\deployctl.mjs validate --root . --profile cloud.production
node ..\sdkwork-specs\tools\deployctl.mjs validate --root . --profile standalone.production
node ..\sdkwork-github-workflow\scripts\sdkwork-workflow.mjs validate --config sdkwork.workflow.json
```

On 2026-08-01 the workspace upgraded to sqlx 0.9 and all dynamically assembled SQL statements
pass the sqlx compile-time injection audit. Nginx risk scanning rejects variable proxy targets,
covers the full proxy directive family (`grpc_pass`/`fastcgi_pass`/`uwsgi_pass`/`scgi_pass`/
`memcached_pass`), scans `set`/`map` URL literals and `upstream` members, and rejects unix
sockets; the pagination gate, API envelope gate, OpenAPI materialization, and all 26 SDK
generation targets verify current against the cursor-only contracts.
On 2026-07-21, a local Docker 28.0.4 Linux daemon completed the pinned PostgreSQL lifecycle/seed/drift
test, PostgreSQL repository parity test, checksummed PostgreSQL custom-format backup/restore,
streaming replication, primary shutdown,
standby promotion, and post-promotion tenant write. On 2026-08-01 the full Rust workspace compiles
cleanly, all workspace library and data-plane integration tests pass, OpenAPI materialization and
all 26 SDK generation targets verify current, and the pagination, API envelope, API operation
pattern, and database framework gates pass. Linux x64/arm64 archive smoke, registry publication,
image signing, Kubernetes rollout, and production observability still require their declared Linux
runners, credentials, infrastructure, and release approval.

Supply-chain verification covers the Web Server application lock and its source Core/UI workspaces;
all three currently report no known Node vulnerabilities. The shared frontend toolchain uses Vite
8.1.5 with esbuild 0.28.1, while VitePress remains scoped to its compatible Vite 6.4.3 line. RustSec
reports only `RUSTSEC-2023-0071` for `rsa` 0.9.10, which has no fixed release. Current consumers use
RSA key generation, signing, and verification, not the advisory's PKCS#1 v1.5 decryption path. The
advisory is intentionally not hidden by an audit ignore; accepting an exception or replacing the
shared IAM/framework crypto implementation requires human security review.
