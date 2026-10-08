-- sdkwork:migration
-- id: 0025_webserver_domain_dns_record_status
-- engine: postgres
-- module: web
-- reversible: true
-- rollback: down-migration drops the record_status column
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

ALTER TABLE webserver_domain_dns_record
    DROP CONSTRAINT IF EXISTS chk_webserver_domain_dns_record_status;

ALTER TABLE webserver_domain_dns_record
    DROP COLUMN IF EXISTS record_status;
