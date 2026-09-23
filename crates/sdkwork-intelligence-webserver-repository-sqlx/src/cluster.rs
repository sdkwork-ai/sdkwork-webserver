// Distributed cluster management persistence: clusters, hosts, instances,
// events, heartbeat samples, and the peer message mailbox. Cluster data is
// platform infrastructure (tenant 0); every statement filters on the tenant
// pair so the composite foreign keys stay enforceable. Optional list filters
// follow the house `($N IS NULL OR column = $N)` pattern so the SQL text stays
// fixed and every parameter binds positionally.
use super::{EngineRow, WebRepository};
use sdkwork_webserver_contract::{
    ClusterEventPage, ClusterEventResponse, ClusterHeartbeatSamplePage,
    ClusterHeartbeatSampleResponse, ClusterHostPage, ClusterHostResponse,
    ClusterInstancePage, ClusterInstanceResponse, ClusterOverviewResponse, ClusterPage,
    ClusterPeer, ClusterPeerMessage, ClusterResponse, UpdateClusterHostRequest,
    UpdateClusterInstanceRequest, UpdateClusterRequest, WebServiceError, WebServiceResult,
};
use serde_json::{json, Value};
use sqlx::Row;

use crate::audited_sql;
use super::support::{
    cursor_instant_from_row, decode_keyset_cursor, encode_keyset_cursor, instant_from_row,
    instant_write_expression, json_write_expression, new_uuid, next_id, now_rfc3339,
    optional_instant_from_row, pagination, sha256_hex, store_error,
};

/// Internal id/uuid pair for a resolved parent row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClusterRowRef {
    pub id: i64,
    pub uuid: String,
}

/// Resolved cluster identity for registration and heartbeat responses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClusterIdentityRow {
    pub cluster_id: i64,
    pub cluster_uuid: String,
    pub name: String,
    pub code: String,
    pub heartbeat_interval_seconds: i32,
    pub offline_threshold_seconds: i32,
}

/// Instance row resolved by heartbeat token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClusterInstanceAuthRow {
    pub instance_id: i64,
    pub instance_uuid: String,
    pub host_id: i64,
    pub host_uuid: String,
    pub cluster_id: i64,
    pub cluster_uuid: String,
    pub tenant_id: i64,
    pub heartbeat_interval_seconds: i32,
    pub offline_threshold_seconds: i32,
    pub desired_config_revision: Option<String>,
    pub applied_config_revision: Option<String>,
    pub desired_applications_revision: Option<String>,
    pub applied_applications_revision: Option<String>,
    pub sync_status: i32,
    pub routing_enabled: bool,
    pub draining: bool,
}

