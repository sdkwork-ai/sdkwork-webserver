-- sdkwork:migration
-- id: 0021_webserver_certificate_expiry_summary_index
-- engine: postgres
-- module: web
-- description: Covering partial index for the certificate-expiry sampler
--   aggregate (certificate_expiry_summary_repo): both the MIN seconds-to-expiry
--   and the 30-day FILTER count scan only `not_after` over the ACTIVE current
--   versions of issued, non-deleted certificates. `not_after` lives on
--   webserver_certificate_version (the certificate row only points at its
--   current version through current_version_id), so the index sits there and
--   the sampler drives from it: an index-only scan over active versions, with
--   a primary-key EXISTS probe per candidate into webserver_certificate. The
--   cost grows with the served set, never with the retained certificate
--   history.
-- reversible: true
-- rollback: down-migration drops the index
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

CREATE INDEX IF NOT EXISTS idx_webserver_certificate_expiry_summary
    ON webserver_certificate_version (not_after)
    WHERE status = 'ACTIVE';
