use crate::audited_sql;
use super::{EngineRow, WebRepository};
use sdkwork_intelligence_webserver_service::{
    RuntimeAssignmentTarget, RuntimeAssignmentWrite, RuntimeObservationWrite,
};
use sdkwork_webserver_contract::{
    RuntimeAssignment, RuntimeAssignmentDelivery, RuntimeObservation, RuntimeObservationState,
    WebServiceError, WebServiceResult, WebsiteRuntimeSetSnapshot,
};
use sqlx::Row;

use super::support::{
    instant_from_row, instant_write_expression, json_write_expression, new_uuid, next_id,
    now_rfc3339, store_error,
};

impl WebRepository {
    pub(super) async fn resolve_runtime_assignment_target_repo(
        &self,
        requester_tenant_id: i64,
        can_cross_tenant: bool,
        node_uuid: &str,
    ) -> WebServiceResult<RuntimeAssignmentTarget> {
        let row = if can_cross_tenant && requester_tenant_id == 0 {
            sqlx::query(
                "SELECT id, uuid, tenant_id, tenant_scope_hash
                 FROM webserver_server WHERE uuid = $1",
            )
            .bind(node_uuid)
            .fetch_optional(&self.pool)
            .await
        } else {
            sqlx::query(
                "SELECT id, uuid, tenant_id, tenant_scope_hash
                 FROM webserver_server WHERE tenant_id = $1 AND uuid = $2",
            )
            .bind(requester_tenant_id)
            .bind(node_uuid)
            .fetch_optional(&self.pool)
            .await
        }
        .map_err(|error| store_error("resolve web runtime assignment target", error))?
        .ok_or_else(|| WebServiceError::not_found("Web Node not found"))?;

        Ok(RuntimeAssignmentTarget {
            server_id: row.try_get("id").map_err(map_row_error)?,
            node_uuid: row.try_get("uuid").map_err(map_row_error)?,
            tenant_id: row.try_get("tenant_id").map_err(map_row_error)?,
            tenant_scope_hash: row.try_get("tenant_scope_hash").map_err(map_row_error)?,
        })
    }

    pub(super) async fn publish_runtime_assignment_repo(
        &self,
        write: RuntimeAssignmentWrite,
    ) -> WebServiceResult<RuntimeAssignment> {
        let generation = i64::try_from(write.generation)
            .map_err(|_| WebServiceError::validation("generation is outside database range"))?;
        let runtime_set_bytes = i64::try_from(write.runtime_set_bytes).map_err(|_| {
            WebServiceError::validation("runtime-set byte length is outside database range")
        })?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin web runtime assignment transaction", error))?;

        let locked = sqlx::query(
            "UPDATE webserver_server SET version = version
             WHERE tenant_id = $1 AND id = $2 AND uuid = $3",
        )
        .bind(write.tenant_id)
        .bind(write.server_id)
        .bind(&write.node_uuid)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("lock web runtime assignment target", error))?;
        if locked.rows_affected() != 1 {
            return Err(WebServiceError::not_found("Web Node not found"));
        }

