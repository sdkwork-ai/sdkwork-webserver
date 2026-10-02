-- sdkwork:migration
-- id: 0022_webserver_certificate_expiry_summary_index_shape
-- engine: postgres
-- module: web
-- description: Reshapes the certificate-expiry sampler index from the
--   0021 partial form (`(not_after) WHERE status = 'ACTIVE'`) to a covering
--   `(status, not_after)` index without a predicate. The sampler aggregate
--   still drives an index-only scan over the contiguous `status = 'ACTIVE'`
--   band (leading column seek) with a primary-key EXISTS probe per candidate
--   into webserver_certificate, so scan cost stays proportional to the
--   served set. The predicate was dropped because schema-drift comparison
--   reads stored predicates back through pg_get_expr, whose rendered cast
--   form for varchar equality is not reconcilable with the authored form.
-- reversible: true
-- rollback: down-migration recreates the 0021 partial index
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

DROP INDEX IF EXISTS idx_webserver_certificate_expiry_summary;

CREATE INDEX IF NOT EXISTS idx_webserver_certificate_expiry_summary
    ON webserver_certificate_version (status, not_after);
