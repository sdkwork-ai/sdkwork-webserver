-- sdkwork:migration
-- id: 0022_webserver_certificate_expiry_summary_index_shape
-- engine: postgres
-- module: web
-- reversible: true
-- rollback: down-migration restores the 0021 partial expiry-summary index
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

DROP INDEX IF EXISTS idx_webserver_certificate_expiry_summary;

CREATE INDEX IF NOT EXISTS idx_webserver_certificate_expiry_summary
    ON webserver_certificate_version (not_after)
    WHERE status = 'ACTIVE';
