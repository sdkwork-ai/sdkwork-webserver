-- sdkwork:migration
-- id: 0027_webserver_tenant_integrity_and_active_indexes
-- engine: postgres
-- module: webserver
-- description: Closes audit findings D3/D5/D6/D7/D14: tenant-scoped business
--   tables stop silently defaulting to tenant 0, site/root-domain children get
--   tenant-bound composite foreign keys, application slugs and active nginx
--   configs become unique through partial indexes, and the cluster admin
--   listings get the indexes their tenant-leading predicates can seek.
-- reversible: true
-- rollback: down-migration restores defaults, single-column FKs and drops the new indexes
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

-- D6: a missing tenant bind must fail loudly instead of writing a tenant-0 row.
-- Platform-scoped tables (audit log, cluster plane) deliberately keep the
-- platform-shared default.
ALTER TABLE webserver_site ALTER COLUMN tenant_id DROP DEFAULT;
ALTER TABLE webserver_deployment ALTER COLUMN tenant_id DROP DEFAULT;
ALTER TABLE webserver_domain ALTER COLUMN tenant_id DROP DEFAULT;
ALTER TABLE webserver_domain_dns_record ALTER COLUMN tenant_id DROP DEFAULT;
ALTER TABLE webserver_domain_wechat_verification ALTER COLUMN tenant_id DROP DEFAULT;
ALTER TABLE webserver_env_variable ALTER COLUMN tenant_id DROP DEFAULT;
ALTER TABLE webserver_health_check ALTER COLUMN tenant_id DROP DEFAULT;
ALTER TABLE webserver_nginx_config ALTER COLUMN tenant_id DROP DEFAULT;
ALTER TABLE webserver_root_domain ALTER COLUMN tenant_id DROP DEFAULT;
ALTER TABLE webserver_server ALTER COLUMN tenant_id DROP DEFAULT;

-- D7: soft delete releases the application slug, matching webserver_site.
ALTER TABLE webserver_application DROP CONSTRAINT IF EXISTS uk_webserver_application_slug;
DROP INDEX IF EXISTS uk_webserver_application_slug;
CREATE UNIQUE INDEX IF NOT EXISTS uk_webserver_application_slug
    ON webserver_application (tenant_id, slug)
    WHERE deleted_at IS NULL;

-- D3 (schema half): one active nginx configuration per site, enforced by the
-- database rather than only by the activation transaction's row lock.
CREATE UNIQUE INDEX IF NOT EXISTS uk_webserver_nginx_config_site_active
    ON webserver_nginx_config (site_id)
    WHERE is_active = TRUE AND status = 1;

-- D5: tenant-bound composite foreign keys. The parent unique keys
-- (tenant_id, id) already exist for site and root domain; source versions gain
-- theirs here so the deployment reference can be tenant-bound too.
CREATE UNIQUE INDEX IF NOT EXISTS uk_webserver_source_version_tenant_id
    ON webserver_source_version (tenant_id, id);

ALTER TABLE webserver_nginx_config DROP CONSTRAINT IF EXISTS fk_webserver_nginx_config_site;
ALTER TABLE webserver_nginx_config
    ADD CONSTRAINT fk_webserver_nginx_config_site
    FOREIGN KEY (tenant_id, site_id) REFERENCES webserver_site (tenant_id, id);

ALTER TABLE webserver_source_version DROP CONSTRAINT IF EXISTS fk_webserver_source_version_site;
ALTER TABLE webserver_source_version
    ADD CONSTRAINT fk_webserver_source_version_site
    FOREIGN KEY (tenant_id, site_id) REFERENCES webserver_site (tenant_id, id);

ALTER TABLE webserver_deployment DROP CONSTRAINT IF EXISTS fk_webserver_deployment_site;
ALTER TABLE webserver_deployment
    ADD CONSTRAINT fk_webserver_deployment_site
    FOREIGN KEY (tenant_id, site_id) REFERENCES webserver_site (tenant_id, id);

ALTER TABLE webserver_deployment DROP CONSTRAINT IF EXISTS fk_webserver_deployment_source_version;
ALTER TABLE webserver_deployment
    ADD CONSTRAINT fk_webserver_deployment_source_version
    FOREIGN KEY (tenant_id, source_version_id)
    REFERENCES webserver_source_version (tenant_id, id);

ALTER TABLE webserver_env_variable DROP CONSTRAINT IF EXISTS fk_webserver_env_variable_site;
ALTER TABLE webserver_env_variable
    ADD CONSTRAINT fk_webserver_env_variable_site
    FOREIGN KEY (tenant_id, site_id) REFERENCES webserver_site (tenant_id, id);

ALTER TABLE webserver_health_check DROP CONSTRAINT IF EXISTS fk_webserver_health_check_site;
ALTER TABLE webserver_health_check
    ADD CONSTRAINT fk_webserver_health_check_site
    FOREIGN KEY (tenant_id, site_id) REFERENCES webserver_site (tenant_id, id);

ALTER TABLE webserver_domain_wechat_verification
    DROP CONSTRAINT IF EXISTS fk_webserver_domain_wechat_verification_root;
ALTER TABLE webserver_domain_wechat_verification
    ADD CONSTRAINT fk_webserver_domain_wechat_verification_root
    FOREIGN KEY (tenant_id, root_domain_id)
    REFERENCES webserver_root_domain (tenant_id, id);

-- D14: tenant-leading indexes for the unfiltered cluster admin lists.
CREATE INDEX IF NOT EXISTS idx_webserver_cluster_host_tenant_updated
    ON webserver_cluster_host (tenant_id, updated_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_webserver_cluster_instance_tenant_updated
    ON webserver_cluster_instance (tenant_id, updated_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_webserver_cluster_event_tenant_occurred
    ON webserver_cluster_event (tenant_id, occurred_at DESC, id DESC);