/// Previous state observed while recording a heartbeat, used by the service to
/// emit lifecycle transition events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClusterHeartbeatTransitionRow {
    pub previous_status: i32,
    pub previous_health_state: String,
    /// The status the heartbeat stored (operator maintenance outranks the
    /// node's reported `online`; see `cluster_operator_owned_status`).
    pub status: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClusterUpsertRow {
    pub id: i64,
    pub uuid: String,
    pub created: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExpiredClusterInstanceRow {
    pub instance_uuid: String,
    pub name: String,
    pub cluster_uuid: String,
    pub host_uuid: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExpiredClusterHostRow {
    pub host_uuid: String,
    pub name: String,
    pub cluster_uuid: String,
}

/// Row projections for the cluster admin queries. jsonb and timestamptz
/// columns are cast to TEXT because the mappers decode strings (RFC 3339
/// instants, JSON objects) — a bare `SELECT *` would return native JSONB and
/// fail the decode.
///
/// Every projection consumed by a keyset-paginated query MUST also carry the
/// table's internal `id`: `finalize_*_page` reads it back to mint the next
/// cursor. Dropping it only fails once a page actually has more rows than the
/// requested size, so a thin fixture hides the defect.
const HOST_PROJECTION: &str = "h.uuid, ch.uuid AS cluster_uuid, h.name, h.hostname,
        h.machine_code, h.os_name, h.os_version, h.kernel_version, h.arch,
        h.cpu_model, h.cpu_cores, h.memory_total_mb, h.remote_ip,
        CAST(h.local_ips AS TEXT) AS local_ips,
        CAST(h.mac_addresses AS TEXT) AS mac_addresses,
        h.daemon_version, h.status, h.join_mode, h.tunnel_route_domain,
        CAST(h.last_heartbeat_at AS TEXT) AS last_heartbeat_at,
        CAST(h.created_at AS TEXT) AS created_at,
        CAST(h.updated_at AS TEXT) AS updated_at,
        h.id,
        (SELECT COUNT(*) FROM webserver_cluster_instance i
          WHERE i.host_id = h.id AND i.deleted_at IS NULL) AS instance_count";

const INSTANCE_PROJECTION: &str = "i.uuid, ci.uuid AS cluster_uuid, ch.uuid AS host_uuid,
        ch.name AS host_name, i.name, i.role, i.environment, i.process_pid,
        CAST(i.process_started_at AS TEXT) AS process_started_at,
        i.bind_host, i.bind_port, i.public_endpoint, i.build_version,
        i.status, i.health_state, i.join_mode, i.tunnel_route_domain,
        i.quality_score, i.desired_config_revision, i.applied_config_revision,
        i.desired_applications_revision, i.applied_applications_revision,
        i.sync_status,
        i.routing_enabled, i.draining, i.ejected_at IS NOT NULL AS ejected,
        i.restart_count, CAST(i.labels AS TEXT) AS labels,
        i.routing_weight, i.maintenance_note, i.probe_failures, i.probe_url,
        CAST(i.last_heartbeat_at AS TEXT) AS last_heartbeat_at,
        CAST(i.last_online_at AS TEXT) AS last_online_at,
        i.uptime_seconds,
        CAST(i.metrics AS TEXT) AS metrics,
        CAST(i.created_at AS TEXT) AS created_at,
        CAST(i.updated_at AS TEXT) AS updated_at,
        i.id";

const EVENT_PROJECTION: &str = "e.uuid, c.uuid AS cluster_uuid, h.uuid AS host_uuid,
        i.uuid AS instance_uuid, e.event_type, e.severity, e.message,
        CAST(e.detail AS TEXT) AS detail,
        CAST(e.occurred_at AS TEXT) AS occurred_at,
        CAST(e.created_at AS TEXT) AS created_at,
        e.id";

/// Maximum members one broadcast enqueue may address (PAGINATION_SPEC §2.5
/// bounded batch discipline).
const CLUSTER_PEER_ENQUEUE_TARGET_LIMIT: usize = 500;

impl WebRepository {
    // ------------------------------------------------------------------
    // Admin surface: clusters
    // ------------------------------------------------------------------

    pub(super) async fn list_clusters_repo(
        &self,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<ClusterPage> {
        let (_page, page_size, offset) = pagination(page, page_size)?;
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM webserver_cluster
             WHERE tenant_id = $1 AND deleted_at IS NULL",
        )
        .bind(0_i64)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count webserver_cluster", error))?;
        let total: i64 = count_row
            .try_get("total")
            .map_err(|error| store_error("map webserver_cluster count", error))?;

        let sql = "SELECT c.uuid, c.name, c.code, c.description, c.status,
                c.heartbeat_interval_seconds, c.offline_threshold_seconds,
                c.lb_strategy, CAST(c.served_domains AS TEXT) AS served_domains,
                CAST(c.created_at AS TEXT) AS created_at, CAST(c.updated_at AS TEXT) AS updated_at,
                (SELECT COUNT(*) FROM webserver_cluster_host h
                  WHERE h.cluster_id = c.id AND h.deleted_at IS NULL) AS host_count,
                (SELECT COUNT(*) FROM webserver_cluster_instance i
                  WHERE i.cluster_id = c.id AND i.deleted_at IS NULL) AS instance_count,
                (SELECT COUNT(*) FROM webserver_cluster_instance i
                  WHERE i.cluster_id = c.id AND i.deleted_at IS NULL AND i.status = 1)
                    AS online_instance_count
         FROM webserver_cluster c
         WHERE c.tenant_id = $1 AND c.deleted_at IS NULL
         ORDER BY c.updated_at DESC, c.id DESC LIMIT $2 OFFSET $3";
        let rows = sqlx::query(sql)
            .bind(0_i64)
            .bind(page_size)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list webserver_cluster", error))?;

        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            items.push(map_cluster_row(row).map_err(|error| {
                WebServiceError::Internal(format!("map webserver_cluster row: {error}"))
            })?);
        }
        Ok(ClusterPage { items, total })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn create_cluster_repo(
        &self,
        name: &str,
        code: &str,
        description: Option<&str>,
        heartbeat_interval_seconds: i32,
        offline_threshold_seconds: i32,
        lb_strategy: &str,
        served_domains: &[String],
    ) -> WebServiceResult<ClusterResponse> {
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();
        // `$9` is `lb_strategy` (a string), so the three timestamp columns need
        // a placeholder of their own. Sharing `$9` bound the strategy text into
        // `created_at` / `updated_at` and failed with `invalid input syntax for
        // type timestamp with time zone: "round_robin"` — a masked 500 on every
        // cluster insert.
        let now_expression = instant_write_expression("$11");
        let domains_expression = json_write_expression("$10");
        let served_domains_json = serde_json::to_string(served_domains)
            .map_err(|error| WebServiceError::Internal(format!("encode servedDomains: {error}")))?;
        let sql = format!(
            "INSERT INTO webserver_cluster (
                id, uuid, tenant_id, name, code, description, status,
                heartbeat_interval_seconds, offline_threshold_seconds, lb_strategy,
                served_domains, metadata,
                created_at, updated_at, version
            ) VALUES ($1, $2, $3, $4, $5, $6, 1, $7, $8, $9, {domains_expression}, CAST('{{}}' AS JSONB), {now_expression}, {now_expression}, 0)"
        );
        sqlx::query(audited_sql(&sql))
            .bind(id)
            .bind(&uuid)
            .bind(0_i64)
            .bind(name)
            .bind(code)
            .bind(description)
            .bind(heartbeat_interval_seconds)
            .bind(offline_threshold_seconds)
            .bind(lb_strategy)
            .bind(&served_domains_json)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("insert webserver_cluster", error))?;

        Ok(ClusterResponse {
            id: uuid,
            name: name.to_string(),
            code: code.to_string(),
            description: description.map(str::to_owned),
            status: 1,
            heartbeat_interval_seconds,
            offline_threshold_seconds,
            lb_strategy: Some(lb_strategy.to_owned()),
            served_domains: Some(served_domains.to_vec()),
            host_count: 0,
            instance_count: 0,
            online_instance_count: 0,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub(super) async fn retrieve_cluster_repo(
        &self,
        cluster_id: &str,
    ) -> WebServiceResult<ClusterResponse> {
        let sql = "SELECT c.uuid, c.name, c.code, c.description, c.status,
                c.heartbeat_interval_seconds, c.offline_threshold_seconds,
                c.lb_strategy, CAST(c.served_domains AS TEXT) AS served_domains,
                CAST(c.created_at AS TEXT) AS created_at, CAST(c.updated_at AS TEXT) AS updated_at,
                (SELECT COUNT(*) FROM webserver_cluster_host h
                  WHERE h.cluster_id = c.id AND h.deleted_at IS NULL) AS host_count,
                (SELECT COUNT(*) FROM webserver_cluster_instance i
                  WHERE i.cluster_id = c.id AND i.deleted_at IS NULL) AS instance_count,
                (SELECT COUNT(*) FROM webserver_cluster_instance i
                  WHERE i.cluster_id = c.id AND i.deleted_at IS NULL AND i.status = 1)
                    AS online_instance_count
         FROM webserver_cluster c
         WHERE c.tenant_id = $1 AND c.uuid = $2 AND c.deleted_at IS NULL";
        let row = sqlx::query(sql)
            .bind(0_i64)
            .bind(cluster_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("retrieve webserver_cluster", error))?
            .ok_or_else(|| WebServiceError::not_found("cluster not found"))?;
        map_cluster_row(&row).map_err(|error| {
            WebServiceError::Internal(format!("map webserver_cluster row: {error}"))
        })
    }

    pub(super) async fn update_cluster_repo(
        &self,
        cluster_id: &str,
        request: &UpdateClusterRequest,
    ) -> WebServiceResult<ClusterResponse> {
        let now = now_rfc3339();
        // Partial update via COALESCE: absent fields keep their current value.
        let now_expression = instant_write_expression("$8");
        let served_domains = request
            .served_domains
            .as_ref()
            .map(|domains| serde_json::to_string(domains))
            .transpose()
            .map_err(|error| WebServiceError::Internal(format!("encode servedDomains: {error}")))?;
        // `served_domains` is JSONB and the bind is a JSON string, so the
        // parameter needs an explicit cast: a bare `COALESCE($10, served_domains)`
        // makes Postgres infer text and fail with
        // `COALESCE types text and jsonb cannot be matched`.
        let served_domains_expression = json_write_expression("$10");
        let sql = format!(
            "UPDATE webserver_cluster SET
                name = COALESCE($3, name),
                description = COALESCE($4, description),
                status = COALESCE($5, status),
                heartbeat_interval_seconds = COALESCE($6, heartbeat_interval_seconds),
                offline_threshold_seconds = COALESCE($7, offline_threshold_seconds),
                lb_strategy = COALESCE($9, lb_strategy),
                served_domains = COALESCE({served_domains_expression}, served_domains),
                updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL"
        );
        let result = sqlx::query(audited_sql(&sql))
            .bind(0_i64)
            .bind(cluster_id)
            .bind(request.name.as_deref())
            .bind(request.description.as_deref())
            .bind(request.status)
            .bind(request.heartbeat_interval_seconds)
            .bind(request.offline_threshold_seconds)
            .bind(&now)
            .bind(request.lb_strategy.as_deref())
            .bind(&served_domains)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("update webserver_cluster", error))?;
        if result.rows_affected() == 0 {
            return Err(WebServiceError::not_found("cluster not found"));
        }
        self.retrieve_cluster_repo(cluster_id).await
    }

    pub(super) async fn delete_cluster_repo(&self, cluster_id: &str) -> WebServiceResult<()> {
        let now = now_rfc3339();
        let now_expression = instant_write_expression("$3");
        let sql = format!(
            "UPDATE webserver_cluster SET deleted_at = {now_expression}, updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL"
        );
        let result = sqlx::query(audited_sql(&sql))
            .bind(0_i64)
            .bind(cluster_id)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("delete webserver_cluster", error))?;
        if result.rows_affected() == 0 {
            return Err(WebServiceError::not_found("cluster not found"));
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Admin surface: hosts
    // ------------------------------------------------------------------

    pub(super) async fn list_cluster_hosts_repo(
        &self,
        cluster_id: Option<&str>,
        status: Option<i32>,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterHostPage> {
        if !(1..=200).contains(&page_size) {
            return Err(WebServiceError::validation(
                "page_size must be between 1 and 200",
            ));
        }
        let cluster_internal: Option<i64> = match cluster_id {
            Some(cluster_uuid) => Some(
                self.resolve_cluster_row(0, cluster_uuid)
                    .await?
                    .ok_or_else(|| WebServiceError::not_found("cluster not found"))?
                    .id,
            ),
            None => None,
        };

        let rows = if let Some(cursor) = cursor {
            let (cursor_updated_at, cursor_id) = decode_keyset_cursor(cursor)
                .ok_or_else(|| WebServiceError::validation("cursor is invalid"))?;
            let sql = format!(
                "SELECT {HOST_PROJECTION}
                 FROM webserver_cluster_host h
                 JOIN webserver_cluster ch ON ch.id = h.cluster_id
                 WHERE h.tenant_id = $1 AND h.deleted_at IS NULL
                   AND (h.updated_at, h.id) < (CAST($2 AS TIMESTAMPTZ), $3)
                   AND ($4::BIGINT IS NULL OR h.cluster_id = $4)
                   AND ($5::INT IS NULL OR h.status = $5)
                 ORDER BY h.updated_at DESC, h.id DESC LIMIT $6");
            sqlx::query(audited_sql(&sql))
                .bind(0_i64)
                .bind(&cursor_updated_at)
                .bind(cursor_id)
                .bind(cluster_internal)
                .bind(status)
                .bind(i64::from(page_size) + 1)
                .fetch_all(&self.pool)
                .await
                .map_err(|error| store_error("list webserver_cluster_host", error))?
        } else {
            let sql = format!(
                "SELECT {HOST_PROJECTION}
                 FROM webserver_cluster_host h
                 JOIN webserver_cluster ch ON ch.id = h.cluster_id
                 WHERE h.tenant_id = $1 AND h.deleted_at IS NULL
                   AND ($4::BIGINT IS NULL OR h.cluster_id = $4)
                   AND ($5::INT IS NULL OR h.status = $5)
                 ORDER BY h.updated_at DESC, h.id DESC LIMIT $2 OFFSET $3");
            sqlx::query(audited_sql(&sql))
                .bind(0_i64)
                .bind(i64::from(page_size) + 1)
                .bind(0_i64)
                .bind(cluster_internal)
                .bind(status)
                .fetch_all(&self.pool)
                .await
                .map_err(|error| store_error("list webserver_cluster_host", error))?
        };
        finalize_cluster_host_page(rows, page_size)
    }

    pub(super) async fn retrieve_cluster_host_repo(
        &self,
        host_id: &str,
    ) -> WebServiceResult<ClusterHostResponse> {
        let sql = format!(
            "SELECT {HOST_PROJECTION}
             FROM webserver_cluster_host h
             JOIN webserver_cluster ch ON ch.id = h.cluster_id
             WHERE h.tenant_id = $1 AND h.uuid = $2 AND h.deleted_at IS NULL");
        let row = sqlx::query(audited_sql(&sql))
            .bind(0_i64)
            .bind(host_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("retrieve webserver_cluster_host", error))?
            .ok_or_else(|| WebServiceError::not_found("cluster host not found"))?;
        map_cluster_host_row(&row).map_err(|error| {
            WebServiceError::Internal(format!("map webserver_cluster_host row: {error}"))
        })
    }

    pub(super) async fn update_cluster_host_repo(
        &self,
        host_id: &str,
        request: &UpdateClusterHostRequest,
    ) -> WebServiceResult<ClusterHostResponse> {
        let target_cluster: Option<i64> = match request.cluster_id.as_deref() {
            Some(cluster_uuid) => Some(
                self.resolve_cluster_row(0, cluster_uuid)
                    .await?
                    .ok_or_else(|| {
                        WebServiceError::validation("target cluster does not exist")
                    })?
                    .id,
            ),
            None => None,
        };
        let now = now_rfc3339();
        let now_expression = instant_write_expression("$5");
        let sql = format!(
            "UPDATE webserver_cluster_host SET
                name = COALESCE($3, name),
                cluster_id = COALESCE($4, cluster_id),
                updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL"
        );
        let result = sqlx::query(audited_sql(&sql))
            .bind(0_i64)
            .bind(host_id)
            .bind(request.name.as_deref())
            .bind(target_cluster)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("update webserver_cluster_host", error))?;
        if result.rows_affected() == 0 {
            return Err(WebServiceError::not_found("cluster host not found"));
        }
        self.retrieve_cluster_host_repo(host_id).await
    }

    pub(super) async fn delete_cluster_host_repo(&self, host_id: &str) -> WebServiceResult<()> {
        let now = now_rfc3339();
        let now_expression = instant_write_expression("$3");
        let sql = format!(
            "UPDATE webserver_cluster_host SET deleted_at = {now_expression}, updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL"
        );
        let result = sqlx::query(audited_sql(&sql))
            .bind(0_i64)
            .bind(host_id)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("delete webserver_cluster_host", error))?;
        if result.rows_affected() == 0 {
            return Err(WebServiceError::not_found("cluster host not found"));
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Admin surface: instances
    // ------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn list_cluster_instances_repo(
        &self,
        cluster_id: Option<&str>,
        host_id: Option<&str>,
        status: Option<i32>,
        health_state: Option<&str>,
        join_mode: Option<i32>,
        sync_status: Option<i32>,
        labels: &[(String, String)],
        search: Option<&str>,
        build_version: Option<&str>,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterInstancePage> {
        if !(1..=200).contains(&page_size) {
            return Err(WebServiceError::validation(
                "page_size must be between 1 and 200",
            ));
        }
        let cluster_internal: Option<i64> = match cluster_id {
            Some(cluster_uuid) => Some(
                self.resolve_cluster_row(0, cluster_uuid)
                    .await?
                    .ok_or_else(|| WebServiceError::not_found("cluster not found"))?
                    .id,
            ),
            None => None,
        };
        let host_internal: Option<i64> = match host_id {
            Some(host_uuid) => Some(
                self.resolve_cluster_host_row(0, host_uuid)
                    .await?
                    .ok_or_else(|| WebServiceError::not_found("cluster host not found"))?
                    .id,
            ),
            None => None,
        };

        // Label containment document ($10) and ILIKE pattern ($11) are built
        // once for both branches.
        let labels_json = if labels.is_empty() {
            None
        } else {
            let pairs: serde_json::Map<String, Value> = labels
                .iter()
                .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                .collect();
            Some(Value::Object(pairs).to_string())
        };
        let search_pattern = search
            .map(|term| format!("%{}%", term.trim().to_ascii_lowercase()));
        let rows = if let Some(cursor) = cursor {
            let (cursor_updated_at, cursor_id) = decode_keyset_cursor(cursor)
                .ok_or_else(|| WebServiceError::validation("cursor is invalid"))?;
            let sql = format!(
                "SELECT {INSTANCE_PROJECTION}
                 FROM webserver_cluster_instance i
                 JOIN webserver_cluster ci ON ci.id = i.cluster_id
                 JOIN webserver_cluster_host ch ON ch.id = i.host_id
                 WHERE i.tenant_id = $1 AND i.deleted_at IS NULL
                   AND (i.updated_at, i.id) < (CAST($2 AS TIMESTAMPTZ), $3)
                   AND ($4::BIGINT IS NULL OR i.cluster_id = $4)
                   AND ($5::BIGINT IS NULL OR i.host_id = $5)
                   AND ($6::INT IS NULL OR i.status = $6)
                   AND ($7::TEXT IS NULL OR i.health_state = $7)
                   AND ($8::INT IS NULL OR i.join_mode = $8)
                   AND ($9::INT IS NULL OR i.sync_status = $9)
                   AND ($10::JSONB IS NULL OR i.labels @> $10::JSONB)
                   AND ($11::TEXT IS NULL OR i.name ILIKE $11
                        OR i.public_endpoint ILIKE $11 OR ch.hostname ILIKE $11)
                   AND ($12::TEXT IS NULL OR i.build_version = $12)
                 ORDER BY i.updated_at DESC, i.id DESC LIMIT $13");
            sqlx::query(audited_sql(&sql))
                .bind(0_i64)
                .bind(&cursor_updated_at)
                .bind(cursor_id)
                .bind(cluster_internal)
                .bind(host_internal)
                .bind(status)
                .bind(health_state)
                .bind(join_mode)
                .bind(sync_status)
                .bind(labels_json)
                .bind(&search_pattern)
                .bind(build_version)
                .bind(i64::from(page_size) + 1)
                .fetch_all(&self.pool)
                .await
                .map_err(|error| store_error("list webserver_cluster_instance", error))?
        } else {
            let sql = format!(
                "SELECT {INSTANCE_PROJECTION}
                 FROM webserver_cluster_instance i
                 JOIN webserver_cluster ci ON ci.id = i.cluster_id
                 JOIN webserver_cluster_host ch ON ch.id = i.host_id
                 WHERE i.tenant_id = $1 AND i.deleted_at IS NULL
                   AND ($4::BIGINT IS NULL OR i.cluster_id = $4)
                   AND ($5::BIGINT IS NULL OR i.host_id = $5)
                   AND ($6::INT IS NULL OR i.status = $6)
                   AND ($7::TEXT IS NULL OR i.health_state = $7)
                   AND ($8::INT IS NULL OR i.join_mode = $8)
                   AND ($9::INT IS NULL OR i.sync_status = $9)
                   AND ($10::JSONB IS NULL OR i.labels @> $10::JSONB)
                   AND ($11::TEXT IS NULL OR i.name ILIKE $11
                        OR i.public_endpoint ILIKE $11 OR ch.hostname ILIKE $11)
                   AND ($12::TEXT IS NULL OR i.build_version = $12)
                 ORDER BY i.updated_at DESC, i.id DESC LIMIT $2 OFFSET $3");
            sqlx::query(audited_sql(&sql))
                .bind(0_i64)
                .bind(i64::from(page_size) + 1)
                .bind(0_i64)
                .bind(cluster_internal)
                .bind(host_internal)
                .bind(status)
                .bind(health_state)
                .bind(join_mode)
                .bind(sync_status)
                .bind(&labels_json)
                .bind(&search_pattern)
                .bind(build_version)
                .fetch_all(&self.pool)
                .await
                .map_err(|error| store_error("list webserver_cluster_instance", error))?
        };
        finalize_cluster_instance_page(rows, page_size)
    }

    pub(super) async fn retrieve_cluster_instance_repo(
        &self,
        instance_id: &str,
    ) -> WebServiceResult<ClusterInstanceResponse> {
        let sql = format!(
            "SELECT {INSTANCE_PROJECTION}
             FROM webserver_cluster_instance i
             JOIN webserver_cluster ci ON ci.id = i.cluster_id
             JOIN webserver_cluster_host ch ON ch.id = i.host_id
             WHERE i.tenant_id = $1 AND i.uuid = $2 AND i.deleted_at IS NULL");
        let row = sqlx::query(audited_sql(&sql))
            .bind(0_i64)
            .bind(instance_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("retrieve webserver_cluster_instance", error))?
            .ok_or_else(|| WebServiceError::not_found("cluster instance not found"))?;
        map_cluster_instance_row(&row).map_err(|error| {
            WebServiceError::Internal(format!("map webserver_cluster_instance row: {error}"))
        })
    }

    pub(super) async fn update_cluster_instance_repo(
        &self,
        instance_id: &str,
        request: &UpdateClusterInstanceRequest,
    ) -> WebServiceResult<ClusterInstanceResponse> {
        let now = now_rfc3339();
        let now_expression = instant_write_expression("$6");
        let labels_json = request
            .labels
            .as_ref()
            .map(|labels| serde_json::to_string(labels))
            .transpose()
            .map_err(|error| WebServiceError::Internal(format!("encode labels: {error}")))?;
        // Drain transitions stamp/clear `drain_started_at` alongside the flag.
        let drain_started_expression = instant_write_expression("$9");
        let labels_expression = json_write_expression("$11");
        let sql = format!(
            "UPDATE webserver_cluster_instance SET
                name = COALESCE($3, name),
                status = COALESCE($4, status),
                public_endpoint = COALESCE($5, public_endpoint),
                routing_enabled = COALESCE($7, routing_enabled),
                draining = COALESCE($8, draining),
                drain_started_at = CASE WHEN $8 = TRUE THEN {drain_started_expression}
                                        WHEN $8 = FALSE THEN NULL
                                        ELSE drain_started_at END,
                probe_url = COALESCE($10, probe_url),
                labels = COALESCE({labels_expression}, labels),
                routing_weight = COALESCE($12, routing_weight),
                maintenance_note = COALESCE($13, maintenance_note),
                updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL"
        );
        let result = sqlx::query(audited_sql(&sql))
            .bind(0_i64)
            .bind(instance_id)
            .bind(request.name.as_deref())
            .bind(request.status)
            .bind(request.public_endpoint.as_deref())
            .bind(&now)
            .bind(request.routing_enabled)
            .bind(request.draining)
            .bind(&now)
            .bind(request.probe_url.as_deref())
            .bind(&labels_json)
            .bind(request.routing_weight)
            .bind(request.maintenance_note.as_deref())
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("update webserver_cluster_instance", error))?;
        if result.rows_affected() == 0 {
            return Err(WebServiceError::not_found("cluster instance not found"));
        }
        self.retrieve_cluster_instance_repo(instance_id).await
    }

    pub(super) async fn delete_cluster_instance_repo(
        &self,
        instance_id: &str,
    ) -> WebServiceResult<()> {
        let now = now_rfc3339();
        let now_expression = instant_write_expression("$3");
        let sql = format!(
            "UPDATE webserver_cluster_instance SET deleted_at = {now_expression}, updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL"
        );
        let result = sqlx::query(audited_sql(&sql))
            .bind(0_i64)
            .bind(instance_id)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("delete webserver_cluster_instance", error))?;
        if result.rows_affected() == 0 {
            return Err(WebServiceError::not_found("cluster instance not found"));
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Admin surface: events and overview
    // ------------------------------------------------------------------

    pub(super) async fn list_cluster_events_repo(
        &self,
        cluster_id: Option<&str>,
        severity: Option<&str>,
        instance_id: Option<&str>,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterEventPage> {
        if !(1..=200).contains(&page_size) {
            return Err(WebServiceError::validation(
                "page_size must be between 1 and 200",
            ));
        }
        let cluster_internal: Option<i64> = match cluster_id {
            Some(cluster_uuid) => Some(
                self.resolve_cluster_row(0, cluster_uuid)
                    .await?
                    .ok_or_else(|| WebServiceError::not_found("cluster not found"))?
                    .id,
            ),
            None => None,
        };

        let rows = if let Some(cursor) = cursor {
            let (cursor_occurred_at, cursor_id) = decode_keyset_cursor(cursor)
                .ok_or_else(|| WebServiceError::validation("cursor is invalid"))?;
            let sql = format!(
                "SELECT {EVENT_PROJECTION}
                 FROM webserver_cluster_event e
                 JOIN webserver_cluster c ON c.id = e.cluster_id
                 LEFT JOIN webserver_cluster_host h ON h.id = e.host_id
                 LEFT JOIN webserver_cluster_instance i ON i.id = e.instance_id
                 WHERE e.tenant_id = $1
                   AND (e.occurred_at, e.id) < (CAST($2 AS TIMESTAMPTZ), $3)
                   AND ($4::BIGINT IS NULL OR e.cluster_id = $4)
                   AND ($5::TEXT IS NULL OR e.severity = $5)
                   AND ($6::TEXT IS NULL OR i.uuid = $6)
                 ORDER BY e.occurred_at DESC, e.id DESC LIMIT $7");
            sqlx::query(audited_sql(&sql))
                .bind(0_i64)
                .bind(&cursor_occurred_at)
                .bind(cursor_id)
                .bind(cluster_internal)
                .bind(severity)
                .bind(instance_id)
                .bind(i64::from(page_size) + 1)
                .fetch_all(&self.pool)
                .await
                .map_err(|error| store_error("list webserver_cluster_event", error))?
        } else {
            let sql = format!(
                "SELECT {EVENT_PROJECTION}
                 FROM webserver_cluster_event e
                 JOIN webserver_cluster c ON c.id = e.cluster_id
                 LEFT JOIN webserver_cluster_host h ON h.id = e.host_id
                 LEFT JOIN webserver_cluster_instance i ON i.id = e.instance_id
                 WHERE e.tenant_id = $1
                   AND ($4::BIGINT IS NULL OR e.cluster_id = $4)
                   AND ($5::TEXT IS NULL OR e.severity = $5)
                   AND ($6::TEXT IS NULL OR i.uuid = $6)
                 ORDER BY e.occurred_at DESC, e.id DESC LIMIT $2 OFFSET $3");
            sqlx::query(audited_sql(&sql))
                .bind(0_i64)
                .bind(i64::from(page_size) + 1)
                .bind(0_i64)
                .bind(cluster_internal)
                .bind(severity)
                .bind(instance_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|error| store_error("list webserver_cluster_event", error))?
        };
        finalize_cluster_event_page(rows, page_size)
    }

    /// Cursor page over one instance's stored heartbeat samples, newest
    /// first, bounded by the sample retention sweep.
    pub(super) async fn list_cluster_heartbeats_repo(
        &self,
        instance_uuid: &str,
        page_size: i32,
        cursor: Option<&str>,
    ) -> WebServiceResult<ClusterHeartbeatSamplePage> {
        if !(1..=200).contains(&page_size) {
            return Err(WebServiceError::validation(
                "page_size must be between 1 and 200",
            ));
        }
        let rows = if let Some(cursor) = cursor {
            let (cursor_reported_at, cursor_id) = decode_keyset_cursor(cursor)
                .ok_or_else(|| WebServiceError::validation("cursor is invalid"))?;
            let sql = "SELECT h.uuid, h.status, h.latency_ms,
                    CAST(h.metrics AS TEXT) AS metrics,
                    CAST(h.reported_at AS TEXT) AS reported_at, h.id
                 FROM webserver_cluster_heartbeat h
                 JOIN webserver_cluster_instance i ON i.id = h.instance_id
                 WHERE h.tenant_id = 0 AND i.uuid = $1
                   AND (h.reported_at, h.id) < (CAST($2 AS TIMESTAMPTZ), $3)
                 ORDER BY h.reported_at DESC, h.id DESC LIMIT $4";
            sqlx::query(sql)
                .bind(instance_uuid)
                .bind(&cursor_reported_at)
                .bind(cursor_id)
                .bind(i64::from(page_size) + 1)
                .fetch_all(&self.pool)
                .await
                .map_err(|error| store_error("list webserver_cluster_heartbeat", error))?
        } else {
            let sql = "SELECT h.uuid, h.status, h.latency_ms,
                    CAST(h.metrics AS TEXT) AS metrics,
                    CAST(h.reported_at AS TEXT) AS reported_at, h.id
                 FROM webserver_cluster_heartbeat h
                 JOIN webserver_cluster_instance i ON i.id = h.instance_id
                 WHERE h.tenant_id = 0 AND i.uuid = $1
                 ORDER BY h.reported_at DESC, h.id DESC LIMIT $2 OFFSET $3";
            sqlx::query(sql)
                .bind(instance_uuid)
                .bind(i64::from(page_size) + 1)
                .bind(0_i64)
                .fetch_all(&self.pool)
                .await
                .map_err(|error| store_error("list webserver_cluster_heartbeat", error))?
        };
        let has_more = rows.len() > page_size as usize;
        let page_rows = rows.into_iter().take(page_size as usize).collect::<Vec<_>>();
        let mut items = Vec::with_capacity(page_rows.len());
        for row in &page_rows {
            items.push(ClusterHeartbeatSampleResponse {
                id: row.try_get("uuid").map_err(store_map_error)?,
                status: row.try_get("status").map_err(store_map_error)?,
                latency_ms: row.try_get("latency_ms").map_err(store_map_error)?,
                metrics: row
                    .try_get::<Option<String>, _>("metrics")
                    .map_err(store_map_error)?
                    .and_then(|raw| serde_json::from_str(&raw).ok())
                    .unwrap_or_else(|| json!({})),
                reported_at: instant_from_row(row, "reported_at")
                    .map_err(|error| store_error("map webserver_cluster_heartbeat instant", error))?,
            });
        }
        let next_cursor = has_more
            .then(|| {
                let last = page_rows.last().expect("non-empty page when has_more");
                let reported_at = cursor_instant_from_row(last, "reported_at")
                    .map_err(|error| store_error("map webserver_cluster_heartbeat cursor", error))?;
                let id: i64 = last
                    .try_get("id")
                    .map_err(|error| store_error("map webserver_cluster_heartbeat cursor id", error))?;
                Ok::<_, WebServiceError>(encode_keyset_cursor(&reported_at, id))
            })
            .transpose()?;
        Ok(ClusterHeartbeatSamplePage {
            items,
            total: 0,
            next_cursor,
            has_more: Some(has_more),
        })
    }

    pub(super) async fn cluster_overview_repo(&self) -> WebServiceResult<ClusterOverviewResponse> {
        let sql = "SELECT
                (SELECT COUNT(*) FROM webserver_cluster_host
                  WHERE tenant_id = $1 AND deleted_at IS NULL) AS total_hosts,
                (SELECT COUNT(*) FROM webserver_cluster_host
                  WHERE tenant_id = $1 AND deleted_at IS NULL AND status = 1) AS online_hosts,
                (SELECT COUNT(*) FROM webserver_cluster_instance
                  WHERE tenant_id = $1 AND deleted_at IS NULL) AS total_instances,
                (SELECT COUNT(*) FROM webserver_cluster_instance
                  WHERE tenant_id = $1 AND deleted_at IS NULL AND status = 1) AS online_instances,
                (SELECT COUNT(*) FROM webserver_cluster_instance
                  WHERE tenant_id = $1 AND deleted_at IS NULL
                    AND health_state IN ('DEGRADED', 'UNHEALTHY')) AS unhealthy_instances,
                (SELECT COUNT(*) FROM webserver_cluster_peer_message
                  WHERE tenant_id = $1 AND state = 'PENDING') AS pending_peer_messages";
        let row = sqlx::query(sql)
            .bind(0_i64)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("cluster overview", error))?;
        Ok(ClusterOverviewResponse {
            total_hosts: row.try_get("total_hosts").map_err(store_map_error)?,
            online_hosts: row.try_get("online_hosts").map_err(store_map_error)?,
            total_instances: row.try_get("total_instances").map_err(store_map_error)?,
            online_instances: row.try_get("online_instances").map_err(store_map_error)?,
            unhealthy_instances: row.try_get("unhealthy_instances").map_err(store_map_error)?,
            pending_peer_messages: row
                .try_get("pending_peer_messages")
                .map_err(store_map_error)?,
            generated_at: now_rfc3339(),
        })
    }

    // ------------------------------------------------------------------
    // Machine surface: registration, heartbeat, peers, mailbox, sweep
    // ------------------------------------------------------------------

    pub(super) async fn resolve_cluster_row(
        &self,
        tenant_id: i64,
        cluster_uuid: &str,
    ) -> WebServiceResult<Option<ClusterRowRef>> {
        let sql = "SELECT id, uuid FROM webserver_cluster
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL";
        let row = sqlx::query(sql)
            .bind(tenant_id)
            .bind(cluster_uuid)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("resolve webserver_cluster", error))?;
        row.map(|row| -> Result<ClusterRowRef, sqlx::Error> {
            Ok(ClusterRowRef {
                id: row.try_get("id")?,
                uuid: row.try_get("uuid")?,
            })
        })
        .transpose()
        .map_err(store_map_error)
    }

    pub(super) async fn resolve_cluster_host_row(
        &self,
        tenant_id: i64,
        host_uuid: &str,
    ) -> WebServiceResult<Option<ClusterRowRef>> {
        let sql = "SELECT id, uuid FROM webserver_cluster_host
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL";
        let row = sqlx::query(sql)
            .bind(tenant_id)
            .bind(host_uuid)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("resolve webserver_cluster_host", error))?;
        row.map(|row| -> Result<ClusterRowRef, sqlx::Error> {
            Ok(ClusterRowRef {
                id: row.try_get("id")?,
                uuid: row.try_get("uuid")?,
            })
        })
        .transpose()
        .map_err(store_map_error)
    }

    /// Resolves the registration target cluster: the cluster addressed by
    /// `code`, or the auto-provisioned `default` cluster when no code is
    /// supplied. The default cluster is created lazily exactly once.
    pub(super) async fn resolve_cluster_identity_repo(
        &self,
        tenant_id: i64,
        cluster_code: Option<&str>,
    ) -> WebServiceResult<ClusterIdentityRow> {
        const IDENTITY_COLUMNS: &str = "id, uuid, name, code, heartbeat_interval_seconds,
                offline_threshold_seconds";
        if let Some(code) = cluster_code {
            let sql = format!(
                "SELECT {IDENTITY_COLUMNS} FROM webserver_cluster
                 WHERE tenant_id = $1 AND code = $2 AND deleted_at IS NULL"
            );
            let row = sqlx::query(audited_sql(&sql))
                .bind(tenant_id)
                .bind(code)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| store_error("resolve webserver_cluster by code", error))?
                .ok_or_else(|| WebServiceError::not_found("cluster not found"))?;
            return map_cluster_identity_row(&row);
        }

        let select_default = format!(
            "SELECT {IDENTITY_COLUMNS} FROM webserver_cluster
             WHERE tenant_id = $1 AND code = 'default' AND deleted_at IS NULL"
        );
        let existing = sqlx::query(audited_sql(&select_default))
            .bind(tenant_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("resolve default webserver_cluster", error))?;
        if let Some(row) = existing {
            return map_cluster_identity_row(&row);
        }

        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();
        let now_expression = instant_write_expression("$4");
        let insert = format!(
            "INSERT INTO webserver_cluster (
                id, uuid, tenant_id, name, code, description, status,
                heartbeat_interval_seconds, offline_threshold_seconds, metadata,
                created_at, updated_at, version
            ) VALUES ($1, $2, $3, 'Default Cluster', 'default',
                'Auto-provisioned default cluster', 1, 15, 60,
                CAST('{{}}' AS JSONB), {now_expression}, {now_expression}, 0)
            ON CONFLICT (tenant_id, code) WHERE deleted_at IS NULL DO NOTHING"
        );
        sqlx::query(audited_sql(&insert))
            .bind(id)
            .bind(&uuid)
            .bind(tenant_id)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("insert default webserver_cluster", error))?;
        let row = sqlx::query(audited_sql(&select_default))
            .bind(tenant_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("reload default webserver_cluster", error))?
            .ok_or_else(|| {
                WebServiceError::Internal("default cluster disappeared during provisioning".into())
            })?;
        map_cluster_identity_row(&row)
    }

    pub(super) async fn upsert_cluster_host_repo(
        &self,
        write: &sdkwork_intelligence_webserver_service::ClusterHostUpsert,
    ) -> WebServiceResult<ClusterUpsertRow> {
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();
        let local_ips = serde_json::to_string(&write.local_ips)
            .map_err(|error| WebServiceError::Internal(format!("encode localIps: {error}")))?;
        let mac_addresses = serde_json::to_string(&write.mac_addresses)
            .map_err(|error| WebServiceError::Internal(format!("encode macAddresses: {error}")))?;
        let local_ips_expression = json_write_expression("$16");
        let mac_addresses_expression = json_write_expression("$17");
        // `$19` is `join_mode` (an INT) and `$20`/`$21` are the tunnel columns,
        // so the three timestamp columns need a placeholder of their own. They
        // used to reuse `$19`, which made Postgres receive an `i32` for
        // `last_heartbeat_at` / `created_at` / `updated_at` and fail with
        // `cannot cast type integer to timestamp with time zone` — a masked 500
        // on every cluster host upsert.
        //
        // `name` is absent from the `DO UPDATE` list for the same reason as on
        // the instance upsert: it seeds the row on first registration and is
        // then the operator's to rename (`update_cluster_host_repo` writes
        // `name = COALESCE($3, name)`), while `host_display_name` re-derives a
        // default from the node's own report on every registration. Writing it
        // back made a console rename a value that silently reverted on the next
        // node restart — which is exactly what maintenance work does, so the
        // two resources disagreed about who owns the same field. `hostname` and
        // `machine_code` stay: those are the machine's own facts.
        let now_expression = instant_write_expression("$22");
        let sql = format!(
            "INSERT INTO webserver_cluster_host (
                id, uuid, tenant_id, cluster_id, name, hostname, machine_code,
                os_name, os_version, kernel_version, arch, cpu_model, cpu_cores,
                memory_total_mb, remote_ip, local_ips, mac_addresses, daemon_version,
                join_mode, tunnel_route_domain, tunnel_endpoint,
                status, last_heartbeat_at, metadata, created_at, updated_at, version
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7,
                $8, $9, $10, $11, $12, $13,
                $14, $15, {local_ips_expression}, {mac_addresses_expression}, $18,
                $19, $20, $21,
                1, {now_expression}, CAST('{{}}' AS JSONB), {now_expression}, {now_expression}, 0
            )
            ON CONFLICT (tenant_id, machine_code) WHERE deleted_at IS NULL DO UPDATE SET
                cluster_id = EXCLUDED.cluster_id,
                hostname = EXCLUDED.hostname,
                os_name = EXCLUDED.os_name,
                os_version = EXCLUDED.os_version,
                kernel_version = EXCLUDED.kernel_version,
                arch = EXCLUDED.arch,
                cpu_model = EXCLUDED.cpu_model,
                cpu_cores = EXCLUDED.cpu_cores,
                memory_total_mb = EXCLUDED.memory_total_mb,
                remote_ip = EXCLUDED.remote_ip,
                local_ips = EXCLUDED.local_ips,
                mac_addresses = EXCLUDED.mac_addresses,
                daemon_version = EXCLUDED.daemon_version,
                join_mode = EXCLUDED.join_mode,
                tunnel_route_domain = EXCLUDED.tunnel_route_domain,
                tunnel_endpoint = EXCLUDED.tunnel_endpoint,
                updated_at = EXCLUDED.updated_at,
                version = webserver_cluster_host.version + 1
            RETURNING id, uuid, (xmax = 0) AS inserted"
        );
        let row = sqlx::query(audited_sql(&sql))
            .bind(id)
            .bind(&uuid)
            .bind(write.tenant_id)
            .bind(write.cluster_id)
            .bind(&write.name)
            .bind(&write.hostname)
            .bind(&write.machine_code)
            .bind(&write.os_name)
            .bind(&write.os_version)
            .bind(&write.kernel_version)
            .bind(&write.arch)
            .bind(&write.cpu_model)
            .bind(write.cpu_cores)
            .bind(write.memory_total_mb)
            .bind(&write.remote_ip)
            .bind(&local_ips)
            .bind(&mac_addresses)
            .bind(&write.daemon_version)
            .bind(write.join_mode)
            .bind(&write.tunnel_route_domain)
            .bind(&write.tunnel_endpoint)
            .bind(&now)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("upsert webserver_cluster_host", error))?;
        Ok(ClusterUpsertRow {
            id: row.try_get("id").map_err(store_map_error)?,
            uuid: row.try_get("uuid").map_err(store_map_error)?,
            created: row.try_get::<bool, _>("inserted").unwrap_or(true),
        })
    }

    /// Registers one instance against its **listening slot** and returns the
    /// row that owns it.
    ///
    /// Identity is `(tenant_id, host_id, instance_key)`, where `instance_key`
    /// is the generated `bindHost:bindPort` slot (migration
    /// `0018_webserver_cluster_instance_slot_identity`). A restarted process
    /// reports a new pid, a new start instant, and the same slot, so it lands on
    /// the existing row: `id` and `uuid` are fixed for the lifetime of the slot
    /// and are deliberately absent from the `DO UPDATE` list. Only run-state
    /// (pid, started-at, uptime, metrics, heartbeat bookkeeping) is rewritten,
    /// and `restart_count` bumps when `process_started_at` moves.
    ///
    /// This replaced a `(tenant_id, host_id, process_pid)` conflict target. A
    /// pid is never stable across restarts, so that target never matched and
    /// every restart inserted a fresh row with a fresh uuid — the defect that
    /// turned a single dev edge into a twenty-row instance inventory.
    pub(super) async fn upsert_cluster_instance_repo(
        &self,
        write: &sdkwork_intelligence_webserver_service::ClusterInstanceUpsert,
    ) -> WebServiceResult<ClusterUpsertRow> {
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();
        let metadata = json!({ "instanceTokenHash": write.instance_token_hash }).to_string();
        let started_expression = instant_write_expression("$10");
        // The bind chain is positional, so the timestamps, `metrics`, and
        // `metadata` placeholders must follow the tunnel columns rather than
        // reusing `$15`/`$16`. Sharing `$15` with `join_mode` handed Postgres an
        // `i32` for every TIMESTAMPTZ column (`cannot cast type integer to
        // timestamp with time zone`), and sharing `$16` with
        // `tunnel_route_domain` handed it a string for `metrics` — a masked 500
        // on every cluster instance upsert.
        let now_expression = instant_write_expression("$17");
        let metrics_expression = json_write_expression("$18");
        let metadata_expression = json_write_expression("$19");
        // `name` is absent from the `DO UPDATE` list on purpose: it seeds the
        // row on first registration and is then the operator's to rename. The
        // registration path re-derives a default name on every heartbeat-cycle
        // re-registration, so writing it back would erase an operator rename the
        // next time the process restarted.
        //
        // `public_endpoint` is the third operator-writable field on this row
        // (`update_cluster_instance_repo` writes `COALESCE($5, public_endpoint)`),
        // so it follows the same rule, in the weaker form the field allows: the
        // node's value wins *when it reports one*, but a node that reports
        // nothing does not get to erase one it never knew. Reporting nothing is
        // the common case - the address comes from an optional
        // `SDKWORK_WEBSERVER_INSTANCE_PUBLIC_ENDPOINT`, and an instance behind
        // NAT is exactly the one an operator pins by hand - so an outright
        // overwrite made the pin a value that reverted on every restart.
        //
        // `status` follows the same ownership rule as the heartbeat: a restart
        // (which is exactly what maintenance work does) must not end an
        // operator's maintenance mark. The literal `5` is the maintenance code
        // the SQL cannot name through `cluster_operator_owned_status` — an
        // `INSERT … ON CONFLICT` cannot call a Rust function — so the value is
        // pinned by `postgres_registration_preserves_operator_maintenance` in
        // `tests/cluster_repository.rs`.
        let sql = format!(
            "INSERT INTO webserver_cluster_instance (
                id, uuid, tenant_id, host_id, cluster_id, name, role, environment,
                process_pid, process_started_at, bind_host, bind_port, public_endpoint,
                build_version, join_mode, tunnel_route_domain,
                status, health_state, last_heartbeat_at, last_online_at,
                uptime_seconds, metrics, metadata, created_at, updated_at, version
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8,
                $9, {started_expression}, $11, $12, $13,
                $14, $15, $16,
                1, 'UNKNOWN', {now_expression}, {now_expression},
                0, {metrics_expression}, {metadata_expression}, {now_expression}, {now_expression}, 0
            )
            ON CONFLICT (tenant_id, host_id, instance_key)
              WHERE deleted_at IS NULL AND instance_key IS NOT NULL
            DO UPDATE SET
                cluster_id = EXCLUDED.cluster_id,
                role = EXCLUDED.role,
                environment = EXCLUDED.environment,
                process_pid = EXCLUDED.process_pid,
                process_started_at = EXCLUDED.process_started_at,
                restart_count = CASE WHEN webserver_cluster_instance.process_started_at
                                          IS DISTINCT FROM EXCLUDED.process_started_at
                                     THEN webserver_cluster_instance.restart_count + 1
                                     ELSE webserver_cluster_instance.restart_count END,
                last_restarted_at = CASE WHEN webserver_cluster_instance.process_started_at
                                          IS DISTINCT FROM EXCLUDED.process_started_at
                                         THEN EXCLUDED.updated_at
                                         ELSE webserver_cluster_instance.last_restarted_at END,
                join_mode = EXCLUDED.join_mode,
                tunnel_route_domain = EXCLUDED.tunnel_route_domain,
                bind_host = EXCLUDED.bind_host,
                bind_port = EXCLUDED.bind_port,
                public_endpoint = COALESCE(EXCLUDED.public_endpoint, webserver_cluster_instance.public_endpoint),
                build_version = EXCLUDED.build_version,
                status = CASE WHEN webserver_cluster_instance.status = 5 THEN 5 ELSE 1 END,
                health_state = 'UNKNOWN',
                last_heartbeat_at = EXCLUDED.last_heartbeat_at,
                last_online_at = EXCLUDED.last_online_at,
                uptime_seconds = 0,
                metrics = EXCLUDED.metrics,
                metadata = webserver_cluster_instance.metadata || EXCLUDED.metadata,
                updated_at = EXCLUDED.updated_at,
                version = webserver_cluster_instance.version + 1
            RETURNING id, uuid, (xmax = 0) AS inserted"
        );
        let row = sqlx::query(audited_sql(&sql))
            .bind(id)
            .bind(&uuid)
            .bind(write.tenant_id)
            .bind(write.host_id)
            .bind(write.cluster_id)
            .bind(&write.name)
            .bind(&write.role)
            .bind(&write.environment)
            .bind(write.process_pid)
            .bind(&write.process_started_at)
            .bind(&write.bind_host)
            .bind(write.bind_port)
            .bind(&write.public_endpoint)
            .bind(&write.build_version)
            .bind(write.join_mode)
            .bind(&write.tunnel_route_domain)
            .bind(&now)
            .bind(json!({}).to_string())
            .bind(&metadata)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("upsert webserver_cluster_instance", error))?;
        Ok(ClusterUpsertRow {
            id: row.try_get("id").map_err(store_map_error)?,
            uuid: row.try_get("uuid").map_err(store_map_error)?,
            created: row.try_get::<bool, _>("inserted").unwrap_or(true),
        })
    }

    pub(super) async fn authenticate_cluster_instance_token_repo(
        &self,
        token_hash: &str,
    ) -> WebServiceResult<Option<ClusterInstanceAuthRow>> {
        // Containment (not `->>` extraction) so the metadata GIN index serves
        // the lookup; an extraction predicate would seq-scan the table on
        // every cluster machine authentication.
        let sql = "SELECT i.id, i.uuid, i.host_id, i.cluster_id, i.tenant_id,
                ch.uuid AS host_uuid, c.uuid AS cluster_uuid,
                c.heartbeat_interval_seconds, c.offline_threshold_seconds,
                i.desired_config_revision, i.applied_config_revision,
                i.desired_applications_revision, i.applied_applications_revision,
                i.sync_status, i.routing_enabled, i.draining
             FROM webserver_cluster_instance i
             JOIN webserver_cluster c ON c.id = i.cluster_id
             JOIN webserver_cluster_host ch ON ch.id = i.host_id
             WHERE i.metadata @> CAST($1 AS JSONB)
             LIMIT 1";
        let credential = json!({ "instanceTokenHash": token_hash }).to_string();
        let row = sqlx::query(sql)
            .bind(credential)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                store_error("authenticate webserver_cluster_instance token", error)
            })?;
        Ok(row.map(|row| map_cluster_instance_auth_row(&row)))
    }

    /// Resolves an instance row by uuid (the machine principal subject). The
    /// heartbeat route authenticates the `winst_` token through the framework
    /// layer and hands the subject id to the service.
    pub(super) async fn resolve_cluster_instance_by_uuid_repo(
        &self,
        instance_uuid: &str,
    ) -> WebServiceResult<Option<ClusterInstanceAuthRow>> {
        let sql = "SELECT i.id, i.uuid, i.host_id, i.cluster_id, i.tenant_id,
                ch.uuid AS host_uuid, c.uuid AS cluster_uuid,
                c.heartbeat_interval_seconds, c.offline_threshold_seconds,
                i.desired_config_revision, i.applied_config_revision,
                i.desired_applications_revision, i.applied_applications_revision,
                i.sync_status, i.routing_enabled, i.draining
             FROM webserver_cluster_instance i
             JOIN webserver_cluster c ON c.id = i.cluster_id
             JOIN webserver_cluster_host ch ON ch.id = i.host_id
             WHERE i.tenant_id = 0 AND i.uuid = $1 AND i.deleted_at IS NULL";
        let row = sqlx::query(sql)
            .bind(instance_uuid)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("resolve webserver_cluster_instance", error))?;
        Ok(row.map(|row| map_cluster_instance_auth_row(&row)))
    }

    /// Records one heartbeat atomically: instance liveness/health, an
    /// append-only sample, and the host touch. Returns the transition
    /// information the service needs for lifecycle events.
    pub(super) async fn record_cluster_heartbeat_repo(
        &self,
        write: &sdkwork_intelligence_webserver_service::ClusterHeartbeatWrite,
    ) -> WebServiceResult<ClusterHeartbeatTransitionRow> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin cluster heartbeat", error))?;

        let previous = sqlx::query(
            "SELECT status, health_state FROM webserver_cluster_instance
             WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL FOR UPDATE",
        )
        .bind(write.instance_id)
        .bind(write.tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| store_error("lock webserver_cluster_instance", error))?
        .ok_or_else(|| WebServiceError::not_found("cluster instance not found"))?;
        let previous_status: i32 = previous.try_get("status").map_err(store_map_error)?;
        let previous_health: String = previous
            .try_get("health_state")
            .map_err(store_map_error)?;
        // The node always claims `online` while its process runs, so the
        // reported status is a proposal, not a fact: an operator's maintenance
        // mark must survive it (otherwise the mark, and the routing exclusion
        // that hangs off `status = online`, lasted one heartbeat interval).
        let recorded_status = sdkwork_intelligence_webserver_service::cluster_operator_owned_status(
            previous_status,
            write.status,
        );

        let now_expression = instant_write_expression("$4");
        let metrics_expression = json_write_expression("$6");
        let update_sql = format!(
            "UPDATE webserver_cluster_instance SET
                status = $3,
                health_state = $5,
                last_heartbeat_at = {now_expression},
                last_online_at = CASE WHEN status <> 1 AND $3 = 1 THEN {now_expression}
                                      ELSE last_online_at END,
                uptime_seconds = $7,
                metrics = {metrics_expression},
                build_version = COALESCE($8, build_version),
                quality_score = COALESCE($9, quality_score),
                updated_at = {now_expression},
                version = version + 1
             WHERE id = $1 AND tenant_id = $2"
        );
        sqlx::query(audited_sql(&update_sql))
            .bind(write.instance_id)
            .bind(write.tenant_id)
            .bind(recorded_status)
            .bind(&write.reported_at)
            .bind(&write.health_state)
            .bind(&write.metrics_json)
            .bind(write.uptime_seconds)
            .bind(&write.build_version)
            .bind(write.quality_score)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("heartbeat webserver_cluster_instance", error))?;

        let sample_id = next_id(self.id_generator())?;
        let sample_uuid = new_uuid();
        // Positional parameters: $1 id, $2 uuid, $3 tenant, $4 instance,
        // $5 host, $6 status, $7 metrics, $8 reported_at.
        let sample_metrics = json_write_expression("$7");
        let reported_expression = instant_write_expression("$8");
        let sample_sql = format!(
            "INSERT INTO webserver_cluster_heartbeat (
                id, uuid, tenant_id, instance_id, host_id, status, latency_ms, metrics,
                reported_at, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, NULL, {sample_metrics}, {reported_expression}, {reported_expression})"
        );
        sqlx::query(audited_sql(&sample_sql))
            .bind(sample_id)
            .bind(&sample_uuid)
            .bind(write.tenant_id)
            .bind(write.instance_id)
            .bind(write.host_id)
            .bind(write.status)
            .bind(&write.metrics_json)
            .bind(&write.reported_at)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("insert webserver_cluster_heartbeat", error))?;

        // Host liveness follows its members without overriding an operator's
        // maintenance (4) or deploying (2) state; error hosts recover on a
        // healthy heartbeat.
        let reported_expression = instant_write_expression("$3");
        let host_sql = format!(
            "UPDATE webserver_cluster_host SET
                status = CASE WHEN status IN (0, 3) THEN 1 ELSE status END,
                last_heartbeat_at = {reported_expression}, updated_at = {reported_expression}, version = version + 1
             WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL"
        );
        sqlx::query(audited_sql(&host_sql))
            .bind(write.host_id)
            .bind(write.tenant_id)
            .bind(&write.reported_at)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("heartbeat webserver_cluster_host", error))?;

        tx.commit()
            .await
            .map_err(|error| store_error("commit cluster heartbeat", error))?;

        Ok(ClusterHeartbeatTransitionRow {
            previous_status,
            previous_health_state: previous_health,
            status: recorded_status,
        })
    }

    pub(super) async fn list_cluster_peers_repo(
        &self,
        exclude_instance_uuid: &str,
        limit: i32,
    ) -> WebServiceResult<Vec<ClusterPeer>> {
        let sql = "SELECT i.uuid, i.name, i.role, i.status, i.environment, i.build_version,
                i.public_endpoint, CAST(i.last_heartbeat_at AS TEXT) AS last_heartbeat_at,
                i.join_mode, i.tunnel_route_domain,
                ch.name AS host_name, ch.remote_ip
             FROM webserver_cluster_instance i
             JOIN webserver_cluster_host ch ON ch.id = i.host_id
             WHERE i.tenant_id = 0 AND i.deleted_at IS NULL AND i.uuid <> $1
             ORDER BY i.status DESC, i.last_heartbeat_at DESC NULLS LAST, i.id DESC
             LIMIT $2";
        let rows = sqlx::query(sql)
            .bind(exclude_instance_uuid)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list webserver_cluster peers", error))?;
        let mut peers = Vec::with_capacity(rows.len());
        for row in &rows {
            peers.push(map_cluster_peer_row(row).map_err(|error| {
                WebServiceError::Internal(format!("map webserver_cluster peer row: {error}"))
            })?);
        }
        Ok(peers)
    }

    /// Enqueues peer messages, materializing one row per target member so
    /// every member claims its own copy. `to_instance_uuid: None` broadcasts
    /// to every ONLINE member of the cluster at enqueue time (members that
    /// register later do not retroactively receive it). Bounded to
    /// [`CLUSTER_PEER_ENQUEUE_TARGET_LIMIT`] targets; ids come from the
    /// generator one row at a time so they can never collide with it.
    pub(super) async fn enqueue_cluster_peer_messages_repo(
        &self,
        write: &sdkwork_intelligence_webserver_service::ClusterPeerMessageEnqueue<'_>,
    ) -> WebServiceResult<u64> {
        let cluster: i64 = sqlx::query_scalar(
            "SELECT id FROM webserver_cluster WHERE tenant_id = 0 AND uuid = $1 AND deleted_at IS NULL",
        )
        .bind(write.cluster_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve webserver_cluster for enqueue", error))?
        .ok_or_else(|| WebServiceError::not_found("cluster not found"))?;
        let from_instance: Option<i64> = match write.from_instance_uuid {
            Some(uuid) => sqlx::query_scalar(
                "SELECT id FROM webserver_cluster_instance
                 WHERE tenant_id = 0 AND uuid = $1 AND deleted_at IS NULL",
            )
            .bind(uuid)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                store_error("resolve webserver_cluster_instance sender", error)
            })?,
            None => None,
        };
        let targets: Vec<i64> = match write.to_instance_uuid {
            Some(to_uuid) => vec![
                sqlx::query_scalar(
                    "SELECT id FROM webserver_cluster_instance
                     WHERE tenant_id = 0 AND uuid = $1 AND deleted_at IS NULL",
                )
                .bind(to_uuid)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| {
                    store_error("resolve webserver_cluster_instance target", error)
                })?
                .ok_or_else(|| WebServiceError::not_found("cluster instance not found"))?,
            ],
            None => sqlx::query_scalar(
                "SELECT id FROM webserver_cluster_instance
                 WHERE tenant_id = 0 AND cluster_id = $1
                   AND deleted_at IS NULL AND status = 1",
            )
            .bind(cluster)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list online cluster members", error))?
            .into_iter()
            .take(CLUSTER_PEER_ENQUEUE_TARGET_LIMIT)
            .collect(),
        };
        if targets.is_empty() {
            return Ok(0);
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin peer message enqueue", error))?;
        let payload_expression = json_write_expression("$5");
        let deliver_expression = instant_write_expression("$6");
        let expires_expression = instant_write_expression("$7");
        let sql = format!(
            "INSERT INTO webserver_cluster_peer_message (
                id, uuid, tenant_id, cluster_id, from_instance_id, to_instance_id,
                message_type, payload, state, deliver_at, expires_at, created_at, updated_at, version
            ) VALUES (
                $1, $2, 0, $3, $4, $8, $9, {payload_expression}, 'PENDING',
                {deliver_expression}, {expires_expression}, {deliver_expression}, {deliver_expression}, 0
            )"
        );
        let mut inserted = 0_u64;
        for target in &targets {
            let id = next_id(self.id_generator())?;
            let result = sqlx::query(audited_sql(&sql))
                .bind(id)
                .bind(new_uuid())
                .bind(cluster)
                .bind(from_instance)
                .bind(write.payload_json)
                .bind(write.deliver_at)
                .bind(write.expires_at)
                .bind(target)
                .bind(write.message_type)
                .execute(&mut *tx)
                .await
                .map_err(|error| store_error("enqueue webserver_cluster_peer_message", error))?;
            inserted += result.rows_affected();
        }
        tx.commit()
            .await
            .map_err(|error| store_error("commit peer message enqueue", error))?;
        Ok(inserted)
    }

    /// Claims and marks delivered the pending peer mailbox messages addressed
    /// to one instance (direct or broadcast). At-most-once handoff: rows are
    /// marked DELIVERED inside the claiming transaction.
    pub(super) async fn claim_cluster_peer_messages_repo(
        &self,
        instance_id: i64,
        limit: i32,
        now: &str,
    ) -> WebServiceResult<Vec<ClusterPeerMessage>> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin peer message claim", error))?;
        let delivered_expression = instant_write_expression("$4");
        let deliver_expression = instant_write_expression("$2");
        // The UPDATE...FROM list cannot reference the target table, so the
        // sender/receiver uuids resolve through RETURNING subqueries instead
        // of joins.
        let sql = format!(
            "WITH claimed AS (
                SELECT m.id FROM webserver_cluster_peer_message m
                WHERE m.tenant_id = 0 AND m.state = 'PENDING' AND m.deliver_at <= {deliver_expression}
                  AND (m.to_instance_id IS NULL OR m.to_instance_id = $1)
                ORDER BY m.deliver_at, m.id
                LIMIT $3
                FOR UPDATE SKIP LOCKED
             )
             UPDATE webserver_cluster_peer_message m
             SET state = 'DELIVERED', delivered_at = {delivered_expression},
                 updated_at = {delivered_expression}, version = m.version + 1
             FROM claimed
             WHERE m.id = claimed.id
             RETURNING m.uuid, m.message_type, CAST(m.payload AS TEXT) AS payload,
                       (SELECT i.uuid FROM webserver_cluster_instance i
                         WHERE i.id = m.from_instance_id) AS from_uuid,
                       (SELECT i.uuid FROM webserver_cluster_instance i
                         WHERE i.id = m.to_instance_id) AS to_uuid,
                       CAST(m.created_at AS TEXT) AS created_at"
        );
        let rows = sqlx::query(audited_sql(&sql))
            .bind(instance_id)
            .bind(now)
            .bind(limit)
            .bind(now)
            .fetch_all(&mut *tx)
            .await
            .map_err(|error| store_error("claim webserver_cluster_peer_message", error))?;
        tx.commit()
            .await
            .map_err(|error| store_error("commit peer message claim", error))?;

        let mut messages = Vec::with_capacity(rows.len());
        for row in &rows {
            messages.push(ClusterPeerMessage {
                id: row.try_get("uuid").map_err(store_map_error)?,
                message_type: row.try_get("message_type").map_err(store_map_error)?,
                from_instance_id: row.try_get("from_uuid").map_err(store_map_error)?,
                to_instance_id: row.try_get("to_uuid").map_err(store_map_error)?,
                payload: row
                    .try_get::<Option<String>, _>("payload")
                    .map_err(store_map_error)?
                    .and_then(|raw| serde_json::from_str(&raw).ok())
                    .unwrap_or_else(|| json!({})),
                created_at: row.try_get("created_at").map_err(store_map_error)?,
            });
        }
        Ok(messages)
    }

    pub(super) async fn insert_cluster_event_repo(
        &self,
        write: &sdkwork_intelligence_webserver_service::ClusterEventWrite<'_>,
    ) -> WebServiceResult<()> {
        let id = next_id(self.id_generator())?;
        let event_uuid = new_uuid();
        // PostgreSQL parameters are positional: placeholder numbers MUST match
        // the bind order below ($1 id, $2 uuid, $3 tenant, $4..$6 parent
        // uuids, $7..$10 payload, $11 occurred_at).
        let detail_expression = json_write_expression("$10");
        let occurred_expression = instant_write_expression("$11");
        // Foreign keys resolve from the uuid plane; a dangling uuid surfaces as
        // a NOT NULL violation on cluster_id and is rejected by the store.
        let sql = format!(
            "INSERT INTO webserver_cluster_event (
                id, uuid, tenant_id, cluster_id, host_id, instance_id, event_type, severity,
                message, detail, occurred_at, created_at
            ) VALUES (
                $1, $2, $3,
                (SELECT id FROM webserver_cluster WHERE tenant_id = $3 AND uuid = $4),
                (SELECT id FROM webserver_cluster_host WHERE tenant_id = $3 AND uuid = $5),
                (SELECT id FROM webserver_cluster_instance WHERE tenant_id = $3 AND uuid = $6),
                $7, $8, $9, {detail_expression}, {occurred_expression}, {occurred_expression}
            )"
        );
        sqlx::query(audited_sql(&sql))
            .bind(id)
            .bind(&event_uuid)
            .bind(write.tenant_id)
            .bind(write.cluster_uuid)
            .bind(write.host_uuid)
            .bind(write.instance_uuid)
            .bind(write.event_type)
            .bind(write.severity)
            .bind(write.message)
            .bind(write.detail_json)
            .bind(write.occurred_at)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("insert webserver_cluster_event", error))?;
        Ok(())
    }

    /// Expires instances that stopped heartbeating; returns the expired rows so
    /// the service can write INSTANCE_OFFLINE events. Bounded batches keep the
    /// sweep O(batch) per tick (PAGINATION_SPEC §2.5).
    pub(super) async fn expire_stale_cluster_instances_repo(
        &self,
        now: &str,
        limit: i32,
    ) -> WebServiceResult<Vec<ExpiredClusterInstanceRow>> {
        let now_expression = instant_write_expression("$1");
        // RETURNING can only see the UPDATE target and the FROM list, so the
        // stale selection (with its cluster/host uuids) lives in a CTE.
        let sql = format!(
            "WITH stale AS (
                SELECT i2.id, i2.uuid, i2.name, c.uuid AS cluster_uuid, ch.uuid AS host_uuid
                FROM webserver_cluster_instance i2
                JOIN webserver_cluster c ON c.id = i2.cluster_id
                JOIN webserver_cluster_host ch ON ch.id = i2.host_id
                WHERE i2.tenant_id = 0 AND i2.deleted_at IS NULL AND i2.status = 1
                  AND i2.last_heartbeat_at IS NOT NULL
                  AND i2.last_heartbeat_at
                      < {now_expression} - (c.offline_threshold_seconds * INTERVAL '1 second')
                ORDER BY i2.last_heartbeat_at
                LIMIT $2
             )
             UPDATE webserver_cluster_instance i
             SET status = 0, updated_at = {now_expression}, version = i.version + 1
             FROM stale
             WHERE i.id = stale.id
             RETURNING stale.uuid, stale.name, stale.cluster_uuid, stale.host_uuid"
        );
        let rows = sqlx::query(audited_sql(&sql))
            .bind(now)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("expire webserver_cluster_instance", error))?;
        let mut expired = Vec::with_capacity(rows.len());
        for row in &rows {
            expired.push(ExpiredClusterInstanceRow {
                instance_uuid: row.try_get("uuid").map_err(store_map_error)?,
                name: row.try_get("name").map_err(store_map_error)?,
                cluster_uuid: row.try_get("cluster_uuid").map_err(store_map_error)?,
                host_uuid: row.try_get("host_uuid").map_err(store_map_error)?,
            });
        }
        Ok(expired)
    }

    /// Expires hosts that stopped heartbeating and no longer have any online
    /// instance; returns the expired rows for HOST_OFFLINE events.
    pub(super) async fn expire_stale_cluster_hosts_repo(
        &self,
        now: &str,
        limit: i32,
    ) -> WebServiceResult<Vec<ExpiredClusterHostRow>> {
        let now_expression = instant_write_expression("$1");
        let sql = format!(
            "WITH stale AS (
                SELECT h2.id, h2.uuid, h2.name, c.uuid AS cluster_uuid
                FROM webserver_cluster_host h2
                JOIN webserver_cluster c ON c.id = h2.cluster_id
                WHERE h2.tenant_id = 0 AND h2.deleted_at IS NULL AND h2.status = 1
                  AND h2.last_heartbeat_at IS NOT NULL
                  AND h2.last_heartbeat_at
                      < {now_expression} - (c.offline_threshold_seconds * INTERVAL '1 second')
                  AND NOT EXISTS (
                    SELECT 1 FROM webserver_cluster_instance i
                    WHERE i.host_id = h2.id AND i.deleted_at IS NULL AND i.status = 1
                  )
                ORDER BY h2.last_heartbeat_at
                LIMIT $2
             )
             UPDATE webserver_cluster_host h
             SET status = 0, updated_at = {now_expression}, version = h.version + 1
             FROM stale
             WHERE h.id = stale.id
             RETURNING stale.uuid, stale.name, stale.cluster_uuid"
        );
        let rows = sqlx::query(audited_sql(&sql))
            .bind(now)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("expire webserver_cluster_host", error))?;
        let mut expired = Vec::with_capacity(rows.len());
        for row in &rows {
            expired.push(ExpiredClusterHostRow {
                host_uuid: row.try_get("uuid").map_err(store_map_error)?,
                name: row.try_get("name").map_err(store_map_error)?,
                cluster_uuid: row.try_get("cluster_uuid").map_err(store_map_error)?,
            });
        }
        Ok(expired)
    }

    pub(super) async fn expire_cluster_peer_messages_repo(
        &self,
        now: &str,
        limit: i32,
    ) -> WebServiceResult<u64> {
        let now_expression = instant_write_expression("$1");
        let sql = format!(
            "UPDATE webserver_cluster_peer_message SET state = 'EXPIRED', updated_at = {now_expression}, version = version + 1
             WHERE id IN (
                SELECT id FROM webserver_cluster_peer_message
                WHERE tenant_id = 0 AND state = 'PENDING'
                  AND expires_at IS NOT NULL AND expires_at < {now_expression}
                ORDER BY expires_at
                LIMIT $2
             )"
        );
        let result = sqlx::query(audited_sql(&sql))
            .bind(now)
            .bind(limit)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("expire webserver_cluster_peer_message", error))?;
        Ok(result.rows_affected())
    }

    pub(super) async fn purge_cluster_heartbeats_repo(
        &self,
        older_than: &str,
        limit: i32,
    ) -> WebServiceResult<u64> {
        let older_expression = instant_write_expression("$1");
        let sql = format!(
            "DELETE FROM webserver_cluster_heartbeat
             WHERE id IN (
                SELECT id FROM webserver_cluster_heartbeat
                WHERE tenant_id = 0 AND reported_at < {older_expression}
                ORDER BY reported_at
                LIMIT $2
             )"
        );
        let result = sqlx::query(audited_sql(&sql))
            .bind(older_than)
            .bind(limit)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("purge webserver_cluster_heartbeat", error))?;
        Ok(result.rows_affected())
    }

    /// Records one active-probe outcome and drives auto-eject / auto-recover:
    /// `PROBE_FAILURES_FOR_EJECT` consecutive failures eject the instance
    /// (out of routing, status error); any success recovers it.
    pub(super) async fn record_cluster_probe_outcome_repo(
        &self,
        write: &sdkwork_intelligence_webserver_service::ClusterProbeWrite,
    ) -> WebServiceResult<sdkwork_intelligence_webserver_service::ClusterProbeOutcome> {
        const PROBE_FAILURES_FOR_EJECT: i32 = 3;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin cluster probe", error))?;

        let row = sqlx::query(
            "SELECT status, probe_failures, ejected_at IS NOT NULL AS ejected
             FROM webserver_cluster_instance
             WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL FOR UPDATE",
        )
        .bind(write.instance_id)
        .bind(write.tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| store_error("lock webserver_cluster_instance", error))?
        .ok_or_else(|| WebServiceError::not_found("cluster instance not found"))?;
        let status: i32 = row.try_get("status").map_err(store_map_error)?;
        let previous_failures: i32 = row.try_get("probe_failures").map_err(store_map_error)?;
        let previously_ejected: bool = row.try_get("ejected").map_err(store_map_error)?;

        let now = now_rfc3339();
        let now_expression = instant_write_expression("$5");
        let (failures, eject, recovered) = if write.healthy {
            (0, false, previously_ejected)
        } else {
            let failures = previous_failures + 1;
            (failures, failures >= PROBE_FAILURES_FOR_EJECT, false)
        };
        let eject_now = eject && !previously_ejected;
        let recover_now = recovered && previously_ejected;
        // The prober's `error` is a machine observation, so it yields to an
        // operator's maintenance mark exactly like the node's `online` claim.
        let ejected_status = sdkwork_intelligence_webserver_service::cluster_operator_owned_status(
            status,
            sdkwork_intelligence_webserver_service::CLUSTER_INSTANCE_STATUS_ERROR,
        );
        // `ejected_at` records the ejection instant, so a still-failing instance
        // keeps the instant it was ejected at: re-stamping it on every failed
        // probe made the ejection age unreadable and the column
        // indistinguishable from `updated_at`.
        let sql = format!(
            "UPDATE webserver_cluster_instance SET
                probe_failures = $3,
                ejected_at = CASE WHEN $4 THEN COALESCE(ejected_at, CAST($5 AS TIMESTAMPTZ)) ELSE NULL END,
                status = CASE
                    WHEN $4 THEN $7
                    WHEN $6 AND status = 4 THEN 1
                    ELSE status END,
                updated_at = {now_expression}, version = version + 1
             WHERE id = $1 AND tenant_id = $2"
        );
        sqlx::query(audited_sql(&sql))
            .bind(write.instance_id)
            .bind(write.tenant_id)
            .bind(failures)
            .bind(eject)
            .bind(&now)
            .bind(recover_now)
            .bind(ejected_status)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("probe webserver_cluster_instance", error))?;
        tx.commit()
            .await
            .map_err(|error| store_error("commit cluster probe", error))?;
        Ok(sdkwork_intelligence_webserver_service::ClusterProbeOutcome {
            failures,
            ejected: eject,
            eject_transition: eject_now,
            recovered: recover_now,
            was_online: status == 1,
        })
    }

    /// Node drain-completion acknowledgment: clears the drain flags and
    /// marks the instance stopped (offline). Idempotent.
    pub(super) async fn record_cluster_drain_complete_repo(
        &self,
        tenant_id: i64,
        instance_uuid: &str,
    ) -> WebServiceResult<()> {
        let now = now_rfc3339();
        let now_expression = instant_write_expression("$3");
        let sql = format!(
            "UPDATE webserver_cluster_instance SET
                draining = FALSE,
                drain_started_at = NULL,
                routing_enabled = FALSE,
                status = 0,
                updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL"
        );
        sqlx::query(audited_sql(&sql))
            .bind(tenant_id)
            .bind(instance_uuid)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("drain complete webserver_cluster_instance", error))?;
        Ok(())
    }

    /// Auto-discovery: routeable instance inventory for one cluster.
    pub(super) async fn discover_cluster_routing_repo(
        &self,
        cluster_code: &str,
    ) -> WebServiceResult<
        Option<sdkwork_intelligence_webserver_service::ClusterRoutingDiscovery>,
    > {
        let identity_sql = "SELECT id, uuid, lb_strategy,
                CAST(served_domains AS TEXT) AS served_domains
             FROM webserver_cluster
             WHERE tenant_id = 0 AND code = $1 AND deleted_at IS NULL";
        let cluster_row = sqlx::query(identity_sql)
            .bind(cluster_code)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("discover webserver_cluster", error))?;
        let Some(cluster_row) = cluster_row else {
            return Ok(None);
        };
        let cluster_id: i64 = cluster_row.try_get("id").map_err(store_map_error)?;
        let cluster_uuid: String = cluster_row.try_get("uuid").map_err(store_map_error)?;
        let lb_strategy: String = cluster_row.try_get("lb_strategy").map_err(store_map_error)?;
        let served_domains = string_vec_from_json(
            cluster_row
                .try_get::<Option<String>, _>("served_domains")
                .map_err(store_map_error)?,
        );

        let instance_sql = "SELECT i.uuid,
                    COALESCE(NULLIF(i.bind_host, '0.0.0.0'), '127.0.0.1') AS dial_host,
                    i.bind_port, i.routing_weight, i.quality_score
                 FROM webserver_cluster_instance i
                 WHERE i.tenant_id = 0 AND i.cluster_id = $1
                   AND i.deleted_at IS NULL AND i.status = 1
                   AND i.routing_enabled = TRUE AND i.draining = FALSE
                   AND i.ejected_at IS NULL
                 ORDER BY i.id";
        let rows = sqlx::query(instance_sql)
            .bind(cluster_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("discover webserver_cluster instances", error))?;
        let mut instances = Vec::with_capacity(rows.len());
        for row in &rows {
            let dial_host: String = row.try_get("dial_host").map_err(store_map_error)?;
            let bind_port: Option<i32> = row.try_get("bind_port").map_err(store_map_error)?;
            // Bind port is required for east-west routing; instances without
            // a bound port are skipped (they cannot receive routed traffic).
            let Some(port) = bind_port else {
                continue;
            };
            instances.push(sdkwork_intelligence_webserver_service::ClusterRoutingInstance {
                uuid: row.try_get("uuid").map_err(store_map_error)?,
                endpoint: format!("{dial_host}:{port}"),
                weight: row.try_get("routing_weight").map_err(store_map_error)?,
                quality_score: row.try_get("quality_score").map_err(store_map_error)?,
            });
        }
        Ok(Some(
            sdkwork_intelligence_webserver_service::ClusterRoutingDiscovery {
                cluster_uuid,
                lb_strategy,
                served_domains,
                instances,
            },
        ))
    }

    /// Latest desired revision per sync kind (config / applications).
    pub(super) async fn latest_cluster_sync_desired_repo(
        &self,
        tenant_id: i64,
        cluster_id: i64,
    ) -> WebServiceResult<Vec<sdkwork_intelligence_webserver_service::ClusterSyncDesired>> {
        let sql = "SELECT DISTINCT ON (kind) kind, revision
             FROM webserver_cluster_sync_revision
             WHERE tenant_id = $1 AND cluster_id = $2
             ORDER BY kind, created_at DESC, id DESC";
        let rows = sqlx::query(sql)
            .bind(tenant_id)
            .bind(cluster_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("latest webserver_cluster_sync_revision", error))?;
        let mut desired = Vec::with_capacity(rows.len());
        for row in &rows {
            desired.push(sdkwork_intelligence_webserver_service::ClusterSyncDesired {
                kind: row.try_get("kind").map_err(store_map_error)?,
                revision: row.try_get("revision").map_err(store_map_error)?,
            });
        }
        Ok(desired)
    }

    /// Payload of one stored sync revision (node fetch on drift).
    pub(super) async fn cluster_sync_revision_payload_repo(
        &self,
        tenant_id: i64,
        cluster_id: i64,
        kind: i32,
        revision: &str,
    ) -> WebServiceResult<
        Option<sdkwork_intelligence_webserver_service::ClusterSyncRevisionPayload>,
    > {
        let sql = "SELECT kind, revision, sha256, CAST(payload AS TEXT) AS payload,
                CAST(created_at AS TEXT) AS created_at
             FROM webserver_cluster_sync_revision
             WHERE tenant_id = $1 AND cluster_id = $2 AND kind = $3 AND revision = $4";
        let row = sqlx::query(sql)
            .bind(tenant_id)
            .bind(cluster_id)
            .bind(kind)
            .bind(revision)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("read webserver_cluster_sync_revision", error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let payload_text: String = row.try_get("payload").map_err(store_map_error)?;
        let payload: serde_json::Value =
            serde_json::from_str(&payload_text).unwrap_or(serde_json::Value::Null);
        Ok(Some(
            sdkwork_intelligence_webserver_service::ClusterSyncRevisionPayload {
                kind: row.try_get("kind").map_err(store_map_error)?,
                revision: row.try_get("revision").map_err(store_map_error)?,
                sha256: row.try_get("sha256").map_err(store_map_error)?,
                payload,
                created_at: row.try_get("created_at").map_err(store_map_error)?,
            },
        ))
    }

    /// Publishes one desired-state revision and flips every non-deleted
    /// instance of the cluster to PENDING with the new per-kind desired
    /// revision (single transaction).
    pub(super) async fn publish_cluster_sync_revision_repo(
        &self,
        write: &sdkwork_intelligence_webserver_service::ClusterSyncRevisionPublish,
    ) -> WebServiceResult<()> {
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();
        let payload = serde_json::to_string(&write.payload)
            .map_err(|error| WebServiceError::Internal(format!("encode payload: {error}")))?;
        let payload_expression = json_write_expression("$8");
        let created_expression = instant_write_expression("$10");
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin cluster sync publish", error))?;
        let insert_sql = format!(
            "INSERT INTO webserver_cluster_sync_revision (
                id, uuid, tenant_id, cluster_id, kind, revision, sha256, payload,
                size_bytes, created_by, created_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, {payload_expression},
                $9, $11, {created_expression}
            )
            ON CONFLICT (tenant_id, cluster_id, kind, revision) DO NOTHING"
        );
        sqlx::query(audited_sql(&insert_sql))
            .bind(id)
            .bind(&uuid)
            .bind(write.tenant_id)
            .bind(write.cluster_id)
            .bind(write.kind)
            .bind(&write.revision)
            .bind(&write.sha256)
            .bind(&payload)
            .bind(write.size_bytes)
            .bind(&write.created_by)
            .bind(&write.created_at)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("insert webserver_cluster_sync_revision", error))?;
        // Flip the cluster to the new desired revision (per-kind column).
        // An instance that already reports the new revision as applied
        // (re-registration) stays IN_SYNC; everything else goes PENDING.
        let flip_target = if write.kind == 1 {
            "desired_applications_revision"
        } else {
            "desired_config_revision"
        };
        let applied_target = if write.kind == 1 {
            "applied_applications_revision"
        } else {
            "applied_config_revision"
        };
        let flip_sql = format!(
            "UPDATE webserver_cluster_instance SET
                {flip_target} = $3,
                sync_status = CASE
                    WHEN {applied_target} = $3 THEN 1
                    ELSE 2 END,
                updated_at = CAST($4 AS TIMESTAMPTZ),
                version = version + 1
             WHERE tenant_id = $1 AND cluster_id = $2 AND deleted_at IS NULL"
        );
        sqlx::query(audited_sql(&flip_sql))
            .bind(write.tenant_id)
            .bind(write.cluster_id)
            .bind(&write.revision)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("flip webserver_cluster_instance sync", error))?;
        tx.commit()
            .await
            .map_err(|error| store_error("commit cluster sync publish", error))?;
        Ok(())
    }

    /// Records one instance sync acknowledgment: per-kind applied revision
    /// plus aggregate sync-status recompute against the desired state.
    pub(super) async fn record_cluster_sync_ack_repo(
        &self,
        write: &sdkwork_intelligence_webserver_service::ClusterSyncAckWrite,
    ) -> WebServiceResult<()> {
        let (applied_column, desired_column) = if write.kind == 1 {
            (
                "applied_applications_revision",
                "desired_applications_revision",
            )
        } else {
            ("applied_config_revision", "desired_config_revision")
        };
        let now_expression = instant_write_expression("$6");
        let sql = format!(
            "UPDATE webserver_cluster_instance SET
                {applied_column} = $3,
                sync_status = CASE
                    WHEN {desired_column} IS NOT NULL AND {desired_column} = $3
                      AND applied_config_revision IS NOT NULL
                      AND desired_config_revision = applied_config_revision
                      AND applied_applications_revision IS NOT NULL
                      AND desired_applications_revision = applied_applications_revision
                    THEN 1
                    WHEN $4 = 3 THEN 3
                    ELSE 2 END,
                updated_at = {now_expression},
                version = version + 1
             WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL"
        );
        sqlx::query(audited_sql(&sql))
            .bind(write.instance_id)
            .bind(write.tenant_id)
            .bind(&write.applied_revision)
            .bind(write.status)
            .bind(write.kind)
            .bind(&write.updated_at)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("ack webserver_cluster_instance sync", error))?;
        Ok(())
    }

    /// Resolves a cluster identity by uuid (admin sync publication).
    pub(super) async fn resolve_cluster_identity_by_uuid_repo(
        &self,
        tenant_id: i64,
        cluster_uuid: &str,
    ) -> WebServiceResult<ClusterIdentityRow> {
        const IDENTITY_COLUMNS: &str = "id, uuid, name, code, heartbeat_interval_seconds,
                offline_threshold_seconds";
        let sql = format!(
            "SELECT {IDENTITY_COLUMNS} FROM webserver_cluster
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL"
        );
        let row = sqlx::query(audited_sql(&sql))
            .bind(tenant_id)
            .bind(cluster_uuid)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("resolve webserver_cluster by uuid", error))?
            .ok_or_else(|| WebServiceError::not_found("cluster not found"))?;
        map_cluster_identity_row(&row)
    }
}

