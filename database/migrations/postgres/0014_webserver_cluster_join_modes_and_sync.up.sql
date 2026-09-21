-- sdkwork:migration
-- id: 0014_webserver_cluster_join_modes_and_sync
-- engine: postgres
-- module: web
-- reversible: true

-- Cluster plane v2: per-host/instance join mode (LAN same-subnet vs TUNNEL
-- API-only remote), node service-quality scores, and the configuration /
-- application data-sync plane (desired revisions + per-instance applied
-- acknowledgments). Tenant 0 = platform-shared cluster infrastructure.

ALTER TABLE webserver_cluster_host
    ADD COLUMN IF NOT EXISTS join_mode INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS tunnel_route_domain VARCHAR(255),
    ADD COLUMN IF NOT EXISTS tunnel_endpoint VARCHAR(255);

ALTER TABLE webserver_cluster_host
    DROP CONSTRAINT IF EXISTS chk_webserver_cluster_host_join_mode;
ALTER TABLE webserver_cluster_host
    ADD CONSTRAINT chk_webserver_cluster_host_join_mode CHECK (join_mode BETWEEN 0 AND 1);

COMMENT ON COLUMN webserver_cluster_host.join_mode IS 'Join mode: 0=LAN (same-subnet direct API/shared database), 1=TUNNEL (API-only through the reverse tunnel)';
COMMENT ON COLUMN webserver_cluster_host.tunnel_route_domain IS 'Tunnel route domain that reaches this host through the public gateway (TUNNEL hosts)';
COMMENT ON COLUMN webserver_cluster_host.tunnel_endpoint IS 'Advertised gateway endpoint (host:port) this host dials for its tunnel (TUNNEL hosts)';

CREATE INDEX IF NOT EXISTS idx_webserver_cluster_host_cluster_join_mode
    ON webserver_cluster_host (tenant_id, cluster_id, join_mode)
    WHERE deleted_at IS NULL;

ALTER TABLE webserver_cluster_instance
    ADD COLUMN IF NOT EXISTS join_mode INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS tunnel_route_domain VARCHAR(255),
    ADD COLUMN IF NOT EXISTS quality_score INTEGER,
    ADD COLUMN IF NOT EXISTS desired_config_revision VARCHAR(64),
    ADD COLUMN IF NOT EXISTS applied_config_revision VARCHAR(64),
    ADD COLUMN IF NOT EXISTS desired_applications_revision VARCHAR(64),
    ADD COLUMN IF NOT EXISTS applied_applications_revision VARCHAR(64),
    ADD COLUMN IF NOT EXISTS sync_status INTEGER NOT NULL DEFAULT 0;

ALTER TABLE webserver_cluster_instance
    DROP CONSTRAINT IF EXISTS chk_webserver_cluster_instance_join_mode;
ALTER TABLE webserver_cluster_instance
    ADD CONSTRAINT chk_webserver_cluster_instance_join_mode CHECK (join_mode BETWEEN 0 AND 1);

ALTER TABLE webserver_cluster_instance
    DROP CONSTRAINT IF EXISTS chk_webserver_cluster_instance_quality_score;
ALTER TABLE webserver_cluster_instance
    ADD CONSTRAINT chk_webserver_cluster_instance_quality_score CHECK (quality_score IS NULL OR quality_score BETWEEN 0 AND 100);

ALTER TABLE webserver_cluster_instance
    DROP CONSTRAINT IF EXISTS chk_webserver_cluster_instance_sync_status;
ALTER TABLE webserver_cluster_instance
    ADD CONSTRAINT chk_webserver_cluster_instance_sync_status CHECK (sync_status BETWEEN 0 AND 3);

COMMENT ON COLUMN webserver_cluster_instance.join_mode IS 'Join mode: 0=LAN (same-subnet direct API/shared database), 1=TUNNEL (API-only through the reverse tunnel)';
COMMENT ON COLUMN webserver_cluster_instance.tunnel_route_domain IS 'Tunnel route domain that reaches this instance through the public gateway (TUNNEL instances)';
COMMENT ON COLUMN webserver_cluster_instance.quality_score IS 'Node service quality score 0..=100 derived from the latest heartbeat quality sample';
COMMENT ON COLUMN webserver_cluster_instance.desired_config_revision IS 'Desired configuration revision (sync plane); NULL until the cluster publishes one';
COMMENT ON COLUMN webserver_cluster_instance.applied_config_revision IS 'Configuration revision the instance last acknowledged as applied';
COMMENT ON COLUMN webserver_cluster_instance.desired_applications_revision IS 'Desired applications-manifest revision (sync plane); NULL until published';
COMMENT ON COLUMN webserver_cluster_instance.applied_applications_revision IS 'Applications-manifest revision the instance last acknowledged as applied';
COMMENT ON COLUMN webserver_cluster_instance.sync_status IS 'Aggregate sync status: 0=unknown, 1=in_sync, 2=pending, 3=failed';

CREATE INDEX IF NOT EXISTS idx_webserver_cluster_instance_cluster_join_mode
    ON webserver_cluster_instance (tenant_id, cluster_id, join_mode, status)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_webserver_cluster_instance_sync_status
    ON webserver_cluster_instance (tenant_id, cluster_id, sync_status)
    WHERE deleted_at IS NULL;

-- Desired-state revisions published by the cluster administrator. One row
-- per (cluster, kind, revision); the latest row per kind is the desired
-- state every instance must apply and acknowledge.
CREATE TABLE IF NOT EXISTS webserver_cluster_sync_revision (
    id            BIGINT       NOT NULL,
    uuid          VARCHAR(64)  NOT NULL,
    tenant_id     BIGINT       NOT NULL DEFAULT 0,
    cluster_id    BIGINT       NOT NULL,
    kind          INTEGER      NOT NULL,
    revision      VARCHAR(64)  NOT NULL,
    sha256        VARCHAR(64)  NOT NULL,
    payload       JSONB        NOT NULL,
    size_bytes    BIGINT       NOT NULL DEFAULT 0,
    created_by    VARCHAR(100),
    created_at    TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT uk_webserver_cluster_sync_uuid UNIQUE (uuid),
    CONSTRAINT uk_webserver_cluster_sync_revision UNIQUE (tenant_id, cluster_id, kind, revision),
    CONSTRAINT fk_webserver_cluster_sync_cluster FOREIGN KEY (tenant_id, cluster_id)
        REFERENCES webserver_cluster (tenant_id, id),
    CONSTRAINT chk_webserver_cluster_sync_kind CHECK (kind BETWEEN 0 AND 1)
);

COMMENT ON TABLE webserver_cluster_sync_revision IS 'Desired-state revision published to cluster instances (config / applications sync plane)';
COMMENT ON COLUMN webserver_cluster_sync_revision.kind IS 'Sync payload kind: 0=config (webserver runtime configuration), 1=applications (application management manifest)';
COMMENT ON COLUMN webserver_cluster_sync_revision.revision IS 'Monotonic revision label (snowflake id string) used by instances to detect drift';
COMMENT ON COLUMN webserver_cluster_sync_revision.sha256 IS 'SHA-256 of the canonical payload JSON for integrity verification on the node';
COMMENT ON COLUMN webserver_cluster_sync_revision.payload IS 'Desired-state payload (JSON document the node applies)';
COMMENT ON COLUMN webserver_cluster_sync_revision.size_bytes IS 'Serialized payload size in bytes';

-- Listing the sync history of one cluster, newest first.
CREATE INDEX IF NOT EXISTS idx_webserver_cluster_sync_cluster_kind_created
    ON webserver_cluster_sync_revision (tenant_id, cluster_id, kind, created_at DESC);
