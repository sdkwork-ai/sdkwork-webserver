# MIG-2026-0062 PostgreSQL Authority V2

```yaml
id: MIG-2026-0062
owner: web-platform
status: accepted
requirement: REQ-2026-0004
type: prelaunch-database-baseline
scope:
  producers:
    - sdkwork-webserver
    - database/database.manifest.json
  consumers:
    - sdkwork-webserver-database-host
    - sdkwork-api-webserver-standalone-gateway
    - sdkwork-webserver-certificate-worker
strategy: prelaunch-consolidation
postgresql_target:
  minimum_version: 16
  required_extensions: []
  authoritative_contract: database/contract/schema.yaml
data_cutover:
  required: false
  reason: application not released and no supported legacy production database exists
dual_write: prohibited
rollback:
  - restore the last verified PostgreSQL backup
  - deploy the prior compatible service build against PostgreSQL
  - forward-fix the PostgreSQL contract when writes crossed the backup boundary
verification:
  - pnpm db:validate
  - pnpm test:postgres:required
  - pnpm test:database:recovery
  - pnpm test:postgres:ha
```

PostgreSQL is the only Web authoritative server database. The repository contains no
alternative-engine baseline, lifecycle test, repository implementation, recovery authority,
deployment profile, release asset, or rollback target. Because the application has not shipped,
this change is a prelaunch baseline consolidation rather than a live data migration.

Future PostgreSQL schema evolution uses reviewed migrations, backup/restore evidence, and explicit
rollback or forward-fix plans. It must not reintroduce a second mutable database authority.
