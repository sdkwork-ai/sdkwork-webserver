use super::{EngineArguments, EngineDatabase, EngineRow, WebRepository};
use crate::audited_sql;
use sdkwork_webserver_contract::{
    CreateNginxConfigRequest, ListNginxConfigsQuery, NginxConfigPage, NginxConfigResponse,
    NginxConfigRevisionPage, NginxConfigRevisionResponse, NginxConfigRevisionSummary,
    NginxStatusResponse, UpdateNginxConfigRequest, WebServiceError, WebServiceResult,
};
use sqlx::Row;

use super::support::{
    bool_from_row, instant_write_expression, new_uuid, next_id, now_rfc3339, pagination,
    resolve_site_internal_id, retry_transient_transaction, sha256_hex, store_error,
};

/// Fail closed: every production caller binds a tenant, and a tenant-less
/// branch would operate on the config uuid alone across all tenants, so a
/// missing tenant context is rejected at the repository boundary (defense in
/// depth, matching the listing path).
fn require_nginx_tenant(tenant_id: Option<i64>) -> WebServiceResult<i64> {
    tenant_id.ok_or_else(|| {
        WebServiceError::validation("tenant context is required for nginx config operations")
    })
}

impl WebRepository {
    pub(super) async fn list_nginx_configs_repo(
        &self,
        tenant_id: Option<i64>,
        query: &ListNginxConfigsQuery,
    ) -> WebServiceResult<NginxConfigPage> {
        // Fail closed: a tenant-less listing would enumerate every tenant's
        // configs, so the repository rejects a missing tenant context even
        // though the service layer already enforces one (defense in depth).
        let tenant_id = tenant_id.ok_or_else(|| {
            WebServiceError::validation("tenant context is required for nginx config listing")
        })?;
        let (page, page_size, offset) = pagination(query.page, query.page_size)?;
        let mut count_sql = String::from(
            "SELECT COUNT(*) AS total
             FROM webserver_nginx_config config
             INNER JOIN webserver_site site
               ON site.id = config.site_id AND site.tenant_id = config.tenant_id
             WHERE 1=1",
        );
        let mut list_sql = String::from(
            "SELECT config.uuid, config.tenant_id, site.uuid AS site_uuid,
                    config.config_name, config.config_type, config.config_content,
                    config.config_hash, config.is_active, config.version_no,
                    CAST(config.deployed_at AS TEXT) AS deployed_at, config.status,
                    CAST(config.created_at AS TEXT) AS created_at,
                    CAST(config.updated_at AS TEXT) AS updated_at
             FROM webserver_nginx_config config
             INNER JOIN webserver_site site
               ON site.id = config.site_id AND site.tenant_id = config.tenant_id
             WHERE 1=1",
        );
        let mut binds: Vec<BindValue> = Vec::new();

        {
            let index = binds.len() + 1;
            let clause = format!(" AND config.tenant_id = ${index}");
            count_sql.push_str(&clause);
            list_sql.push_str(&clause);
            binds.push(BindValue::I64(tenant_id));
        }
        if let Some(site_uuid) = query.site_id.as_deref() {
            let index = binds.len() + 1;
            let clause = format!(" AND site.uuid = ${index}");
            count_sql.push_str(&clause);
            list_sql.push_str(&clause);
            binds.push(BindValue::String(site_uuid.to_string()));
        }
        if let Some(config_type) = query.config_type {
            let index = binds.len() + 1;
            let clause = format!(" AND config.config_type = ${index}");
            count_sql.push_str(&clause);
            list_sql.push_str(&clause);
            binds.push(BindValue::I32(config_type));
        }
        if let Some(is_active) = query.is_active {
            let index = binds.len() + 1;
            let clause = format!(" AND config.is_active = ${index}");
            count_sql.push_str(&clause);
            list_sql.push_str(&clause);
            binds.push(BindValue::Bool(is_active));
        }

        let limit_index = binds.len() + 1;
        let offset_index = binds.len() + 2;
        list_sql.push_str(&format!(
            " ORDER BY config.updated_at DESC, config.id DESC LIMIT ${limit_index} OFFSET ${offset_index}"
        ));

        let count_row = apply_binds(sqlx::query(audited_sql(&count_sql)), &binds)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count webserver_nginx_config", error))?;
        let total: i64 = count_row
            .try_get("total")
            .map_err(|error| store_error("map webserver_nginx_config count", error))?;

        let mut list_query = apply_binds(sqlx::query(audited_sql(&list_sql)), &binds);
        list_query = list_query.bind(page_size).bind(offset);
        let rows = list_query
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list webserver_nginx_config", error))?;

        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            items.push(map_nginx_config_row(row, false).map_err(|error| {
                WebServiceError::Internal(format!("map webserver_nginx_config row: {error}"))
            })?);
        }

