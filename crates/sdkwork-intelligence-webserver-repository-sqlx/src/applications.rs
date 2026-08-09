use crate::audited_sql;
use super::{EngineRow, WebRepository};
use sdkwork_utils_rust::slugify;
use sdkwork_webserver_contract::{
    ApplicationPage, ApplicationResponse, ApplicationStoreListing, CreateApplicationRequest,
    ListApplicationsQuery, UpdateApplicationRequest, WebServiceError, WebServiceResult,
};
use sqlx::Row;

use super::support::{
    instant_from_row, instant_write_expression, json_from_row, json_write_expression, new_uuid,
    next_id, now_rfc3339, pagination, resolve_site_internal_id, store_error,
};

/// SELECT projection joining the application resource row with its backing
/// site carrier row. The application owns the resource identity (name/slug);
/// the site owns the technical runtime state (type, status, runtime config).
const APPLICATION_SELECT: &str = "SELECT a.uuid AS application_id, a.name AS name, a.slug AS slug,
                    a.description AS description, a.application_kind AS app_kind,
                    s.uuid AS site_id, s.application_type, s.site_type, s.status,
                    CAST(s.runtime_config AS TEXT) AS runtime_config,
                    CAST(s.metadata AS TEXT) AS metadata,
                    CAST(a.created_at AS TEXT) AS created_at,
                    CAST(a.updated_at AS TEXT) AS updated_at
             FROM web_application a
             JOIN web_site s ON s.id = a.site_id AND s.tenant_id = a.tenant_id";

impl WebRepository {
    pub(super) async fn list_applications_repo(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        query: &ListApplicationsQuery,
    ) -> WebServiceResult<ApplicationPage> {
        let (page, page_size, offset) = pagination(query.page, query.page_size)?;
        let keyword = query
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("%{value}%"));
        let count_sql = "SELECT COUNT(*) AS total FROM web_application a
             JOIN web_site s ON s.id = a.site_id AND s.tenant_id = a.tenant_id
             WHERE a.tenant_id = $1 AND a.deleted_at IS NULL AND s.deleted_at IS NULL
               AND ($2 IS NULL OR s.status = $2)
               AND ($3 IS NULL OR s.application_type = $3)
               AND ($4 IS NULL OR s.site_type = $4)
               AND ($5 IS NULL OR a.name LIKE $5 OR a.slug LIKE $5)
               AND ($6 IS NULL OR (a.data_scope = 3 AND a.user_id = $6))";
        let list_sql = format!(
            "{APPLICATION_SELECT}
             WHERE a.tenant_id = $1 AND a.deleted_at IS NULL AND s.deleted_at IS NULL
               AND ($2 IS NULL OR s.status = $2)
               AND ($3 IS NULL OR s.application_type = $3)
               AND ($4 IS NULL OR s.site_type = $4)
               AND ($5 IS NULL OR a.name LIKE $5 OR a.slug LIKE $5)
               AND ($6 IS NULL OR (a.data_scope = 3 AND a.user_id = $6))
             ORDER BY a.updated_at DESC, a.id DESC LIMIT $7 OFFSET $8"
        );

        let count_query = sqlx::query(count_sql)
            .bind(tenant_id)
            .bind(query.status)
            .bind(query.application_type.as_deref())
            .bind(query.site_type)
            .bind(keyword.as_deref())
            .bind(owner_id);
        let list_query = sqlx::query(audited_sql(&list_sql))
            .bind(tenant_id)
            .bind(query.status)
            .bind(query.application_type.as_deref())
            .bind(query.site_type)
            .bind(keyword.as_deref())
            .bind(owner_id)
            .bind(page_size)
            .bind(offset);

        let count_row = count_query
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count web_application", error))?;
        let total: i64 = count_row
            .try_get("total")
            .map_err(|error| store_error("map web_application count", error))?;

