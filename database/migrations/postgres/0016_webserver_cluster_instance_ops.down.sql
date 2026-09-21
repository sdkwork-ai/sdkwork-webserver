-- sdkwork:migration
-- id: 0016_webserver_cluster_instance_ops
-- engine: postgres
-- module: web
-- reversible: true

DROP INDEX IF EXISTS idx_webserver_cluster_instance_routing;

ALTER TABLE webserver_cluster_instance
    DROP COLUMN IF EXISTS labels,
    DROP COLUMN IF EXISTS probe_url,
    DROP COLUMN IF EXISTS probe_failures,
    DROP COLUMN IF EXISTS ejected_at,
    DROP COLUMN IF EXISTS last_restarted_at,
    DROP COLUMN IF EXISTS restart_count,
    DROP COLUMN IF EXISTS drain_started_at,
    DROP COLUMN IF EXISTS draining,
    DROP COLUMN IF EXISTS routing_enabled;
