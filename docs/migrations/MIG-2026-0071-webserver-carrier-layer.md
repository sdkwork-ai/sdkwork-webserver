# MIG-2026-0071 Web Server Carrier Layer

```yaml
id: MIG-2026-0071
owner: sdkwork-webserver
status: executed
requirement: REQ-2026-0071
type: breaking
scope:
  producers:
    - database/ddl/baseline/postgres/0001_webserver_baseline.sql
    - database/migrations/postgres/0031_webserver_application_deploy_application_id.up.sql
    - database/backfill/postgres/0031_webserver_application_deploy_application_id.sql
    - database/contract/schema.yaml
    - apis/backend-api/web/openapi.yaml
    - crates/sdkwork-webserver-contract
    - crates/sdkwork-intelligence-webserver-repository-sqlx
    - crates/sdkwork-intelligence-webserver-service
    - crates/sdkwork-api-webserver-standalone-gateway
  consumers:
    - sdkwork-webserver-backend-sdk
    - sdkwork-deployments (deploy_app)
compatibility_window: none (pre-launch; MIGRATION_SPEC section 4.4)
strategy: no-compatibility-approved
rollback:
  supported: true
  steps:
    - `0031_..._deploy_application_id.down.sql` drops the unique index and the
      column; the row data it carried is the only thing lost, and the backfill
      procedure can be re-run after a re-apply.
    - The command retirements are not reversible by a down migration: a
      webserver deployment record written after the retirement would be a second
      release authority. Restore the capability by moving it to Deploy instead.
verification:
  - cargo check -p sdkwork-webserver-contract -p sdkwork-intelligence-webserver-service -p sdkwork-intelligence-webserver-repository-sqlx -p sdkwork-api-webserver-standalone-gateway -p sdkwork-routes-webserver-backend-api
  - node ../sdkwork-specs/tools/check-database-framework-standard.mjs --root .
  - pnpm api:materialize
```

## Summary

`webserver_application` carried a second application identity next to
`deploy_app` (sdkwork-deployments): its own slug, application kind, default
environment, plus two satellite models — `webserver_application_host` (derived
hostname inventory) and `webserver_app_platform_target` (release channels and
bundle ids) — and a second publishing plane (`webserver_source_version` +
`webserver_deployment`). Two rows could describe the same application and
disagree.

Deploy is now the only application identity and publishing authority. The
webserver module is downgraded to the carrier layer: site, domain, certificate,
listener and reverse-proxy configuration.

- `webserver_application` keeps only what a carrier row needs and gains
  `deploy_application_id`, the 1:1 reference to `deploy_app.id`
  (`uk_webserver_application_deploy_application_id`, partial unique on live
  rows).
- Creating a carrier row requires `deployApplicationId`. A missing value is a
  validation error that names the field; the module never generates an
  application identity.
- The publishing writes are retired. The service keeps the request validation
  and then refuses with a conflict that names Deploy, instead of writing a
  release record Deploy never sees.
- The edge startup reconciler no longer writes an application inventory derived
  from imported module configuration, because such a row cannot know its Deploy
  application.

The column is nullable **only** for rows that existed before the migration. No
statement synthesizes a link: the mapping is a fact Deploy owns, and the
operator backfills it with
`database/backfill/postgres/0031_webserver_application_deploy_application_id.sql`.
A follow-up migration may tighten the column to `NOT NULL` once query 3 of that
script returns no rows.

## Compatibility

Breaking, pre-launch (no compatibility window):

- `POST /backend/v3/api/applications` without `deployApplicationId` now fails
  with a validation problem naming the field (it used to succeed).
- `POST /backend/v3/api/applications/{applicationId}/source_versions` and
  `.../source_versions/git_import` return the retirement conflict
  (`409`) after validating the body. Both operations are marked
  `deprecated: true` in the OpenAPI authority.
- `applications.activate` keeps its existing precondition (a successful
  `webserver_deployment`), which the retirement makes unreachable for new
  applications. Activating an application is a Deploy lifecycle action; see the
  deletion list.
- Reads of the historical `webserver_source_version` / `webserver_deployment`
  rows are unchanged.
- The carrier create contract no longer accepts `appKind` (2026-11-17, deletion
  list item 6 part 1): `CreateApplicationRequest` has no such member and the
  schema is `additionalProperties: false`, so a client that still sends it fails
  deserialization. The carrier row is written with the server default
  `STATIC_WEB`, which derives `site_type = 1` and the site carrier application
  type `WEB` — the values the carrier console used to request explicitly.

## Forward Fix

1. Migration `0031` (`ALTER TABLE ... ADD COLUMN IF NOT EXISTS`, partial unique
   index) and the mirrored folded block in the baseline snapshot.
