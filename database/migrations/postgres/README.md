# PostgreSQL Migrations

SDKWork Web Server uses the `baseline-plus-migrations` strategy declared in
`database/database.manifest.json`:

- `database/ddl/baseline/postgres/0001_webserver_baseline.sql` is the authoritative
  schema snapshot for fresh installations. It is also the source for the
  `database/contract/schema.yaml` contract.
- This directory holds expand-only migrations for already-installed databases.
  The applied sequence starts at `0005_webserver_application`; earlier production
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

## Header metadata is byte-sensitive once a migration has been applied

`ops_schema_migration_history` stores the SHA-256 of each applied `.up.sql` file and
the lifecycle compares it against `sha256(file bytes)` on every start and every drift
run. A mismatch is an **error-level** `checksum_mismatch` diff, and the release
packaging incident on 2026-09-10 (`docs/reports/2026-09-10-webserver-0.1.5-packaging-verification.md`
§4.1) shows the container crash-looping on exactly this. Consequences for editing
files in this directory:

- **End-of-line style counts.** `file_checksum` hashes raw bytes with no normalisation,
  so an LF/CRLF flip is a content change. Verified at the time of writing: the recorded
  hash for version `0005` equals `sha256(CRLF form of the HEAD blob of
  0005_web_application.up.sql)`.
- **Comments count.** Changing a header comment — including a cosmetic one — changes
  the hash of an already-applied migration and breaks every database that has it.
- **`-- module:` records the module identity at authoring time, and is deliberately
  historical.** `0005`, `0007`, `0008` and `0009` declare `-- module: web`; `0006`
  declares `-- module: sdkwork-webserver`. The authority for module identity is
  `database/database.manifest.json` → `moduleId: webserver`. No gate reads the
  `-- module:` field (`reversible` and `rollback` are the only header keys the layout
  validator consumes), so **do not edit these headers to make them agree** — the
  agreement is already provided by the manifest, and editing them would invalidate the
  recorded checksums of five migrations.

## Missing header metadata: use the sidecar, never the SQL file

The `-- rollback` / `-- transactional` / `-- lock` / `-- lock_timeout` /
`-- statement_timeout` keys are required for PostgreSQL migrations (§7.2), but several
migrations in this directory were authored without them (`0014`–`0017`). Because every
file here is already recorded by checksum, **the fix is never to edit the `.up.sql`** —
that is exactly the edit the section above forbids.

The canonical escape hatch is the checksum-external sidecar
`database/migrations/postgres/metadata.json` (`sourcePolicy: historical-immutable`).
The layout validator merges it over the header (`{...header, ...sidecar}`), so the
missing keys are satisfied while the SQL bytes — and therefore every recorded checksum —
stay untouched. Generate it with the canonical tool rather than by hand:

```powershell
node ../sdkwork-specs/tools/align-database-migration-metadata-workspace.mjs --workspace ..\ --write
```

The tool only rewrites `.up.sql` for **untracked** migrations; tracked ones are routed to
the sidecar. Verify with `pnpm db:validate`, which must pass without touching the SQL.

The aligner is workspace-scoped: `--workspace <dir>` walks every `sdkwork-*` sibling under
`<dir>`, so point it at the workspace root (`D:\sdkwork-space`), not at a single repository.
It is idempotent and only adds sidecar entries for migrations whose headers are genuinely
missing keys, so running it fleet-wide is the normal way to close this class of debt — but
expect it to touch other repositories too, and review `git status` across them before
committing. To touch only this repository, import `alignMigrationDirectory()` from the tool
and call it with `databaseDir` set here, as the incident on 2026-09-21 did.

## Changing the module identity (`web` → `webserver`, 2026-09-18)

`ADR-20260917-web-framework-table-prefix.md` Amendment 2 renamed the module's tables
from `web_*` to `webserver_*` and its `moduleId` from `web` to `webserver`. Because the
lifecycle keys migration state on `(module_id, version, engine)`, an existing database
carries rows under the old identity that a `webserver` run cannot see. Migration `0010`
cannot repair that from inside itself — the engine decides the pending set before it
runs anything, and would replay `0005`–`0008` against a database that already has their
effects. The reconciliation is therefore a documented out-of-band step, run **before**
`pnpm db:migrate` on a database that was installed under `web`:

1. Record row counts for every `web_*` table.
2. Re-key the ledger and refresh the checksums to the current on-disk bytes:

   ```sql
   UPDATE ops_schema_migration_history
   SET module_id = 'webserver',
       name      = replace(name, 'web_', 'webserver_'),
       checksum  = <sha256 of the current .up.sql file>
   WHERE engine = 'postgres' AND module_id = 'web';
   ```

   Version `0009` keeps its filename and bytes (its up-migration retires a table under
   the legacy name, which is an L0 historical fact), so only its `module_id` changes.
3. `pnpm db:migrate` — expects `applied 1 migration(s)` (migration `0010` only).
4. `pnpm db:drift:check` — expects `drift check passed`.
5. Re-count rows: the same tables must hold the same counts.

Apply the same procedure whenever a module's `moduleId` changes.

