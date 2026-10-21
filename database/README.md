# Web Server Database Module

Canonical lifecycle assets for the `sdkwork-webserver` PostgreSQL control-plane authority.

- moduleId: `webserver`
- serviceCode: `WEB`
- owner: `web-platform`
- databaseRole: `authoritative-server`
- engine: PostgreSQL 16 or later
- required extensions: none
- tablePrefix: `webserver_`

## Initialization State

- `baselineStrategy`: committed primary baseline
- baseline: `database/ddl/baseline/postgres/0001_webserver_baseline.sql` (full PostgreSQL DDL snapshot)
- ordered migration range: `0005_webserver_application` through
  `0029_webserver_nginx_config_revision`
- consolidation level: the baseline is the consolidated pre-launch snapshot; the ordered post-baseline
  migrations remain the load-bearing upgrade path and are replayed idempotently by the lifecycle on a
  fresh install, so both a baseline-only and a baseline-plus-migrations database converge on the same
  schema and migration history.

`0005`–`0012` upgrade databases created before the Website control plane, `application_type`, the
`webserver_` table prefix and the cluster plane. `0013`–`0026` complete the application kind checks,
runtime assignments, certificate lifecycle, node WeChat verification and their indexes. `0027` closes
the tenant-integrity audit findings: tenant-scoped business tables no longer default to tenant `0`,
site/root-domain children carry tenant-bound composite foreign keys, application slugs and active
nginx configurations are unique through partial indexes, and the cluster admin listings gain the
tenant-leading indexes their predicates seek. `0028` adds the per-owner plugin catalog
(`webserver_plugin`): tenant/user scoping, one active plugin key per owner, draft/active status,
git/archive source fields, and checksum/plugin-key checks. `0029` adds
`webserver_nginx_config_revision`: one immutable snapshot per `(config_id, version_no)`, so the
revision counter written by nginx config updates has readable, restorable history.

SQLite is not an authoritative server engine or deployment profile. This Web Server repository does
not provide a server-side SQLite repository or SQLite release profile. Any future SQLite fixture must
be declared separately as `client-local`; it cannot be used as server parity evidence, an authority
fallback, or a rollback target.

## Commands

```bash
pnpm run db:validate
pnpm run db:materialize:contract
pnpm run db:plan
pnpm run db:init
pnpm run db:migrate
pnpm run db:seed
pnpm run db:status
pnpm run db:drift:check
```
