-- sdkwork:migration
-- id: 0021_webserver_certificate_expiry_summary_index
-- engine: postgres
-- module: web
-- description: Covering partial index for the certificate-expiry sampler
--   aggregate (certificate_expiry_summary_repo): both the MIN
--   seconds-to-expiry and the 30-day FILTER count scan only `not_after` over
--   active, non-deleted certificates. Without it the sampler aggregates the
--   whole table on every interval; with it the aggregate is an index-only
--   scan whose cost no longer grows with the retained certificate history.
-- reversible: true
-- rollback: down-migration drops the index
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

CREATE INDEX IF NOT EXISTS idx_webserver_certificate_expiry_summary
    ON webserver_certificate (not_after)
    WHERE deleted_at IS NULL AND status = 1;