        Ok(NginxConfigPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn create_nginx_config_repo(
        &self,
        tenant_id: i64,
        request: &CreateNginxConfigRequest,
    ) -> WebServiceResult<NginxConfigResponse> {
        let site_internal_id =
            resolve_site_internal_id(&self.pool, tenant_id, &request.site_id).await?;
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();
        let config_hash = sha256_hex(&request.config_content);

        let now_expression = instant_write_expression("$9");
        let insert_sql = format!(
            "INSERT INTO webserver_nginx_config (
                id, uuid, tenant_id, site_id, config_type, config_name, config_content, config_hash,
                is_active, status, metadata, created_at, updated_at, version
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, FALSE, 0, '{{}}',
                {now_expression}, {now_expression}, 0
             )"
        );

        sqlx::query(audited_sql(&insert_sql))
            .bind(id)
            .bind(&uuid)
            .bind(tenant_id)
            .bind(site_internal_id)
            .bind(request.config_type)
            .bind(&request.config_name)
            .bind(&request.config_content)
            .bind(&config_hash)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("insert webserver_nginx_config", error))?;

        self.capture_nginx_config_revision(
            tenant_id,
            id,
            1,
            &request.config_name,
            &request.config_content,
            &config_hash,
        )
        .await?;

        self.retrieve_nginx_config_repo(Some(tenant_id), &uuid)
            .await
    }

    pub(super) async fn retrieve_nginx_config_repo(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> WebServiceResult<NginxConfigResponse> {
        let tenant_id = require_nginx_tenant(tenant_id)?;
        let row = sqlx::query(
            "SELECT config.uuid, config.tenant_id, site.uuid AS site_uuid,
                    config.config_name, config.config_type, config.config_content,
                    config.config_hash, config.is_active, config.version_no,
                    CAST(config.deployed_at AS TEXT) AS deployed_at, config.status,
                    CAST(config.created_at AS TEXT) AS created_at,
                    CAST(config.updated_at AS TEXT) AS updated_at
             FROM webserver_nginx_config config
             INNER JOIN webserver_site site
               ON site.id = config.site_id AND site.tenant_id = config.tenant_id
             WHERE config.tenant_id = $1 AND config.uuid = $2",
        )
        .bind(tenant_id)
        .bind(config_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve webserver_nginx_config", error))?
        .ok_or_else(|| WebServiceError::not_found("nginx config not found"))?;

        map_nginx_config_row(&row, true).map_err(|error| WebServiceError::Internal(error.to_string()))
    }

    pub(super) async fn update_nginx_config_repo(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
        request: &UpdateNginxConfigRequest,
    ) -> WebServiceResult<NginxConfigResponse> {
        let tenant_id = require_nginx_tenant(tenant_id)?;
        let row = sqlx::query(
            "SELECT id, config_name, config_content, version FROM webserver_nginx_config
             WHERE tenant_id = $1 AND uuid = $2",
        )
        .bind(tenant_id)
        .bind(config_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("load webserver_nginx_config for update", error))?
        .ok_or_else(|| WebServiceError::not_found("nginx config not found"))?;

        let stored_config_name: String = row
            .try_get("config_name")
            .map_err(|error| store_error("map webserver_nginx_config config_name", error))?;
        let config_internal_id: i64 = row
            .try_get("id")
            .map_err(|error| store_error("map webserver_nginx_config id", error))?;
        let stored_config_content: String = row
            .try_get("config_content")
            .map_err(|error| store_error("map webserver_nginx_config config_content", error))?;
        let stored_version: i64 = row
            .try_get("version")
            .map_err(|error| store_error("map webserver_nginx_config version", error))?;
        let config_name = request
            .config_name
            .as_ref()
            .cloned()
            .unwrap_or(stored_config_name);
        let config_content = request
            .config_content
            .as_ref()
            .cloned()
            .unwrap_or(stored_config_content);
        let config_hash = sha256_hex(&config_content);
        let now = now_rfc3339();

        // Positional parameters: $1 tenant, $2 uuid, $3 name, $4 content,
        // $5 hash, $6 now, $7 expected version (CAS).
        let tenant_time = instant_write_expression("$6");
        let tenant_update_sql = format!(
            "UPDATE webserver_nginx_config
             SET config_name = $3, config_content = $4, config_hash = $5,
                 version_no = version_no + 1,
                 updated_at = {tenant_time}, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND version = $7"
        );

        let result = sqlx::query(audited_sql(&tenant_update_sql))
            .bind(tenant_id)
            .bind(config_id)
            .bind(&config_name)
            .bind(&config_content)
            .bind(&config_hash)
            .bind(&now)
            .bind(stored_version)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("update webserver_nginx_config", error))?;

        if result.rows_affected() == 0 {
            return self
                .conflict_or_missing_nginx_config(Some(tenant_id), config_id)
                .await;
        }

        let updated = self
            .retrieve_nginx_config_repo(Some(tenant_id), config_id)
            .await?;
        self.capture_nginx_config_revision(
            tenant_id,
            config_internal_id,
            updated.version_no,
            &updated.config_name,
            &updated.config_content,
            &updated.config_hash,
        )
        .await?;
        Ok(updated)
    }

    pub(super) async fn load_nginx_config_content_repo(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> WebServiceResult<String> {
        let tenant_id = require_nginx_tenant(tenant_id)?;
        let row = sqlx::query(
            "SELECT config_content FROM webserver_nginx_config WHERE tenant_id = $1 AND uuid = $2",
        )
        .bind(tenant_id)
        .bind(config_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("load webserver_nginx_config content", error))?
        .ok_or_else(|| WebServiceError::not_found("nginx config not found"))?;

        row.try_get("config_content")
            .map_err(|error| store_error("load webserver_nginx_config content column", error))
    }

    /// Content of the currently active config for a site (`None` when no
    /// config is active yet). Used as the edge rollback target.
    pub(super) async fn load_active_nginx_config_content_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
    ) -> WebServiceResult<Option<String>> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;
        let row = sqlx::query(
            "SELECT config_content FROM webserver_nginx_config
             WHERE site_id = $1 AND is_active = TRUE AND status = 1
             ORDER BY updated_at DESC, id DESC
             LIMIT 1",
        )
        .bind(site_internal_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("load active webserver_nginx_config content", error))?;
        row.map(|row| row.try_get("config_content"))
            .transpose()
            .map_err(|error| store_error("map active webserver_nginx_config content", error))
    }

    async fn nginx_config_internal_id(
        &self,
        tenant_id: i64,
        config_id: &str,
    ) -> WebServiceResult<i64> {
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM webserver_nginx_config WHERE tenant_id = $1 AND uuid = $2",
        )
        .bind(tenant_id)
        .bind(config_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve webserver_nginx_config id", error))?;
        id.ok_or_else(|| WebServiceError::not_found("nginx config not found"))
    }

    pub(super) async fn list_nginx_config_revisions_repo(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<NginxConfigRevisionPage> {
        let tenant_id = require_nginx_tenant(tenant_id)?;
        let internal_id = self.nginx_config_internal_id(tenant_id, config_id).await?;
        let (page, page_size, offset) = pagination(page, page_size)?;
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM webserver_nginx_config_revision
             WHERE tenant_id = $1 AND config_id = $2",
        )
        .bind(tenant_id)
        .bind(internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count webserver_nginx_config_revision", error))?;
        let rows = sqlx::query(
            "SELECT version_no, config_name, config_hash, created_by,
                    CAST(created_at AS TEXT) AS created_at
             FROM webserver_nginx_config_revision
             WHERE tenant_id = $1 AND config_id = $2
             ORDER BY version_no DESC
             LIMIT $3 OFFSET $4",
        )
        .bind(tenant_id)
        .bind(internal_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list webserver_nginx_config_revision", error))?;
        let items = rows
            .into_iter()
            .map(|row| {
                Ok(NginxConfigRevisionSummary {
                    version_no: row.try_get("version_no")?,
                    config_name: row.try_get("config_name")?,
                    config_hash: row.try_get("config_hash")?,
                    created_by: row.try_get("created_by")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(|error| store_error("map webserver_nginx_config_revision", error))?;
        Ok(NginxConfigRevisionPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_nginx_config_revision_repo(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
        version_no: i32,
    ) -> WebServiceResult<NginxConfigRevisionResponse> {
        let tenant_id = require_nginx_tenant(tenant_id)?;
        let internal_id = self.nginx_config_internal_id(tenant_id, config_id).await?;
        let row = sqlx::query(
            "SELECT version_no, config_name, config_content, config_hash, created_by,
                    CAST(created_at AS TEXT) AS created_at
             FROM webserver_nginx_config_revision
             WHERE tenant_id = $1 AND config_id = $2 AND version_no = $3",
        )
        .bind(tenant_id)
        .bind(internal_id)
        .bind(version_no)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve webserver_nginx_config_revision", error))?
        .ok_or_else(|| WebServiceError::not_found("nginx config revision not found"))?;
        Ok(NginxConfigRevisionResponse {
            version_no: row
                .try_get("version_no")
                .map_err(|error| store_error("map revision version_no", error))?,
            config_name: row
                .try_get("config_name")
                .map_err(|error| store_error("map revision config_name", error))?,
            config_content: row
                .try_get("config_content")
                .map_err(|error| store_error("map revision config_content", error))?,
            config_hash: row
                .try_get("config_hash")
                .map_err(|error| store_error("map revision config_hash", error))?,
            created_by: row
                .try_get("created_by")
                .map_err(|error| store_error("map revision created_by", error))?,
            created_at: row
                .try_get("created_at")
                .map_err(|error| store_error("map revision created_at", error))?,
        })
    }

    pub(super) async fn rollback_nginx_config_revision_repo(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
        version_no: i32,
    ) -> WebServiceResult<NginxConfigResponse> {
        let revision = self
            .retrieve_nginx_config_revision_repo(tenant_id, config_id, version_no)
            .await?;
        // A rollback is a forward write: it restores the old body as a new
        // revision (and a new version number), so the history stays append-only
        // and the rollback itself is auditable.
        self.update_nginx_config_repo(
            tenant_id,
            config_id,
            &UpdateNginxConfigRequest {
                config_name: Some(revision.config_name),
                config_content: Some(revision.config_content),
            },
        )
        .await
    }

    /// Snapshots one content revision. `(config_id, version_no)` is unique, so
    /// a retried write is idempotent instead of appending a duplicate.
    async fn capture_nginx_config_revision(
        &self,
        tenant_id: i64,
        config_id: i64,
        version_no: i32,
        config_name: &str,
        config_content: &str,
        config_hash: &str,
    ) -> WebServiceResult<()> {
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();
        sqlx::query(audited_sql(
            "INSERT INTO webserver_nginx_config_revision (
                id, uuid, tenant_id, config_id, version_no, config_name,
                config_content, config_hash, created_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::timestamptz)
             ON CONFLICT (config_id, version_no) DO NOTHING",
        ))
        .bind(id)
        .bind(&uuid)
        .bind(tenant_id)
        .bind(config_id)
        .bind(version_no)
        .bind(config_name)
        .bind(config_content)
        .bind(config_hash)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("insert webserver_nginx_config_revision", error))?;
        Ok(())
    }

    pub(super) async fn webserver_nginx_config_repo(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> WebServiceResult<NginxConfigResponse> {
        retry_transient_transaction(|| self.activate_nginx_config_once(tenant_id, config_id)).await
    }

    async fn activate_nginx_config_once(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> WebServiceResult<NginxConfigResponse> {
        let tenant_id = require_nginx_tenant(tenant_id)?;
        let scope = sqlx::query(
            "SELECT tenant_id, site_id FROM webserver_nginx_config
             WHERE tenant_id = $1 AND uuid = $2",
        )
        .bind(tenant_id)
        .bind(config_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("load webserver_nginx_config activation scope", error))?
        .ok_or_else(|| WebServiceError::not_found("nginx config not found"))?;
        let site_internal_id: i64 = scope
            .try_get("site_id")
            .map_err(|error| store_error("map webserver_nginx_config activation site", error))?;
        let now = now_rfc3339();

        let deactivate_time = instant_write_expression("$3");
        let tenant_activate_time = instant_write_expression("$3");
        let deactivate_sql = format!(
            "UPDATE webserver_nginx_config SET is_active = FALSE, updated_at = {deactivate_time},
                    version = version + 1
             WHERE tenant_id = $1 AND site_id = $2 AND is_active = TRUE"
        );
        let tenant_activate_sql = format!(
            "UPDATE webserver_nginx_config
             SET is_active = TRUE, status = 1, deployed_at = {tenant_activate_time},
                 updated_at = {tenant_activate_time}, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2"
        );

        // 事务边界：停用旧 active config + 激活目标 config 必须原子完成，
        // 避免停用成功但激活失败导致站点丢失生效配置。
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin deploy webserver_nginx_config transaction", error))?;

        // Serialize activations for one site. Without this parent-row lock two
        // concurrent activations each deactivate the set visible in their own
        // READ COMMITTED snapshot and then activate their own target, leaving
        // two `is_active = TRUE` rows. Migration 0027's partial unique index is
        // the database-level backstop, but the lock is what lets both requests
        // finish instead of turning the second into a 23505 conflict.
        let site_locked = sqlx::query(
            "SELECT 1 FROM webserver_site
             WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| store_error("lock webserver_site for nginx activation", error))?;
        if site_locked.is_none() {
            tx.rollback().await.map_err(|error| {
                store_error("rollback missing webserver_site nginx activation", error)
            })?;
            return Err(WebServiceError::not_found("site not found"));
        }

        sqlx::query(audited_sql(&deactivate_sql))
            .bind(tenant_id)
            .bind(site_internal_id)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("deactivate webserver_nginx_config", error))?;

        let result = sqlx::query(audited_sql(&tenant_activate_sql))
            .bind(tenant_id)
            .bind(config_id)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("Web webserver_nginx_config", error))?;

        if result.rows_affected() == 0 {
            tx.rollback().await.map_err(|error| {
                store_error("rollback missing webserver_nginx_config activation", error)
            })?;
            return Err(WebServiceError::not_found("nginx config not found"));
        }

        tx.commit()
            .await
            .map_err(|error| store_error("commit deploy webserver_nginx_config transaction", error))?;

        self.retrieve_nginx_config_repo(Some(tenant_id), config_id).await
    }

    pub(super) async fn retrieve_nginx_status_repo(
        &self,
        tenant_id: Option<i64>,
    ) -> WebServiceResult<NginxStatusResponse> {
        let tenant_id = require_nginx_tenant(tenant_id)?;
        let active_configs = {
            let row = sqlx::query(
                "SELECT COUNT(*) AS total FROM webserver_nginx_config
                 WHERE tenant_id = $1 AND is_active = TRUE AND status = 1",
            )
            .bind(tenant_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count active webserver_nginx_config", error))?;
            row.try_get::<i64, _>("total")
                .map_err(|error| store_error("map active webserver_nginx_config count", error))?
        };

        Ok(NginxStatusResponse {
            running: active_configs > 0,
            active_configs,
        })
    }

    pub(super) async fn resolve_site_primary_hostname_repo(
        &self,
        tenant_id: i64,
        site_uuid: &str,
    ) -> WebServiceResult<String> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_uuid).await?;
        let row = sqlx::query(
            "SELECT domain.hostname
             FROM webserver_site_binding binding
             INNER JOIN webserver_domain domain
               ON domain.tenant_id = binding.tenant_id
              AND domain.id = binding.domain_id
              AND domain.deleted_at IS NULL
             WHERE binding.tenant_id = $1
               AND binding.site_id = $2
               AND binding.deleted_at IS NULL
             ORDER BY (binding.status = 'ACTIVE') DESC,
                      binding.is_primary DESC,
                      binding.created_at ASC, binding.id ASC
             LIMIT 1",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve primary hostname", error))?
        .ok_or_else(|| WebServiceError::not_found("domain not found for site"))?;

        row.try_get("hostname")
            .map_err(|error| store_error("resolve primary hostname column", error))
    }

    /// Distinguishes a concurrent-write conflict from a missing row after a
    /// compare-and-swap update affected zero rows.
    async fn conflict_or_missing_nginx_config(
        &self,
        tenant_id: Option<i64>,
        config_id: &str,
    ) -> WebServiceResult<NginxConfigResponse> {
        let tenant_id = require_nginx_tenant(tenant_id)?;
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT 1 FROM webserver_nginx_config WHERE tenant_id = $1 AND uuid = $2",
        )
        .bind(tenant_id)
        .bind(config_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("recheck webserver_nginx_config existence", error))?;
        if exists.is_some() {
            return Err(WebServiceError::conflict(
                "nginx config was modified concurrently; reload and retry",
            ));
        }
        Err(WebServiceError::not_found("nginx config not found"))
    }
}

enum BindValue {
    I64(i64),
    I32(i32),
    Bool(bool),
    String(String),
}

fn apply_binds<'q>(
    mut query: sqlx::query::Query<'q, EngineDatabase, EngineArguments<'q>>,
    binds: &[BindValue],
) -> sqlx::query::Query<'q, EngineDatabase, EngineArguments<'q>> {
    for value in binds {
        query = match value {
            BindValue::I64(value) => query.bind(*value),
            BindValue::I32(value) => query.bind(*value),
            BindValue::Bool(value) => query.bind(*value),
            BindValue::String(value) => query.bind(value.clone()),
        };
    }
    query
}

fn map_nginx_config_row(
    row: &EngineRow,
    include_content: bool,
) -> Result<NginxConfigResponse, sqlx::Error> {
    Ok(NginxConfigResponse {
        id: row.try_get("uuid")?,
        site_id: row.try_get("site_uuid")?,
        config_name: row.try_get("config_name")?,
        config_type: row.try_get("config_type")?,
        // A page must not carry every config body: one row may be up to the
        // 1 MiB materialization limit, and the list only needs the summary.
        config_content: if include_content {
            row.try_get("config_content")?
        } else {
            String::new()
        },
        config_hash: row.try_get("config_hash")?,
        is_active: bool_from_row(row, "is_active")?,
        version_no: row.try_get("version_no")?,
        deployed_at: row.try_get("deployed_at")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
