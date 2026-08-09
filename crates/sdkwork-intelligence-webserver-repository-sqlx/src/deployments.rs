use crate::audited_sql;
use super::{EngineRow, WebRepository};
use sdkwork_webserver_contract::{
    CreateDeploymentRequest, DeploymentPage, DeploymentResponse, WebServiceError, WebServiceResult,
};
use sqlx::Row;

use super::support::{
    decode_keyset_cursor, encode_keyset_cursor, instant_from_row, instant_write_expression,
    is_unique_violation, new_uuid, next_id, now_rfc3339, optional_instant_from_row, pagination,
    resolve_site_internal_id, sha256_hex, store_error,
};

struct DeploymentIdempotencyLookup<'a> {
    tenant_id: i64,
    site_internal_id: i64,
    site_id: &'a str,
    source_version_internal_id: Option<i64>,
    deploy_type: i32,
    environment: &'a str,
    version_tag: Option<&'a str>,
    commit_hash: Option<&'a str>,
    source_ref: Option<&'a str>,
    artifact_drive_uri: Option<&'a str>,
    artifact_size: Option<i64>,
    artifact_hash: Option<&'a str>,
    rollback_from_internal_id: Option<i64>,
    idempotency_key: &'a str,
}

struct SourceVersionDeploymentSnapshot {
    internal_id: i64,
    version_tag: String,
    commit_hash: Option<String>,
    source_ref: Option<String>,
    artifact_drive_uri: String,
    artifact_size: i64,
    artifact_hash: String,
}

/// Deployment projection shared by offset and cursor list queries.
const DEPLOYMENT_LIST_SELECT: &str = "SELECT deployment.id, deployment.uuid, deployment.site_id, deployment.status,
        deployment.deploy_type, deployment.environment, deployment.version_tag,
        deployment.commit_hash, deployment.source_ref, deployment.artifact_path,
        deployment.artifact_size, deployment.artifact_hash,
        source_version.uuid AS source_version_id,
        source.uuid AS rollback_from_deployment_id,
        CAST(deployment.started_at AS TEXT) AS started_at,
        CAST(deployment.completed_at AS TEXT) AS completed_at,
        deployment.duration_ms,
        CAST(deployment.created_at AS TEXT) AS created_at
 FROM web_deployment deployment
 LEFT JOIN web_deployment source
   ON source.id = deployment.rollback_from
  AND source.tenant_id = deployment.tenant_id
  AND source.site_id = deployment.site_id
 LEFT JOIN web_source_version source_version
   ON source_version.id = deployment.source_version_id
  AND source_version.tenant_id = deployment.tenant_id
  AND source_version.site_id = deployment.site_id";

