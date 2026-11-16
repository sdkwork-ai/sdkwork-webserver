-- Migration: 0030_webserver_consolidation_reconciliation
-- Description: reconcile databases provisioned before the initialization-state
--   consolidation: the folded baseline carries tables some existing databases
--   never received (their provisioning binaries predated those migrations), and
--   the consolidated binary's known-migration set no longer lists the folded
--   history, so the regular migrate step cannot bridge the delta. Every
--   statement is idempotent (IF NOT EXISTS) and mirrors the baseline DDL
--   verbatim; fresh installs already created these objects and no-op here.
-- Author: SDKWork Web Server

CREATE TABLE IF NOT EXISTS webserver_domain_verification_file (
    id              BIGINT        NOT NULL,
    uuid            VARCHAR(64)   NOT NULL,
    tenant_id       BIGINT        NOT NULL DEFAULT 0,
    organization_id BIGINT        NOT NULL DEFAULT 0,
    root_domain_id  BIGINT        NOT NULL,
    platform        VARCHAR(32)   NOT NULL,
    file_name       VARCHAR(128)  NOT NULL,
    content         VARCHAR(4096) NOT NULL,
    user_id         BIGINT,
    created_by      BIGINT,
    updated_by      BIGINT,
    created_at      TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    version         BIGINT        NOT NULL DEFAULT 1,
    deleted_at      TIMESTAMPTZ,
    PRIMARY KEY (id),
    CONSTRAINT uk_webserver_domain_verification_file_uuid UNIQUE (uuid),
    CONSTRAINT uk_webserver_domain_verification_file_name UNIQUE (root_domain_id, file_name),
    CONSTRAINT fk_webserver_domain_verification_file_root
        FOREIGN KEY (root_domain_id) REFERENCES webserver_root_domain(id),
    CONSTRAINT chk_webserver_domain_verification_file_platform CHECK (platform IN (
        'WECHAT_OFFICIAL_ACCOUNT', 'WECHAT_MINI_PROGRAM', 'ALIPAY',
        'BYTEDANCE', 'APPLE', 'GOOGLE', 'CUSTOM'
    )),
    CONSTRAINT chk_webserver_domain_verification_file_name CHECK (
        file_name ~ '^[A-Za-z0-9][A-Za-z0-9._-]*\.(txt|html|xml)$'
    ),
    CONSTRAINT chk_webserver_domain_verification_file_content CHECK (content <> '')
);

COMMENT ON TABLE webserver_domain_verification_file IS 'Domain ownership verification file; a root domain may register several (one per platform), served verbatim per request hostname';

CREATE INDEX IF NOT EXISTS idx_webserver_domain_verification_file_tenant
    ON webserver_domain_verification_file (tenant_id, root_domain_id)
    WHERE deleted_at IS NULL;
