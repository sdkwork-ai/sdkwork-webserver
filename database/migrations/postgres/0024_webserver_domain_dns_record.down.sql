-- sdkwork:migration
-- id: 0024_webserver_domain_dns_record
-- engine: postgres
-- module: web
-- reversible: true
-- rollback: down-migration drops the snapshot table and its indexes
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

DROP INDEX IF EXISTS idx_webserver_domain_dns_record_domain;
DROP INDEX IF EXISTS idx_webserver_domain_dns_record_zone;

DROP TABLE IF EXISTS webserver_domain_dns_record;
