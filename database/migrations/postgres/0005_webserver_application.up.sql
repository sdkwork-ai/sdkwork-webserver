-- sdkwork:migration
-- version: 0005
-- engine: postgres
-- module: web
-- description: Application resource model: webserver_application is the tenant-facing
--   application entity (name/slug identity); webserver_site remains the internal
--   site carrier. Existing webserver_site rows get a matching application row linked
--   through webserver_application.site_id.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

CREATE TABLE IF NOT EXISTS webserver_application (
    id                   BIGINT       NOT NULL,
    uuid                 VARCHAR(64)  NOT NULL,
    tenant_id            BIGINT       NOT NULL,
    organization_id      BIGINT       NOT NULL DEFAULT 0,
    data_scope           INTEGER      NOT NULL DEFAULT 1,
    user_id              BIGINT,
    name                 VARCHAR(100) NOT NULL,
    slug                 VARCHAR(100) NOT NULL,
    description          VARCHAR(500),
    application_kind     VARCHAR(16)  NOT NULL DEFAULT 'WEB',
    status               INTEGER      NOT NULL DEFAULT 0,
    site_id              BIGINT,
    default_environment  VARCHAR(64)  NOT NULL DEFAULT 'production',
    created_at           TIMESTAMPTZ  NOT NULL,
    updated_at           TIMESTAMPTZ  NOT NULL,
    version              BIGINT       NOT NULL DEFAULT 0,
    deleted_at           TIMESTAMPTZ,
    deleted_by           BIGINT,
    PRIMARY KEY (id),
    CONSTRAINT uk_webserver_application_uuid UNIQUE (uuid),
    CONSTRAINT uk_webserver_application_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT uk_webserver_application_slug UNIQUE (tenant_id, slug),
    CONSTRAINT fk_webserver_application_site FOREIGN KEY (tenant_id, site_id)
        REFERENCES webserver_site(tenant_id, id) ON DELETE SET NULL,
    CONSTRAINT chk_webserver_application_kind CHECK (application_kind IN ('WEB', 'API')),
    CONSTRAINT chk_webserver_application_status CHECK (status BETWEEN 0 AND 3)
);

CREATE INDEX IF NOT EXISTS idx_webserver_application_tenant_status_updated
    ON webserver_application (tenant_id, status, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_webserver_application_user_updated
    ON webserver_application (user_id, updated_at DESC)
    WHERE user_id IS NOT NULL;

DO $$
BEGIN
    INSERT INTO webserver_application (
        id, uuid, tenant_id, organization_id, data_scope, user_id, name, slug, description,
        application_kind, status, site_id, default_environment, created_at, updated_at, version
    )
    SELECT s.id, gen_random_uuid()::text, s.tenant_id, s.organization_id, s.data_scope, s.user_id,
           s.name, s.slug, s.description, s.application_type, s.status, s.id,
           'production', s.created_at, s.updated_at, 0
    FROM webserver_site s
    WHERE s.deleted_at IS NULL
      AND NOT EXISTS (
          SELECT 1 FROM webserver_application a WHERE a.site_id = s.id
      );
END $$;
