-- Nginx configuration revision history.
--
-- `webserver_nginx_config` keeps only the current body, so an operator could
-- bump `version_no` without any way to read or restore an earlier body. This
-- table snapshots every distinct content revision. The application writes a
-- snapshot for the create (version 1) and for every content update; the unique
-- `(config_id, version_no)` key makes a retried write idempotent.
CREATE TABLE IF NOT EXISTS webserver_nginx_config_revision (
    id BIGINT NOT NULL,
    uuid VARCHAR(64) NOT NULL,
    tenant_id BIGINT NOT NULL,
    config_id BIGINT NOT NULL,
    version_no INTEGER NOT NULL,
    config_name VARCHAR(200) NOT NULL,
    config_content TEXT NOT NULL,
    config_hash VARCHAR(64) NOT NULL,
    created_by VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT webserver_nginx_config_revision_pkey PRIMARY KEY (id),
    CONSTRAINT uk_webserver_nginx_config_revision_uuid UNIQUE (uuid),
    CONSTRAINT uk_webserver_nginx_config_revision_version
        UNIQUE (config_id, version_no),
    CONSTRAINT fk_webserver_nginx_config_revision_config
        FOREIGN KEY (config_id) REFERENCES webserver_nginx_config (id)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_webserver_nginx_config_revision_lookup
    ON webserver_nginx_config_revision (tenant_id, config_id, version_no DESC);