/// Hashes a cluster instance heartbeat token for metadata storage/lookup.
pub(crate) fn hash_cluster_instance_token(token: &str) -> String {
    sha256_hex(token)
}

fn store_map_error(error: sqlx::Error) -> WebServiceError {
    WebServiceError::Internal(format!("map cluster row: {error}"))
}

fn map_cluster_row(row: &EngineRow) -> Result<ClusterResponse, sqlx::Error> {
    Ok(ClusterResponse {
        id: row.try_get("uuid")?,
        name: row.try_get("name")?,
        code: row.try_get("code")?,
        description: row.try_get("description")?,
        status: row.try_get("status")?,
        heartbeat_interval_seconds: row.try_get("heartbeat_interval_seconds")?,
        offline_threshold_seconds: row.try_get("offline_threshold_seconds")?,
        lb_strategy: row.try_get("lb_strategy")?,
        served_domains: Some(string_vec_from_json(
            row.try_get::<Option<String>, _>("served_domains")?,
        )),
        host_count: row.try_get("host_count")?,
        instance_count: row.try_get("instance_count")?,
        online_instance_count: row.try_get("online_instance_count")?,
        created_at: instant_from_row(row, "created_at")?,
        updated_at: instant_from_row(row, "updated_at")?,
    })
}

fn string_vec_from_json(raw: Option<String>) -> Vec<String> {
    raw.and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| serde_json::from_value::<Vec<String>>(value).ok())
        .unwrap_or_default()
}

