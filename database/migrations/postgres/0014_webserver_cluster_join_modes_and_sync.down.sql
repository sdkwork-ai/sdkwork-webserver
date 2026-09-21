-- sdkwork:migration
-- id: 0014_webserver_cluster_join_modes_and_sync
-- engine: postgres
-- module: web
-- reversible: true

DROP TABLE IF EXISTS webserver_cluster_sync_revision;

DROP INDEX IF EXISTS idx_webserver_cluster_instance_sync_status;
DROP INDEX IF EXISTS idx_webserver_cluster_instance_cluster_join_mode;
DROP INDEX IF EXISTS idx_webserver_cluster_host_cluster_join_mode;

ALTER TABLE webserver_cluster_instance
    DROP CONSTRAINT IF EXISTS chk_webserver_cluster_instance_sync_status;
ALTER TABLE webserver_cluster_instance
    DROP CONSTRAINT IF EXISTS chk_webserver_cluster_instance_quality_score;
ALTER TABLE webserver_cluster_instance
    DROP CONSTRAINT IF EXISTS chk_webserver_cluster_instance_join_mode;
ALTER TABLE webserver_cluster_instance
    DROP COLUMN IF EXISTS sync_status,
    DROP COLUMN IF EXISTS applied_applications_revision,
    DROP COLUMN IF EXISTS desired_applications_revision,
    DROP COLUMN IF EXISTS applied_config_revision,
    DROP COLUMN IF EXISTS desired_config_revision,
    DROP COLUMN IF EXISTS quality_score,
    DROP COLUMN IF EXISTS tunnel_route_domain,
    DROP COLUMN IF EXISTS join_mode;

ALTER TABLE webserver_cluster_host
    DROP CONSTRAINT IF EXISTS chk_webserver_cluster_host_join_mode;
ALTER TABLE webserver_cluster_host
    DROP COLUMN IF EXISTS tunnel_endpoint,
    DROP COLUMN IF EXISTS tunnel_route_domain,
    DROP COLUMN IF EXISTS join_mode;
