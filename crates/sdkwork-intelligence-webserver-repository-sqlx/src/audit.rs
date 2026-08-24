use crate::audited_sql;
use sdkwork_intelligence_webserver_service::AuditLogWrite;
use sdkwork_webserver_contract::{
    AuditLogPage, AuditLogResponse, ListAuditLogsQuery, WebServiceError, WebServiceResult,
};
use super::{EngineRow, WebRepository};
use sqlx::Row;

use super::support::{
    decode_keyset_cursor, encode_keyset_cursor, instant_from_row, instant_write_expression,
    json_write_expression, new_uuid, next_id, now_rfc3339, store_error,
};

/// Upper bound for the first audit-log keyset page when no opaque cursor is
/// supplied. Matches PAGINATION_SPEC keyset head semantics for growing logs.
const AUDIT_LOG_FIRST_PAGE_CREATED_AT: &str = "9999-12-31T23:59:59Z";

fn resolve_audit_list_cursor(query: &ListAuditLogsQuery) -> WebServiceResult<String> {
    if let Some(cursor) = query.cursor.as_deref() {
        if !cursor.is_empty() {
            return Ok(cursor.to_owned());
        }
    }
    Ok(encode_keyset_cursor(
        AUDIT_LOG_FIRST_PAGE_CREATED_AT,
        i64::MAX,
    ))
}

/// Strongly typed audit list filter value so PostgreSQL receives correctly
/// typed parameters instead of text literals.
enum AuditBindValue {
    Int(i64),
    Text(String),
}

/// One fully numbered SQL filter fragment with its typed bind values.
/// Numbering is resolved at push time so the final statement needs no
/// post-processing of placeholder fragments.
struct AuditFilter {
    sql: String,
    bindings: Vec<AuditBindValue>,
}

impl AuditFilter {
    fn push(
        filters: &mut Vec<AuditFilter>,
        next_index: &mut usize,
        sql: &str,
        bindings: Vec<AuditBindValue>,
    ) {
        let numbered = sql.replace('$', &format!("${next_index}"));
        *next_index += bindings.len();
        filters.push(AuditFilter { sql: numbered, bindings });
    }
}

/// Appends the typed filter clauses shared by audit listing. `next_index` is
/// the next free `$N` placeholder index and is advanced by the pushed values.
fn push_audit_filters(
    query: &ListAuditLogsQuery,
    filters: &mut Vec<AuditFilter>,
    next_index: &mut usize,
) -> WebServiceResult<()> {
    if let Some(target_type) = query.target_type.as_deref() {
        let target_type = target_type.trim();
        if target_type.is_empty() || target_type.len() > 64 {
            return Err(WebServiceError::validation(
                "targetType must contain 1..64 trimmed characters",
            ));
        }
        AuditFilter::push(
            filters,
            next_index,
            "target_type = $",
            vec![AuditBindValue::Text(target_type.to_string())],
        );
    }
    if let Some(action) = query.action.as_deref() {
        let action = action.trim();
        if action.is_empty() || action.len() > 128 {
            return Err(WebServiceError::validation(
                "action must contain 1..128 trimmed characters",
            ));
        }
        AuditFilter::push(
            filters,
            next_index,
            "action = $",
            vec![AuditBindValue::Text(action.to_string())],
        );
    }
    if let Some(operator_id) = query.operator_id {
        AuditFilter::push(
            filters,
            next_index,
            "operator_id = $",
            vec![AuditBindValue::Int(operator_id)],
        );
    }
    if let Some(start_date) = query.start_date.as_deref() {
        validate_audit_date_range(start_date, "startDate")?;
        AuditFilter::push(
            filters,
            next_index,
            "created_at >= $",
            vec![AuditBindValue::Text(start_date.to_string())],
        );
    }
    if let Some(end_date) = query.end_date.as_deref() {
        validate_audit_date_range(end_date, "endDate")?;
        AuditFilter::push(
            filters,
            next_index,
            "created_at < $",
            vec![AuditBindValue::Text(end_date.to_string())],
        );
    }
    if let (Some(start_date), Some(end_date)) = (query.start_date.as_deref(), query.end_date.as_deref()) {
        if start_date >= end_date {
            return Err(WebServiceError::validation(
                "startDate must be earlier than endDate",
            ));
        }
    }
    Ok(())
}

/// Validates an audit log date filter as an RFC 3339 timestamp so malformed
/// values never reach the database comparison. Service-layer normalization
/// converts Adaptive Web `YYYY-MM-DD` filters before this check runs.
fn validate_audit_date_range(value: &str, name: &str) -> WebServiceResult<()> {
    if value.trim().is_empty() || value.len() > 64 {
        return Err(WebServiceError::validation(format!(
            "{name} must contain 1..64 characters"
        )));
    }
    chrono::DateTime::parse_from_rfc3339(value).map_err(|_| {
        WebServiceError::validation(format!(
            "{name} must be an RFC 3339 timestamp or YYYY-MM-DD date"
        ))
    })?;
    Ok(())
}