fn map_cluster_host_row(row: &EngineRow) -> Result<ClusterHostResponse, sqlx::Error> {
    let local_ips = string_vec_from_json(row.try_get::<Option<String>, _>("local_ips")?);
    let mac_addresses =
        string_vec_from_json(row.try_get::<Option<String>, _>("mac_addresses")?);
    Ok(ClusterHostResponse {
        id: row.try_get("uuid")?,
        cluster_id: row.try_get("cluster_uuid")?,
        name: row.try_get("name")?,
        hostname: row.try_get("hostname")?,
        machine_code: row.try_get("machine_code")?,
        os_name: row.try_get("os_name")?,
        os_version: row.try_get("os_version")?,
        kernel_version: row.try_get("kernel_version")?,
        arch: row.try_get("arch")?,
        cpu_model: row.try_get("cpu_model")?,
        cpu_cores: row.try_get("cpu_cores")?,
        memory_total_mb: row.try_get("memory_total_mb")?,
        remote_ip: row.try_get("remote_ip")?,
        local_ips,
        mac_addresses,
        daemon_version: row.try_get("daemon_version")?,
        status: row.try_get("status")?,
        join_mode: Some(
            sdkwork_webserver_contract::cluster_join_mode_label(
                row.try_get::<i32, _>("join_mode").unwrap_or(0),
            )
            .to_owned(),
        ),
        tunnel_route_domain: row.try_get("tunnel_route_domain")?,
        last_heartbeat_at: optional_instant_from_row(row, "last_heartbeat_at")?,
        instance_count: row.try_get("instance_count")?,
        created_at: instant_from_row(row, "created_at")?,
        updated_at: instant_from_row(row, "updated_at")?,
    })
}

