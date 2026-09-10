-- sdkwork:migration
-- version: 0008
-- engine: postgres
-- module: web
-- description: Partial index backing the Nginx status active-config count
--   (is_active = TRUE AND status = 1) so the management status probe no longer
--   scans the active-config listing index.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

CREATE INDEX IF NOT EXISTS idx_web_nginx_config_active_status
    ON web_nginx_config (tenant_id)
    WHERE is_active = TRUE AND status = 1;
