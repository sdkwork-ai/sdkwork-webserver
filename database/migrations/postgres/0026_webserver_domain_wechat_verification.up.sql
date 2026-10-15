-- sdkwork:migration
-- id: 0026_webserver_domain_wechat_verification
-- engine: postgres
-- module: web
-- description: Stores the WeChat MP domain-ownership verification file for one
--   root domain (公众号/小程序 JS 接口安全域名、网页授权域名、业务域名验证).
--   The platform issues an MP_verify_xxx.txt file the operator must make
--   reachable at the domain root; the operator uploads the file verbatim and
--   the edge data plane serves it for the root domain's hostname and every
--   registered subdomain under it. One row per root domain: a fresh platform
--   file replaces the previous one whole. Content is kept byte-exact — WeChat
--   compares bytes, and any normalization would turn a passing verification
--   into "content mismatch".
-- reversible: true
-- rollback: down-migration drops the table and its indexes
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

-- Column shapes follow the module baseline: snowflake `id` with a `uuid`
-- mirror, tenant-scoped, `version`-stamped. The unique constraint on
-- `root_domain_id` is what makes "one file per root domain, replaced whole"
-- true at the schema level even if a caller races itself.
CREATE TABLE IF NOT EXISTS webserver_domain_wechat_verification (
    id              BIGINT        NOT NULL,
    uuid            VARCHAR(64)   NOT NULL,
    tenant_id       BIGINT        NOT NULL DEFAULT 0,
    organization_id BIGINT        NOT NULL DEFAULT 0,
    root_domain_id  BIGINT        NOT NULL,
    file_name       VARCHAR(64)   NOT NULL,
    content         VARCHAR(2048) NOT NULL,
    user_id         BIGINT,
    created_by      BIGINT,
    updated_by      BIGINT,
    created_at      TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    version         BIGINT        NOT NULL DEFAULT 1,
    deleted_at      TIMESTAMPTZ,
    PRIMARY KEY (id),
    CONSTRAINT uk_webserver_domain_wechat_verification_uuid UNIQUE (uuid),
    CONSTRAINT uk_webserver_domain_wechat_verification_root UNIQUE (root_domain_id),
    CONSTRAINT fk_webserver_domain_wechat_verification_root
        FOREIGN KEY (root_domain_id) REFERENCES webserver_root_domain(id),
    CONSTRAINT chk_webserver_domain_wechat_verification_name CHECK (file_name LIKE '%.txt'),
    CONSTRAINT chk_webserver_domain_wechat_verification_content CHECK (content <> '')
);

COMMENT ON TABLE webserver_domain_wechat_verification IS
    '微信公众号域名归属验证文件；一个根域名至多一份，边缘按请求主机名原样提供';
COMMENT ON COLUMN webserver_domain_wechat_verification.file_name IS
    '平台发放的文件名，形如 MP_verify_xxx.txt，按精确名称服务，不做大小写归一';
COMMENT ON COLUMN webserver_domain_wechat_verification.content IS
    '校验文件原文：逐字节存储、逐字节返回';
COMMENT ON COLUMN webserver_domain_wechat_verification.user_id IS
    '文件归属用户；NULL 表示 tenant 级，与 webserver_domain.user_id 同义';

-- The edge lookup resolves a request Host to the root domain's file; the
-- hostname path is covered by the baseline hostname unique indexes, this
-- index keeps the tenant-scoped management read direct.
CREATE INDEX IF NOT EXISTS idx_webserver_domain_wechat_verification_tenant
    ON webserver_domain_wechat_verification (tenant_id, root_domain_id)
    WHERE deleted_at IS NULL;
