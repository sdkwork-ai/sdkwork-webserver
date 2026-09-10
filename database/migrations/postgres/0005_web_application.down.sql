-- sdkwork:migration
-- version: 0005
-- engine: postgres
-- module: web
-- description: Rollback of 0005_web_application: drops the tenant-facing
--   application entity declared by the up migration header
--   (rollback: drop-web-application). Application rows are re-derivable from
--   web_site by re-running the up migration back-fill; deployed bindings that
--   reference application ids are not preserved.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

DROP TABLE IF EXISTS web_application;