        let rows = list_query
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list web_application", error))?;

        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            items.push(map_application_row(row).map_err(|error| {
                WebServiceError::Internal(format!("map web_application row: {error}"))
            })?);
        }

        Ok(ApplicationPage {
            items,
            total,
            page,
            page_size,
        })
    }

    /// Creates the application resource together with its backing site
    /// carrier row in one transaction, then links `web_application.site_id`.
    pub(super) async fn create_application_repo(
        &self,
        tenant_id: i64,
        organization_id: Option<i64>,
        owner_id: Option<i64>,
        request: &CreateApplicationRequest,
    ) -> WebServiceResult<ApplicationResponse> {
        let site_id = next_id(self.id_generator())?;
        let site_uuid = new_uuid();
        let application_id = next_id(self.id_generator())?;
        let application_uuid = new_uuid();
        let slug = request
            .slug
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(slugify)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| slugify(&request.name));
        if slug.is_empty() {
            return Err(WebServiceError::validation("slug cannot be empty"));
        }
        // The app kind is the business type; the site carrier rows derive
        // their technical type from it (AppKind::carrier_site_type /
        // carrier_application_type).
        let app_kind = sdkwork_webserver_contract::AppKind::parse(&request.app_kind)
            .ok_or_else(|| WebServiceError::validation("appKind is not a supported app kind"))?;
        let carrier_site_type = app_kind.carrier_site_type();
        let carrier_application_type = app_kind.carrier_application_type();
        let now = now_rfc3339();
        let runtime_config = request
            .runtime_config
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));
        let metadata = request
            .store_listing
            .as_ref()
            .map(|store_listing| serde_json::json!({ "storeListing": store_listing }))
            .unwrap_or_else(|| serde_json::json!({}));
        let org_id = organization_id.unwrap_or(0);
        let data_scope = if owner_id.is_some() { 3 } else { 1 };

        let runtime_config_expression = json_write_expression("$12");
        let metadata_expression = json_write_expression("$13");
        let now_expression = instant_write_expression("$14");
        let insert_site_sql = format!(
            "INSERT INTO web_site (
                id, uuid, tenant_id, organization_id, data_scope, user_id, name, slug, description,
                application_type, site_type, status, runtime_config, metadata, created_at, updated_at, version
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 0,
                {runtime_config_expression}, {metadata_expression}, {now_expression}, {now_expression}, 0
             )"
        );

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin create web_application transaction", error))?;

        sqlx::query(audited_sql(&insert_site_sql))
            .bind(site_id)
            .bind(&site_uuid)
            .bind(tenant_id)
            .bind(org_id)
            .bind(data_scope)
            .bind(owner_id)
            .bind(&request.name)
            .bind(&slug)
            .bind(&request.description)
            .bind(carrier_application_type)
            .bind(carrier_site_type)
            .bind(runtime_config.to_string())
            .bind(metadata.to_string())
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("insert web_site for application", error))?;

        let application_now_expression = instant_write_expression("$12");
        sqlx::query(audited_sql(&format!(
            "INSERT INTO web_application (
                id, uuid, tenant_id, organization_id, data_scope, user_id, name, slug, description,
                application_kind, status, site_id, default_environment, created_at, updated_at, version
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 0, $11, 'production',
                {application_now_expression}, {application_now_expression}, 0
             )"
        )))
        .bind(application_id)
        .bind(&application_uuid)
        .bind(tenant_id)
        .bind(org_id)
        .bind(data_scope)
        .bind(owner_id)
        .bind(&request.name)
        .bind(&slug)
        .bind(&request.description)
        .bind(&request.app_kind)
        .bind(site_id)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("insert web_application", error))?;

        tx.commit()
            .await
            .map_err(|error| store_error("commit create web_application transaction", error))?;

        self.retrieve_application_repo(tenant_id, owner_id, &application_uuid)
            .await
    }

    pub(super) async fn retrieve_application_repo(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        application_id: &str,
    ) -> WebServiceResult<ApplicationResponse> {
        let row = sqlx::query(audited_sql(&format!(
            "{APPLICATION_SELECT}
             WHERE a.tenant_id = $1 AND a.uuid = $2 AND a.deleted_at IS NULL AND s.deleted_at IS NULL
               AND ($3 IS NULL OR (a.data_scope = 3 AND a.user_id = $3))"
        )))
        .bind(tenant_id)
        .bind(application_id)
        .bind(owner_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve web_application", error))?
        .ok_or_else(|| WebServiceError::not_found("application not found"))?;

        map_application_row(&row).map_err(|error| WebServiceError::Internal(error.to_string()))
    }

    pub(super) async fn update_application_repo(
        &self,
        tenant_id: i64,
        application_id: &str,
        request: &UpdateApplicationRequest,
    ) -> WebServiceResult<ApplicationResponse> {
        let existing = self.retrieve_application_repo(tenant_id, None, application_id).await?;
        let existing_site_id = existing.site_id.clone().unwrap_or_default();
        let current_version = self.retrieve_site_version_repo(tenant_id, &existing_site_id).await?;
        let name = request.name.as_ref().unwrap_or(&existing.name);
        let description = request
            .description
            .as_ref()
            .or(existing.description.as_ref());
        let runtime_config = request
            .runtime_config
            .clone()
            .or(existing.runtime_config)
            .unwrap_or_else(|| serde_json::json!({}));
        let now = now_rfc3339();

        let runtime_config_expression = json_write_expression("$5");
        let store_listing_json = request
            .store_listing
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| WebServiceError::Internal(format!("serialize store listing: {error}")))?;
        // PostgreSQL-only metadata expression (single authoritative engine).
        let metadata_expression =
            "CASE WHEN $6 IS NULL THEN metadata ELSE jsonb_set(metadata, '{storeListing}', CAST($6 AS JSONB), true) END"
                .to_string();
        let now_expression = instant_write_expression("$7");
        let update_site_sql = format!(
            "UPDATE web_site
             SET name = $3, description = $4, runtime_config = {runtime_config_expression},
                 metadata = {metadata_expression},
                 updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL AND version = $8"
        );

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin update web_application transaction", error))?;

        let updated = sqlx::query(audited_sql(&update_site_sql))
            .bind(tenant_id)
            .bind(&existing_site_id)
            .bind(name)
            .bind(description)
            .bind(runtime_config.to_string())
            .bind(store_listing_json)
            .bind(&now)
            .bind(current_version)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("update web_site for application", error))?;

        if updated.rows_affected() == 0 {
            tx.rollback().await.map_err(|error| {
                store_error("rollback update web_application transaction", error)
            })?;
            return self.conflict_or_missing_site(tenant_id, &existing_site_id).await;
        }

        sqlx::query(audited_sql(
            "UPDATE web_application
             SET name = $3, description = $4, updated_at = $5, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        ))
        .bind(tenant_id)
        .bind(application_id)
        .bind(name)
        .bind(description)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("update web_application", error))?;

        tx.commit()
            .await
            .map_err(|error| store_error("commit update web_application transaction", error))?;

        self.retrieve_application_repo(tenant_id, None, application_id).await
    }

    pub(super) async fn delete_application_repo(
        &self,
        tenant_id: i64,
        application_id: &str,
        actor_id: Option<i64>,
    ) -> WebServiceResult<()> {
        let site_id = self.resolve_site_id_repo(tenant_id, application_id).await?;
        let status: i32 = sqlx::query_scalar(
            "SELECT status
             FROM web_site
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(&site_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("delete web_application status lookup", error))?
        .ok_or_else(|| WebServiceError::not_found("application not found"))?;

        if status == 1 {
            return Err(WebServiceError::conflict(
                "active applications must be disabled before deletion",
            ));
        }

        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, &site_id).await?;
        let now = now_rfc3339();

        let now_expression = instant_write_expression("$3");
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin delete web_application transaction", error))?;

        let application_update = sqlx::query(audited_sql(&format!(
            "UPDATE web_application
             SET deleted_at = {now_expression}, deleted_by = $4,
                 updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL"
        )))
        .bind(tenant_id)
        .bind(application_id)
        .bind(&now)
        .bind(actor_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("delete web_application", error))?;

        if application_update.rows_affected() == 0 {
            tx.rollback().await.map_err(|error| {
                store_error("rollback delete web_application transaction", error)
            })?;
            return Err(WebServiceError::not_found("application not found"));
        }

        let site_update = sqlx::query(audited_sql(&format!(
            "UPDATE web_site
             SET deleted_at = {now_expression}, deleted_by = $4,
                 updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL AND status <> 1"
        )))
        .bind(tenant_id)
        .bind(&site_id)
        .bind(&now)
        .bind(actor_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("delete web_site for application", error))?;

        if site_update.rows_affected() == 0 {
            tx.rollback().await.map_err(|error| {
                store_error("rollback delete web_site for application", error)
            })?;
            return Err(WebServiceError::conflict(
                "application state changed; disable it before deletion",
            ));
        }

        // Archive the site's owned route surface so deleted applications never
        // keep occupying domain routes, TLS policies, or listener bindings.
        sqlx::query(audited_sql(&format!(
            "UPDATE web_site_binding
             SET status = 'ARCHIVED', deleted_at = {now_expression},
                 updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND site_id = $2 AND deleted_at IS NULL"
        )))
        .bind(tenant_id)
        .bind(site_internal_id)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("archive web_site bindings", error))?;
        sqlx::query(audited_sql(&format!(
            "UPDATE web_tls_policy policy
             SET status = 'ARCHIVED', deleted_at = {now_expression},
                 updated_at = {now_expression}, version = version + 1
             FROM web_site_binding binding
             WHERE binding.tenant_id = policy.tenant_id
               AND binding.id = policy.site_binding_id
               AND binding.tenant_id = $1 AND binding.site_id = $2
               AND policy.deleted_at IS NULL"
        )))
        .bind(tenant_id)
        .bind(site_internal_id)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("archive web_site TLS policies", error))?;
        sqlx::query(audited_sql(&format!(
            "UPDATE web_listener_certificate_binding listener
             SET status = 'ARCHIVED', deleted_at = {now_expression},
                 updated_at = {now_expression}, version = version + 1
             FROM web_site_binding binding
             WHERE binding.tenant_id = listener.tenant_id
               AND binding.id = listener.site_binding_id
               AND binding.tenant_id = $1 AND binding.site_id = $2
               AND listener.deleted_at IS NULL"
        )))
        .bind(tenant_id)
        .bind(site_internal_id)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("archive web_site listener certificate bindings", error))?;
        // Deactivate the site's environment variables and health checks.
        sqlx::query(audited_sql(&format!(
            "UPDATE web_env_variable
             SET status = 0, updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND site_id = $2 AND status = 1"
        )))
        .bind(tenant_id)
        .bind(site_internal_id)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("deactivate web_site environment variables", error))?;
        sqlx::query(audited_sql(&format!(
            "UPDATE web_health_check
             SET status = 0, updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND site_id = $2 AND status = 1"
        )))
        .bind(tenant_id)
        .bind(site_internal_id)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("deactivate web_site health checks", error))?;

        tx.commit()
            .await
            .map_err(|error| store_error("commit delete web_application transaction", error))?;
        Ok(())
    }

    pub(super) async fn set_application_status_repo(
        &self,
        tenant_id: i64,
        application_id: &str,
        status: i32,
    ) -> WebServiceResult<ApplicationResponse> {
        let site_id = self.resolve_site_id_repo(tenant_id, application_id).await?;
        let current_version = self.retrieve_site_version_repo(tenant_id, &site_id).await?;
        let now = now_rfc3339();

        let now_expression = instant_write_expression("$4");
        let update_sql = format!(
            "UPDATE web_site
             SET status = $3, updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL AND version = $5"
        );
        let result = sqlx::query(audited_sql(&update_sql))
            .bind(tenant_id)
            .bind(&site_id)
            .bind(status)
            .bind(&now)
            .bind(current_version)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("update web_site status", error))?;

        if result.rows_affected() == 0 {
            return self.conflict_or_missing_site(tenant_id, &site_id).await;
        }

        self.retrieve_application_repo(tenant_id, None, application_id).await
    }

    /// Resolves the backing site row uuid for an application resource.
    pub(super) async fn resolve_site_id_repo(
        &self,
        tenant_id: i64,
        application_id: &str,
    ) -> WebServiceResult<String> {
        sqlx::query_scalar(
            "SELECT s.uuid
             FROM web_application a
             JOIN web_site s ON s.id = a.site_id AND s.tenant_id = a.tenant_id
             WHERE a.tenant_id = $1 AND a.uuid = $2 AND a.deleted_at IS NULL AND s.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(application_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve web_application site id", error))?
        .ok_or_else(|| WebServiceError::not_found("application not found"))
    }

    /// Reads the current optimistic-concurrency version of a live site row.
    pub(super) async fn retrieve_site_version_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
    ) -> WebServiceResult<i64> {
        sqlx::query_scalar(
            "SELECT version FROM web_site
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(site_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("load web_site version", error))?
        .ok_or_else(|| WebServiceError::not_found("application not found"))
    }

    /// Distinguishes a concurrent-write conflict from a missing row after a
    /// compare-and-swap update affected zero rows.
    async fn conflict_or_missing_site(
        &self,
        tenant_id: i64,
        site_id: &str,
    ) -> WebServiceResult<ApplicationResponse> {
        if self.retrieve_site_version_repo(tenant_id, site_id).await.is_ok() {
            return Err(WebServiceError::conflict(
                "application was modified concurrently; reload and retry",
            ));
        }
        Err(WebServiceError::not_found("application not found"))
    }
}

fn map_application_row(row: &EngineRow) -> Result<ApplicationResponse, sqlx::Error> {
    Ok(ApplicationResponse {
        id: row.try_get("application_id")?,
        site_id: row.try_get("site_id")?,
        name: row.try_get("name")?,
        slug: row.try_get("slug")?,
        description: row.try_get("description")?,
        app_kind: row.try_get("app_kind")?,
        site_type: row.try_get("site_type")?,
        status: row.try_get("status")?,
        runtime_config: json_from_row(row, "runtime_config")?,
        store_listing: store_listing_from_row(row)?,
        created_at: instant_from_row(row, "created_at")?,
        updated_at: instant_from_row(row, "updated_at")?,
    })
}

fn store_listing_from_row(row: &EngineRow) -> Result<Option<ApplicationStoreListing>, sqlx::Error> {
    let Some(metadata) = json_from_row(row, "metadata")? else {
        return Ok(None);
    };
    metadata
        .get("storeListing")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))
}
