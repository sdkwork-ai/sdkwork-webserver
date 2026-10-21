-- Per-owner plugin catalog.
--
-- The PC console previously kept this catalog in localStorage keyed by the IAM
-- subject, so an administrator could never review another user's plugins. The
-- table makes the catalog server-owned while keeping the same ownership key:
-- `(tenant_id, user_id)` scopes every row, and the partial unique index keeps
-- one active record per plugin key per owner.
CREATE TABLE IF NOT EXISTS webserver_plugin (
    id BIGINT NOT NULL,
    uuid VARCHAR(64) NOT NULL,
    tenant_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL DEFAULT 0,
    data_scope INTEGER NOT NULL DEFAULT 1,
    user_id BIGINT NOT NULL,
    plugin_key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    summary TEXT NOT NULL DEFAULT '',
    plugin_version TEXT NOT NULL,
    category_id TEXT NOT NULL,
    supported_host_tools_json JSONB NOT NULL DEFAULT '[]'::jsonb,
    contributed_capabilities_json JSONB NOT NULL DEFAULT '[]'::jsonb,
    source_kind TEXT NOT NULL,
    git_repository TEXT,
    git_ref TEXT,
    artifact_ref TEXT,
    checksum_sha256 TEXT,
    archive_file_name TEXT,
    plugin_status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL DEFAULT 0,
    deleted_at TIMESTAMPTZ,
    deleted_by BIGINT,
    CONSTRAINT webserver_plugin_pkey PRIMARY KEY (id),
    CONSTRAINT uk_webserver_plugin_uuid UNIQUE (uuid),
    CONSTRAINT uk_webserver_plugin_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT chk_webserver_plugin_key
        CHECK (plugin_key ~ '^plugin\.[a-z][a-z0-9-]*(\.[a-z0-9-]+)+$'),
    CONSTRAINT chk_webserver_plugin_source CHECK (source_kind IN ('git', 'archive')),
    CONSTRAINT chk_webserver_plugin_status CHECK (plugin_status IN ('draft', 'active')),
    CONSTRAINT chk_webserver_plugin_checksum
        CHECK (checksum_sha256 IS NULL OR checksum_sha256 ~ '^[0-9a-f]{64}$')
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_webserver_plugin_active_key
    ON webserver_plugin (tenant_id, user_id, plugin_key)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_webserver_plugin_owner_updated
    ON webserver_plugin (tenant_id, user_id, updated_at DESC, id DESC)
    WHERE deleted_at IS NULL;
