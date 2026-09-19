-- sdkwork:migration
-- version: 0009
-- engine: postgres
-- module: web
-- description: Rollback of 0009_retire_web_health_result: recreates the legacy
--   web_health_result table, its unique/foreign-key constraints and its two
--   listing indexes, restoring the pre-Phase-3 shape exactly.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

CREATE TABLE IF NOT EXISTS web_health_result (
    id              BIGINT       NOT NULL,
    uuid            VARCHAR(64)  NOT NULL,
    tenant_id       BIGINT       NOT NULL DEFAULT 0,
    health_check_id BIGINT       NOT NULL,
    site_id         BIGINT       NOT NULL,
    is_healthy      BOOLEAN      NOT NULL,
    response_ms     INTEGER,
    status_code     INTEGER,
    error_message   VARCHAR(1000),
    checked_at      TIMESTAMPTZ  NOT NULL,
    created_at      TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT uk_web_health_result_uuid UNIQUE (uuid),
    CONSTRAINT fk_web_health_result_check FOREIGN KEY (health_check_id)
        REFERENCES webserver_health_check(id),
    CONSTRAINT fk_web_health_result_site FOREIGN KEY (site_id) REFERENCES webserver_site(id)
);

COMMENT ON TABLE web_health_result IS 'Web health check result';
COMMENT ON COLUMN web_health_result.is_healthy IS 'Whether the check was healthy';
COMMENT ON COLUMN web_health_result.response_ms IS 'Response time in milliseconds';
COMMENT ON COLUMN web_health_result.status_code IS 'HTTP status code';
COMMENT ON COLUMN web_health_result.checked_at IS 'Check execution timestamp';

CREATE INDEX IF NOT EXISTS idx_web_health_result_check_time
    ON web_health_result (health_check_id, checked_at DESC);

CREATE INDEX IF NOT EXISTS idx_web_health_result_site_time
    ON web_health_result (site_id, checked_at DESC);
