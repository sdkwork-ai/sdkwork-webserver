-- sdkwork:migration
-- id: 0027_webserver_tenant_integrity_and_active_indexes
-- engine: postgres
-- module: webserver
-- description: Reverses the tenant-integrity and active-index migration. The
--   defaults, single-column foreign keys and non-partial slug index are
--   restored; rows written without a tenant are not repaired (there is no
--   correct value to invent).
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

ALTER TABLE webserver_site ALTER COLUMN tenant_id SET DEFAULT 0;
ALTER TABLE webserver_deployment ALTER COLUMN tenant_id SET DEFAULT 0;
ALTER TABLE webserver_domain ALTER COLUMN tenant_id SET DEFAULT 0;
ALTER TABLE webserver_domain_dns_record ALTER COLUMN tenant_id SET DEFAULT 0;
ALTER TABLE webserver_domain_wechat_verification ALTER COLUMN tenant_id SET DEFAULT 0;
ALTER TABLE webserver_env_variable ALTER COLUMN tenant_id SET DEFAULT 0;
ALTER TABLE webserver_health_check ALTER COLUMN tenant_id SET DEFAULT 0;
ALTER TABLE webserver_nginx_config ALTER COLUMN tenant_id SET DEFAULT 0;
ALTER TABLE webserver_root_domain ALTER COLUMN tenant_id SET DEFAULT 0;
ALTER TABLE webserver_server ALTER COLUMN tenant_id SET DEFAULT 0;

DROP INDEX IF EXISTS uk_webserver_application_slug;
CREATE UNIQUE INDEX IF NOT EXISTS uk_webserver_application_slug
    ON webserver_application (tenant_id, slug);

DROP INDEX IF EXISTS uk_webserver_nginx_config_site_active;
CREATE INDEX IF NOT EXISTS idx_webserver_nginx_config_site_active
    ON webserver_nginx_config (site_id, is_active);
DROP INDEX IF EXISTS idx_webserver_cluster_event_tenant_occurred;
DROP INDEX IF EXISTS idx_webserver_cluster_instance_tenant_updated;
DROP INDEX IF EXISTS idx_webserver_cluster_host_tenant_updated;

ALTER TABLE webserver_domain_wechat_verification
    DROP CONSTRAINT IF EXISTS fk_webserver_domain_wechat_verification_root;
ALTER TABLE webserver_domain_wechat_verification
    ADD CONSTRAINT fk_webserver_domain_wechat_verification_root
    FOREIGN KEY (root_domain_id) REFERENCES webserver_root_domain (id);

ALTER TABLE webserver_health_check DROP CONSTRAINT IF EXISTS fk_webserver_health_check_site;
ALTER TABLE webserver_health_check
    ADD CONSTRAINT fk_webserver_health_check_site
    FOREIGN KEY (site_id) REFERENCES webserver_site (id);

ALTER TABLE webserver_env_variable DROP CONSTRAINT IF EXISTS fk_webserver_env_variable_site;
ALTER TABLE webserver_env_variable
    ADD CONSTRAINT fk_webserver_env_variable_site
    FOREIGN KEY (site_id) REFERENCES webserver_site (id);

ALTER TABLE webserver_deployment DROP CONSTRAINT IF EXISTS fk_webserver_deployment_source_version;
ALTER TABLE webserver_deployment
    ADD CONSTRAINT fk_webserver_deployment_source_version
    FOREIGN KEY (source_version_id) REFERENCES webserver_source_version (id);

ALTER TABLE webserver_deployment DROP CONSTRAINT IF EXISTS fk_webserver_deployment_site;
ALTER TABLE webserver_deployment
    ADD CONSTRAINT fk_webserver_deployment_site
    FOREIGN KEY (site_id) REFERENCES webserver_site (id);

ALTER TABLE webserver_source_version DROP CONSTRAINT IF EXISTS fk_webserver_source_version_site;
ALTER TABLE webserver_source_version
    ADD CONSTRAINT fk_webserver_source_version_site
    FOREIGN KEY (site_id) REFERENCES webserver_site (id);

ALTER TABLE webserver_nginx_config DROP CONSTRAINT IF EXISTS fk_webserver_nginx_config_site;
ALTER TABLE webserver_nginx_config
    ADD CONSTRAINT fk_webserver_nginx_config_site
    FOREIGN KEY (site_id) REFERENCES webserver_site (id);

DROP INDEX IF EXISTS uk_webserver_source_version_tenant_id;