        let current = sqlx::query(
            "SELECT a.uuid AS assignment_uuid, s.uuid AS node_uuid, a.environment,
                    a.generation, a.snapshot_uuid, a.snapshot_sha256,
                    CAST(a.created_at AS TEXT) AS assigned_at
             FROM webserver_runtime_assignment a
             INNER JOIN webserver_server s ON s.id = a.server_id AND s.tenant_id = a.tenant_id
             WHERE a.tenant_id = $1 AND a.server_id = $2 AND a.environment = $3
             ORDER BY a.generation DESC LIMIT 1",
        )
        .bind(write.tenant_id)
        .bind(write.server_id)
        .bind(&write.environment)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("load current web runtime assignment", error))?;

        if let Some(row) = current {
            let current_generation: i64 = row.try_get("generation").map_err(map_row_error)?;
            let current_hash: String = row.try_get("snapshot_sha256").map_err(map_row_error)?;
            if generation == current_generation && write.snapshot_sha256 == current_hash {
                transaction.rollback().await.map_err(|error| {
                    store_error(
                        "rollback idempotent web runtime assignment transaction",
                        error,
                    )
                })?;
                return map_assignment_row(&row);
            }
            if generation <= current_generation {
                return Err(WebServiceError::conflict(
                    "runtime assignment generation must be strictly increasing",
                ));
            }
        }

        let id = next_id(self.id_generator())?;
        let assignment_uuid = new_uuid();
        let assigned_at = now_rfc3339();

        let runtime_set_expression = json_write_expression("$9");
        let assigned_at_expression = instant_write_expression("$12");
        let insert_sql = format!(
            "INSERT INTO webserver_runtime_assignment (
                id, uuid, tenant_id, server_id, environment, generation, snapshot_uuid,
                snapshot_sha256, runtime_set, runtime_set_bytes, assigned_by_subject,
                created_at, updated_at, version
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, {runtime_set_expression}, $10, $11,
                {assigned_at_expression}, {assigned_at_expression}, 0
             )"
        );
        sqlx::query(audited_sql(&insert_sql))
            .bind(id)
            .bind(&assignment_uuid)
            .bind(write.tenant_id)
            .bind(write.server_id)
            .bind(&write.environment)
            .bind(generation)
            .bind(&write.snapshot_uuid)
            .bind(&write.snapshot_sha256)
            .bind(&write.runtime_set_json)
            .bind(runtime_set_bytes)
            .bind(&write.assigned_by_subject)
            .bind(&assigned_at)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("insert web runtime assignment", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit web runtime assignment transaction", error))?;

        Ok(RuntimeAssignment {
            assignment_uuid,
            node_uuid: write.node_uuid,
            environment: write.environment,
            generation: write.generation.to_string(),
            snapshot_uuid: write.snapshot_uuid,
            snapshot_sha256: write.snapshot_sha256,
            assigned_at,
        })
    }

    pub(super) async fn retrieve_current_runtime_assignment_repo(
        &self,
        tenant_id: i64,
        node_uuid: &str,
        environment: &str,
        if_generation: Option<&str>,
        if_snapshot_sha256: Option<&str>,
    ) -> WebServiceResult<RuntimeAssignmentDelivery> {
        let row = sqlx::query(
            "SELECT a.uuid AS assignment_uuid, s.uuid AS node_uuid, a.environment,
                    a.generation, a.snapshot_uuid, a.snapshot_sha256,
                    CAST(a.runtime_set AS TEXT) AS runtime_set,
                    CAST(a.created_at AS TEXT) AS assigned_at,
                    (SELECT o.state FROM webserver_runtime_observation o
                     WHERE o.tenant_id = a.tenant_id AND o.assignment_id = a.id
                     ORDER BY o.id DESC LIMIT 1) AS latest_observation_state
             FROM webserver_runtime_assignment a
             INNER JOIN webserver_server s ON s.id = a.server_id AND s.tenant_id = a.tenant_id
             WHERE a.tenant_id = $1 AND s.uuid = $2 AND a.environment = $3
             ORDER BY a.generation DESC LIMIT 1",
        )
        .bind(tenant_id)
        .bind(node_uuid)
        .bind(environment)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve current web runtime assignment", error))?
        .ok_or_else(|| WebServiceError::not_found("runtime assignment not found"))?;

        let assignment = map_assignment_row(&row)?;
        let has_condition = if_generation.is_some() || if_snapshot_sha256.is_some();
        let generation_matches =
            if_generation.is_none_or(|value| value == assignment.generation.as_str());
        let hash_matches =
            if_snapshot_sha256.is_none_or(|value| value == assignment.snapshot_sha256.as_str());
        let unchanged = has_condition && generation_matches && hash_matches;
        let runtime_set = if unchanged {
            None
        } else {
            let raw: String = row.try_get("runtime_set").map_err(map_row_error)?;
            Some(
                serde_json::from_str::<WebsiteRuntimeSetSnapshot>(&raw).map_err(|error| {
                    WebServiceError::Internal(format!(
                        "stored web runtime assignment is invalid: {error}"
                    ))
                })?,
            )
        };
        Ok(RuntimeAssignmentDelivery {
            unchanged,
            assignment,
            latest_observation_state: row
                .try_get::<Option<String>, _>("latest_observation_state")
                .map_err(map_row_error)?
                .as_deref()
                .map(parse_state)
                .transpose()?,
            runtime_set,
        })
    }

    pub(super) async fn create_runtime_observation_repo(
        &self,
        write: RuntimeObservationWrite,
    ) -> WebServiceResult<RuntimeObservation> {
        let generation = i64::try_from(write.generation)
            .map_err(|_| WebServiceError::validation("generation is outside database range"))?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin web runtime observation transaction", error))?;

        let assignment = sqlx::query(
            "SELECT a.id AS assignment_id, a.uuid AS assignment_uuid, a.generation,
                    a.snapshot_uuid, a.snapshot_sha256, a.server_id, a.environment
             FROM webserver_runtime_assignment a
             INNER JOIN webserver_server s ON s.id = a.server_id AND s.tenant_id = a.tenant_id
             WHERE a.tenant_id = $1 AND s.uuid = $2 AND a.snapshot_uuid = $3",
        )
        .bind(write.tenant_id)
        .bind(&write.node_uuid)
        .bind(&write.snapshot_uuid)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("resolve web runtime observation assignment", error))?
        .ok_or_else(|| WebServiceError::not_found("runtime assignment not found"))?;
        let assignment_id: i64 = assignment.try_get("assignment_id").map_err(map_row_error)?;
        let server_id: i64 = assignment.try_get("server_id").map_err(map_row_error)?;
        let assignment_generation: i64 = assignment.try_get("generation").map_err(map_row_error)?;
        let assignment_hash: String = assignment
            .try_get("snapshot_sha256")
            .map_err(map_row_error)?;
        if assignment_generation != generation || assignment_hash != write.snapshot_sha256 {
            return Err(WebServiceError::conflict(
                "observation generation or snapshot hash does not match the assignment",
            ));
        }

        sqlx::query(
            "UPDATE webserver_runtime_assignment SET version = version WHERE tenant_id = $1 AND id = $2",
        )
        .bind(write.tenant_id)
        .bind(assignment_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("lock web runtime observation assignment", error))?;

        let latest = sqlx::query(
            "SELECT o.uuid AS observation_uuid, a.uuid AS assignment_uuid,
                    a.tenant_id AS observation_tenant_id, s.uuid AS node_uuid,
                    a.environment, a.generation, a.snapshot_uuid,
                    a.snapshot_sha256, o.state, o.node_version, o.reason_code, o.detail,
                    CAST(o.observed_at AS TEXT) AS observed_at
             FROM webserver_runtime_observation o
             INNER JOIN webserver_runtime_assignment a
                ON a.id = o.assignment_id AND a.tenant_id = o.tenant_id
             INNER JOIN webserver_server s
                ON s.id = a.server_id AND s.tenant_id = a.tenant_id
             WHERE o.tenant_id = $1 AND o.assignment_id = $2
             ORDER BY o.id DESC LIMIT 1",
        )
        .bind(write.tenant_id)
        .bind(assignment_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| store_error("load latest web runtime observation", error))?;

        if let Some(row) = latest {
            let latest_state =
                parse_state(&row.try_get::<String, _>("state").map_err(map_row_error)?)?;
            if latest_state == write.state {
                let same_payload = row
                    .try_get::<Option<String>, _>("node_version")
                    .map_err(map_row_error)?
                    == write.node_version
                    && row
                        .try_get::<Option<String>, _>("reason_code")
                        .map_err(map_row_error)?
                        == write.reason_code
                    && row
                        .try_get::<Option<String>, _>("detail")
                        .map_err(map_row_error)?
                        == write.detail;
                if !same_payload {
                    return Err(WebServiceError::conflict(
                        "an observation state cannot be replayed with different details",
                    ));
                }
                transaction.rollback().await.map_err(|error| {
                    store_error(
                        "rollback idempotent web runtime observation transaction",
                        error,
                    )
                })?;
                return map_observation_row(&row, latest_state);
            }
            let is_next_normal_state = write.state != RuntimeObservationState::Rejected
                && write.state.rank() == latest_state.rank() + 1;
            if latest_state.is_terminal()
                || (write.state != RuntimeObservationState::Rejected && !is_next_normal_state)
            {
                return Err(WebServiceError::conflict(
                    "runtime observation states must advance one phase at a time and terminal states are immutable",
                ));
            }
        } else if write.state != RuntimeObservationState::Received {
            return Err(WebServiceError::conflict(
                "runtime observations must start with RECEIVED",
            ));
        }

        let id = next_id(self.id_generator())?;
        let observation_uuid = new_uuid();
        let observed_at = now_rfc3339();

        let observed_at_expression = instant_write_expression("$10");
        let insert_sql = format!(
            "INSERT INTO webserver_runtime_observation (
                id, uuid, tenant_id, assignment_id, server_id, state, node_version,
                reason_code, detail, observed_at, created_at, updated_at, version
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9,
                {observed_at_expression}, {observed_at_expression}, {observed_at_expression}, 0
             )"
        );
        sqlx::query(audited_sql(&insert_sql))
            .bind(id)
            .bind(&observation_uuid)
            .bind(write.tenant_id)
            .bind(assignment_id)
            .bind(server_id)
            .bind(write.state.as_str())
            .bind(&write.node_version)
            .bind(&write.reason_code)
            .bind(&write.detail)
            .bind(&observed_at)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("insert web runtime observation", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit web runtime observation transaction", error))?;

        Ok(RuntimeObservation {
            observation_uuid,
            assignment_uuid: assignment
                .try_get("assignment_uuid")
                .map_err(map_row_error)?,
            tenant_id: write.tenant_id.to_string(),
            node_uuid: write.node_uuid,
            environment: assignment.try_get("environment").map_err(map_row_error)?,
            generation: write.generation.to_string(),
            snapshot_uuid: write.snapshot_uuid,
            snapshot_sha256: write.snapshot_sha256,
            state: write.state,
            node_version: write.node_version,
            reason_code: write.reason_code,
            detail: write.detail,
            observed_at,
        })
    }

    pub(super) async fn retrieve_latest_runtime_observation_repo(
        &self,
        requester_tenant_id: i64,
        can_cross_tenant: bool,
        snapshot_uuid: &str,
    ) -> WebServiceResult<RuntimeObservation> {
        let row = if can_cross_tenant && requester_tenant_id == 0 {
            sqlx::query(
                "SELECT o.uuid AS observation_uuid, a.uuid AS assignment_uuid,
                        a.tenant_id AS observation_tenant_id, s.uuid AS node_uuid,
                        a.environment, a.generation, a.snapshot_uuid,
                        a.snapshot_sha256, o.state, o.node_version, o.reason_code, o.detail,
                        CAST(o.observed_at AS TEXT) AS observed_at
                 FROM webserver_runtime_observation o
                 INNER JOIN webserver_runtime_assignment a
                    ON a.id = o.assignment_id AND a.tenant_id = o.tenant_id
                 INNER JOIN webserver_server s
                    ON s.id = a.server_id AND s.tenant_id = a.tenant_id
                 WHERE a.snapshot_uuid = $1
                 ORDER BY o.id DESC LIMIT 1",
            )
            .bind(snapshot_uuid)
            .fetch_optional(&self.pool)
            .await
        } else {
            sqlx::query(
                "SELECT o.uuid AS observation_uuid, a.uuid AS assignment_uuid,
                        a.tenant_id AS observation_tenant_id, s.uuid AS node_uuid,
                        a.environment, a.generation, a.snapshot_uuid,
                        a.snapshot_sha256, o.state, o.node_version, o.reason_code, o.detail,
                        CAST(o.observed_at AS TEXT) AS observed_at
                 FROM webserver_runtime_observation o
                 INNER JOIN webserver_runtime_assignment a
                    ON a.id = o.assignment_id AND a.tenant_id = o.tenant_id
                 INNER JOIN webserver_server s
                    ON s.id = a.server_id AND s.tenant_id = a.tenant_id
                 WHERE a.tenant_id = $1 AND a.snapshot_uuid = $2
                 ORDER BY o.id DESC LIMIT 1",
            )
            .bind(requester_tenant_id)
            .bind(snapshot_uuid)
            .fetch_optional(&self.pool)
            .await
        }
        .map_err(|error| store_error("retrieve latest web runtime observation", error))?
        .ok_or_else(|| WebServiceError::not_found("runtime observation not found"))?;

        let state = parse_state(&row.try_get::<String, _>("state").map_err(map_row_error)?)?;
        map_observation_row(&row, state)
    }
}

