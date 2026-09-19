-- sdkwork:migration
-- id: 0011_application_list_covering_index
-- engine: postgres
-- module: web
-- description: Adds the covering index for the tenant application listing
--   (ORDER BY updated_at DESC, id DESC with a tenant_id equality predicate
--   and no status predicate). The existing indexes lead with status or
--   user_id, so every page of applications.list sorted the tenant's whole
--   application set. The new index matches the repository's keyset convention
--   (tenant_id, updated_at DESC, id DESC) and is partial on deleted_at IS
--   NULL to mirror the listing predicate.
-- reversible: true
-- rollback: down-migration drops the index
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

CREATE INDEX IF NOT EXISTS idx_webserver_application_tenant_updated_id
    ON webserver_application (tenant_id, updated_at DESC, id DESC)
    WHERE deleted_at IS NULL;