impl WebRepository {
    pub(super) async fn list_deployments_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        page: i32,
        page_size: i32,
        status: Option<i32>,
        cursor: Option<&str>,
    ) -> WebServiceResult<DeploymentPage> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;
        if let Some(cursor) = cursor {
            return self
                .list_deployments_cursor_repo(tenant_id, site_id, page_size, status, cursor)
                .await;
        }
        // Offset mode remains only for internal single-page lookups (page 1,
        // e.g. the latest-deployment status projection). Deep OFFSET on this
        // growing collection is rejected; clients must use cursor pagination
        // (PRD-FR-011, PAGINATION_SPEC §6/§12).
        if page > 1 {
            return Err(WebServiceError::validation(
                "cursor is required beyond the first page of deployment history; offset pagination is not supported on this growing collection",
            ));
        }
        let (page, page_size, offset) = pagination(page, page_size)?;

        let (count_row, rows) = if let Some(status) = status {
            let count_row = sqlx::query(
                "SELECT COUNT(*) AS total FROM web_deployment
                 WHERE tenant_id = $1 AND site_id = $2 AND status = $3",
            )
            .bind(tenant_id)
            .bind(site_internal_id)
            .bind(status)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count web_deployment", error))?;

            let rows = sqlx::query(audited_sql(&format!(
                "{DEPLOYMENT_LIST_SELECT}
                 WHERE deployment.tenant_id = $1
                   AND deployment.site_id = $2
                   AND deployment.status = $3
                 ORDER BY deployment.created_at DESC, deployment.id DESC LIMIT $4 OFFSET $5"
            )))
            .bind(tenant_id)
            .bind(site_internal_id)
            .bind(status)
            .bind(page_size)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list web_deployment", error))?;

            (count_row, rows)
        } else {
            let count_row = sqlx::query(
                "SELECT COUNT(*) AS total FROM web_deployment
                 WHERE tenant_id = $1 AND site_id = $2",
            )
            .bind(tenant_id)
            .bind(site_internal_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count web_deployment", error))?;

            let rows = sqlx::query(audited_sql(&format!(
                "{DEPLOYMENT_LIST_SELECT}
                 WHERE deployment.tenant_id = $1 AND deployment.site_id = $2
                 ORDER BY deployment.created_at DESC, deployment.id DESC LIMIT $3 OFFSET $4"
            )))
            .bind(tenant_id)
            .bind(site_internal_id)
            .bind(page_size)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list web_deployment", error))?;

            (count_row, rows)
        };

        let total: i64 = count_row
            .try_get("total")
            .map_err(|error| store_error("map web_deployment count", error))?;
        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            items.push(map_deployment_row(row, site_id).map_err(|error| {
                WebServiceError::Internal(format!("map web_deployment row: {error}"))
            })?);
        }

        Ok(DeploymentPage {
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
    async fn list_deployments_cursor_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        page_size: i32,
        status: Option<i32>,
        cursor: &str,
    ) -> WebServiceResult<DeploymentPage> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;
        if !(1..=200).contains(&page_size) {
            return Err(WebServiceError::validation(
                "page_size must be between 1 and 200",
            ));
        }
        let (cursor_created_at, cursor_id) = decode_keyset_cursor(cursor)
            .ok_or_else(|| WebServiceError::validation("cursor is invalid"))?;
        let sql = format!(
            "{DEPLOYMENT_LIST_SELECT}
             WHERE deployment.tenant_id = $1 AND deployment.site_id = $2
               AND ($3 IS NULL OR deployment.status = $3)
               AND (deployment.created_at, deployment.id) < ($4, $5)
             ORDER BY deployment.created_at DESC, deployment.id DESC LIMIT $6"
        );
        let fetch_size = i64::from(page_size) + 1;
        let rows = sqlx::query(audited_sql(&sql))
            .bind(tenant_id)
            .bind(site_internal_id)
            .bind(status)
            .bind(&cursor_created_at)
            .bind(cursor_id)
            .bind(fetch_size)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list web_deployment cursor", error))?;
        let has_more = rows.len() > page_size as usize;
        let page_rows = rows.into_iter().take(page_size as usize).collect::<Vec<_>>();
        let mut items = Vec::with_capacity(page_rows.len());
        for row in &page_rows {
            items.push(map_deployment_row(row, site_id).map_err(|error| {
                WebServiceError::Internal(format!("map web_deployment row: {error}"))
            })?);
        }
        let next_cursor = has_more
            .then(|| {
                let last = page_rows.last().expect("non-empty page when has_more");
                let created_at: String = last
                    .try_get("created_at")
                    .map_err(|error| store_error("map web_deployment cursor instant", error))?;
                let id: i64 = last
                    .try_get("id")
                    .map_err(|error| store_error("map web_deployment cursor id", error))?;
                Ok::<_, WebServiceError>(encode_keyset_cursor(&created_at, id))
            })
            .transpose()?;
        Ok(DeploymentPage {
            items,
            total: 0,
            page: 0,
            page_size,
            next_cursor,
            has_more: Some(has_more),
        })
    }

    pub(super) async fn create_deployment_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        actor_id: Option<i64>,
        request: &CreateDeploymentRequest,
    ) -> WebServiceResult<DeploymentResponse> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;
        let environment = request
            .environment
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("production");
        let source_version = if let Some(source_version_id) = request.source_version_id.as_deref() {
            let row = sqlx::query(
                "SELECT id, version_tag, commit_hash, source_ref, artifact_path,
                        artifact_size, artifact_hash, status
                 FROM web_source_version
                 WHERE tenant_id = $1 AND site_id = $2 AND uuid = $3",
            )
            .bind(tenant_id)
            .bind(site_internal_id)
            .bind(source_version_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("resolve deployment source version", error))?
            .ok_or_else(|| WebServiceError::not_found("source version not found"))?;
            let status: i32 = row
                .try_get("status")
                .map_err(|error| store_error("map deployment source version status", error))?;
            if status != 1 {
                return Err(WebServiceError::conflict(
                    "source version is not ready or is outside the retained release window",
                ));
            }
            Some(SourceVersionDeploymentSnapshot {
                internal_id: row
                    .try_get("id")
                    .map_err(|error| store_error("map deployment source version id", error))?,
                version_tag: row
                    .try_get("version_tag")
                    .map_err(|error| store_error("map deployment source version tag", error))?,
                commit_hash: row.try_get("commit_hash").map_err(|error| {
                    store_error("map deployment source version commit hash", error)
                })?,
                source_ref: row.try_get("source_ref").map_err(|error| {
                    store_error("map deployment source version source ref", error)
                })?,
                artifact_drive_uri: row.try_get("artifact_path").map_err(|error| {
                    store_error("map deployment source version artifact path", error)
                })?,
                artifact_size: row.try_get("artifact_size").map_err(|error| {
                    store_error("map deployment source version artifact size", error)
                })?,
                artifact_hash: row.try_get("artifact_hash").map_err(|error| {
                    store_error("map deployment source version artifact hash", error)
                })?,
            })
        } else {
            None
        };
        let source_version_internal_id = source_version
            .as_ref()
            .map(|source_version| source_version.internal_id);
        let version_tag = normalized_optional(request.version_tag.as_deref()).or_else(|| {
            source_version
                .as_ref()
                .map(|source_version| source_version.version_tag.as_str())
        });
        let commit_hash = source_version.as_ref().map_or_else(
            || normalized_optional(request.commit_hash.as_deref()),
            |source_version| source_version.commit_hash.as_deref(),
        );
        let source_ref = source_version.as_ref().map_or_else(
            || normalized_optional(request.source_ref.as_deref()),
            |source_version| source_version.source_ref.as_deref(),
        );
        let artifact_drive_uri = source_version.as_ref().map_or_else(
            || normalized_optional(request.artifact_drive_uri.as_deref()),
            |source_version| Some(source_version.artifact_drive_uri.as_str()),
        );
        let artifact_size = source_version
            .as_ref()
            .map_or(request.artifact_size, |source_version| {
                Some(source_version.artifact_size)
            });
        let artifact_hash = source_version.as_ref().map_or_else(
            || normalized_optional(request.artifact_hash.as_deref()),
            |source_version| Some(source_version.artifact_hash.as_str()),
        );

        // 幂等性：如果客户端提供了非空 idempotency_key，
        // 先查找是否已存在相同 (tenant_id, idempotency_key) 的 deployment。
        // 存在则直接返回已创建的记录，保证网络重试不会产生重复部署。
        let idempotency_key_hash = deployment_idempotency_key_hash(
            "create",
            actor_id,
            site_id,
            None,
            request.idempotency_key.as_deref(),
        )?;
        let idempotency_key = idempotency_key_hash.as_deref();
        let idempotency_lookup = idempotency_key.map(|key| DeploymentIdempotencyLookup {
            tenant_id,
            site_internal_id,
            site_id,
            source_version_internal_id,
            deploy_type: request.deploy_type,
            environment,
            version_tag,
            commit_hash,
            source_ref,
            artifact_drive_uri,
            artifact_size,
            artifact_hash,
            rollback_from_internal_id: None,
            idempotency_key: key,
        });
        if let Some(lookup) = idempotency_lookup.as_ref() {
            if let Some(existing) = self.find_deployment_by_idempotency_repo(lookup).await? {
                return Ok(existing);
            }
        }

        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();

        let now_expression = instant_write_expression("$16");
        let insert_sql = format!(
            "INSERT INTO web_deployment (
                id, uuid, tenant_id, organization_id, user_id, site_id, source_version_id,
                deploy_type, environment, version_tag,
                commit_hash, source_ref, artifact_path, artifact_size, artifact_hash, status,
                idempotency_key, metadata,
                created_at, updated_at, version
             ) VALUES (
                $1, $2, $3,
                COALESCE((SELECT organization_id FROM web_site WHERE tenant_id = $3 AND id = $5), 0),
                $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, 0, $15, '{{}}',
                {now_expression}, {now_expression}, 0
             )"
        );
        let insert_result = sqlx::query(audited_sql(&insert_sql))
            .bind(id)
            .bind(&uuid)
            .bind(tenant_id)
            .bind(actor_id)
            .bind(site_internal_id)
            .bind(source_version_internal_id)
            .bind(request.deploy_type)
            .bind(environment)
            .bind(version_tag)
            .bind(commit_hash)
            .bind(source_ref)
            .bind(artifact_drive_uri)
            .bind(artifact_size)
            .bind(artifact_hash)
            .bind(idempotency_key)
            .bind(&now)
            .execute(&self.pool)
            .await;

        if let Err(error) = insert_result {
            if is_unique_violation(&error) {
                let Some(lookup) = idempotency_lookup.as_ref() else {
                    return Err(store_error("insert web_deployment", error));
                };
                if let Some(existing) = self.find_deployment_by_idempotency_repo(lookup).await? {
                    return Ok(existing);
                }
            }
            return Err(store_error("insert web_deployment", error));
        }

        self.retrieve_deployment_repo(tenant_id, site_id, &uuid)
            .await
    }

    /// 通过 (tenant_id, site_id, idempotency_key) 查找已存在的 deployment。
    /// 用于 create_deployment 的幂等性检查。
    async fn find_deployment_by_idempotency_repo(
        &self,
        lookup: &DeploymentIdempotencyLookup<'_>,
    ) -> WebServiceResult<Option<DeploymentResponse>> {
        let row = sqlx::query(
            "SELECT deployment.uuid, deployment.site_id, deployment.status,
                    deployment.deploy_type, deployment.environment, deployment.version_tag,
                    deployment.commit_hash, deployment.source_ref, deployment.artifact_path,
                    deployment.artifact_size, deployment.artifact_hash,
                    deployment.source_version_id AS source_version_internal_id,
                    deployment.rollback_from,
                    source_version.uuid AS source_version_id,
                    source.uuid AS rollback_from_deployment_id,
                    CAST(deployment.started_at AS TEXT) AS started_at,
                    CAST(deployment.completed_at AS TEXT) AS completed_at,
                    deployment.duration_ms,
                    CAST(deployment.created_at AS TEXT) AS created_at
             FROM web_deployment deployment
             LEFT JOIN web_deployment source
               ON source.id = deployment.rollback_from
              AND source.tenant_id = deployment.tenant_id
              AND source.site_id = deployment.site_id
             LEFT JOIN web_source_version source_version
               ON source_version.id = deployment.source_version_id
              AND source_version.tenant_id = deployment.tenant_id
              AND source_version.site_id = deployment.site_id
             WHERE deployment.tenant_id = $1 AND deployment.idempotency_key = $2",
        )
        .bind(lookup.tenant_id)
        .bind(lookup.idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("find web_deployment by idempotency_key", error))?;

        let Some(row) = row else {
            return Ok(None);
        };
        let existing_site_internal_id: i64 = row
            .try_get("site_id")
            .map_err(|error| store_error("map idempotent deployment site_id", error))?;
        let existing_deploy_type: i32 = row
            .try_get("deploy_type")
            .map_err(|error| store_error("map idempotent deployment deploy_type", error))?;
        let existing_environment: String = row
            .try_get("environment")
            .map_err(|error| store_error("map idempotent deployment environment", error))?;
        let existing_version_tag: Option<String> = row
            .try_get("version_tag")
            .map_err(|error| store_error("map idempotent deployment version_tag", error))?;
        let existing_commit_hash: Option<String> = row
            .try_get("commit_hash")
            .map_err(|error| store_error("map idempotent deployment commit_hash", error))?;
        let existing_source_ref: Option<String> = row
            .try_get("source_ref")
            .map_err(|error| store_error("map idempotent deployment source_ref", error))?;
        let existing_artifact_drive_uri: Option<String> = row
            .try_get("artifact_path")
            .map_err(|error| store_error("map idempotent deployment artifact_path", error))?;
        let existing_artifact_size: Option<i64> = row
            .try_get("artifact_size")
            .map_err(|error| store_error("map idempotent deployment artifact_size", error))?;
        let existing_artifact_hash: Option<String> = row
            .try_get("artifact_hash")
            .map_err(|error| store_error("map idempotent deployment artifact_hash", error))?;
        let existing_rollback_from_internal_id: Option<i64> = row
            .try_get("rollback_from")
            .map_err(|error| store_error("map idempotent deployment rollback_from", error))?;
        let existing_source_version_internal_id: Option<i64> = row
            .try_get("source_version_internal_id")
            .map_err(|error| store_error("map idempotent deployment source_version_id", error))?;
        if existing_site_internal_id != lookup.site_internal_id
            || existing_source_version_internal_id != lookup.source_version_internal_id
            || existing_deploy_type != lookup.deploy_type
            || existing_environment != lookup.environment
            || existing_version_tag.as_deref() != lookup.version_tag
            || existing_commit_hash.as_deref() != lookup.commit_hash
            || existing_source_ref.as_deref() != lookup.source_ref
            || existing_artifact_drive_uri.as_deref() != lookup.artifact_drive_uri
            || existing_artifact_size != lookup.artifact_size
            || existing_artifact_hash.as_deref() != lookup.artifact_hash
            || existing_rollback_from_internal_id != lookup.rollback_from_internal_id
        {
            return Err(WebServiceError::conflict(
                "idempotency key was already used with different deployment input",
            ));
        }

        map_deployment_row(&row, lookup.site_id)
            .map(Some)
            .map_err(|error| WebServiceError::Internal(format!("map web_deployment row: {error}")))
    }

    pub(super) async fn retrieve_deployment_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        deployment_id: &str,
    ) -> WebServiceResult<DeploymentResponse> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;
        let row = sqlx::query(
            "SELECT deployment.uuid, deployment.site_id, deployment.status,
                    deployment.deploy_type, deployment.environment, deployment.version_tag,
                    deployment.commit_hash, deployment.source_ref, deployment.artifact_path,
                    deployment.artifact_size, deployment.artifact_hash,
                    source_version.uuid AS source_version_id,
                    source.uuid AS rollback_from_deployment_id,
                    CAST(deployment.started_at AS TEXT) AS started_at,
                    CAST(deployment.completed_at AS TEXT) AS completed_at,
                    deployment.duration_ms,
                    CAST(deployment.created_at AS TEXT) AS created_at
             FROM web_deployment deployment
             LEFT JOIN web_deployment source
               ON source.id = deployment.rollback_from
              AND source.tenant_id = deployment.tenant_id
              AND source.site_id = deployment.site_id
             LEFT JOIN web_source_version source_version
               ON source_version.id = deployment.source_version_id
              AND source_version.tenant_id = deployment.tenant_id
              AND source_version.site_id = deployment.site_id
             WHERE deployment.tenant_id = $1
               AND deployment.site_id = $2
               AND deployment.uuid = $3",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .bind(deployment_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve web_deployment", error))?
        .ok_or_else(|| WebServiceError::not_found("deployment not found"))?;

        map_deployment_row(&row, site_id)
            .map_err(|error| WebServiceError::Internal(error.to_string()))
    }

    pub(super) async fn rollback_deployment_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        deployment_id: &str,
        actor_id: Option<i64>,
        idempotency_key: Option<&str>,
    ) -> WebServiceResult<DeploymentResponse> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;
        let source = sqlx::query(
            "SELECT deployment.id, deployment.status, deployment.deploy_type,
                    deployment.environment, deployment.version_tag, deployment.commit_hash,
                    deployment.source_ref, deployment.artifact_path, deployment.artifact_size,
                    deployment.artifact_hash, deployment.source_version_id,
                    source_version.status AS source_version_status
             FROM web_deployment deployment
             LEFT JOIN web_source_version source_version
               ON source_version.id = deployment.source_version_id
              AND source_version.tenant_id = deployment.tenant_id
              AND source_version.site_id = deployment.site_id
             WHERE deployment.tenant_id = $1 AND deployment.site_id = $2 AND deployment.uuid = $3",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .bind(deployment_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("rollback web_deployment lookup", error))?
        .ok_or_else(|| WebServiceError::not_found("deployment not found"))?;

        let source_id: i64 = source
            .try_get("id")
            .map_err(|error| store_error("rollback web_deployment source id", error))?;
        let source_status: i32 = source
            .try_get("status")
            .map_err(|error| store_error("rollback web_deployment source status", error))?;
        if source_status != 2 {
            return Err(WebServiceError::conflict(
                "only a successful deployment can be rolled back",
            ));
        }
        let deploy_type: i32 = source
            .try_get("deploy_type")
            .map_err(|error| store_error("rollback web_deployment deploy_type", error))?;
        let environment: String = source
            .try_get("environment")
            .map_err(|error| store_error("rollback web_deployment environment", error))?;
        let version_tag: Option<String> = source
            .try_get("version_tag")
            .map_err(|error| store_error("rollback web_deployment version_tag", error))?;
        let commit_hash: Option<String> = source
            .try_get("commit_hash")
            .map_err(|error| store_error("rollback web_deployment commit_hash", error))?;
        let source_ref: Option<String> = source
            .try_get("source_ref")
            .map_err(|error| store_error("rollback web_deployment source_ref", error))?;
        let artifact_drive_uri: Option<String> = source
            .try_get("artifact_path")
            .map_err(|error| store_error("rollback web_deployment artifact_path", error))?;
        let artifact_size: Option<i64> = source
            .try_get("artifact_size")
            .map_err(|error| store_error("rollback web_deployment artifact_size", error))?;
        let artifact_hash: Option<String> = source
            .try_get("artifact_hash")
            .map_err(|error| store_error("rollback web_deployment artifact_hash", error))?;
        let source_version_internal_id: Option<i64> = source
            .try_get("source_version_id")
            .map_err(|error| store_error("rollback web_deployment source_version_id", error))?;
        let source_version_status: Option<i32> = source
            .try_get("source_version_status")
            .map_err(|error| store_error("rollback web_deployment source version status", error))?;
        if source_version_internal_id.is_some() && source_version_status != Some(1) {
            return Err(WebServiceError::conflict(
                "the source version is outside the retained release window and cannot be rolled back",
            ));
        }
        let idempotency_key_hash = deployment_idempotency_key_hash(
            "rollback",
            actor_id,
            site_id,
            Some(deployment_id),
            idempotency_key,
        )?;
        let idempotency_key = idempotency_key_hash.as_deref();
        let idempotency_lookup = idempotency_key.map(|key| DeploymentIdempotencyLookup {
            tenant_id,
            site_internal_id,
            site_id,
            source_version_internal_id,
            deploy_type,
            environment: &environment,
            version_tag: version_tag.as_deref(),
            commit_hash: commit_hash.as_deref(),
            source_ref: source_ref.as_deref(),
            artifact_drive_uri: artifact_drive_uri.as_deref(),
            artifact_size,
            artifact_hash: artifact_hash.as_deref(),
            rollback_from_internal_id: Some(source_id),
            idempotency_key: key,
        });
        if let Some(lookup) = idempotency_lookup.as_ref() {
            if let Some(existing) = self.find_deployment_by_idempotency_repo(lookup).await? {
                return Ok(existing);
            }
        }

        let now = now_rfc3339();
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();

        let rollback_insert_time = instant_write_expression("$17");
        let insert_sql = format!(
            "INSERT INTO web_deployment (
                id, uuid, tenant_id, organization_id, user_id, site_id, source_version_id,
                deploy_type, environment, version_tag,
                commit_hash, source_ref, artifact_path, artifact_size, artifact_hash, status,
                rollback_from, idempotency_key, metadata,
                created_at, updated_at, version
             ) VALUES (
                $1, $2, $3,
                COALESCE((SELECT organization_id FROM web_site WHERE tenant_id = $3 AND id = $5), 0),
                $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, 0, $15, $16, '{{}}',
                {rollback_insert_time}, {rollback_insert_time}, 0
             )"
        );

        // Keep the immutable source untouched; this transaction only creates a restore command.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin rollback web_deployment transaction", error))?;

        let insert_result = sqlx::query(audited_sql(&insert_sql))
            .bind(id)
            .bind(&uuid)
            .bind(tenant_id)
            .bind(actor_id)
            .bind(site_internal_id)
            .bind(source_version_internal_id)
            .bind(deploy_type)
            .bind(&environment)
            .bind(&version_tag)
            .bind(&commit_hash)
            .bind(&source_ref)
            .bind(&artifact_drive_uri)
            .bind(artifact_size)
            .bind(&artifact_hash)
            .bind(source_id)
            .bind(idempotency_key)
            .bind(&now)
            .execute(&mut *tx)
            .await;

        if let Err(error) = insert_result {
            tx.rollback().await.map_err(|rollback_error| {
                store_error("abort restore web_deployment transaction", rollback_error)
            })?;
            if is_unique_violation(&error) {
                let Some(lookup) = idempotency_lookup.as_ref() else {
                    return Err(store_error("insert restore web_deployment", error));
                };
                if let Some(existing) = self.find_deployment_by_idempotency_repo(lookup).await? {
                    return Ok(existing);
                }
            }
            return Err(store_error("insert restore web_deployment", error));
        }

        tx.commit()
            .await
            .map_err(|error| store_error("commit restore web_deployment transaction", error))?;

        self.retrieve_deployment_repo(tenant_id, site_id, &uuid)
            .await
    }
}

