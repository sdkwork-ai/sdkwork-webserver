-- Migration: 0031_webserver_application_deploy_application_id
-- Description: Links the webserver carrier record to the Deploy application it
--   carries. `deploy_app` (sdkwork-deployments) is the single application
--   identity and publishing authority; `webserver_application` is downgraded to
--   a site-carrier record and MUST NOT exist without a deploy application. The
--   column stays nullable only because pre-existing rows have no mapping yet:
--   an operator backfills them from the Deploy inventory with
--   `database/backfill/postgres/0031_webserver_application_deploy_application_id.sql`.
--   Every row the module writes from this migration forward supplies the value,
--   and the service rejects a create without it.
-- engine: postgres
-- module: webserver
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

ALTER TABLE webserver_application
    ADD COLUMN IF NOT EXISTS deploy_application_id BIGINT;

COMMENT ON COLUMN webserver_application.deploy_application_id IS
    'Deploy application identity (deploy_app.id). 1:1 with this carrier row; NULL only on legacy rows awaiting the operator backfill';

-- 1:1 with the Deploy application: one live carrier row per deploy app. The
-- partial predicate keeps soft-deleted carrier rows from blocking a re-link and
-- ignores the legacy rows the backfill has not reached yet.
CREATE UNIQUE INDEX IF NOT EXISTS uk_webserver_application_deploy_application_id
    ON webserver_application (deploy_application_id)
    WHERE deploy_application_id IS NOT NULL AND deleted_at IS NULL;
