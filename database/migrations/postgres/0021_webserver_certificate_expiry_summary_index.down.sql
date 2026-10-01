-- sdkwork:migration
-- id: 0021_webserver_certificate_expiry_summary_index
-- engine: postgres
-- module: web
-- reversible: true
-- rollback: down-migration drops the expiry-summary covering index
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

DROP INDEX IF EXISTS idx_webserver_certificate_expiry_summary;