fn map_cluster_instance_row(row: &EngineRow) -> Result<ClusterInstanceResponse, sqlx::Error> {
    let metrics = row
        .try_get::<Option<String>, _>("metrics")?
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or_else(|| json!({}));
    Ok(ClusterInstanceResponse {
        id: row.try_get("uuid")?,
        cluster_id: row.try_get("cluster_uuid")?,
        host_id: row.try_get("host_uuid")?,
        host_name: row.try_get("host_name")?,
        name: row.try_get("name")?,
        role: row.try_get("role")?,
        environment: row.try_get("environment")?,
        process_pid: row.try_get("process_pid")?,
        process_started_at: optional_instant_from_row(row, "process_started_at")?,
        bind_host: row.try_get("bind_host")?,
        bind_port: row.try_get("bind_port")?,
        public_endpoint: row.try_get("public_endpoint")?,
        build_version: row.try_get("build_version")?,
        status: row.try_get("status")?,
        health_state: row.try_get("health_state")?,
        last_heartbeat_at: optional_instant_from_row(row, "last_heartbeat_at")?,
        last_online_at: optional_instant_from_row(row, "last_online_at")?,
        uptime_seconds: row.try_get("uptime_seconds")?,
        metrics,
        join_mode: Some(
            sdkwork_webserver_contract::cluster_join_mode_label(
                row.try_get::<i32, _>("join_mode").unwrap_or(0),
            )
            .to_owned(),
        ),
        quality_score: row.try_get("quality_score")?,
        desired_config_revision: row.try_get("desired_config_revision")?,
        applied_config_revision: row.try_get("applied_config_revision")?,
        desired_applications_revision: row.try_get("desired_applications_revision")?,
        applied_applications_revision: row.try_get("applied_applications_revision")?,
        sync_status: Some(
            sdkwork_webserver_contract::cluster_sync_status_label(
                row.try_get::<i32, _>("sync_status").unwrap_or(0),
            )
            .to_owned(),
        ),
        routing_enabled: row.try_get("routing_enabled")?,
        draining: row.try_get("draining")?,
        ejected: row.try_get("ejected")?,
        restart_count: row.try_get::<Option<i32>, _>("restart_count")?,
        labels: row
            .try_get::<Option<String>, _>("labels")?
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .and_then(|value| {
                serde_json::from_value::<std::collections::BTreeMap<String, String>>(value).ok()
            }),
        routing_weight: row.try_get("routing_weight")?,
        maintenance_note: row.try_get("maintenance_note")?,
        probe_failures: row.try_get("probe_failures")?,
        probe_url: row.try_get("probe_url")?,
        created_at: instant_from_row(row, "created_at")?,
        updated_at: instant_from_row(row, "updated_at")?,
    })
}

