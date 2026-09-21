-- sdkwork:migration
-- id: 0017_webserver_cluster_instance_detail
-- engine: postgres
-- module: web
-- reversible: true

-- Per-instance flexible configuration: routing weight override (LB input)
-- and operator maintenance note (instance detail context).

ALTER TABLE webserver_cluster_instance
    ADD COLUMN IF NOT EXISTS routing_weight INTEGER NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS maintenance_note VARCHAR(255);

ALTER TABLE webserver_cluster_instance
    DROP CONSTRAINT IF EXISTS chk_webserver_cluster_instance_routing_weight;
ALTER TABLE webserver_cluster_instance
    ADD CONSTRAINT chk_webserver_cluster_instance_routing_weight
    CHECK (routing_weight BETWEEN 1 AND 10000);

COMMENT ON COLUMN webserver_cluster_instance.routing_weight IS 'Per-instance load balancing weight override (1..=10000); consumed by the routing topology';
COMMENT ON COLUMN webserver_cluster_instance.maintenance_note IS 'Operator maintenance reason/context shown on the instance detail surface';
