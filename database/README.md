# WEB Database Module

Canonical lifecycle assets for the `sdkwork-webserver` PostgreSQL control-plane authority.

- moduleId: `web`
- serviceCode: `WEB`
- owner: `web-platform`
- databaseRole: `authoritative-server`
- engine: PostgreSQL 16 or later
- required extensions: none
- tablePrefix: `web_`

## Initialization State

This module is in initialization state for greenfield PostgreSQL deployments:

1. `database/ddl/baseline/postgres/0001_web_baseline.sql` is the full PostgreSQL DDL snapshot.
2. `database/migrations/postgres/` contains checksum-tracked forward migrations for every schema
   change made after the initial baseline. Existing databases must never be upgraded by replaying
   or editing the baseline.
3. Production and staging use explicit migration commands; `lifecycle.autoMigrate` defaults to `false`.
4. `pnpm db:drift:check` verifies the deployed schema before release.

The pre-launch reconciliation migration
(`0001_web_schema_hardening`, plus `0002` env-variable rotation and `0003`
certificate lifecycle completion) upgrades databases initialized before the
Website runtime control plane and `application_type` were added. It preserves
existing sites as `WEB` and refuses to invent tenant-scope hashes for legacy
Web Nodes; operators must supply those hashes from their authoritative tenant
assignments before rerunning the migration. `0004_web_list_index_hardening`
adds the tenant-prefixed list-query indexes.

SQLite is not an authoritative server engine or deployment profile. This Web Server repository
does not provide a server-side SQLite repository or SQLite release profile. Any future SQLite
fixture must be declared separately as `client-local`; it cannot be used as server parity evidence,
an authority fallback, or a rollback target.

## Initialization state

This module is in **initialization state** for greenfield deployments:

1. **Baseline** — `database/ddl/baseline/{engine}/0001_web_baseline.sql` contains the full DDL snapshot.
2. **Migrations** — `database/migrations/{engine}/` is reserved for post-GA incremental schema changes only. It is intentionally empty at initialization.
3. **Drift** — run `pnpm db:drift:check` before release.

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