fn map_cluster_event_row(row: &EngineRow) -> Result<ClusterEventResponse, sqlx::Error> {
    let detail = row
        .try_get::<Option<String>, _>("detail")?
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or_else(|| json!({}));
    Ok(ClusterEventResponse {
        id: row.try_get("uuid")?,
        cluster_id: row.try_get("cluster_uuid")?,
        host_id: row.try_get("host_uuid")?,
        instance_id: row.try_get("instance_uuid")?,
        event_type: row.try_get("event_type")?,
        severity: row.try_get("severity")?,
        message: row.try_get("message")?,
        detail,
        occurred_at: instant_from_row(row, "occurred_at")?,
        created_at: instant_from_row(row, "created_at")?,
    })
}

fn map_cluster_peer_row(row: &EngineRow) -> Result<ClusterPeer, sqlx::Error> {
    let join_mode = row
        .try_get::<Option<i32>, _>("join_mode")
        .ok()
        .flatten()
        .map(|value| {
            sdkwork_webserver_contract::cluster_join_mode_label(value).to_owned()
        });
    Ok(ClusterPeer {
        join_mode,
        tunnel_route_domain: row.try_get("tunnel_route_domain")?,
        instance_id: row.try_get("uuid")?,
        name: row.try_get("name")?,
        role: row.try_get("role")?,
        status: row.try_get("status")?,
        host_name: row.try_get("host_name")?,
        remote_ip: row.try_get("remote_ip")?,
        public_endpoint: row.try_get("public_endpoint")?,
        environment: row.try_get("environment")?,
        build_version: row.try_get("build_version")?,
        last_heartbeat_at: optional_instant_from_row(row, "last_heartbeat_at")?,
    })
}

