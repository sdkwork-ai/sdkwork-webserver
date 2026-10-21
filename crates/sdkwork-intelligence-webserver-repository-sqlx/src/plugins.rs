use sdkwork_webserver_contract::{
    CreatePluginRequest, PluginPage, PluginResponse, UpdatePluginRequest, WebServiceError,
    WebServiceResult,
};
use serde_json::Value;
use sqlx::Row;

use super::support::{
    instant_write_expression, new_uuid, next_id, now_rfc3339, store_error,
};
use super::WebRepository;
use crate::audited_sql;

const MAX_PLUGIN_PAGE_SIZE: i32 = 200;
const PLUGIN_COLUMNS: &str = "uuid, plugin_key, display_name, summary, plugin_version, category_id,
     supported_host_tools_json, contributed_capabilities_json, source_kind, git_repository, git_ref,
     artifact_ref, checksum_sha256, archive_file_name, plugin_status,
     CAST(created_at AS TEXT) AS created_at, CAST(updated_at AS TEXT) AS updated_at";

fn strings_from_json(value: Value, field: &str) -> WebServiceResult<Vec<String>> {
    serde_json::from_value(value)
        .map_err(|error| WebServiceError::Internal(format!("decode plugin {field}: {error}")))
}

fn map_plugin_row(row: &sqlx::postgres::PgRow) -> WebServiceResult<PluginResponse> {
    Ok(PluginResponse {
        id: row
            .try_get("uuid")
            .map_err(|error| store_error("map webserver_plugin uuid", error))?,
        plugin_key: row
            .try_get("plugin_key")
            .map_err(|error| store_error("map webserver_plugin plugin_key", error))?,
        display_name: row
            .try_get("display_name")
            .map_err(|error| store_error("map webserver_plugin display_name", error))?,
        summary: row
            .try_get("summary")
            .map_err(|error| store_error("map webserver_plugin summary", error))?,
        version: row
            .try_get("plugin_version")
            .map_err(|error| store_error("map webserver_plugin plugin_version", error))?,
        category_id: row
            .try_get("category_id")
            .map_err(|error| store_error("map webserver_plugin category_id", error))?,
        supported_host_tools: strings_from_json(
            row.try_get("supported_host_tools_json")
                .map_err(|error| store_error("map webserver_plugin host tools", error))?,
            "supportedHostTools",
        )?,
        contributed_capabilities: strings_from_json(
            row.try_get("contributed_capabilities_json")
                .map_err(|error| store_error("map webserver_plugin capabilities", error))?,
            "contributedCapabilities",
        )?,
        source_kind: row
            .try_get("source_kind")
            .map_err(|error| store_error("map webserver_plugin source_kind", error))?,
        git_repository: row
            .try_get("git_repository")
            .map_err(|error| store_error("map webserver_plugin git_repository", error))?,
        git_ref: row
            .try_get("git_ref")
            .map_err(|error| store_error("map webserver_plugin git_ref", error))?,
        artifact_ref: row
            .try_get("artifact_ref")
            .map_err(|error| store_error("map webserver_plugin artifact_ref", error))?,
        checksum_sha256: row
            .try_get("checksum_sha256")
            .map_err(|error| store_error("map webserver_plugin checksum_sha256", error))?,
        archive_file_name: row
            .try_get("archive_file_name")
            .map_err(|error| store_error("map webserver_plugin archive_file_name", error))?,
        status: row
            .try_get("plugin_status")
            .map_err(|error| store_error("map webserver_plugin plugin_status", error))?,
        created_at: row
            .try_get("created_at")
            .map_err(|error| store_error("map webserver_plugin created_at", error))?,
        updated_at: row
            .try_get("updated_at")
            .map_err(|error| store_error("map webserver_plugin updated_at", error))?,
    })
}

fn normalize_status(status: Option<&str>) -> WebServiceResult<String> {
    match status.unwrap_or("draft") {
        "draft" => Ok("draft".to_string()),
        "active" => Ok("active".to_string()),
        other => Err(WebServiceError::validation(format!(
            "plugin status must be draft or active, got {other}"
        ))),
    }
}

