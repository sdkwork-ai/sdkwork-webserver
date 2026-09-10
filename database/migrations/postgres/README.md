# PostgreSQL Migrations

SDKWork Web Server uses the `baseline-plus-migrations` strategy declared in
`database/database.manifest.json`:

- `database/ddl/baseline/postgres/0001_web_baseline.sql` is the authoritative
  schema snapshot for fresh installations. It is also the source for the
  `database/contract/schema.yaml` contract.
- This directory holds expand-only migrations for already-installed databases.
  The applied sequence starts at `0005_web_application`; earlier production
  hardening (partial slug uniqueness, tenant list indexes, credential GIN
  index, referential integrity, organization NOT NULL backfill) is already
  consolidated into the baseline snapshot.

Conventions:

- File names must match `\d{4}_[a-z0-9_]+.up.sql` / `.down.sql` (four-digit
  zero-padded sequence).
- Up migrations must be idempotent where PostgreSQL allows (`IF NOT EXISTS` /
  `DROP ... IF EXISTS` before creating), because the module may be re-applied.
- Every DDL change ships with a paired down migration, unless the up
  migration header declares `reversible: false` with a documented
  `rollback:` strategy (for example a forward-fix whose sentinel backfill is
  the canonical repair, as in `0006_organization_id_not_null`).
- After changing the baseline, regenerate the contract:

  ```powershell
  pnpm db:materialize:contract
  pnpm db:validate
  ```

- Apply and verify with the database CLI:

  ```powershell
  pnpm db:migrate
  pnpm db:drift:check
  ```
