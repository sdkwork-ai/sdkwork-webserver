-- sdkwork:migration
-- id: 0011_application_list_covering_index
-- engine: postgres
-- module: web
-- reversible: true

DROP INDEX IF EXISTS idx_webserver_application_tenant_updated_id;