impl WebRepository {
    pub(super) async fn create_plugin_repo(
        &self,
        tenant_id: i64,
        user_id: i64,
        request: &CreatePluginRequest,
    ) -> WebServiceResult<PluginResponse> {
        if request.plugin_key.trim().is_empty() || request.display_name.trim().is_empty() {
            return Err(WebServiceError::validation(
                "pluginKey and displayName are required",
            ));
        }
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();
        let status = normalize_status(request.status.as_deref())?;
        let host_tools = serde_json::to_value(&request.supported_host_tools)
            .map_err(|error| WebServiceError::Internal(format!("encode host tools: {error}")))?;
        let capabilities = serde_json::to_value(&request.contributed_capabilities)
            .map_err(|error| WebServiceError::Internal(format!("encode capabilities: {error}")))?;
        let insert_sql = format!(
            "INSERT INTO webserver_plugin (
                id, uuid, tenant_id, organization_id, data_scope, user_id, plugin_key,
                display_name, summary, plugin_version, category_id, supported_host_tools_json,
                contributed_capabilities_json, source_kind, git_repository, git_ref, artifact_ref,
                checksum_sha256, archive_file_name, plugin_status, created_at, updated_at, version
             ) VALUES (
                $1, $2, $3, 0, 3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16,
                $17, $18, {now_expression}, {now_expression}, 0
             )
             RETURNING {PLUGIN_COLUMNS}",
            now_expression = instant_write_expression("$19")
        );
        let row = sqlx::query(audited_sql(&insert_sql))
            .bind(id)
            .bind(&uuid)
            .bind(tenant_id)
            .bind(user_id)
            .bind(&request.plugin_key)
            .bind(&request.display_name)
            .bind(&request.summary)
            .bind(&request.version)
            .bind(&request.category_id)
            .bind(host_tools)
            .bind(capabilities)
            .bind(&request.source_kind)
            .bind(&request.git_repository)
            .bind(&request.git_ref)
            .bind(&request.artifact_ref)
            .bind(&request.checksum_sha256)
            .bind(&request.archive_file_name)
            .bind(&status)
            .bind(&now)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("create webserver_plugin", error))?;
        map_plugin_row(&row)
    }

    pub(super) async fn list_plugins_repo(
        &self,
        tenant_id: i64,
        user_id: i64,
        page: i32,
        page_size: i32,
        status: Option<&str>,
        keyword: Option<&str>,
    ) -> WebServiceResult<PluginPage> {
        let page = page.max(1);
        let page_size = page_size.clamp(1, MAX_PLUGIN_PAGE_SIZE);
        let status = status.map(|value| value.trim().to_ascii_lowercase());
        let keyword = keyword
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM webserver_plugin
             WHERE tenant_id = $1 AND user_id = $2 AND deleted_at IS NULL
               AND ($3::text IS NULL OR plugin_status = $3)
               AND ($4::text IS NULL OR display_name ILIKE '%' || $4 || '%'
                    OR plugin_key ILIKE '%' || $4 || '%')",
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(status.as_deref())
        .bind(keyword.as_deref())
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count webserver_plugin", error))?;
        let total: i64 = count_row
            .try_get("total")
            .map_err(|error| store_error("map webserver_plugin count", error))?;
        let offset = i64::from(page - 1) * i64::from(page_size);
        let rows = sqlx::query(audited_sql(&format!(
            "SELECT {PLUGIN_COLUMNS} FROM webserver_plugin
             WHERE tenant_id = $1 AND user_id = $2 AND deleted_at IS NULL
               AND ($3::text IS NULL OR plugin_status = $3)
               AND ($4::text IS NULL OR display_name ILIKE '%' || $4 || '%'
                    OR plugin_key ILIKE '%' || $4 || '%')
             ORDER BY updated_at DESC, id DESC
             LIMIT $5 OFFSET $6"
        )))
        .bind(tenant_id)
        .bind(user_id)
        .bind(status.as_deref())
        .bind(keyword.as_deref())
        .bind(i64::from(page_size))
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list webserver_plugin", error))?;
        let items = rows
            .iter()
            .map(map_plugin_row)
            .collect::<WebServiceResult<Vec<_>>>()?;
        Ok(PluginPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub(super) async fn retrieve_plugin_repo(
        &self,
        tenant_id: i64,
        user_id: i64,
        plugin_id: &str,
    ) -> WebServiceResult<PluginResponse> {
        let row = sqlx::query(audited_sql(&format!(
            "SELECT {PLUGIN_COLUMNS} FROM webserver_plugin
             WHERE tenant_id = $1 AND user_id = $2 AND uuid = $3 AND deleted_at IS NULL"
        )))
        .bind(tenant_id)
        .bind(user_id)
        .bind(plugin_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve webserver_plugin", error))?;
        match row {
            Some(row) => map_plugin_row(&row),
            None => Err(WebServiceError::not_found("plugin not found")),
        }
    }

    pub(super) async fn update_plugin_repo(
        &self,
        tenant_id: i64,
        user_id: i64,
        plugin_id: &str,
        request: &UpdatePluginRequest,
    ) -> WebServiceResult<PluginResponse> {
        let current = self
            .retrieve_plugin_repo(tenant_id, user_id, plugin_id)
            .await?;
        let status = match request.status.as_deref() {
            Some(value) => normalize_status(Some(value))?,
            None => current.status.clone(),
        };
        let host_tools = request
            .supported_host_tools
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|error| WebServiceError::Internal(format!("encode host tools: {error}")))?
            .unwrap_or_else(|| serde_json::to_value(&current.supported_host_tools).unwrap_or_default());
        let capabilities = request
            .contributed_capabilities
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|error| WebServiceError::Internal(format!("encode capabilities: {error}")))?
            .unwrap_or_else(|| {
                serde_json::to_value(&current.contributed_capabilities).unwrap_or_default()
            });
        let now = now_rfc3339();
        let row = sqlx::query(audited_sql(&format!(
            "UPDATE webserver_plugin SET
                display_name = $4, summary = $5, plugin_version = $6, category_id = $7,
                supported_host_tools_json = $8, contributed_capabilities_json = $9,
                git_repository = $10, git_ref = $11, artifact_ref = $12, checksum_sha256 = $13,
                archive_file_name = $14, plugin_status = $15,
                updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND user_id = $2 AND uuid = $3 AND deleted_at IS NULL
             RETURNING {PLUGIN_COLUMNS}",
            now_expression = instant_write_expression("$16")
        )))
        .bind(tenant_id)
        .bind(user_id)
        .bind(plugin_id)
        .bind(request.display_name.as_deref().unwrap_or(&current.display_name))
        .bind(request.summary.as_deref().unwrap_or(&current.summary))
        .bind(request.version.as_deref().unwrap_or(&current.version))
        .bind(request.category_id.as_deref().unwrap_or(&current.category_id))
        .bind(host_tools)
        .bind(capabilities)
        .bind(
            request
                .git_repository
                .as_deref()
                .or(current.git_repository.as_deref()),
        )
        .bind(request.git_ref.as_deref().or(current.git_ref.as_deref()))
        .bind(
            request
                .artifact_ref
                .as_deref()
                .or(current.artifact_ref.as_deref()),
        )
        .bind(
            request
                .checksum_sha256
                .as_deref()
                .or(current.checksum_sha256.as_deref()),
        )
        .bind(
            request
                .archive_file_name
                .as_deref()
                .or(current.archive_file_name.as_deref()),
        )
        .bind(&status)
        .bind(&now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("update webserver_plugin", error))?;
        match row {
            Some(row) => map_plugin_row(&row),
            None => Err(WebServiceError::not_found("plugin not found")),
        }
    }

    pub(super) async fn delete_plugin_repo(
        &self,
        tenant_id: i64,
        user_id: i64,
        plugin_id: &str,
    ) -> WebServiceResult<()> {
        let now = now_rfc3339();
        let result = sqlx::query(audited_sql(&format!(
            "UPDATE webserver_plugin SET
                deleted_at = {now_expression}, deleted_by = $4, updated_at = {now_expression},
                version = version + 1
             WHERE tenant_id = $1 AND user_id = $2 AND uuid = $3 AND deleted_at IS NULL",
            now_expression = instant_write_expression("$5")
        )))
        .bind(tenant_id)
        .bind(user_id)
        .bind(plugin_id)
        .bind(user_id)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("delete webserver_plugin", error))?;
        if result.rows_affected() == 0 {
            return Err(WebServiceError::not_found("plugin not found"));
        }
        Ok(())
    }
}