fn map_deployment_row(row: &EngineRow, site_id: &str) -> Result<DeploymentResponse, sqlx::Error> {
    Ok(DeploymentResponse {
        id: row.try_get("uuid")?,
        application_id: site_id.to_owned(),
        status: row.try_get("status")?,
        deploy_type: row.try_get("deploy_type")?,
        source_version_id: row.try_get("source_version_id")?,
        environment: row.try_get("environment")?,
        version_tag: row.try_get("version_tag")?,
        commit_hash: row.try_get("commit_hash")?,
        source_ref: row.try_get("source_ref")?,
        rollback_from_deployment_id: row.try_get("rollback_from_deployment_id")?,
        artifact_drive_uri: row.try_get("artifact_path")?,
        artifact_size: row.try_get("artifact_size")?,
        artifact_hash: row.try_get("artifact_hash")?,
        started_at: optional_instant_from_row(row, "started_at")?,
        completed_at: optional_instant_from_row(row, "completed_at")?,
        duration_ms: row.try_get("duration_ms")?,
        created_at: instant_from_row(row, "created_at")?,
    })
}

fn normalized_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn deployment_idempotency_key_hash(
    operation: &str,
    actor_id: Option<i64>,
    site_id: &str,
    deployment_id: Option<&str>,
    raw_key: Option<&str>,
) -> WebServiceResult<Option<String>> {
    let Some(raw_key) = raw_key else {
        return Ok(None);
    };
    if raw_key != raw_key.trim() || !(1..=128).contains(&raw_key.len()) {
        return Err(WebServiceError::validation(
            "idempotency key must contain between 1 and 128 bytes without surrounding whitespace",
        ));
    }
    let deployment_id = deployment_id.unwrap_or("");
    let scope = format!(
        "v1:{}:{operation}:{}:{}:{site_id}:{}:{deployment_id}:{}:{raw_key}",
        operation.len(),
        actor_id.unwrap_or_default(),
        site_id.len(),
        deployment_id.len(),
        raw_key.len(),
    );
    Ok(Some(sha256_hex(&scope)))
}
