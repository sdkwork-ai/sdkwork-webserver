use crate::audited_sql;
use super::{EngineRow, WebRepository};
use sdkwork_webserver_contract::{
    CreatePlatformTargetRequest, PlatformTargetPage, PlatformTargetResponse, WebServiceError,
    WebServiceResult,
};
use sqlx::Row;

use super::support::{
    instant_from_row, instant_write_expression, new_uuid, next_id, now_rfc3339, pagination,
    store_error,
};

/// Resolves the application's internal id and uuid for the tenant. Platform
/// targets are app-scoped child rows; the internal id is used for the FK and
/// the uuid for the wire `appId` response field.
async fn resolve_application_identity(
    pool: &sqlx::PgPool,
    tenant_id: i64,
    application_id: &str,
) -> WebServiceResult<(i64, String)> {
    sqlx::query_as::<_, (i64, String)>(
        "SELECT id, uuid FROM web_application
         WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(application_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("resolve web_application identity", error))?
    .ok_or_else(|| WebServiceError::not_found("application not found"))
}

impl WebRepository {
    pub(super) async fn create_platform_target_repo(
        &self,
        tenant_id: i64,
        application_id: &str,
        request: &CreatePlatformTargetRequest,
    ) -> WebServiceResult<PlatformTargetResponse> {
        let (application_internal_id, application_uuid) =
            resolve_application_identity(&self.pool, tenant_id, application_id).await?;
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();
        let tech_stack = request
            .tech_stack
            .clone()
            .unwrap_or_else(|| "OTHER".to_string());
        let architectures = request.architectures.clone().unwrap_or_default();
        let allowed_channels = request
            .allowed_channels
            .clone()
            .unwrap_or_else(|| vec!["stable".to_string()]);
        let architectures_json = serde_json::to_string(&architectures).map_err(|error| {
            WebServiceError::Internal(format!("serialize architectures: {error}"))
        })?;
        let channels_json = serde_json::to_string(&allowed_channels).map_err(|error| {
            WebServiceError::Internal(format!("serialize allowed channels: {error}"))
        })?;
        let now_expression = instant_write_expression("$14");

        let insert_sql = format!(
            "INSERT INTO web_app_platform_target (
                id, uuid, tenant_id, organization_id, data_scope, user_id, app_id,
                target_key, platform, tech_stack, architectures_json,
                bundle_id, package_name, app_id_value, bundle_name,
                allowed_channels_json, target_status, created_at, updated_at, version
             ) VALUES (
                $1, $2, $3, 0, 1, NULL, $4,
                $5, $6, $7, $8,
                $9, $10, $11, $12,
                $13, 'ACTIVE', {now_expression}, {now_expression}, 0
             )"
        );

        sqlx::query(audited_sql(&insert_sql))
            .bind(id)
            .bind(&uuid)
            .bind(tenant_id)
            .bind(application_internal_id)
            .bind(&request.target_key)
            .bind(&request.platform)
            .bind(&tech_stack)
            .bind(&architectures_json)
            .bind(&request.bundle_id)
            .bind(&request.package_name)
            .bind(&request.app_id)
            .bind(&request.bundle_name)
            .bind(&channels_json)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("insert web_app_platform_target", error))?;

        Ok(PlatformTargetResponse {
            id: uuid,
            app_id: application_uuid,
            target_key: request.target_key.clone(),
            platform: request.platform.clone(),
            tech_stack: Some(tech_stack),
            architectures: Some(architectures),
            bundle_id: request.bundle_id.clone(),
            package_name: request.package_name.clone(),
            app_id_value: request.app_id.clone(),
            bundle_name: request.bundle_name.clone(),
            target_status: Some("ACTIVE".to_string()),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub(super) async fn list_platform_targets_repo(
        &self,
        tenant_id: i64,
        application_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<PlatformTargetPage> {
        let (page, page_size, offset) = pagination(page, page_size)?;
        let (application_internal_id, application_uuid) =
            resolve_application_identity(&self.pool, tenant_id, application_id).await?;
        let count_sql = "SELECT COUNT(*) AS total FROM web_app_platform_target
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL";
        let list_sql = "SELECT uuid, target_key, platform, tech_stack,
                    CAST(architectures_json AS TEXT) AS architectures_json,
                    bundle_id, package_name, app_id_value, bundle_name,
                    target_status,
                    CAST(created_at AS TEXT) AS created_at,
                    CAST(updated_at AS TEXT) AS updated_at
             FROM web_app_platform_target
             WHERE tenant_id = $1 AND app_id = $2 AND deleted_at IS NULL
             ORDER BY id ASC LIMIT $3 OFFSET $4";

        let count_row = sqlx::query(count_sql)
            .bind(tenant_id)
            .bind(application_internal_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count web_app_platform_target", error))?;
        let total: i64 = count_row
            .try_get("total")
            .map_err(|error| store_error("map web_app_platform_target count", error))?;

        let rows = sqlx::query(list_sql)
            .bind(tenant_id)
            .bind(application_internal_id)
            .bind(page_size)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list web_app_platform_target", error))?;

        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            items.push(map_platform_target_row(row, &application_uuid).map_err(|error| {
                WebServiceError::Internal(format!("map web_app_platform_target row: {error}"))
            })?);
        }

        Ok(PlatformTargetPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_platform_target_repo(
        &self,
        tenant_id: i64,
        application_id: &str,
        platform_target_id: &str,
    ) -> WebServiceResult<PlatformTargetResponse> {
        let (application_internal_id, application_uuid) =
            resolve_application_identity(&self.pool, tenant_id, application_id).await?;
        let row = sqlx::query(
            "SELECT uuid, target_key, platform, tech_stack,
                    CAST(architectures_json AS TEXT) AS architectures_json,
                    bundle_id, package_name, app_id_value, bundle_name,
                    target_status,
                    CAST(created_at AS TEXT) AS created_at,
                    CAST(updated_at AS TEXT) AS updated_at
             FROM web_app_platform_target
             WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(application_internal_id)
        .bind(platform_target_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve web_app_platform_target", error))?
        .ok_or_else(|| WebServiceError::not_found("platform target not found"))?;

        map_platform_target_row(&row, &application_uuid)
            .map_err(|error| WebServiceError::Internal(error.to_string()))
    }
}

fn map_platform_target_row(
    row: &EngineRow,
    application_uuid: &str,
) -> Result<PlatformTargetResponse, sqlx::Error> {
    let architectures_json: Option<String> = row.try_get("architectures_json")?;
    let architectures = architectures_json
        .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok());
    Ok(PlatformTargetResponse {
        id: row.try_get("uuid")?,
        app_id: application_uuid.to_owned(),
        target_key: row.try_get("target_key")?,
        platform: row.try_get("platform")?,
        tech_stack: row.try_get("tech_stack")?,
        architectures,
        bundle_id: row.try_get("bundle_id")?,
        package_name: row.try_get("package_name")?,
        app_id_value: row.try_get("app_id_value")?,
        bundle_name: row.try_get("bundle_name")?,
        target_status: row.try_get("target_status")?,
        created_at: instant_from_row(row, "created_at")?,
        updated_at: instant_from_row(row, "updated_at")?,
    })
}
