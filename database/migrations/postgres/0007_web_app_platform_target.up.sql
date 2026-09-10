-- sdkwork:migration
-- version: 0007
-- engine: postgres
-- module: web
-- description: Application platform distribution targets (web_app_platform_target):
--   per-application OS/platform delivery targets with tech stack, architectures,
--   bundle identifiers, and allowed release channels. Active target keys are
--   unique per tenant and application; soft-deleted targets release their key.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

CREATE TABLE IF NOT EXISTS web_app_platform_target (
    id                    BIGINT       NOT NULL,
    uuid                  VARCHAR(64)  NOT NULL,
    tenant_id             BIGINT       NOT NULL,
    organization_id       BIGINT       NOT NULL DEFAULT 0,
    data_scope            INTEGER      NOT NULL DEFAULT 1,
    user_id               BIGINT,
    app_id                BIGINT       NOT NULL,
    target_key            VARCHAR(100) NOT NULL,
    platform              VARCHAR(32)  NOT NULL,
    tech_stack            VARCHAR(32)  NOT NULL DEFAULT 'OTHER',
    architectures_json    JSONB        NOT NULL DEFAULT '[]',
    bundle_id             VARCHAR(255),
    package_name          VARCHAR(255),
    app_id_value          VARCHAR(255),
    bundle_name           VARCHAR(255),
    allowed_channels_json JSONB        NOT NULL DEFAULT '["stable"]',
    target_status         VARCHAR(16)  NOT NULL DEFAULT 'ACTIVE',
    created_at            TIMESTAMPTZ  NOT NULL,
    updated_at            TIMESTAMPTZ  NOT NULL,
    version               BIGINT       NOT NULL DEFAULT 0,
    deleted_at            TIMESTAMPTZ,
    deleted_by            BIGINT,
    PRIMARY KEY (id),
    CONSTRAINT uk_web_app_platform_target_uuid UNIQUE (uuid),
    CONSTRAINT uk_web_app_platform_target_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT fk_web_app_platform_target_application FOREIGN KEY (tenant_id, app_id)
        REFERENCES web_application (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT chk_web_app_platform_target_status CHECK (
        target_status IN ('ACTIVE', 'INACTIVE', 'RETIRED')
    )
);

COMMENT ON TABLE web_app_platform_target IS
    'Application platform distribution target (OS/platform/bundle identity per application)';
COMMENT ON COLUMN web_app_platform_target.target_status IS 'Status: ACTIVE, INACTIVE, RETIRED';

-- Platform-target listing filters by tenant then application and sorts by id ASC.
CREATE INDEX IF NOT EXISTS idx_web_app_platform_target_app_id
    ON web_app_platform_target (tenant_id, app_id, id)
    WHERE deleted_at IS NULL;

-- Active target keys are unique per tenant and application; soft-deleted
-- targets release their key for re-registration.
CREATE UNIQUE INDEX IF NOT EXISTS uk_web_app_platform_target_active_key
    ON web_app_platform_target (tenant_id, app_id, target_key)
    WHERE deleted_at IS NULL;
