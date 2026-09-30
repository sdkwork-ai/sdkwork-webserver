-- sdkwork:migration
-- id: 0020_webserver_application_surface_registry
-- engine: postgres
-- module: web
-- reversible: true
-- rollback: down-migration drops webserver_application_host and the two added
--   webserver_application columns
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

DROP TABLE IF EXISTS webserver_application_host;

ALTER TABLE webserver_application
    DROP CONSTRAINT IF EXISTS chk_webserver_application_access_surfaces;

ALTER TABLE webserver_application
    DROP COLUMN IF EXISTS access_surfaces;

ALTER TABLE webserver_application
    DROP COLUMN IF EXISTS metadata;