fn map_assignment_row(row: &EngineRow) -> WebServiceResult<RuntimeAssignment> {
    let generation: i64 = row.try_get("generation").map_err(map_row_error)?;
    Ok(RuntimeAssignment {
        assignment_uuid: row.try_get("assignment_uuid").map_err(map_row_error)?,
        node_uuid: row.try_get("node_uuid").map_err(map_row_error)?,
        environment: row.try_get("environment").map_err(map_row_error)?,
        generation: generation.to_string(),
        snapshot_uuid: row.try_get("snapshot_uuid").map_err(map_row_error)?,
        snapshot_sha256: row.try_get("snapshot_sha256").map_err(map_row_error)?,
        assigned_at: instant_from_row(row, "assigned_at").map_err(map_row_error)?,
    })
}

fn map_observation_row(
    row: &EngineRow,
    state: RuntimeObservationState,
) -> WebServiceResult<RuntimeObservation> {
    let generation: i64 = row.try_get("generation").map_err(map_row_error)?;
    let tenant_id: i64 = row
        .try_get("observation_tenant_id")
        .map_err(map_row_error)?;
    Ok(RuntimeObservation {
        observation_uuid: row.try_get("observation_uuid").map_err(map_row_error)?,
        assignment_uuid: row.try_get("assignment_uuid").map_err(map_row_error)?,
        tenant_id: tenant_id.to_string(),
        node_uuid: row.try_get("node_uuid").map_err(map_row_error)?,
        environment: row.try_get("environment").map_err(map_row_error)?,
        generation: generation.to_string(),
        snapshot_uuid: row.try_get("snapshot_uuid").map_err(map_row_error)?,
        snapshot_sha256: row.try_get("snapshot_sha256").map_err(map_row_error)?,
        state,
        node_version: row.try_get("node_version").map_err(map_row_error)?,
        reason_code: row.try_get("reason_code").map_err(map_row_error)?,
        detail: row.try_get("detail").map_err(map_row_error)?,
        observed_at: instant_from_row(row, "observed_at").map_err(map_row_error)?,
    })
}

fn parse_state(value: &str) -> WebServiceResult<RuntimeObservationState> {
    RuntimeObservationState::try_from(value).map_err(|_| {
        WebServiceError::Internal(format!(
            "stored runtime observation state is invalid: {value}"
        ))
    })
}

fn map_row_error(error: sqlx::Error) -> WebServiceError {
    WebServiceError::Internal(format!("map web runtime distribution row: {error}"))
}
