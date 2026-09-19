-- sdkwork:migration
-- id: 0012_webserver_cluster_management
-- engine: postgres
-- module: web
-- description: Adds the distributed Web Server cluster management plane:
--   webserver_cluster (deployment grouping), webserver_cluster_host (machine
--   identity: system basics, remote/local IPs, machine code, MAC addresses),
--   webserver_cluster_instance (one row per webserver process), the
--   append-only webserver_cluster_event log, webserver_cluster_heartbeat
--   samples, and the webserver_cluster_peer_message mailbox that carries
--   instance-to-instance traffic through the control plane. Cluster data is
--   platform infrastructure and lives under tenant 0.
-- reversible: true
-- rollback: down-migration drops the cluster tables in reverse dependency
--   order
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

CREATE TABLE IF NOT EXISTS webserver_cluster (
    id                         BIGINT       NOT NULL,
    uuid                       VARCHAR(64)  NOT NULL,
    tenant_id                  BIGINT       NOT NULL DEFAULT 0,
    name                       VARCHAR(100) NOT NULL,
    code                       VARCHAR(64)  NOT NULL,
    description                VARCHAR(500),
    status                     INTEGER      NOT NULL DEFAULT 1,
    heartbeat_interval_seconds INTEGER      NOT NULL DEFAULT 15,
    offline_threshold_seconds  INTEGER      NOT NULL DEFAULT 60,
    metadata                   JSONB        NOT NULL DEFAULT '{}',
    created_at                 TIMESTAMPTZ  NOT NULL,
    updated_at                 TIMESTAMPTZ  NOT NULL,
    version                    BIGINT       NOT NULL DEFAULT 0,
    deleted_at                 TIMESTAMPTZ,
    PRIMARY KEY (id),
    CONSTRAINT uk_webserver_cluster_uuid UNIQUE (uuid),
    CONSTRAINT uk_webserver_cluster_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT chk_webserver_cluster_status CHECK (status BETWEEN 0 AND 1),
    CONSTRAINT chk_webserver_cluster_heartbeat_interval CHECK (heartbeat_interval_seconds BETWEEN 5 AND 600),
    CONSTRAINT chk_webserver_cluster_offline_threshold CHECK (offline_threshold_seconds BETWEEN 10 AND 3600)
);

COMMENT ON TABLE webserver_cluster IS 'Web Server cluster registry (distributed deployment grouping)';
COMMENT ON COLUMN webserver_cluster.id IS 'Snowflake primary key';
COMMENT ON COLUMN webserver_cluster.uuid IS 'Globally unique identifier';
COMMENT ON COLUMN webserver_cluster.tenant_id IS 'Tenant ID; 0 = platform-shared cluster infrastructure';
COMMENT ON COLUMN webserver_cluster.name IS 'Cluster display name';
COMMENT ON COLUMN webserver_cluster.code IS 'URL-friendly unique code within the platform tenant';
COMMENT ON COLUMN webserver_cluster.description IS 'Cluster description';
COMMENT ON COLUMN webserver_cluster.status IS 'Status: 0=inactive, 1=active';
COMMENT ON COLUMN webserver_cluster.heartbeat_interval_seconds IS 'Expected instance heartbeat interval in seconds';
COMMENT ON COLUMN webserver_cluster.offline_threshold_seconds IS 'Heartbeat silence after which an instance/host is marked offline';
COMMENT ON COLUMN webserver_cluster.metadata IS 'Cluster metadata JSON';
COMMENT ON COLUMN webserver_cluster.version IS 'Optimistic concurrency version';

-- Cluster codes are unique while active; soft-deleted clusters release the code.
CREATE UNIQUE INDEX IF NOT EXISTS uk_webserver_cluster_code
    ON webserver_cluster (tenant_id, code)
    WHERE deleted_at IS NULL;

