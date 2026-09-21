-- sdkwork:migration
-- id: 0015_webserver_cluster_load_balancing
-- engine: postgres
-- module: web
-- reversible: true

-- Cluster auto-routing configuration: the load balancing strategy used when
-- routing requests across the cluster's healthy instances, and the service
-- domains the cluster serves (auto-routing matches request hosts against
-- these).

ALTER TABLE webserver_cluster
    ADD COLUMN IF NOT EXISTS lb_strategy VARCHAR(32) NOT NULL DEFAULT 'round_robin',
    ADD COLUMN IF NOT EXISTS served_domains JSONB NOT NULL DEFAULT '[]';

COMMENT ON COLUMN webserver_cluster.lb_strategy IS 'Request routing strategy across cluster instances: round_robin (default) | weighted_round_robin | least_connections | random | random_two_choices | ip_hash | consistent_hash';
COMMENT ON COLUMN webserver_cluster.served_domains IS 'Service domains auto-routed to cluster instances (JSON array, lowercase)';
