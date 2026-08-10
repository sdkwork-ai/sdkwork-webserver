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
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

ALTER TABLE web_site ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE web_site SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE web_site ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE web_site ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE web_root_domain ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE web_root_domain SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE web_root_domain ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE web_root_domain ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE web_domain ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE web_domain SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE web_domain ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE web_domain ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE web_site_binding ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE web_site_binding SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE web_site_binding ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE web_site_binding ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE web_certificate ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE web_certificate SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE web_certificate ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE web_certificate ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE web_source_version ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE web_source_version SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE web_source_version ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE web_source_version ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE web_deployment ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE web_deployment SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE web_deployment ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE web_deployment ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE web_audit_log ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE web_audit_log SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE web_audit_log ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE web_audit_log ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE web_application ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE web_application SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE web_application ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE web_application ALTER COLUMN organization_id SET NOT NULL;

COMMIT;
