-- sdkwork:migration
-- version: 0007
-- engine: postgres
-- module: web
-- description: Rollback of 0007_web_app_platform_target: drops the application
--   platform distribution target table with its indexes. Data is destroyed;
--   only run when the platform-target feature is being removed.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

DROP INDEX IF EXISTS uk_web_app_platform_target_active_key;
DROP INDEX IF EXISTS idx_web_app_platform_target_app_id;
DROP TABLE IF EXISTS web_app_platform_target;
