-- sdkwork:migration
-- id: 0023_webserver_root_domain_cloud_account
-- engine: postgres
-- module: web
-- description: Binds a root-domain Zone to the cloud account that owns its DNS
--   automation. Until now a Zone carried only a descriptive `dns_provider`
--   string and a free-text `provider_zone_ref`, so "which account does this
--   domain publish through" was answerable only by reading the edge's
--   deployment configuration, and an operator with several accounts had no way
--   to tell two Zones apart or to list the Zones one account serves.
--   `cloud_account_id` is that missing identity, and it is what the Domains page
--   filters on. NULL keeps the previous behaviour exactly: the account resolves
--   per operation, which is what every root domain reconciled from the edge's
--   own configuration does. A reference rather than a foreign key, because
--   `iam_provider_account` is owned by sdkwork-iam
--   (DATABASE_FRAMEWORK_SPEC cross-module ownership) and a hard FK would couple
--   this module's schema to another module's table lifecycle; liveness is
--   enforced on write by the service layer.
-- reversible: true
-- rollback: down-migration drops the cloud_account_id column, its shape constraint and its index
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

-- ---------------------------------------------------------------------------
-- 1. The account reference.
--
-- VARCHAR(128) matches the wire bound the service plane enforces
-- (`CLOUD_ACCOUNT_ID_MAX_CHARS`) and the column the deployments DNS Zone
-- already uses, so one account id is storable in every module that references
-- the account center.
-- ---------------------------------------------------------------------------
ALTER TABLE webserver_root_domain
    ADD COLUMN IF NOT EXISTS cloud_account_id VARCHAR(128);

COMMENT ON COLUMN webserver_root_domain.cloud_account_id IS
    'Cloud account whose DNS automation this root domain is bound to; NULL resolves the account per operation';

-- ---------------------------------------------------------------------------
-- 2. Shape rule.
--
-- PostgreSQL has no `ADD CONSTRAINT IF NOT EXISTS`, so the guard is the catalog
-- itself, which also makes this re-runnable per the expand-only convention. The
-- baseline declares the same constraint inline for fresh installations; the two
-- forms must stay identical or a fresh database and a migrated one would accept
-- different values.
--
-- Both halves are reactive to the same rule the service plane checks, so a value
-- that reaches the store by any path is still bounded: a leading alphanumeric
-- keeps a stray separator out of the first position, and the class admits the
-- characters account codes actually use while refusing whitespace and control
-- characters.
-- ---------------------------------------------------------------------------
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'webserver_root_domain'::regclass
          AND conname = 'chk_webserver_root_domain_cloud_account'
    ) THEN
        ALTER TABLE webserver_root_domain
            ADD CONSTRAINT chk_webserver_root_domain_cloud_account CHECK (
                cloud_account_id IS NULL
                OR cloud_account_id ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{1,127}$'
            );
    END IF;
END
$$;

-- ---------------------------------------------------------------------------
-- 3. The filter's index.
--
-- Partial on purpose: the column is NULL for every root domain that resolves its
-- account per operation, so a full index would be mostly empty. `tenant_id`
-- leads because the filter is always applied inside one tenant, and
-- `deleted_at IS NULL` is in the predicate because the list read never returns a
-- soft-deleted row.
-- ---------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_webserver_root_domain_cloud_account
    ON webserver_root_domain (tenant_id, cloud_account_id)
    WHERE cloud_account_id IS NOT NULL AND deleted_at IS NULL;
