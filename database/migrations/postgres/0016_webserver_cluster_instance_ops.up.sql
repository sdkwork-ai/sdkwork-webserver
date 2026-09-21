-- sdkwork:migration
-- id: 0016_webserver_cluster_instance_ops
-- engine: postgres
-- module: web
-- reversible: true

-- Per-instance operations: graceful drain, routing cordon/uncordon, labels,
-- restart tracking (auto-recovery evidence), and active-probe auto-eject /
-- auto-recover state.

ALTER TABLE webserver_cluster_instance
    ADD COLUMN IF NOT EXISTS routing_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS draining BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS drain_started_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS restart_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS last_restarted_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS ejected_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS probe_failures INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS probe_url VARCHAR(255),
    ADD COLUMN IF NOT EXISTS labels JSONB NOT NULL DEFAULT '{}';

COMMENT ON COLUMN webserver_cluster_instance.routing_enabled IS 'Cordon switch: FALSE removes the instance from the routing pool while it keeps serving existing work';
COMMENT ON COLUMN webserver_cluster_instance.draining IS 'Graceful drain in progress: excluded from routing and expected to stop after in-flight work completes';
COMMENT ON COLUMN webserver_cluster_instance.drain_started_at IS 'Drain start instant (NULL when not draining)';
COMMENT ON COLUMN webserver_cluster_instance.restart_count IS 'Process restarts observed via registration (process_started_at changes); auto-recovery evidence';
COMMENT ON COLUMN webserver_cluster_instance.last_restarted_at IS 'Latest observed restart instant';
COMMENT ON COLUMN webserver_cluster_instance.ejected_at IS 'Set when the active prober auto-ejects the instance; cleared on probe recovery';
COMMENT ON COLUMN webserver_cluster_instance.probe_failures IS 'Consecutive active-probe failures';
COMMENT ON COLUMN webserver_cluster_instance.probe_url IS 'Override URL the active prober checks (defaults to the instance bind endpoint)';
COMMENT ON COLUMN webserver_cluster_instance.labels IS 'Operator labels JSON object (free-form organization metadata)';

CREATE INDEX IF NOT EXISTS idx_webserver_cluster_instance_routing
    ON webserver_cluster_instance (tenant_id, cluster_id, routing_enabled, draining)
    WHERE deleted_at IS NULL;
