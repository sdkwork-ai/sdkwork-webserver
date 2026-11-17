-- Rollback of 0031_webserver_application_deploy_application_id. Both objects
-- exist only in this migration, so the reversal is exact: dropping the unique
-- index first releases the 1:1 constraint, then the column goes with its values.
DROP INDEX IF EXISTS uk_webserver_application_deploy_application_id;

ALTER TABLE webserver_application
    DROP COLUMN IF EXISTS deploy_application_id;