-- Cluster listing sorts by recency for the admin overview.
CREATE INDEX IF NOT EXISTS idx_webserver_cluster_tenant_status_updated
    ON webserver_cluster (tenant_id, status, updated_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS webserver_cluster_host (
    id                BIGINT       NOT NULL,
    uuid              VARCHAR(64)  NOT NULL,
    tenant_id         BIGINT       NOT NULL DEFAULT 0,
    cluster_id        BIGINT       NOT NULL,
    name              VARCHAR(100) NOT NULL,
    hostname          VARCHAR(255) NOT NULL,
    machine_code      VARCHAR(128) NOT NULL,
    os_name           VARCHAR(100),
    os_version        VARCHAR(100),
    kernel_version    VARCHAR(100),
    arch              VARCHAR(32),
    cpu_model         VARCHAR(200),
    cpu_cores         INTEGER,
    memory_total_mb   BIGINT,
    remote_ip         VARCHAR(64),
    local_ips         JSONB        NOT NULL DEFAULT '[]',
    mac_addresses     JSONB        NOT NULL DEFAULT '[]',
    daemon_version    VARCHAR(64),
    status            INTEGER      NOT NULL DEFAULT 0,
    last_heartbeat_at TIMESTAMPTZ,
    metadata          JSONB        NOT NULL DEFAULT '{}',
    created_at        TIMESTAMPTZ  NOT NULL,
    updated_at        TIMESTAMPTZ  NOT NULL,
    version           BIGINT       NOT NULL DEFAULT 0,
    deleted_at        TIMESTAMPTZ,
    PRIMARY KEY (id),
    CONSTRAINT uk_webserver_cluster_host_uuid UNIQUE (uuid),
    CONSTRAINT uk_webserver_cluster_host_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT fk_webserver_cluster_host_cluster FOREIGN KEY (tenant_id, cluster_id)
        REFERENCES webserver_cluster (tenant_id, id),
    CONSTRAINT chk_webserver_cluster_host_status CHECK (status BETWEEN 0 AND 4)
);

COMMENT ON TABLE webserver_cluster_host IS 'Web Server cluster host (machine) inventory';
COMMENT ON COLUMN webserver_cluster_host.id IS 'Snowflake primary key';
COMMENT ON COLUMN webserver_cluster_host.uuid IS 'Globally unique identifier';
COMMENT ON COLUMN webserver_cluster_host.tenant_id IS 'Tenant ID; 0 = platform-shared cluster infrastructure';
COMMENT ON COLUMN webserver_cluster_host.cluster_id IS 'Owning cluster ID';
COMMENT ON COLUMN webserver_cluster_host.name IS 'Host display name (defaults to hostname)';
COMMENT ON COLUMN webserver_cluster_host.hostname IS 'Operating system hostname';
COMMENT ON COLUMN webserver_cluster_host.machine_code IS 'Stable hardware/machine fingerprint reported by the host daemon';
COMMENT ON COLUMN webserver_cluster_host.os_name IS 'Operating system name (e.g. Linux, Windows)';
COMMENT ON COLUMN webserver_cluster_host.os_version IS 'Operating system version';
COMMENT ON COLUMN webserver_cluster_host.kernel_version IS 'Kernel version string';
COMMENT ON COLUMN webserver_cluster_host.arch IS 'CPU architecture (e.g. x86_64, aarch64)';
COMMENT ON COLUMN webserver_cluster_host.cpu_model IS 'CPU model string';
COMMENT ON COLUMN webserver_cluster_host.cpu_cores IS 'Logical CPU core count';
COMMENT ON COLUMN webserver_cluster_host.memory_total_mb IS 'Total physical memory in MiB';
COMMENT ON COLUMN webserver_cluster_host.remote_ip IS 'Remote IP the host was observed from by the control plane';
COMMENT ON COLUMN webserver_cluster_host.local_ips IS 'Local network interface IP addresses (JSON array)';
COMMENT ON COLUMN webserver_cluster_host.mac_addresses IS 'Network interface MAC addresses (JSON array)';
COMMENT ON COLUMN webserver_cluster_host.daemon_version IS 'Last reported webserver/daemon build version';
COMMENT ON COLUMN webserver_cluster_host.status IS 'Status: 0=offline, 1=online, 2=deploying, 3=error, 4=maintenance';
COMMENT ON COLUMN webserver_cluster_host.last_heartbeat_at IS 'Last host heartbeat instant (latest member instance heartbeat)';
COMMENT ON COLUMN webserver_cluster_host.metadata IS 'Host metadata JSON';
COMMENT ON COLUMN webserver_cluster_host.version IS 'Optimistic concurrency version';

-- One machine fingerprint per platform tenant; soft-deleted hosts release it.
CREATE UNIQUE INDEX IF NOT EXISTS uk_webserver_cluster_host_machine_code
    ON webserver_cluster_host (tenant_id, machine_code)
    WHERE deleted_at IS NULL;

-- Host listing filters by cluster and status, sorted by recency.
CREATE INDEX IF NOT EXISTS idx_webserver_cluster_host_cluster_status_updated
    ON webserver_cluster_host (tenant_id, cluster_id, status, updated_at DESC, id DESC)
    WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS webserver_cluster_instance (
    id                 BIGINT       NOT NULL,
    uuid               VARCHAR(64)  NOT NULL,
    tenant_id          BIGINT       NOT NULL DEFAULT 0,
    host_id            BIGINT       NOT NULL,
    cluster_id         BIGINT       NOT NULL,
    name               VARCHAR(100) NOT NULL,
    role               VARCHAR(32)  NOT NULL DEFAULT 'GATEWAY',
    environment        VARCHAR(16)  NOT NULL DEFAULT 'production',
    process_pid        INTEGER,
    process_started_at TIMESTAMPTZ,
    bind_host          VARCHAR(64),
    bind_port          INTEGER,
    public_endpoint    VARCHAR(255),
    build_version      VARCHAR(64),
    status             INTEGER      NOT NULL DEFAULT 0,
    health_state       VARCHAR(16)  NOT NULL DEFAULT 'UNKNOWN',
    last_heartbeat_at  TIMESTAMPTZ,
    last_online_at     TIMESTAMPTZ,
    uptime_seconds     BIGINT       NOT NULL DEFAULT 0,
    metrics            JSONB        NOT NULL DEFAULT '{}',
    metadata           JSONB        NOT NULL DEFAULT '{}',
    created_at         TIMESTAMPTZ  NOT NULL,
    updated_at         TIMESTAMPTZ  NOT NULL,
    version            BIGINT       NOT NULL DEFAULT 0,
    deleted_at         TIMESTAMPTZ,
    PRIMARY KEY (id),
    CONSTRAINT uk_webserver_cluster_instance_uuid UNIQUE (uuid),
    CONSTRAINT uk_webserver_cluster_instance_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT fk_webserver_cluster_instance_host FOREIGN KEY (tenant_id, host_id)
        REFERENCES webserver_cluster_host (tenant_id, id),
    CONSTRAINT fk_webserver_cluster_instance_cluster FOREIGN KEY (tenant_id, cluster_id)
        REFERENCES webserver_cluster (tenant_id, id),
    CONSTRAINT chk_webserver_cluster_instance_role CHECK (
        role IN ('GATEWAY', 'MANAGEMENT', 'DATA_PLANE', 'WORKER', 'OTHER')
    ),
    CONSTRAINT chk_webserver_cluster_instance_environment CHECK (
        environment IN ('development', 'test', 'staging', 'production')
    ),
    CONSTRAINT chk_webserver_cluster_instance_status CHECK (status BETWEEN 0 AND 5),
    CONSTRAINT chk_webserver_cluster_instance_health_state CHECK (
        health_state IN ('HEALTHY', 'DEGRADED', 'UNHEALTHY', 'UNKNOWN')
    )
);

COMMENT ON TABLE webserver_cluster_instance IS 'Web Server cluster process instance registry';
COMMENT ON COLUMN webserver_cluster_instance.id IS 'Snowflake primary key';
COMMENT ON COLUMN webserver_cluster_instance.uuid IS 'Globally unique identifier';
COMMENT ON COLUMN webserver_cluster_instance.tenant_id IS 'Tenant ID; 0 = platform-shared cluster infrastructure';
COMMENT ON COLUMN webserver_cluster_instance.host_id IS 'Hosting machine ID';
COMMENT ON COLUMN webserver_cluster_instance.cluster_id IS 'Owning cluster ID (denormalized from the host for listing)';
COMMENT ON COLUMN webserver_cluster_instance.name IS 'Instance display name';
COMMENT ON COLUMN webserver_cluster_instance.role IS 'Process role: GATEWAY, MANAGEMENT, DATA_PLANE, WORKER, OTHER';
COMMENT ON COLUMN webserver_cluster_instance.environment IS 'Deployment environment: development, test, staging, production';
COMMENT ON COLUMN webserver_cluster_instance.process_pid IS 'Operating system process ID';
COMMENT ON COLUMN webserver_cluster_instance.process_started_at IS 'Process start instant (restarts with the same PID take a new instant)';
COMMENT ON COLUMN webserver_cluster_instance.bind_host IS 'Ingress bind address';
COMMENT ON COLUMN webserver_cluster_instance.bind_port IS 'Ingress bind port';
COMMENT ON COLUMN webserver_cluster_instance.public_endpoint IS 'Advertised endpoint peers use to reach this instance';
COMMENT ON COLUMN webserver_cluster_instance.build_version IS 'webserver build version of the process';
COMMENT ON COLUMN webserver_cluster_instance.status IS 'Status: 0=offline, 1=online, 2=starting, 3=stopping, 4=error, 5=maintenance';
COMMENT ON COLUMN webserver_cluster_instance.health_state IS 'Health: HEALTHY, DEGRADED, UNHEALTHY, UNKNOWN';
COMMENT ON COLUMN webserver_cluster_instance.last_heartbeat_at IS 'Last accepted heartbeat instant';
COMMENT ON COLUMN webserver_cluster_instance.last_online_at IS 'Last instant the instance transitioned to or was confirmed online';
COMMENT ON COLUMN webserver_cluster_instance.uptime_seconds IS 'Process uptime in seconds at the last heartbeat';
COMMENT ON COLUMN webserver_cluster_instance.metrics IS 'Latest resource metrics snapshot (CPU/memory/connections JSON)';
COMMENT ON COLUMN webserver_cluster_instance.metadata IS 'Instance metadata JSON (includes the heartbeat token hash)';
COMMENT ON COLUMN webserver_cluster_instance.version IS 'Optimistic concurrency version';

-- One live process per PID per host; soft-deleted rows release the identity.
CREATE UNIQUE INDEX IF NOT EXISTS uk_webserver_cluster_instance_process
    ON webserver_cluster_instance (tenant_id, host_id, process_pid)
    WHERE deleted_at IS NULL AND process_pid IS NOT NULL;

-- Instance listing filters by cluster/host and status, sorted by recency.
CREATE INDEX IF NOT EXISTS idx_webserver_cluster_instance_cluster_status_updated
    ON webserver_cluster_instance (tenant_id, cluster_id, status, updated_at DESC, id DESC)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_webserver_cluster_instance_host_status_updated
    ON webserver_cluster_instance (tenant_id, host_id, status, updated_at DESC, id DESC)
    WHERE deleted_at IS NULL;

-- Cluster heartbeat authentication resolves the instance by its token hash
-- through metadata containment; the GIN index keeps the lookup indexed
-- (same pattern as webserver_server agent tokens).
CREATE INDEX IF NOT EXISTS idx_webserver_cluster_instance_metadata_gin
    ON webserver_cluster_instance USING GIN (metadata);

CREATE TABLE IF NOT EXISTS webserver_cluster_event (
    id           BIGINT       NOT NULL,
    uuid         VARCHAR(64)  NOT NULL,
    tenant_id    BIGINT       NOT NULL DEFAULT 0,
    cluster_id   BIGINT       NOT NULL,
    host_id      BIGINT,
    instance_id  BIGINT,
    event_type   VARCHAR(64)  NOT NULL,
    severity     VARCHAR(16)  NOT NULL DEFAULT 'INFO',
    message      VARCHAR(500) NOT NULL,
    detail       JSONB        NOT NULL DEFAULT '{}',
    occurred_at  TIMESTAMPTZ  NOT NULL,
    created_at   TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT uk_webserver_cluster_event_uuid UNIQUE (uuid),
    CONSTRAINT uk_webserver_cluster_event_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT fk_webserver_cluster_event_cluster FOREIGN KEY (tenant_id, cluster_id)
        REFERENCES webserver_cluster (tenant_id, id),
    CONSTRAINT fk_webserver_cluster_event_host FOREIGN KEY (tenant_id, host_id)
        REFERENCES webserver_cluster_host (tenant_id, id),
    CONSTRAINT fk_webserver_cluster_event_instance FOREIGN KEY (tenant_id, instance_id)
        REFERENCES webserver_cluster_instance (tenant_id, id),
    CONSTRAINT chk_webserver_cluster_event_severity CHECK (severity IN ('INFO', 'WARNING', 'ERROR'))
);

COMMENT ON TABLE webserver_cluster_event IS 'Append-only Web Server cluster lifecycle event log';
COMMENT ON COLUMN webserver_cluster_event.id IS 'Snowflake primary key';
COMMENT ON COLUMN webserver_cluster_event.uuid IS 'Globally unique identifier';
COMMENT ON COLUMN webserver_cluster_event.tenant_id IS 'Tenant ID; 0 = platform-shared cluster infrastructure';
COMMENT ON COLUMN webserver_cluster_event.cluster_id IS 'Cluster the event belongs to';
COMMENT ON COLUMN webserver_cluster_event.host_id IS 'Host the event is about (nullable for cluster-level events)';
COMMENT ON COLUMN webserver_cluster_event.instance_id IS 'Instance the event is about (nullable for host/cluster-level events)';
COMMENT ON COLUMN webserver_cluster_event.event_type IS 'Lifecycle event type (e.g. HOST_ONLINE, INSTANCE_OFFLINE, HEALTH_DEGRADED)';
COMMENT ON COLUMN webserver_cluster_event.severity IS 'Severity: INFO, WARNING, ERROR';
COMMENT ON COLUMN webserver_cluster_event.message IS 'Human-readable event message';
COMMENT ON COLUMN webserver_cluster_event.detail IS 'Structured event detail JSON';
COMMENT ON COLUMN webserver_cluster_event.occurred_at IS 'Instant the event occurred (as reported)';
COMMENT ON COLUMN webserver_cluster_event.created_at IS 'Record persistence instant';

CREATE INDEX IF NOT EXISTS idx_webserver_cluster_event_cluster_occurred
    ON webserver_cluster_event (tenant_id, cluster_id, occurred_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_webserver_cluster_event_instance_occurred
    ON webserver_cluster_event (tenant_id, instance_id, occurred_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS webserver_cluster_heartbeat (
    id           BIGINT      NOT NULL,
    uuid         VARCHAR(64) NOT NULL,
    tenant_id    BIGINT      NOT NULL DEFAULT 0,
    instance_id  BIGINT      NOT NULL,
    host_id      BIGINT      NOT NULL,
    status       INTEGER     NOT NULL,
    latency_ms   INTEGER,
    metrics      JSONB       NOT NULL DEFAULT '{}',
    reported_at  TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT uk_webserver_cluster_heartbeat_uuid UNIQUE (uuid),
    CONSTRAINT uk_webserver_cluster_heartbeat_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT fk_webserver_cluster_heartbeat_instance FOREIGN KEY (tenant_id, instance_id)
        REFERENCES webserver_cluster_instance (tenant_id, id),
    CONSTRAINT fk_webserver_cluster_heartbeat_host FOREIGN KEY (tenant_id, host_id)
        REFERENCES webserver_cluster_host (tenant_id, id)
);

COMMENT ON TABLE webserver_cluster_heartbeat IS 'Web Server instance heartbeat samples (bounded retention)';
COMMENT ON COLUMN webserver_cluster_heartbeat.id IS 'Snowflake primary key';
COMMENT ON COLUMN webserver_cluster_heartbeat.uuid IS 'Globally unique identifier';
COMMENT ON COLUMN webserver_cluster_heartbeat.tenant_id IS 'Tenant ID; 0 = platform-shared cluster infrastructure';
COMMENT ON COLUMN webserver_cluster_heartbeat.instance_id IS 'Reporting instance ID';
COMMENT ON COLUMN webserver_cluster_heartbeat.host_id IS 'Hosting machine ID';
COMMENT ON COLUMN webserver_cluster_heartbeat.status IS 'Reported instance status at heartbeat time';
COMMENT ON COLUMN webserver_cluster_heartbeat.latency_ms IS 'Control-plane observed request latency in milliseconds';
COMMENT ON COLUMN webserver_cluster_heartbeat.metrics IS 'Resource metrics JSON (CPU/memory/connections)';
COMMENT ON COLUMN webserver_cluster_heartbeat.reported_at IS 'Instant the instance reported the heartbeat';
COMMENT ON COLUMN webserver_cluster_heartbeat.created_at IS 'Record persistence instant';

CREATE INDEX IF NOT EXISTS idx_webserver_cluster_heartbeat_instance_reported
    ON webserver_cluster_heartbeat (tenant_id, instance_id, reported_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS webserver_cluster_peer_message (
    id                BIGINT       NOT NULL,
    uuid              VARCHAR(64)  NOT NULL,
    tenant_id         BIGINT       NOT NULL DEFAULT 0,
    cluster_id        BIGINT       NOT NULL,
    from_instance_id  BIGINT,
    to_instance_id    BIGINT,
    message_type      VARCHAR(64)  NOT NULL,
    payload           JSONB        NOT NULL DEFAULT '{}',
    state             VARCHAR(16)  NOT NULL DEFAULT 'PENDING',
    deliver_at        TIMESTAMPTZ  NOT NULL,
    delivered_at      TIMESTAMPTZ,
    expires_at        TIMESTAMPTZ,
    created_at        TIMESTAMPTZ  NOT NULL,
    updated_at        TIMESTAMPTZ  NOT NULL,
    version           BIGINT       NOT NULL DEFAULT 0,
    PRIMARY KEY (id),
    CONSTRAINT uk_webserver_cluster_peer_message_uuid UNIQUE (uuid),
    CONSTRAINT uk_webserver_cluster_peer_message_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT fk_webserver_cluster_peer_message_cluster FOREIGN KEY (tenant_id, cluster_id)
        REFERENCES webserver_cluster (tenant_id, id),
    CONSTRAINT fk_webserver_cluster_peer_message_from FOREIGN KEY (tenant_id, from_instance_id)
        REFERENCES webserver_cluster_instance (tenant_id, id),
    CONSTRAINT fk_webserver_cluster_peer_message_to FOREIGN KEY (tenant_id, to_instance_id)
        REFERENCES webserver_cluster_instance (tenant_id, id),
    CONSTRAINT chk_webserver_cluster_peer_message_state CHECK (state IN ('PENDING', 'DELIVERED', 'EXPIRED'))
);

COMMENT ON TABLE webserver_cluster_peer_message IS 'Instance-to-instance peer message mailbox delivered through the control plane';
COMMENT ON COLUMN webserver_cluster_peer_message.id IS 'Snowflake primary key';
COMMENT ON COLUMN webserver_cluster_peer_message.uuid IS 'Globally unique identifier';
COMMENT ON COLUMN webserver_cluster_peer_message.tenant_id IS 'Tenant ID; 0 = platform-shared cluster infrastructure';
COMMENT ON COLUMN webserver_cluster_peer_message.cluster_id IS 'Cluster the message belongs to';
COMMENT ON COLUMN webserver_cluster_peer_message.from_instance_id IS 'Sending instance ID (nullable for control-plane-originated messages)';
COMMENT ON COLUMN webserver_cluster_peer_message.to_instance_id IS 'Target instance ID; NULL broadcasts to every cluster member';
COMMENT ON COLUMN webserver_cluster_peer_message.message_type IS 'Application-level message type tag';
COMMENT ON COLUMN webserver_cluster_peer_message.payload IS 'Message payload JSON';
COMMENT ON COLUMN webserver_cluster_peer_message.state IS 'Delivery state: PENDING, DELIVERED, EXPIRED';
COMMENT ON COLUMN webserver_cluster_peer_message.deliver_at IS 'Earliest delivery instant';
COMMENT ON COLUMN webserver_cluster_peer_message.delivered_at IS 'Instant the message was handed to the target instance';
COMMENT ON COLUMN webserver_cluster_peer_message.expires_at IS 'Instant after which an undelivered message expires';
COMMENT ON COLUMN webserver_cluster_peer_message.version IS 'Optimistic concurrency version';

-- Delivery scans messages for one instance in dispatch order; the claim
-- query filters the delivery state itself.
CREATE INDEX IF NOT EXISTS idx_webserver_cluster_peer_message_pending
    ON webserver_cluster_peer_message (tenant_id, to_instance_id, deliver_at, id);

CREATE INDEX IF NOT EXISTS idx_webserver_cluster_peer_message_cluster_created
    ON webserver_cluster_peer_message (tenant_id, cluster_id, created_at DESC, id DESC);
