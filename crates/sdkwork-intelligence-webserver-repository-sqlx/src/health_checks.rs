use crate::audited_sql;
use sdkwork_webserver_contract::{
    CreateHealthCheckRequest, HealthCheckPage, HealthCheckResponse, WebServiceError,
    WebServiceResult,
};
use sqlx::Row;

use super::{EngineRow, WebRepository};
use super::support::{
    instant_from_row, instant_write_expression, new_uuid, next_id, now_rfc3339,
    resolve_site_internal_id, store_error,
};

const MAX_SITE_HEALTH_CHECKS: i64 = 100;

impl WebRepository {
    pub(super) async fn list_health_checks_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
    ) -> WebServiceResult<HealthCheckPage> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;

        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM webserver_health_check
             WHERE tenant_id = $1 AND site_id = $2",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count webserver_health_check", error))?;
        let total: i64 = count_row
            .try_get("total")
            .map_err(|error| store_error("map webserver_health_check count", error))?;
        if total > MAX_SITE_HEALTH_CHECKS {
            tracing::error!(
                tenant_id,
                site_id,
                total,
                maximum = MAX_SITE_HEALTH_CHECKS,
                "web health-check cardinality invariant violated"
            );
            return Err(WebServiceError::Internal(
                "health-check collection exceeds its configured capacity".to_string(),
            ));
        }

        let rows = sqlx::query(
            "SELECT uuid, check_type, check_url, check_interval, timeout_ms, retry_count, status,
                    CAST(created_at AS TEXT) AS created_at
             FROM webserver_health_check
             WHERE tenant_id = $1 AND site_id = $2
             ORDER BY created_at DESC, id DESC
             LIMIT 100",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list webserver_health_check", error))?;

        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            items.push(map_health_check_row(row).map_err(|error| {
                WebServiceError::Internal(format!("map webserver_health_check row: {error}"))
            })?);
        }

        Ok(HealthCheckPage { items, total })
    }

    pub(super) async fn create_health_check_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        request: &CreateHealthCheckRequest,
    ) -> WebServiceResult<HealthCheckResponse> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();

        let now_expression = instant_write_expression("$10");
        let insert_sql = format!(
            "INSERT INTO webserver_health_check (
                id, uuid, tenant_id, site_id, check_type, check_url, check_interval,
                timeout_ms, retry_count, status,
                created_at, updated_at, version
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, 1,
                {now_expression}, {now_expression}, 0
             )"
        );

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin create webserver_health_check transaction", error))?;
        let locked = sqlx::query(
            "UPDATE webserver_site SET version = version
             WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("lock webserver_health_check site capacity", error))?;
        if locked.rows_affected() != 1 {
            return Err(WebServiceError::not_found("site not found"));
        }

        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM webserver_health_check
             WHERE tenant_id = $1 AND site_id = $2",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| store_error("count webserver_health_check capacity", error))?;
        let total: i64 = count_row
            .try_get("total")
            .map_err(|error| store_error("map webserver_health_check capacity", error))?;
        if total >= MAX_SITE_HEALTH_CHECKS {
            transaction.rollback().await.map_err(|error| {
                store_error("rollback full webserver_health_check collection", error)
            })?;
            return Err(WebServiceError::conflict(
                "a site supports at most 100 health checks",
            ));
        }

        sqlx::query(audited_sql(&insert_sql))
            .bind(id)
            .bind(&uuid)
            .bind(tenant_id)
            .bind(site_internal_id)
            .bind(request.check_type)
            .bind(&request.check_url)
            .bind(request.check_interval)
            .bind(request.timeout_ms)
            .bind(request.retry_count)
            .bind(&now)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("insert webserver_health_check", error))?;

        transaction.commit().await.map_err(|error| {
            store_error("commit create webserver_health_check transaction", error)
        })?;

        Ok(HealthCheckResponse {
            id: uuid,
            check_type: request.check_type,
            check_url: request.check_url.clone(),
            check_interval: request.check_interval,
            timeout_ms: request.timeout_ms,
            retry_count: request.retry_count,
            status: 1,
            created_at: now,
        })
    }
}

fn map_health_check_row(row: &EngineRow) -> Result<HealthCheckResponse, sqlx::Error> {
    Ok(HealthCheckResponse {
        id: row.try_get("uuid")?,
        check_type: row.try_get("check_type")?,
        check_url: row.try_get("check_url")?,
        check_interval: row.try_get("check_interval")?,
        timeout_ms: row.try_get("timeout_ms")?,
        retry_count: row.try_get("retry_count")?,
        status: row.try_get("status")?,
        created_at: instant_from_row(row, "created_at")?,
    })
}
