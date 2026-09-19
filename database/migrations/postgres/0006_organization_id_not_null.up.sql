-- sdkwork:migration
-- id: 0006_organization_id_not_null
-- engine: postgres
-- module: sdkwork-webserver
-- purpose: Enforce organization_id NOT NULL DEFAULT on all tables in the
--   consolidated baseline. NULL rows (pre-standard data anomalies) are
--   backfilled with the platform sentinel before NOT NULL is set, and
--   NOT NULL columns without an explicit default receive the sentinel
--   default, keeping existing deployments consistent with fresh baseline
--   installs.
-- reversible: false
-- rollback: forward-fix (sentinel backfill is the canonical fix; NULL
--   organization rows are data anomalies)
-- transactional: true
-- lock: access-exclusive (each ALTER TABLE ... SET NOT NULL and the
--            backfill UPDATE take ACCESS EXCLUSIVE and rewrite the table; on
--            webserver_audit_log this is a full-table write pause)
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

ALTER TABLE webserver_site ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE webserver_site SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE webserver_site ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE webserver_site ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE webserver_root_domain ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE webserver_root_domain SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE webserver_root_domain ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE webserver_root_domain ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE webserver_domain ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE webserver_domain SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE webserver_domain ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE webserver_domain ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE webserver_site_binding ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE webserver_site_binding SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE webserver_site_binding ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE webserver_site_binding ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE webserver_certificate ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE webserver_certificate SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE webserver_certificate ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE webserver_certificate ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE webserver_source_version ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE webserver_source_version SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE webserver_source_version ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE webserver_source_version ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE webserver_deployment ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE webserver_deployment SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE webserver_deployment ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE webserver_deployment ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE webserver_audit_log ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE webserver_audit_log SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE webserver_audit_log ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE webserver_audit_log ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE webserver_application ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE webserver_application SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE webserver_application ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE webserver_application ALTER COLUMN organization_id SET NOT NULL;

COMMIT;