fn map_cluster_instance_auth_row(row: &EngineRow) -> ClusterInstanceAuthRow {
    ClusterInstanceAuthRow {
        instance_id: row.try_get("id").unwrap_or(0),
        instance_uuid: row.try_get("uuid").unwrap_or_default(),
        host_id: row.try_get("host_id").unwrap_or(0),
        host_uuid: row.try_get("host_uuid").unwrap_or_default(),
        cluster_id: row.try_get("cluster_id").unwrap_or(0),
        cluster_uuid: row.try_get("cluster_uuid").unwrap_or_default(),
        tenant_id: row.try_get("tenant_id").unwrap_or(0),
        heartbeat_interval_seconds: row.try_get("heartbeat_interval_seconds").unwrap_or(15),
        offline_threshold_seconds: row.try_get("offline_threshold_seconds").unwrap_or(60),
        desired_config_revision: row
            .try_get::<Option<String>, _>("desired_config_revision")
            .ok()
            .flatten(),
        applied_config_revision: row
            .try_get::<Option<String>, _>("applied_config_revision")
            .ok()
            .flatten(),
        desired_applications_revision: row
            .try_get::<Option<String>, _>("desired_applications_revision")
            .ok()
            .flatten(),
        applied_applications_revision: row
            .try_get::<Option<String>, _>("applied_applications_revision")
            .ok()
            .flatten(),
        sync_status: row.try_get("sync_status").unwrap_or(0),
        routing_enabled: row.try_get("routing_enabled").unwrap_or(true),
        draining: row.try_get("draining").unwrap_or(false),
    }
}

