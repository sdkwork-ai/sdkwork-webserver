-- sdkwork:migration
-- id: 0024_webserver_domain_dns_record
-- engine: postgres
-- module: web
-- description: Stores the last synced snapshot of one Zone's DNS resolution
--   records. Until now "how does this subdomain resolve" was answerable only
--   by opening the provider console: the edge held no inventory of the A /
--   AAAA / CNAME records a Zone's hostnames resolve to, and the subdomain page
--   could not show a resolution type or a resolution IP. The snapshot is read
--   from the provider through the Zone's cloud account (0023) and replaced
--   whole per sync run, so the table always answers with one provider answer
--   rather than a merge of several. `domain_id` is the subdomain a record
--   resolves, matched at sync time; NULL is a record whose owner matches no
--   registered hostname, which the Zone-scoped read still shows.
-- reversible: true
-- rollback: down-migration drops the snapshot table and its indexes
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

-- The table itself. Column shapes follow the module baseline: snowflake `id`
-- with a `uuid` mirror, tenant-scoped, soft-deletable, `version`-stamped.
-- `record_value` bounds the longest content the inventory reads can carry
-- (a TXT SPF chain, a CAA pair); the DDL limit matches the adapters' own
-- validation ceiling.
CREATE TABLE IF NOT EXISTS webserver_domain_dns_record (
    id              BIGINT        NOT NULL,
    uuid            VARCHAR(64)   NOT NULL,
    tenant_id       BIGINT        NOT NULL DEFAULT 0,
    organization_id BIGINT        NOT NULL DEFAULT 0,
    root_domain_id  BIGINT        NOT NULL,
    domain_id       BIGINT,
    record_name     VARCHAR(253)  NOT NULL,
    record_type     VARCHAR(16)   NOT NULL,
    record_value    VARCHAR(1024) NOT NULL,
    ttl_seconds     INTEGER,
    priority        INTEGER,
    record_line     VARCHAR(64),
    dns_provider    VARCHAR(32)   NOT NULL,
    cloud_account_id VARCHAR(128) NOT NULL,
    provider_record_ref VARCHAR(128),
    synced_at       TIMESTAMPTZ   NOT NULL,
    created_at      TIMESTAMPTZ   NOT NULL,
    updated_at      TIMESTAMPTZ   NOT NULL,
    version         BIGINT        NOT NULL DEFAULT 0,
    deleted_at      TIMESTAMPTZ,
    PRIMARY KEY (id),
    CONSTRAINT uk_webserver_domain_dns_record_uuid UNIQUE (uuid),
    CONSTRAINT uk_webserver_domain_dns_record_tenant_id UNIQUE (tenant_id, id),
    CONSTRAINT fk_webserver_domain_dns_record_root_domain FOREIGN KEY (tenant_id, root_domain_id)
        REFERENCES webserver_root_domain(tenant_id, id),
    CONSTRAINT fk_webserver_domain_dns_record_domain FOREIGN KEY (tenant_id, domain_id)
        REFERENCES webserver_domain(tenant_id, id),
    CONSTRAINT chk_webserver_domain_dns_record_owner CHECK (record_name <> ''),
    CONSTRAINT chk_webserver_domain_dns_record_cloud_account CHECK (
        cloud_account_id ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{1,127}$'
    )
);

COMMENT ON TABLE webserver_domain_dns_record IS 'Synced snapshot of one root-domain Zone DNS resolution records';
COMMENT ON COLUMN webserver_domain_dns_record.domain_id IS 'Subdomain the record resolves; NULL when its owner matches no registered hostname';
COMMENT ON COLUMN webserver_domain_dns_record.synced_at IS 'Instant this snapshot row was read from the provider';

-- The Zone-scoped paged read (the resolution page's primary query), newest
-- snapshot first. `tenant_id` leads because every read is tenant-scoped.
CREATE INDEX IF NOT EXISTS idx_webserver_domain_dns_record_zone
    ON webserver_domain_dns_record (tenant_id, root_domain_id, synced_at DESC, id DESC)
    WHERE deleted_at IS NULL;

-- The subdomain-scoped read (the detail page's query).
CREATE INDEX IF NOT EXISTS idx_webserver_domain_dns_record_domain
    ON webserver_domain_dns_record (tenant_id, domain_id)
    WHERE deleted_at IS NULL;
