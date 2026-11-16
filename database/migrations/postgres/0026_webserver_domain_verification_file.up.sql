-- sdkwork:migration
-- id: 0026_webserver_domain_verification_file
-- engine: postgres
-- module: web
-- description: Stores the domain-ownership verification files a root domain
--   publishes (WeChat MP / mini program JS 接口安全域名与业务域名、支付宝/抖音
--   小程序、Google Search Console、Apple Associated Domains、Bing 站点验证)。
--   Each platform issues a file the operator must make reachable at the domain
--   root; the operator uploads it verbatim and the edge data plane serves it for
--   the root domain's hostname and every registered subdomain under it. A root
--   domain may carry several files at once, so uniqueness is (root_domain_id,
--   file_name) rather than root_domain_id. Content is kept byte-exact — platforms
--   compare bytes, and any normalization would turn a passing verification into
--   "content mismatch".
-- reversible: true
-- rollback: down-migration drops the table and its indexes
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

-- Column shapes follow the module baseline: snowflake `id` with a `uuid`
-- mirror, tenant-scoped, `version`-stamped.
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

COMMENT ON TABLE webserver_domain_verification_file IS
    '域名归属校验文件；一个根域名可登记多份（每个平台一份），边缘按请求主机名原样提供';
COMMENT ON COLUMN webserver_domain_verification_file.platform IS
    '签发平台：WECHAT_OFFICIAL_ACCOUNT / WECHAT_MINI_PROGRAM / ALIPAY / BYTEDANCE / APPLE / GOOGLE / CUSTOM';
COMMENT ON COLUMN webserver_domain_verification_file.file_name IS
    '平台发放的文件名，形如 MP_verify_xxx.txt / googleXXX.html，按精确名称服务，不做大小写归一';
COMMENT ON COLUMN webserver_domain_verification_file.content IS
    '校验文件原文：逐字节存储、逐字节返回';
COMMENT ON COLUMN webserver_domain_verification_file.user_id IS
    '文件归属用户；NULL 表示 tenant 级，与 webserver_domain.user_id 同义';

-- The edge lookup resolves a request Host to the root domain's file set; the
-- hostname path is covered by the baseline hostname unique indexes, this index
-- keeps the tenant-scoped management read direct.
CREATE INDEX IF NOT EXISTS idx_webserver_domain_verification_file_tenant
    ON webserver_domain_verification_file (tenant_id, root_domain_id)
    WHERE deleted_at IS NULL;
