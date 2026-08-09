use crate::audited_sql;
use super::{EngineRow, WebRepository};
use sdkwork_webserver_contract::{
    CreateSourceVersionRequest, SourceVersionPage,
    SourceVersionResponse, WebServiceError, WebServiceResult,
};
use sqlx::Row;

use super::support::{
    decode_keyset_cursor, encode_keyset_cursor, instant_from_row, json_from_row, new_uuid,
    next_id, now_rfc3339, pagination, resolve_site_internal_id, store_error,
};

impl WebRepository {
    pub(super) async fn list_source_versions_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        page: i32,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<SourceVersionPage> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;
        if let Some(cursor) = cursor {
            return self
                .list_source_versions_cursor_repo(tenant_id, site_id, page_size, cursor)
                .await;
        }
        // Offset mode remains only for internal single-page lookups (page 1).
        // Deep OFFSET on this growing revision collection is rejected; clients
        // must use cursor pagination (PRD-FR-011, PAGINATION_SPEC §6/§12).
        if page > 1 {
            return Err(WebServiceError::validation(
                "cursor is required beyond the first page of source versions; offset pagination is not supported on this growing collection",
            ));
        }
        let (page, page_size, offset) = pagination(page, page_size)?;
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM web_source_version
             WHERE tenant_id = $1 AND site_id = $2",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count web_source_version", error))?;
        let rows = sqlx::query(
            "SELECT uuid, version_tag, source_type, source_ref, commit_hash, artifact_path,
                    artifact_size, artifact_hash, CAST(config_snapshot AS TEXT) AS config_snapshot,
                    status, CAST(created_at AS TEXT) AS created_at
             FROM web_source_version
             WHERE tenant_id = $1 AND site_id = $2
             ORDER BY created_at DESC, id DESC LIMIT $3 OFFSET $4",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list web_source_version", error))?;

        let total = count_row
            .try_get("total")
            .map_err(|error| store_error("map web_source_version count", error))?;
        let items = rows
            .iter()
            .map(|row| map_source_version_row(row, site_id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| WebServiceError::Internal(format!("map web_source_version: {error}")))?;
        Ok(SourceVersionPage {
            items,
            total,
            page,
            page_size,
            next_cursor: None,
            has_more: None,
        })
    }

    /// Keyset page over `(created_at DESC, id DESC)` with an opaque cursor;
    /// fetches `page_size + 1` rows so `has_more` is exact and no COUNT runs.
    async fn list_source_versions_cursor_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        page_size: i32,
        cursor: &str,
    ) -> WebServiceResult<SourceVersionPage> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;
        if !(1..=200).contains(&page_size) {
            return Err(WebServiceError::validation(
                "page_size must be between 1 and 200",
            ));
        }
        let (cursor_created_at, cursor_id) = decode_keyset_cursor(cursor)
            .ok_or_else(|| WebServiceError::validation("cursor is invalid"))?;
        let sql = "SELECT uuid, version_tag, source_type, source_ref, commit_hash, artifact_path,
                    artifact_size, artifact_hash, CAST(config_snapshot AS TEXT) AS config_snapshot,
                    status, CAST(created_at AS TEXT) AS created_at
             FROM web_source_version
             WHERE tenant_id = $1 AND site_id = $2
               AND (created_at, id) < ($3, $4)
             ORDER BY created_at DESC, id DESC LIMIT $5".to_string();
        let fetch_size = i64::from(page_size) + 1;
        let rows = sqlx::query(audited_sql(&sql))
            .bind(tenant_id)
            .bind(site_internal_id)
            .bind(&cursor_created_at)
            .bind(cursor_id)
            .bind(fetch_size)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list web_source_version cursor", error))?;
        let has_more = rows.len() > page_size as usize;
        let page_rows = rows.into_iter().take(page_size as usize).collect::<Vec<_>>();
        let items = page_rows
            .iter()
            .map(|row| map_source_version_row(row, site_id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| WebServiceError::Internal(format!("map web_source_version: {error}")))?;
        let next_cursor = has_more
            .then(|| {
                let last = page_rows.last().expect("non-empty page when has_more");
                let created_at: String = last
                    .try_get("created_at")
                    .map_err(|error| store_error("map web_source_version cursor instant", error))?;
                let id: i64 = last
                    .try_get("id")
                    .map_err(|error| store_error("map web_source_version cursor id", error))?;
                Ok::<_, WebServiceError>(encode_keyset_cursor(&created_at, id))
            })
            .transpose()?;
        Ok(SourceVersionPage {
            items,
            total: 0,
            page: 0,
            page_size,
            next_cursor,
            has_more: Some(has_more),
        })
    }

    pub(super) async fn create_source_version_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        actor_id: Option<i64>,
        retention_limit: i32,
        request: &CreateSourceVersionRequest,
    ) -> WebServiceResult<SourceVersionResponse> {
        if !(1..=50).contains(&retention_limit) {
            return Err(WebServiceError::validation(
                "source version retention limit must be between 1 and 50",
            ));
        }
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();
        let config_snapshot = serde_json::to_string(&request.config_snapshot)
            .map_err(|error| WebServiceError::Internal(error.to_string()))?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin source version transaction", error))?;
        let site_row = sqlx::query(
            "SELECT id, organization_id FROM web_site
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(site_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("lock source version site", error))?
        .ok_or_else(|| WebServiceError::not_found("site not found"))?;
        let site_internal_id: i64 = site_row
            .try_get("id")
            .map_err(|error| store_error("map source version site id", error))?;
        let organization_id: i64 = site_row
            .try_get("organization_id")
            .map_err(|error| store_error("map source version organization id", error))?;

        sqlx::query(
            "INSERT INTO web_source_version (
                id, uuid, tenant_id, organization_id, user_id, site_id, version_tag,
                source_type, source_ref, commit_hash, artifact_path, artifact_size,
                artifact_hash, config_snapshot, status, metadata, created_at, updated_at, version
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                CAST($14 AS JSONB), 1, '{}', CAST($15 AS TIMESTAMPTZ),
                CAST($15 AS TIMESTAMPTZ), 0
             )",
        )
            .bind(id)
            .bind(&uuid)
            .bind(tenant_id)
            .bind(organization_id)
            .bind(actor_id)
            .bind(site_internal_id)
            .bind(request.version_tag.trim())
            .bind(request.source_type.trim())
            .bind(normalized_optional(request.source_ref.as_deref()))
            .bind(normalized_optional(request.commit_hash.as_deref()))
            .bind(request.artifact_drive_uri.trim())
            .bind(request.artifact_size)
            .bind(request.artifact_hash.trim())
            .bind(&config_snapshot)
            .bind(&now)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("insert web_source_version", error))?;

        sqlx::query(
            "WITH retained AS (
                 SELECT id
                 FROM web_source_version
                 WHERE tenant_id = $1 AND site_id = $2 AND status = 1
                 ORDER BY created_at DESC, id DESC
                 OFFSET $3
             )
             UPDATE web_source_version AS source_version
             SET status = 3, pruned_at = CAST($5 AS TIMESTAMPTZ), pruned_by = $4,
                 updated_at = CAST($5 AS TIMESTAMPTZ), version = source_version.version + 1
             FROM retained
             WHERE source_version.tenant_id = $1
               AND source_version.id = retained.id
               AND source_version.status = 1",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .bind(retention_limit)
        .bind(actor_id)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("prune web_source_version", error))?;

        let row = sqlx::query(
            "SELECT uuid, version_tag, source_type, source_ref, commit_hash, artifact_path,
                    artifact_size, artifact_hash, CAST(config_snapshot AS TEXT) AS config_snapshot,
                    status, CAST(created_at AS TEXT) AS created_at
             FROM web_source_version
             WHERE tenant_id = $1 AND site_id = $2 AND uuid = $3",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .bind(&uuid)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| store_error("retrieve created web_source_version", error))?;
        let response = map_source_version_row(&row, site_id)
            .map_err(|error| WebServiceError::Internal(format!("map web_source_version: {error}")))?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit source version transaction", error))?;
        Ok(response)
    }

    pub(super) async fn retrieve_source_version_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        source_version_id: &str,
    ) -> WebServiceResult<SourceVersionResponse> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;
        let row = sqlx::query(
            "SELECT uuid, version_tag, source_type, source_ref, commit_hash, artifact_path,
                    artifact_size, artifact_hash, CAST(config_snapshot AS TEXT) AS config_snapshot,
                    status, CAST(created_at AS TEXT) AS created_at
             FROM web_source_version
             WHERE tenant_id = $1 AND site_id = $2 AND uuid = $3",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .bind(source_version_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve web_source_version", error))?
        .ok_or_else(|| WebServiceError::not_found("source version not found"))?;
        map_source_version_row(&row, site_id)
            .map_err(|error| WebServiceError::Internal(format!("map web_source_version: {error}")))
    }
}

fn map_source_version_row(
    row: &EngineRow,
    site_id: &str,
) -> Result<SourceVersionResponse, sqlx::Error> {
    let status: i32 = row.try_get("status")?;
    let config_snapshot = json_from_row(row, "config_snapshot")?
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))?
        .unwrap_or_default();
    Ok(SourceVersionResponse {
        id: row.try_get("uuid")?,
        application_id: site_id.to_string(),
        version_tag: row.try_get("version_tag")?,
        source_type: row.try_get("source_type")?,
        source_ref: row.try_get("source_ref")?,
        commit_hash: row.try_get("commit_hash")?,
        artifact_drive_uri: row.try_get("artifact_path")?,
        artifact_size: row.try_get("artifact_size")?,
        artifact_hash: row.try_get("artifact_hash")?,
        config_snapshot,
        status,
        retained: status != 3,
        created_at: instant_from_row(row, "created_at")?,
    })
}

fn normalized_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
