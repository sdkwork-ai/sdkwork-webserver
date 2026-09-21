-- sdkwork:migration
-- id: 0017_webserver_cluster_instance_detail
-- engine: postgres
-- module: web
-- reversible: true

ALTER TABLE webserver_cluster_instance
    DROP CONSTRAINT IF EXISTS chk_webserver_cluster_instance_routing_weight;
ALTER TABLE webserver_cluster_instance
    DROP COLUMN IF EXISTS maintenance_note,
    DROP COLUMN IF EXISTS routing_weight;
