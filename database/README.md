# Web Server Database Module

Canonical lifecycle assets for the `sdkwork-webserver` PostgreSQL control-plane authority.

- moduleId: `webserver`
- serviceCode: `WEB`
- owner: `web-platform`
- databaseRole: `authoritative-server`
- engine: PostgreSQL 16 or later
- required extensions: none
- tablePrefix: `webserver_`

## Initialization state

This module is in **initialization state** for greenfield deployments:

1. **Baseline** — `database/ddl/baseline/{engine}/0001_webserver_baseline.sql` contains the full DDL snapshot.
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
