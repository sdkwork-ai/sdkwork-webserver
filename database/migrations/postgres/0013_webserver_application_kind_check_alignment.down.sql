-- sdkwork:migration
-- id: 0013_webserver_application_kind_check_alignment
-- engine: postgres
-- module: web
-- reversible: true

ALTER TABLE webserver_application
    DROP CONSTRAINT IF EXISTS chk_webserver_application_kind;

ALTER TABLE webserver_application
    ADD CONSTRAINT chk_webserver_application_kind CHECK (application_kind IN ('WEB', 'API'));
