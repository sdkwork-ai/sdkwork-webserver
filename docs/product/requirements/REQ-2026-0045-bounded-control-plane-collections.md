# REQ-2026-0045 Bounded Control-Plane Collections And Agent Delta Sync

```yaml
id: REQ-2026-0045
title: Bound every control-plane collection and replace tenant-wide agent bundles with delta sync
owner: sdkwork-webserver
status: draft
source: reliability
problem: Environment-variable and health-check list operations currently read every matching row, while agent synchronization constructs complete tenant-wide Nginx and certificate vectors. Large tenants can therefore cause unbounded database reads and process memory growth even though the HTTP data plane is bounded.
goals:
  - Apply SDKWork store-level pagination to every control-plane list/search operation.
  - Replace tenant-wide agent bundle construction with node-scoped, versioned, bounded delta pages.
  - Bound rows, serialized bytes, database work, in-process allocations, and response size for every request.
  - Preserve tenant and node isolation across pagination and synchronization cursors.
non_goals:
  - Changing database table ownership or adding PostgreSQL HA topology support.
  - Loading all pages inside the server, SDK, agent, or frontend as a compatibility shortcut.
  - Claiming cluster convergence, offline retention, or rollout orchestration without separate HA evidence.
users:
  - Web control-plane operators
  - edge agents
  - SDK and console consumers of list operations
acceptance_criteria:
  - Environment-variable and health-check list APIs use canonical `page`/`page_size` or reviewed cursor inputs, default page size 20, maximum 200, and standard `data.items` plus `data.pageInfo` output.
  - Every persisted list filters, orders, and pages in SQL before row decoding; no repository, service, SDK, agent, or frontend path collects all rows and then slices.
  - Stable ordering includes a unique tie-breaker and tests prove adjacent pages do not overlap when timestamps tie.
  - Agent sync is scoped to the authenticated server/node and a fixed snapshot version; one response is bounded by item count and serialized bytes.
  - Agent delta cursors are opaque, versioned, tenant/node-bound, tamper-resistant, finite-lived, and rejected when mixed with another tenant, node, or snapshot.
  - Configuration and certificate tombstones are delivered so agents can remove retired state without requiring a full tenant snapshot.
  - Agents request successive pages explicitly, apply one validated snapshot atomically, and never buffer an unbounded history or retry queue.
  - Database and application tests use high-cardinality synthetic fixtures and prove query bounds, bounded allocations, cancellation, cursor isolation, and deterministic completion without OOM.
  - OpenAPI, generated app/backend SDKs, service ports, agent protocol, PC consumers, PRD, architecture, and migration guidance remain aligned.
non_functional_requirements:
  security: Cursors cannot expand tenant/node scope, reveal identifiers or secrets, or authorize access independently of typed request context and agent credentials.
  privacy: Secret environment values remain masked; certificate private-key material is returned only to an authorized target node and is not retained in cursor state or logs.
  performance: Default page size is 20, maximum page size is 200, agent item/byte ceilings are finite, and SQL uses suitable tenant/scope/order indexes.
  reliability: Snapshot application is atomic, resumable, idempotent, cancellation-safe, and fails closed on stale or inconsistent cursor state.
affected_surfaces:
  - api
  - sdk
  - backend
  - pc
  - agent
  - database
trace:
  specs:
    - API_SPEC.md
    - PAGINATION_SPEC.md
    - SDK_SPEC.md
    - DATABASE_SPEC.md
    - WEB_BACKEND_SPEC.md
    - SECURITY_SPEC.md
    - TEST_SPEC.md
  components:
    - apis/app-api/web/web-app-api.openapi.json
    - apis/backend-api/web/web-backend-api.openapi.json
    - crates/sdkwork-intelligence-webserver-service
    - crates/sdkwork-intelligence-webserver-repository-sqlx
    - crates/sdkwork-web-agent
    - apps/sdkwork-webserver-pc
verification:
  - node ../sdkwork-specs/tools/check-pagination.mjs --workspace .
  - node ../sdkwork-specs/tools/check-api-operation-patterns.mjs --workspace .
  - node ../sdkwork-specs/tools/check-api-response-envelope.mjs --workspace .
  - cargo test -p sdkwork-intelligence-webserver-repository-sqlx
  - cargo test -p sdkwork-web-agent
  - pnpm verify
```

## Human Review Gate

This requirement changes public list contracts, generated SDK methods, and the agent sync
protocol. Human review must approve the compatibility window, cursor/snapshot authority, legacy
method retirement, and agent rollout sequence before implementation changes those contracts.
No `listAll` compatibility implementation or server-side page aggregation is acceptable because
either would preserve the unbounded memory behavior this requirement exists to remove.

## Current Evidence

- REQ-2026-0004 proves PostgreSQL lifecycle and public repository behavior, including
  JSONB/TIMESTAMPTZ conversion and tenant isolation.
- Static inspection and the SDKWork pagination standard identify `list_env_variables`,
  `list_health_checks`, and tenant-wide agent sync vector construction as remaining unbounded
  control-plane paths.
- Existing data-plane admission, body, header, connection, and pressure controls do not bound these
  control-plane database result sets; this requirement is therefore a separate commercial gate.
- Accepted REQ-2026-0054 makes the legacy v3 full-manifest path finite by streaming deterministic
  database projections under single-field, item, retained-byte, post-decryption, and HTTP-body
  ceilings. It also removes the Nginx-domain N+1 query and fails closed on malformed active
  certificate records. This is an interim OOM boundary, not evidence that the cursor, immutable
  snapshot, delta, tombstone, atomic apply, or high-cardinality requirements above are complete.