fn map_cluster_identity_row(row: &EngineRow) -> WebServiceResult<ClusterIdentityRow> {
    Ok(ClusterIdentityRow {
        cluster_id: row.try_get("id").map_err(store_map_error)?,
        cluster_uuid: row.try_get("uuid").map_err(store_map_error)?,
        name: row.try_get("name").map_err(store_map_error)?,
        code: row.try_get("code").map_err(store_map_error)?,
        heartbeat_interval_seconds: row
            .try_get("heartbeat_interval_seconds")
            .map_err(store_map_error)?,
        offline_threshold_seconds: row
            .try_get("offline_threshold_seconds")
            .map_err(store_map_error)?,
    })
}

fn finalize_cluster_host_page(
    rows: Vec<EngineRow>,
    page_size: i32,
) -> WebServiceResult<ClusterHostPage> {
    let has_more = rows.len() > page_size as usize;
    let page_rows = rows.into_iter().take(page_size as usize).collect::<Vec<_>>();
    let mut items = Vec::with_capacity(page_rows.len());
    for row in &page_rows {
        items.push(map_cluster_host_row(row).map_err(|error| {
            WebServiceError::Internal(format!("map webserver_cluster_host row: {error}"))
        })?);
    }
    let next_cursor = has_more
        .then(|| {
            let last = page_rows.last().expect("non-empty page when has_more");
            let updated_at = cursor_instant_from_row(last, "updated_at")
                .map_err(|error| store_error("map webserver_cluster_host cursor", error))?;
            let id: i64 = last
                .try_get("id")
                .map_err(|error| store_error("map webserver_cluster_host cursor id", error))?;
            Ok::<_, WebServiceError>(encode_keyset_cursor(&updated_at, id))
        })
        .transpose()?;
    Ok(ClusterHostPage {
        items,
        total: 0,
        next_cursor,
        has_more: Some(has_more),
    })
}

fn finalize_cluster_instance_page(
    rows: Vec<EngineRow>,
    page_size: i32,
) -> WebServiceResult<ClusterInstancePage> {
    let has_more = rows.len() > page_size as usize;
    let page_rows = rows.into_iter().take(page_size as usize).collect::<Vec<_>>();
    let mut items = Vec::with_capacity(page_rows.len());
    for row in &page_rows {
        items.push(map_cluster_instance_row(row).map_err(|error| {
            WebServiceError::Internal(format!("map webserver_cluster_instance row: {error}"))
        })?);
    }
    let next_cursor = has_more
        .then(|| {
            let last = page_rows.last().expect("non-empty page when has_more");
            let updated_at = cursor_instant_from_row(last, "updated_at")
                .map_err(|error| store_error("map webserver_cluster_instance cursor", error))?;
            let id: i64 = last.try_get("id").map_err(|error| {
                store_error("map webserver_cluster_instance cursor id", error)
            })?;
            Ok::<_, WebServiceError>(encode_keyset_cursor(&updated_at, id))
        })
        .transpose()?;
    Ok(ClusterInstancePage {
        items,
        total: 0,
        next_cursor,
        has_more: Some(has_more),
    })
}

fn finalize_cluster_event_page(
    rows: Vec<EngineRow>,
    page_size: i32,
) -> WebServiceResult<ClusterEventPage> {
    let has_more = rows.len() > page_size as usize;
    let page_rows = rows.into_iter().take(page_size as usize).collect::<Vec<_>>();
    let mut items = Vec::with_capacity(page_rows.len());
    for row in &page_rows {
        items.push(map_cluster_event_row(row).map_err(|error| {
            WebServiceError::Internal(format!("map webserver_cluster_event row: {error}"))
        })?);
    }
    let next_cursor = has_more
        .then(|| {
            let last = page_rows.last().expect("non-empty page when has_more");
            let occurred_at = cursor_instant_from_row(last, "occurred_at")
                .map_err(|error| store_error("map webserver_cluster_event cursor", error))?;
            let id: i64 = last
                .try_get("id")
                .map_err(|error| store_error("map webserver_cluster_event cursor id", error))?;
            Ok::<_, WebServiceError>(encode_keyset_cursor(&occurred_at, id))
        })
        .transpose()?;
    Ok(ClusterEventPage {
        items,
        total: 0,
        next_cursor,
        has_more: Some(has_more),
    })
}