2. Contract: `database/contract/schema.yaml` declares the column and the index;
   `CreateApplicationRequest.deployApplicationId` is required on the wire
   (int64-as-string, `x-sdkwork-int64-string`), and `ApplicationResponse`
   returns it.
3. Repository: the insert writes the reference and maps a 1:1 violation to a
   conflict naming the rule; the projection reads it back.
4. Service: `create_application` requires the field;
   `create_source_version`, `import_git_source_version`, `create_deployment`,
   `rollback_deployment` and `create_platform_target` validate and then refuse.
5. Edge: `application_registry::reconcile_application_registry_at_startup` logs
   the retirement and writes nothing; `reconcile_application_registry` returns
   the retirement error.

## Still Held By Web Server (Deletion List)

Each item is a publishing or identity capability this module still *contains*
even though it no longer *writes* it. Delete in this order; every step must
first prove no live caller remains.

| # | Item | Location | Delete when |
| - | ---- | -------- | ----------- |
| 1 | `webserver_source_version` table + write path (`create_source_version_repo`, retention pruning) | `database/ddl/baseline/postgres/0001_webserver_baseline.sql`, `crates/sdkwork-intelligence-webserver-repository-sqlx/src/source_versions.rs`, `crates/sdkwork-intelligence-webserver-service/src/repository.rs` | No console/report reads the historical rows (they are 409-refused for writes already) |
| 2 | `webserver_deployment` table + write/rollback paths | `crates/sdkwork-intelligence-webserver-repository-sqlx/src/deployments.rs` | Deploy exposes the deployment history the console needs |
| 3 | `applications.activate` deployment precondition (`webserver_deployment.status = 2`) | `crates/sdkwork-intelligence-webserver-repository-sqlx/src/applications.rs`, `crates/sdkwork-intelligence-webserver-service/src/app.rs` | The activate/pause lifecycle moves to Deploy `app_status` |
| 4 | `webserver_app_platform_target` table + write path | `crates/sdkwork-intelligence-webserver-repository-sqlx/src/platform_targets.rs` | Deploy owns platform/bundle targets |
| 5 | Retired command surface: `WebAppApi::{create_source_version, import_git_source_version, create_deployment, rollback_deployment, create_platform_target}`, the `WebBackendApi` delegations, the two routed POSTs, and their OpenAPI operations | `crates/sdkwork-webserver-contract/src/app_ports.rs`, `crates/sdkwork-intelligence-webserver-service/src/{app,backend}.rs`, `crates/sdkwork-routes-webserver-backend-api/src/{routes,paths}.rs`, `apis/backend-api/web/openapi.yaml` | The generated SDKs drop the operations (regenerate with `pnpm sdk:generate`) |
| 6 | Identity fields duplicated on the carrier row: `application_kind`, `default_environment`, `metadata->storeListing` | `database/.../0001_webserver_baseline.sql`, `crates/sdkwork-webserver-contract/src/dto.rs` | The carrier row reads them through `deploy_application_id` instead |
| 7 | `webserver_application_host` (derived hostname inventory) | `database/.../0001_webserver_baseline.sql`, `crates/sdkwork-api-webserver-standalone-gateway/src/application_registry.rs` | `deploy_app_binding` is the single hostname inventory |
| 8 | `application_registry.rs` module (derive helpers, retired writer, summary) + the startup hook call | `crates/sdkwork-api-webserver-standalone-gateway/src/{application_registry,main,lib}.rs` | The Deploy-side reconciliation adopts `derive_applications`/`application_from_import` |
| 9 | Operator backfill script | `database/backfill/postgres/0031_webserver_application_deploy_application_id.sql` | A follow-up migration makes `deploy_application_id` `NOT NULL` |
| 10 | Cross-module existence check for `deployApplicationId` | `crates/sdkwork-intelligence-webserver-repository-sqlx/src/applications.rs` | Deploy exposes an app-id lookup the create path can call (no cross-module FK is expressible) |

## Deletion Progress

An item is checked only when the whole capability it names is gone; a capability
that is only partly removed stays unchecked and records the slice that is done.

- [x] **Item 6, part 1/2 — the client-authored `appKind` on the carrier row
  (2026-11-17).** `CreateApplicationRequest` no longer carries `appKind`, so the
  regenerated SDKs cannot send it; the repository writes the documented server
  default `CARRIER_APP_KIND` (`STATIC_WEB` → `webserver_site.site_type = 1`,
  `application_type = 'WEB'`, exactly what the carrier page used to request), and
  the page's constant is gone. No migration is needed: the DB column keeps its
  `NOT NULL` DDL default and is still written explicitly, so existing rows and
  the `appKind` response projection are unchanged.
- [ ] **Item 6, part 2/2 — the stored identity duplicates.** Drop
  `webserver_application.application_kind` (only once `ApplicationResponse` no
  longer projects `appKind`), `default_environment`, and the
  `metadata->storeListing` copy, and read them through `deploy_application_id`.
