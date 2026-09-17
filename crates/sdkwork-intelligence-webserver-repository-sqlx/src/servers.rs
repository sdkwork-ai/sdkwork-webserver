use crate::audited_sql;
use super::{EngineRow, WebRepository};
use sdkwork_webserver_contract::{
    CreateServerRequest, CreateServerResponse, ServerPage, ServerResponse, WebServiceError,
    WebServiceResult,
};
use serde_json::json;
use sqlx::Row;

use super::agents::{generate_agent_token, hash_agent_token, parse_last_heartbeat_at};
use super::support::{
    cursor_instant_from_row, decode_keyset_cursor, encode_keyset_cursor, instant_from_row,
    instant_write_expression, json_from_row, json_write_expression, new_uuid, next_id, now_rfc3339,
    pagination, store_error,
};

impl WebRepository {
    pub(super) async fn list_servers_repo(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ServerPage> {
        if let Some(cursor) = cursor {
            return self.list_servers_cursor_repo(tenant_id, page_size, cursor).await;
        }
        // Offset mode remains only for internal single-page lookups (page 1).
        // Deep OFFSET on this growing node collection is rejected; clients
        // must use cursor pagination (PRD-FR-011, PAGINATION_SPEC §6/§12).
        if page > 1 {
            return Err(WebServiceError::validation(
                "cursor is required beyond the first page of servers; offset pagination is not supported on this growing collection",
            ));
        }
        let (_page, page_size, offset) = pagination(page, page_size)?;

        let count_row =
            sqlx::query("SELECT COUNT(*) AS total FROM web_server WHERE tenant_id = $1")
                .bind(tenant_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|error| store_error("count web_server", error))?;
        let total: i64 = count_row
            .try_get("total")
            .map_err(|error| store_error("map web_server count", error))?;

        let rows = sqlx::query(
            "SELECT uuid, name, host, tenant_scope_hash, ssh_port, status,
                    CAST(metadata AS TEXT) AS metadata,
                    CAST(updated_at AS TEXT) AS updated_at,
                    CAST(created_at AS TEXT) AS created_at
             FROM web_server
             WHERE tenant_id = $1
             ORDER BY updated_at DESC, id DESC LIMIT $2 OFFSET $3",
        )
        .bind(tenant_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list web_server", error))?;

        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            items.push(map_server_row(row).map_err(|error| {
                WebServiceError::Internal(format!("map web_server row: {error}"))
            })?);
        }

        Ok(ServerPage {
            items,
            total,
            next_cursor: None,
            has_more: None,
        })
    }

    /// Keyset page over `(updated_at DESC, id DESC)` with an opaque cursor;
    /// fetches `page_size + 1` rows so `has_more` is exact and no COUNT runs.
    async fn list_servers_cursor_repo(
        &self,
        tenant_id: i64,
        page_size: i32,
        cursor: &str,
    ) -> WebServiceResult<ServerPage> {
        if !(1..=200).contains(&page_size) {
            return Err(WebServiceError::validation(
                "page_size must be between 1 and 200",
            ));
        }
        let (cursor_updated_at, cursor_id) = decode_keyset_cursor(cursor)
            .ok_or_else(|| WebServiceError::validation("cursor is invalid"))?;
        let sql = "SELECT uuid, name, host, tenant_scope_hash, ssh_port, status,
                    CAST(metadata AS TEXT) AS metadata,
                    CAST(updated_at AS TEXT) AS updated_at,
                    CAST(created_at AS TEXT) AS created_at
             FROM web_server
             WHERE tenant_id = $1
               AND (updated_at, id) < (CAST($2 AS TIMESTAMPTZ), $3)
             ORDER BY updated_at DESC, id DESC LIMIT $4".to_string();
        let fetch_size = i64::from(page_size) + 1;
        let rows = sqlx::query(audited_sql(&sql))
            .bind(tenant_id)
            .bind(&cursor_updated_at)
            .bind(cursor_id)
            .bind(fetch_size)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list web_server cursor", error))?;
        let has_more = rows.len() > page_size as usize;
        let page_rows = rows.into_iter().take(page_size as usize).collect::<Vec<_>>();
        let mut items = Vec::with_capacity(page_rows.len());
        for row in &page_rows {
            items.push(map_server_row(row).map_err(|error| {
                WebServiceError::Internal(format!("map web_server row: {error}"))
            })?);
        }
        let next_cursor = has_more
            .then(|| {
                let last = page_rows.last().expect("non-empty page when has_more");
                let updated_at = cursor_instant_from_row(last, "updated_at")
                    .map_err(|error| store_error("map web_server cursor instant", error))?;
                let id: i64 = last
                    .try_get("id")
                    .map_err(|error| store_error("map web_server cursor id", error))?;
                Ok::<_, WebServiceError>(encode_keyset_cursor(&updated_at, id))
            })
            .transpose()?;
        Ok(ServerPage {
            items,
            total: 0,
            next_cursor,
            has_more: Some(has_more),
        })
    }

    pub(super) async fn create_server_repo(
        &self,
        tenant_id: i64,
        request: &CreateServerRequest,
    ) -> WebServiceResult<CreateServerResponse> {
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();
        let agent_token = generate_agent_token();
        let metadata = json!({
            "agentTokenHash": hash_agent_token(&agent_token),
        });

        let metadata_expression = json_write_expression("$8");
        let now_expression = instant_write_expression("$9");
        let insert_sql = format!(
            "INSERT INTO web_server (
                id, uuid, tenant_id, name, host, tenant_scope_hash, ssh_port, status, metadata,
                created_at, updated_at, version
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, 0, {metadata_expression},
                {now_expression}, {now_expression}, 0
             )"
        );

        sqlx::query(audited_sql(&insert_sql))
            .bind(id)
            .bind(&uuid)
            .bind(tenant_id)
            .bind(&request.name)
            .bind(&request.host)
            .bind(&request.tenant_scope_hash)
            .bind(request.ssh_port)
            .bind(metadata.to_string())
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("insert web_server", error))?;

        Ok(CreateServerResponse {
            server: ServerResponse {
                id: uuid,
                name: request.name.clone(),
                host: request.host.clone(),
                tenant_scope_hash: request.tenant_scope_hash.clone(),
                ssh_port: request.ssh_port,
                status: 0,
                last_heartbeat_at: None,
                created_at: now,
            },
            agent_token,
        })
    }
}

fn map_server_row(row: &EngineRow) -> Result<ServerResponse, sqlx::Error> {
    let metadata_raw = json_from_row(row, "metadata")?
        .unwrap_or_else(|| json!({}))
        .to_string();
    Ok(ServerResponse {
        id: row.try_get("uuid")?,
        name: row.try_get("name")?,
        host: row.try_get("host")?,
        tenant_scope_hash: row.try_get("tenant_scope_hash")?,
        ssh_port: row.try_get("ssh_port")?,
        status: row.try_get("status")?,
        last_heartbeat_at: parse_last_heartbeat_at(&metadata_raw),
        created_at: instant_from_row(row, "created_at")?,
    })
}
