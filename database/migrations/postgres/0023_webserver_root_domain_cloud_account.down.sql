-- sdkwork:migration
-- id: 0023_webserver_root_domain_cloud_account
-- engine: postgres
-- module: web
-- reversible: true
-- rollback: down-migration drops the cloud_account_id column, its shape constraint and its index
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

-- ---------------------------------------------------------------------------
-- The constraint goes first: PostgreSQL refuses to drop a column a CHECK still
-- references, so dropping the column first would fail on every database where
-- the baseline did not already declare the constraint inline.
-- ---------------------------------------------------------------------------
ALTER TABLE webserver_root_domain
    DROP CONSTRAINT IF EXISTS chk_webserver_root_domain_cloud_account;

DROP INDEX IF EXISTS idx_webserver_root_domain_cloud_account;

ALTER TABLE webserver_root_domain
    DROP COLUMN IF EXISTS cloud_account_id;