impl WebRepository {
    pub(super) async fn list_audit_logs_repo(
        &self,
        tenant_id: Option<i64>,
        query: &ListAuditLogsQuery,
    ) -> WebServiceResult<AuditLogPage> {
        // Cursor mode (keyset on (created_at, id)) is the only contract for
        // this growing log table (PAGINATION_SPEC §6/§12): no deep OFFSET and
        // no full COUNT per request. The first page may omit cursor; later
        // pages must use the opaque `nextCursor` from the prior response.
        let cursor = resolve_audit_list_cursor(query)?;
        self.list_audit_logs_cursor_repo(tenant_id, query, &cursor)
            .await
    }

    /// Keyset page over `(created_at DESC, id DESC)` with an opaque cursor.
    /// Fetches `page_size + 1` rows so `has_more` is exact; `total` is not
    /// computed in cursor mode (PAGINATION_SPEC §6).
    async fn list_audit_logs_cursor_repo(
        &self,
        tenant_id: Option<i64>,
        query: &ListAuditLogsQuery,
        cursor: &str,
    ) -> WebServiceResult<AuditLogPage> {
        let page_size = query.resolved_page_size();
        if !(1..=200).contains(&page_size) {
            return Err(WebServiceError::validation(
                "page_size must be between 1 and 200",
            ));
        }
        let (cursor_created_at, cursor_id) = decode_keyset_cursor(cursor)
            .ok_or_else(|| WebServiceError::validation("cursor is invalid"))?;

        let mut filters: Vec<AuditFilter> = Vec::new();
        let mut next_index = 1_usize;
        if let Some(tenant_id) = tenant_id {
            AuditFilter::push(
                &mut filters,
                &mut next_index,
                "tenant_id = $",
                vec![AuditBindValue::Int(tenant_id)],
            );
        }
        push_audit_filters(query, &mut filters, &mut next_index)?;
        AuditFilter::push(
            &mut filters,
            &mut next_index,
            "(created_at, id) < ($",
            vec![
                AuditBindValue::Text(cursor_created_at),
                AuditBindValue::Int(cursor_id),
            ],
        );

        let mut filter_sql = String::from(" WHERE 1=1");
        for filter in &filters {
            filter_sql.push_str(" AND ");
            filter_sql.push_str(&filter.sql);
        }
        let list_sql = format!(
            "SELECT id, uuid, action, target_type, CAST(created_at AS TEXT) AS created_at
             FROM web_audit_log{filter_sql}
             ORDER BY created_at DESC, id DESC LIMIT ${next_index}"
        );
        let mut list_query = sqlx::query(audited_sql(&list_sql));
        for filter in &filters {
            for binding in &filter.bindings {
                match binding {
                    AuditBindValue::Int(value) => {
                        list_query = list_query.bind(*value);
                    }
                    AuditBindValue::Text(value) => {
                        list_query = list_query.bind(value);
                    }
                }
            }
        }
        let fetch_size = i64::from(page_size) + 1;
        let fetch_size_bind = fetch_size.to_string();
        list_query = list_query.bind(&fetch_size_bind);

        let rows = list_query
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list web_audit_log cursor", error))?;
        let has_more = rows.len() > page_size as usize;
        let page_rows = rows.into_iter().take(page_size as usize).collect::<Vec<_>>();

        let mut items = Vec::with_capacity(page_rows.len());
        for row in &page_rows {
            items.push(map_audit_log_row(row).map_err(|error| {
                WebServiceError::Internal(format!("map web_audit_log row: {error}"))
            })?);
        }
        let next_cursor = has_more
            .then(|| {
                let last = page_rows.last().expect("non-empty page when has_more");
                let created_at: String = last
                    .try_get("created_at")
                    .map_err(|error| store_error("map web_audit_log cursor instant", error))?;
                let id: i64 = last
                    .try_get("id")
                    .map_err(|error| store_error("map web_audit_log cursor id", error))?;
                Ok::<_, WebServiceError>(encode_keyset_cursor(&created_at, id))
            })
            .transpose()?;

        Ok(AuditLogPage {
            items,
            total: 0,
            page: 0,
            page_size,
            next_cursor,
            has_more: Some(has_more),
        })
    }

    pub(super) async fn insert_audit_log_repo(
        &self,
        entry: AuditLogWrite<'_>,
    ) -> WebServiceResult<()> {
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();

        let now_expression = instant_write_expression("$13");
        let metadata_expression = json_write_expression("$12");
        let insert_sql = format!(
            "INSERT INTO web_audit_log (
                id, uuid, tenant_id, organization_id, operator_id, operator_type, action,
                target_type, target_id, target_uuid, request_id, metadata, created_at
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                {metadata_expression}, {now_expression}
             )"
        );

        sqlx::query(audited_sql(&insert_sql))
            .bind(id)
            .bind(&uuid)
            .bind(entry.tenant_id)
            .bind(entry.organization_id)
            .bind(entry.operator_id)
            .bind(entry.operator_type)
            .bind(entry.action)
            .bind(entry.target_type)
            .bind(entry.target_id)
            .bind(entry.target_uuid)
            .bind(entry.request_id)
            .bind(entry.metadata_json)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("insert web_audit_log", error))?;

        Ok(())
    }
}

fn map_audit_log_row(row: &EngineRow) -> Result<AuditLogResponse, sqlx::Error> {
    Ok(AuditLogResponse {
        id: row.try_get("uuid")?,
        action: row.try_get("action")?,
        resource: row.try_get("target_type")?,
        created_at: instant_from_row(row, "created_at")?,
    })
}
