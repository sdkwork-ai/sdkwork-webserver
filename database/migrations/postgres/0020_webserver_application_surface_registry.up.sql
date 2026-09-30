-- sdkwork:migration
-- id: 0020_webserver_application_surface_registry
-- engine: postgres
-- module: web
-- description: Imported-application surface registry. Every module import the
--   edge materializes is one application (PC desktop surface, H5 mobile
--   browser surface, or both), so the tenant-facing webserver_application row
--   must be able to carry that access-surface fact and the provenance the
--   startup reconciler keys its ownership on: `access_surfaces` is a JSONB
--   array of `{"surface": "PC" | "H5"}` objects and `metadata` carries
--   `source`/`importId`/`profile`/`environment`. The per-server host inventory
--   moves to its own table: one application is served through one or more
--   server blocks and each server block answers for one or more hostnames, so
--   webserver_application_host records (application, server block, hostname,
--   environment) instead of overloading the application row. Hosts are keyed
--   per application rather than globally: the same hostname may serve
--   different applications, and the tenant-level hostname asset already lives
--   in webserver_domain.
-- reversible: true
-- rollback: down-migration drops webserver_application_host and the two added
--   webserver_application columns
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

ALTER TABLE webserver_application
    ADD COLUMN metadata JSONB NOT NULL DEFAULT '{}';

ALTER TABLE webserver_application
    ADD COLUMN access_surfaces JSONB NOT NULL DEFAULT '[]';

-- The array element shape ({"surface": "PC" | "H5"}) is owned by the writers
-- and documented in database/contract/schema.yaml; the constraint pins the
-- container so a malformed document cannot enter silently.
ALTER TABLE webserver_application
    ADD CONSTRAINT chk_webserver_application_access_surfaces
    CHECK (jsonb_typeof(access_surfaces) = 'array');

COMMENT ON COLUMN webserver_application.access_surfaces IS
    'Access surfaces of the application: JSONB array of {"surface": "PC" | "H5"}; empty when the application serves no static web surface';
COMMENT ON COLUMN webserver_application.metadata IS
    'Provenance and extension metadata; rows reconciled from the served edge configuration carry metadata.source = ''served-config'' and metadata.importId';

CREATE TABLE IF NOT EXISTS webserver_application_host (
    id              BIGINT        NOT NULL,
    uuid            VARCHAR(64)   NOT NULL,
    tenant_id       BIGINT        NOT NULL,
    organization_id BIGINT        NOT NULL DEFAULT 0,
    data_scope      INTEGER       NOT NULL DEFAULT 1,
    user_id         BIGINT,
    app_id          BIGINT        NOT NULL,
    hostname        VARCHAR(255)  NOT NULL,
    hostname_type   VARCHAR(16)   NOT NULL DEFAULT 'EXACT',
    environment     VARCHAR(16)   NOT NULL DEFAULT 'production',
    vhost_id        VARCHAR(200)  NOT NULL DEFAULT '',
    path_prefix     VARCHAR(4096) NOT NULL DEFAULT '/',
    status          INTEGER       NOT NULL DEFAULT 1,
    metadata        JSONB         NOT NULL DEFAULT '{}',
    created_at      TIMESTAMPTZ   NOT NULL,
    updated_at      TIMESTAMPTZ   NOT NULL,
    version         BIGINT        NOT NULL DEFAULT 0,
    deleted_at      TIMESTAMPTZ,
    deleted_by      BIGINT,
    PRIMARY KEY (id),
    CONSTRAINT uk_webserver_application_host_uuid UNIQUE (uuid),
    CONSTRAINT uk_webserver_application_host_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT fk_webserver_application_host_application FOREIGN KEY (tenant_id, app_id)
        REFERENCES webserver_application (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT chk_webserver_application_host_hostname_type
        CHECK (hostname_type IN ('EXACT', 'WILDCARD')),
    CONSTRAINT chk_webserver_application_host_environment CHECK (environment IN (
        'development', 'test', 'staging', 'demo', 'production'
    )),
    CONSTRAINT chk_webserver_application_host_status CHECK (status BETWEEN 0 AND 3)
);

COMMENT ON TABLE webserver_application_host IS
    'Per-application served hostname inventory: which server block of which application answers for which hostname in which lifecycle environment';
COMMENT ON COLUMN webserver_application_host.app_id IS 'Owning application (webserver_application)';
COMMENT ON COLUMN webserver_application_host.hostname IS 'Normalized lowercase ASCII hostname; wildcard prefix preserved (*.example.com)';
COMMENT ON COLUMN webserver_application_host.hostname_type IS 'Hostname type: EXACT or WILDCARD';
COMMENT ON COLUMN webserver_application_host.environment IS 'Lifecycle environment the server block is materialized for: development, test, staging, demo, production';
COMMENT ON COLUMN webserver_application_host.vhost_id IS 'Identifier of the server block (virtual host) inside the application configuration that declares this hostname';
COMMENT ON COLUMN webserver_application_host.path_prefix IS 'Path prefix the binding answers for; always ''/'' for reconciled rows';
COMMENT ON COLUMN webserver_application_host.status IS 'Status: 0=draft, 1=active, 2=paused, 3=archived';
COMMENT ON COLUMN webserver_application_host.metadata IS
    'Provenance; reconciled rows carry metadata.source = ''served-config'' and metadata.importId';

-- One live host row per (application, environment, hostname); soft-deleted
-- rows release the name so a hostname can return after a config round-trip.
CREATE UNIQUE INDEX IF NOT EXISTS uk_webserver_application_host_active_hostname
    ON webserver_application_host (tenant_id, app_id, environment, hostname)
    WHERE deleted_at IS NULL;

-- Reverse lookup: which applications answer for a hostname.
CREATE INDEX IF NOT EXISTS idx_webserver_application_host_hostname
    ON webserver_application_host (tenant_id, hostname)
    WHERE deleted_at IS NULL;

-- Per-application listing filters by tenant then application and environment,
-- ordered for stable pagination.
CREATE INDEX IF NOT EXISTS idx_webserver_application_host_app
    ON webserver_application_host (tenant_id, app_id, environment, id)
    WHERE deleted_at IS NULL;
