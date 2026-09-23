-- sdkwork:migration
-- id: 0019_webserver_certificate_operation_failure_detail
-- engine: postgres
-- module: web
-- reversible: true
-- rollback: down-migration drops the failure_detail column and its constraint
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

-- ---------------------------------------------------------------------------
-- The constraint goes first: PostgreSQL refuses to drop a column a CHECK still
-- references, so dropping the column first would fail on every database where
-- the baseline did not already declare the constraint inline.
-- ---------------------------------------------------------------------------
ALTER TABLE webserver_certificate_operation
    DROP CONSTRAINT IF EXISTS chk_webserver_certificate_operation_failure_detail;

ALTER TABLE webserver_certificate_operation
    DROP COLUMN IF EXISTS failure_detail;
