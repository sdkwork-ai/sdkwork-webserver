-- sdkwork:migration
-- version: 0008
-- engine: postgres
-- module: web
-- description: Rollback of 0008_web_nginx_config_active_index: drops the
--   partial active-config count index.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

DROP INDEX IF EXISTS idx_web_nginx_config_active_status;
