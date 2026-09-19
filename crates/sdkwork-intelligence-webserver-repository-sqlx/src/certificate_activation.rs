use crate::audited_sql;
use futures_util::TryStreamExt;
use sdkwork_webserver_contract::{
    AgentCertificateObservation, WebServiceError, WebServiceResult,
};
use sqlx::Row;
use std::collections::HashSet;

use super::agents::{AgentSyncFingerprint, AuthenticatedAgent};
use super::support::{new_uuid, next_id, store_error};
use super::WebRepository;

const MAX_PENDING_LISTENER_BINDINGS: usize = 256;

impl WebRepository {
    pub(super) async fn record_certificate_observations(
        &self,
        agent: &AuthenticatedAgent,
        observations: &[AgentCertificateObservation],
        desired: &AgentSyncFingerprint,
    ) -> WebServiceResult<bool> {
        if observations.is_empty() {
            return Ok(false);
        }
        // The observation batch is bounded by the node manifest budget so a
        // misbehaving agent cannot stretch the transaction or the lock hold.
        if observations.len() > MAX_PENDING_LISTENER_BINDINGS {
            return Err(WebServiceError::validation(format!(
                "certificate observations exceed the maximum batch of {MAX_PENDING_LISTENER_BINDINGS}"
            )));
        }
        let expected = desired
            .certificate_fingerprints
            .iter()
            .map(|(certificate_id, fingerprint)| (certificate_id.as_str(), fingerprint.as_str()))
            .collect::<HashSet<_>>();
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin certificate node observation", error))?;
        let mut recorded = false;
        for observation in observations {
            if observation.sync_version != desired.sync_version {
                continue;
            }
            if !expected.contains(&(
                    observation.certificate_id.as_str(),
                    observation.fingerprint.as_str(),
                )) {
                return Err(WebServiceError::conflict(
                    "certificate observation is not part of the current node manifest",
                ));
            }
            let result = sqlx::query(
                "INSERT INTO webserver_certificate_node_state (
                    id, uuid, tenant_id, server_id, certificate_id,
                    certificate_version_id, state, fingerprint_sha256, sync_version,
                    failure_code, observed_at, created_at, updated_at, version
                 )
                 SELECT $1, $2, $3, s.id, c.id, v.id, $7,
                        v.fingerprint_sha256, $8, $9, CAST($10 AS TIMESTAMPTZ),
                        NOW(), NOW(), 0
                 FROM webserver_server s
                 INNER JOIN webserver_certificate c ON c.tenant_id = s.tenant_id
                     AND c.uuid = $5 AND c.deleted_at IS NULL
                 INNER JOIN webserver_certificate_version v ON v.tenant_id = c.tenant_id
                     AND v.certificate_id = c.id AND v.fingerprint_sha256 = $6
                 WHERE s.tenant_id = $3 AND s.uuid = $4
                   AND EXISTS (
                       SELECT 1
                       FROM webserver_listener_certificate_binding l
                       WHERE l.tenant_id = c.tenant_id
                         AND l.certificate_id = c.id AND l.desired_version_id = v.id
                         AND l.status IN ('PENDING', 'DEPLOYING', 'ACTIVE', 'FAILED')
                         AND l.deleted_at IS NULL
                   )
                 ON CONFLICT ON CONSTRAINT uk_webserver_certificate_node_state_version
                 DO UPDATE SET state = EXCLUDED.state,
                     fingerprint_sha256 = EXCLUDED.fingerprint_sha256,
                     sync_version = EXCLUDED.sync_version,
                     failure_code = EXCLUDED.failure_code,
                     observed_at = EXCLUDED.observed_at,
                     updated_at = EXCLUDED.updated_at,
                     version = webserver_certificate_node_state.version + 1",
            )
            .bind(next_id(self.id_generator())?)
            .bind(new_uuid())
            .bind(agent.tenant_id)
            .bind(&agent.server_uuid)
            .bind(&observation.certificate_id)
            .bind(&observation.fingerprint)
            .bind(&observation.state)
            .bind(&observation.sync_version)
            .bind(observation.failure_code.as_deref())
            .bind(&observation.observed_at)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("record active certificate node observation", error))?;
            if result.rows_affected() != 1 {
                return Err(WebServiceError::conflict(
                    "observed certificate is no longer the desired version",
                ));
            }
            recorded = true;
        }
        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit certificate node observations", error))?;
        Ok(recorded)
    }

    pub(super) async fn promote_converged_listener_certificate_bindings(
        &self,
        tenant_id: i64,
    ) -> WebServiceResult<()> {
        // Candidate discovery runs without row locks; each candidate is
        // then processed in its own short transaction that re-locks the row
        // with a status guard, so concurrent workers never block each other
        // across a long transaction (each row holds its lock only for its own
        // count + update statements).
        let sql = format!(
            "SELECT l.id AS binding_id, l.certificate_id,
                    l.desired_version_id AS desired_version_id
             FROM webserver_listener_certificate_binding l
             INNER JOIN webserver_certificate_version v ON v.tenant_id = l.tenant_id
                 AND v.certificate_id = l.certificate_id
                 AND v.id = l.desired_version_id
                 AND v.status IN ('ACTIVE', 'SUPERSEDED')
             WHERE l.tenant_id = $1
               AND l.status IN ('PENDING', 'DEPLOYING', 'FAILED')
               AND l.deleted_at IS NULL
             ORDER BY l.id ASC
             LIMIT {}",
            MAX_PENDING_LISTENER_BINDINGS + 1
        );
        let mut candidates = sqlx::query(audited_sql(&sql)).bind(tenant_id).fetch(&self.pool);
        let mut rows = Vec::new();
        while let Some(row) = candidates
            .try_next()
            .await
            .map_err(|error| store_error("load candidate certificate versions", error))?
        {
            rows.push(row);
        }
        drop(candidates);
        if rows.len() > MAX_PENDING_LISTENER_BINDINGS {
            return Err(WebServiceError::Internal(format!(
                "certificate convergence exceeds {MAX_PENDING_LISTENER_BINDINGS} pending listener bindings"
            )));
        }

        for row in rows {
            let binding_id: i64 = row
                .try_get("binding_id")
                .map_err(|error| store_error("map converging listener binding id", error))?;
            let certificate_id: i64 = row
                .try_get("certificate_id")
                .map_err(|error| store_error("map converging certificate id", error))?;
            let desired_version_id: i64 = row
                .try_get("desired_version_id")
                .map_err(|error| store_error("map desired certificate version id", error))?;
            // Short per-row transaction: lock the binding row with the same
            // status guard used by the update statements. A concurrent
            // worker that already advanced the row finds the guard empty and
            // skips it, so convergence is idempotent and lock contention is
            // bounded to one row at a time.
            let mut transaction = self
                .pool
                .begin()
                .await
                .map_err(|error| store_error("begin listener certificate convergence row", error))?;
            let lock_row = sqlx::query(
                "SELECT 1
                 FROM webserver_listener_certificate_binding
                 WHERE tenant_id = $1 AND id = $2 AND desired_version_id = $3
                   AND status IN ('PENDING', 'DEPLOYING', 'FAILED')
                   AND deleted_at IS NULL
                 FOR UPDATE",
            )
            .bind(tenant_id)
            .bind(binding_id)
            .bind(desired_version_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| store_error("lock converging listener certificate binding", error))?;
            if lock_row.is_none() {
                transaction
                    .rollback()
                    .await
                    .map_err(|error| store_error("rollback skipped convergence row", error))?;
                continue;
            }
            let counts = sqlx::query(
                "WITH assigned_servers AS (
                    SELECT DISTINCT a.server_id
                    FROM webserver_listener_certificate_binding l
                    INNER JOIN webserver_site_binding b ON b.tenant_id = l.tenant_id
                        AND b.id = l.site_binding_id AND b.deleted_at IS NULL
                    INNER JOIN webserver_site s ON s.tenant_id = b.tenant_id
                        AND s.id = b.site_id AND s.deleted_at IS NULL
                    INNER JOIN webserver_runtime_assignment a ON a.tenant_id = l.tenant_id
                        AND NOT EXISTS (
                            SELECT 1 FROM webserver_runtime_assignment newer
                            WHERE newer.tenant_id = a.tenant_id
                              AND newer.server_id = a.server_id
                              AND newer.environment = a.environment
                              AND newer.generation > a.generation
                        )
                        AND a.runtime_set @> jsonb_build_object(
                            'descriptors',
                            jsonb_build_array(jsonb_build_object('siteUuid', s.uuid))
                        )
                    WHERE l.tenant_id = $1 AND l.id = $3
                      AND l.desired_version_id = $2 AND l.deleted_at IS NULL
                 )
                 SELECT COUNT(*) AS assigned_count,
                        COUNT(o.server_id) AS observed_count,
                        COUNT(o.server_id) FILTER (WHERE o.state = 'SERVED') AS served_count,
                        COUNT(o.server_id) FILTER (WHERE o.state = 'FAILED') AS failed_count
                 FROM assigned_servers assigned
                 LEFT JOIN webserver_certificate_node_state o ON o.tenant_id = $1
                     AND o.server_id = assigned.server_id
                     AND o.certificate_version_id = $2",
            )
            .bind(tenant_id)
            .bind(desired_version_id)
            .bind(binding_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| store_error("count certificate node convergence", error))?;
            let assigned_count: i64 = counts
                .try_get("assigned_count")
                .map_err(|error| store_error("map assigned certificate node count", error))?;
            let observed_count: i64 = counts
                .try_get("observed_count")
                .map_err(|error| store_error("map observed certificate node count", error))?;
            let served_count: i64 = counts
                .try_get("served_count")
                .map_err(|error| store_error("map served certificate node count", error))?;
            let failed_count: i64 = counts
                .try_get("failed_count")
                .map_err(|error| store_error("map failed certificate node count", error))?;
            if failed_count > 0 {
                sqlx::query(
                    "UPDATE webserver_listener_certificate_binding
                     SET status = 'FAILED', updated_at = NOW(), version = version + 1
                     WHERE tenant_id = $1 AND id = $2 AND desired_version_id = $3
                       AND status IN ('PENDING', 'DEPLOYING', 'FAILED')
                       AND deleted_at IS NULL",
                )
                .bind(tenant_id)
                .bind(binding_id)
                .bind(desired_version_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| store_error("fail listener certificate rollout", error))?;
                // Commit the FAILED transition before continuing: dropping
                // the transaction without an explicit commit would roll back
                // the status update and leave the binding permanently stuck
                // in PENDING/DEPLOYING with an endless retry loop.
                transaction
                    .commit()
                    .await
                    .map_err(|error| store_error("commit failed listener certificate rollout", error))?;
                continue;
            }
            if assigned_count > 0 && served_count == assigned_count {
                sqlx::query(
                    "UPDATE webserver_listener_certificate_binding
                     SET current_version_id = desired_version_id, status = 'ACTIVE',
                         activated_at = NOW(), updated_at = NOW(), version = version + 1
                     WHERE tenant_id = $1 AND id = $2 AND certificate_id = $3
                       AND desired_version_id = $4
                       AND status IN ('PENDING', 'DEPLOYING', 'FAILED')
                       AND deleted_at IS NULL",
                )
                .bind(tenant_id)
                .bind(binding_id)
                .bind(certificate_id)
                .bind(desired_version_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| store_error("activate converged listener certificate", error))?;
            } else if observed_count > 0 {
                sqlx::query(
                    "UPDATE webserver_listener_certificate_binding
                     SET status = 'DEPLOYING', updated_at = NOW(), version = version + 1
                     WHERE tenant_id = $1 AND id = $2 AND desired_version_id = $3
                       AND status IN ('PENDING', 'DEPLOYING', 'FAILED')
                       AND deleted_at IS NULL",
                )
                .bind(tenant_id)
                .bind(binding_id)
                .bind(desired_version_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| store_error("advance listener certificate rollout", error))?;
            }
            transaction
                .commit()
                .await
                .map_err(|error| store_error("commit certificate convergence row", error))?;
        }
        Ok(())
    }
}
