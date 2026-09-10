use sdkwork_webserver_contract::{
    CertificateOperationAcceptedResponse, CertificateOperationLease, CertificateOperationResponse,
    IssueCertificateRequest, WebServiceError, WebServiceResult,
};
use serde_json::json;
use sqlx::Row;

use super::support::{
    bool_from_row, instant_from_row, is_unique_violation, new_uuid, next_id,
    optional_instant_from_row, sha256_hex, store_error,
};
use super::{EngineRow, WebRepository};

const CERTIFICATE_OPERATION_MAX_ATTEMPTS: i32 = 5;
const MAX_CERTIFICATE_IDENTIFIERS: usize = 8;
const MAX_OPERATION_CLAIM_BATCH: i32 = 32;
const MIN_OPERATION_LEASE_SECONDS: i64 = 60;
const MAX_OPERATION_LEASE_SECONDS: i64 = 3_600;
const TERMINAL_OPERATION_COOLDOWN_SECONDS: i64 = 86_400;
const EXPIRED_OPERATION_FAILURE_CODE: &str = "CERTIFICATE_OPERATION_LEASE_EXPIRED";

impl WebRepository {
    pub(super) async fn enqueue_certificate_issue_repo(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        requested_by: Option<i64>,
        request: &IssueCertificateRequest,
        idempotency_key: Option<&str>,
    ) -> WebServiceResult<CertificateOperationAcceptedResponse> {
        validate_certificate_issue_request(request)?;
        let request_sha256 = certificate_issue_request_sha256(request);
        let idempotency_key_hash = certificate_operation_idempotency_key_hash(
            tenant_id,
            requested_by,
            "ISSUE",
            "certificate_collection",
            idempotency_key,
        )?;

        if let Some(existing) = self
            .find_idempotent_certificate_operation(
                tenant_id,
                &idempotency_key_hash,
                &request_sha256,
            )
            .await?
        {
            return Ok(existing);
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin certificate issue operation", error))?;
        let domains = sqlx::query(
            "SELECT d.id, d.user_id, d.hostname, d.hostname_type, requested.position
             FROM UNNEST($2::text[]) WITH ORDINALITY requested(uuid, position)
             INNER JOIN web_domain d ON d.tenant_id = $1 AND d.uuid = requested.uuid
             WHERE d.deleted_at IS NULL
               AND d.verification_status = 'VERIFIED'
               AND ($3 IS NULL OR d.user_id = $3)
             ORDER BY requested.position ASC",
        )
        .bind(tenant_id)
        .bind(&request.domain_ids)
        .bind(owner_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| store_error("resolve verified certificate domains", error))?;
        if domains.len() != request.domain_ids.len() {
            return Err(WebServiceError::validation(
                "one or more verified certificate domains are unavailable",
            ));
        }

        let asset_owner_id: Option<i64> = domains[0]
            .try_get("user_id")
            .map_err(|error| store_error("map certificate domain owner", error))?;
        for domain in &domains[1..] {
            let domain_owner_id: Option<i64> = domain
                .try_get("user_id")
                .map_err(|error| store_error("map certificate domain owner", error))?;
            if domain_owner_id != asset_owner_id {
                return Err(WebServiceError::validation(
                    "certificate domains must share the same asset owner",
                ));
            }
        }

        let certificate_internal_id = next_id(self.id_generator())?;
        let certificate_uuid = new_uuid();
        sqlx::query(
            "INSERT INTO web_certificate (
                id, uuid, tenant_id, user_id, cert_name, cert_type, ca_profile,
                preferred_key_algorithm, auto_renew, renewal_status, status, metadata,
                created_at, updated_at, version
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 2, 0, '{}', NOW(), NOW(), 0)",
        )
        .bind(certificate_internal_id)
        .bind(&certificate_uuid)
        .bind(tenant_id)
        .bind(asset_owner_id)
        .bind(&certificate_uuid)
        .bind(request.cert_type)
        .bind(certificate_ca_profile(request.cert_type))
        .bind(&request.key_algorithm)
        .bind(request.auto_renew)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("insert pending web_certificate", error))?;

        for (position, domain) in domains.iter().enumerate() {
            let domain_id: i64 = domain
                .try_get("id")
                .map_err(|error| store_error("map certificate domain id", error))?;
            let hostname: String = domain
                .try_get("hostname")
                .map_err(|error| store_error("map certificate hostname", error))?;
            let hostname_type: String = domain
                .try_get("hostname_type")
                .map_err(|error| store_error("map certificate hostname type", error))?;
            sqlx::query(
                "INSERT INTO web_certificate_identifier (
                    id, uuid, tenant_id, certificate_id, domain_id, identifier_type,
                    hostname, position, created_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())",
            )
            .bind(next_id(self.id_generator())?)
            .bind(new_uuid())
            .bind(tenant_id)
            .bind(certificate_internal_id)
            .bind(domain_id)
            .bind(hostname_type)
            .bind(hostname)
            .bind(position as i32)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("insert certificate identifier", error))?;
        }

        let operation_uuid = new_uuid();
        let insert_operation = sqlx::query(
            "INSERT INTO web_certificate_operation (
                id, uuid, tenant_id, certificate_id, requested_by, operation_type, status,
                attempt_count, max_attempts, next_attempt_at, fencing_token, failure_code,
                idempotency_key_hash, request_sha256, created_at, updated_at
             ) VALUES ($1, $2, $3, $4, $5, 'ISSUE', 'PENDING', 0, $6, NOW(), 0, NULL,
                       $7, $8, NOW(), NOW())",
        )
        .bind(next_id(self.id_generator())?)
        .bind(&operation_uuid)
        .bind(tenant_id)
        .bind(certificate_internal_id)
        .bind(requested_by)
        .bind(CERTIFICATE_OPERATION_MAX_ATTEMPTS)
        .bind(&idempotency_key_hash)
        .bind(&request_sha256)
        .execute(&mut *tx)
        .await;

        if let Err(error) = insert_operation {
            tx.rollback().await.ok();
            if is_unique_violation(&error) {
                if let Some(existing) = self
                    .find_idempotent_certificate_operation(
                        tenant_id,
                        &idempotency_key_hash,
                        &request_sha256,
                    )
                    .await?
                {
                    return Ok(existing);
                }
            }
            return Err(store_error("insert certificate issue operation", error));
        }

        tx.commit()
            .await
            .map_err(|error| store_error("commit certificate issue operation", error))?;
        Ok(accepted_operation(&operation_uuid, "PENDING"))
    }

    pub(super) async fn enqueue_certificate_renewal_repo(
        &self,
        tenant_id: i64,
        certificate_uuid: &str,
        requested_by: Option<i64>,
        idempotency_key: Option<&str>,
    ) -> WebServiceResult<CertificateOperationAcceptedResponse> {
        let idempotency_key_hash = certificate_operation_idempotency_key_hash(
            tenant_id,
            requested_by,
            "RENEW",
            certificate_uuid,
            idempotency_key,
        )?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin certificate renewal operation", error))?;
        let certificate = sqlx::query(
            "SELECT id, cert_type, current_version_id
             FROM web_certificate
             WHERE tenant_id = $1 AND uuid = $2 AND status = 1 AND deleted_at IS NULL
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(certificate_uuid)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| store_error("lock certificate for renewal", error))?
        .ok_or_else(|| WebServiceError::not_found("active certificate not found"))?;
        let certificate_internal_id: i64 = certificate
            .try_get("id")
            .map_err(|error| store_error("map renewal certificate id", error))?;
        let cert_type: i32 = certificate
            .try_get("cert_type")
            .map_err(|error| store_error("map renewal certificate type", error))?;
        if !matches!(cert_type, 1 | 3) {
            return Err(WebServiceError::validation(format!(
                "certType {cert_type} is not eligible for renewal"
            )));
        }
        certificate
            .try_get::<Option<i64>, _>("current_version_id")
            .map_err(|error| store_error("map renewal certificate version", error))?
            .ok_or_else(|| WebServiceError::conflict("certificate has no active version"))?;
        let request_sha256 = certificate_renewal_request_sha256(certificate_uuid);

        if let Some(existing) =
            find_idempotent_certificate_operation_in_tx(&mut tx, tenant_id, &idempotency_key_hash)
                .await?
        {
            ensure_matching_request(&existing, &request_sha256)?;
            tx.commit()
                .await
                .map_err(|error| store_error("commit renewal idempotency replay", error))?;
            return Ok(accepted_operation(&existing.uuid, &existing.status));
        }
        if let Some(existing) =
            find_active_certificate_operation_in_tx(&mut tx, tenant_id, certificate_internal_id)
                .await?
        {
            tx.commit()
                .await
                .map_err(|error| store_error("commit active renewal lookup", error))?;
            return Ok(accepted_operation(&existing.uuid, &existing.status));
        }

        let operation_uuid = new_uuid();
        sqlx::query(
            "INSERT INTO web_certificate_operation (
                id, uuid, tenant_id, certificate_id, requested_by, operation_type, status,
                attempt_count, max_attempts, next_attempt_at, fencing_token, failure_code,
                idempotency_key_hash, request_sha256, created_at, updated_at
             ) VALUES ($1, $2, $3, $4, $5, 'RENEW', 'PENDING', 0, $6, NOW(), 0, NULL,
                       $7, $8, NOW(), NOW())",
        )
        .bind(next_id(self.id_generator())?)
        .bind(&operation_uuid)
        .bind(tenant_id)
        .bind(certificate_internal_id)
        .bind(requested_by)
        .bind(CERTIFICATE_OPERATION_MAX_ATTEMPTS)
        .bind(&idempotency_key_hash)
        .bind(&request_sha256)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("insert certificate renewal operation", error))?;
        sqlx::query(
            "UPDATE web_certificate
             SET renewal_status = 2, metadata = metadata - 'certificateOperationFailureCode', updated_at = NOW(), version = version + 1
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant_id)
        .bind(certificate_internal_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("mark certificate renewal pending", error))?;
        tx.commit()
            .await
            .map_err(|error| store_error("commit certificate renewal operation", error))?;
        Ok(accepted_operation(&operation_uuid, "PENDING"))
    }

    pub(super) async fn retrieve_certificate_operation_repo(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        operation_uuid: &str,
    ) -> WebServiceResult<CertificateOperationResponse> {
        let row = sqlx::query(
            "SELECT operation.uuid, certificate.uuid AS certificate_uuid,
                    operation.operation_type, operation.status, operation.attempt_count,
                    operation.max_attempts, CAST(operation.next_attempt_at AS TEXT) AS next_attempt_at,
                    operation.failure_code, CAST(operation.created_at AS TEXT) AS created_at,
                    CAST(operation.updated_at AS TEXT) AS updated_at,
                    CAST(operation.completed_at AS TEXT) AS completed_at
             FROM web_certificate_operation operation
             INNER JOIN web_certificate certificate
               ON certificate.tenant_id = operation.tenant_id
              AND certificate.id = operation.certificate_id
             WHERE operation.tenant_id = $1 AND operation.uuid = $2
               AND ($3 IS NULL OR certificate.user_id = $3)",
        )
        .bind(tenant_id)
        .bind(operation_uuid)
        .bind(owner_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve certificate operation", error))?
        .ok_or_else(|| WebServiceError::not_found("certificate operation not found"))?;
        map_certificate_operation(&row)
            .map_err(|error| store_error("map certificate operation", error))
    }

    pub(super) async fn schedule_due_certificate_renewals_repo(
        &self,
        renew_before_days: u32,
        limit: i32,
    ) -> WebServiceResult<usize> {
        let limit = limit.clamp(1, 100);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin scheduled certificate renewals", error))?;
        let candidates = sqlx::query(
            "SELECT certificate.id, certificate.uuid, certificate.tenant_id,
                    certificate.current_version_id
             FROM web_certificate certificate
             INNER JOIN web_certificate_version version
               ON version.tenant_id = certificate.tenant_id
              AND version.id = certificate.current_version_id
              AND version.certificate_id = certificate.id
             WHERE certificate.auto_renew = TRUE AND certificate.status = 1
               AND certificate.cert_type = 1 AND certificate.deleted_at IS NULL
               AND COALESCE(
                     CASE WHEN certificate.metadata #>> '{ari,windowStart}' ~
                               '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}'
                          THEN (certificate.metadata #>> '{ari,windowStart}')::timestamptz
                          ELSE NULL END,
                     version.not_after - ($1 * INTERVAL '1 day')
                   ) <= NOW()
               AND NOT EXISTS (
                   SELECT 1 FROM web_certificate_operation active_operation
                   WHERE active_operation.tenant_id = certificate.tenant_id
                     AND active_operation.certificate_id = certificate.id
                     AND active_operation.status IN ('PENDING', 'RUNNING')
               )
               AND NOT EXISTS (
                   SELECT 1 FROM web_certificate_operation cooling_operation
                   WHERE cooling_operation.tenant_id = certificate.tenant_id
                     AND cooling_operation.certificate_id = certificate.id
                     AND cooling_operation.operation_type = 'RENEW'
                     AND cooling_operation.status = 'FAILED'
                     AND cooling_operation.next_attempt_at > NOW()
               )
             ORDER BY version.not_after ASC, certificate.id ASC
             FOR UPDATE OF certificate SKIP LOCKED LIMIT $2",
        )
        .bind(i64::from(renew_before_days.clamp(1, 365)))
        .bind(limit)
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| store_error("select due certificate renewals", error))?;

        for candidate in &candidates {
            let certificate_id: i64 = candidate
                .try_get("id")
                .map_err(|error| store_error("map scheduled certificate id", error))?;
            let tenant_id: i64 = candidate
                .try_get("tenant_id")
                .map_err(|error| store_error("map scheduled certificate tenant", error))?;
            let certificate_uuid: String = candidate
                .try_get("uuid")
                .map_err(|error| store_error("map scheduled certificate uuid", error))?;
            candidate
                .try_get::<Option<i64>, _>("current_version_id")
                .map_err(|error| store_error("map scheduled certificate version", error))?
                .ok_or_else(|| WebServiceError::conflict("certificate has no active version"))?;
            let request_sha256 = certificate_renewal_request_sha256(&certificate_uuid);
            sqlx::query(
                "INSERT INTO web_certificate_operation (
                    id, uuid, tenant_id, certificate_id, requested_by, operation_type, status,
                    attempt_count, max_attempts, next_attempt_at, fencing_token, failure_code,
                    idempotency_key_hash, request_sha256, created_at, updated_at
                 ) VALUES ($1, $2, $3, $4, NULL, 'RENEW', 'PENDING', 0, $5, NOW(), 0,
                           NULL, NULL, $6, NOW(), NOW())",
            )
            .bind(next_id(self.id_generator())?)
            .bind(new_uuid())
            .bind(tenant_id)
            .bind(certificate_id)
            .bind(CERTIFICATE_OPERATION_MAX_ATTEMPTS)
            .bind(request_sha256)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("insert scheduled certificate renewal", error))?;
            sqlx::query(
                "UPDATE web_certificate
                 SET renewal_status = 2, metadata = metadata - 'certificateOperationFailureCode', updated_at = NOW(), version = version + 1
                 WHERE tenant_id = $1 AND id = $2",
            )
            .bind(tenant_id)
            .bind(certificate_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("mark scheduled certificate renewal pending", error))?;
        }

        tx.commit()
            .await
            .map_err(|error| store_error("commit scheduled certificate renewals", error))?;
        Ok(candidates.len())
    }

    pub(super) async fn claim_certificate_operations_repo(
        &self,
        lease_owner: &str,
        lease_seconds: i64,
        limit: i32,
    ) -> WebServiceResult<Vec<CertificateOperationLease>> {
        validate_lease_owner(lease_owner)?;
        if !(MIN_OPERATION_LEASE_SECONDS..=MAX_OPERATION_LEASE_SECONDS).contains(&lease_seconds) {
            return Err(WebServiceError::validation(format!(
                "certificate operation lease must be between {MIN_OPERATION_LEASE_SECONDS} and {MAX_OPERATION_LEASE_SECONDS} seconds"
            )));
        }
        let limit = limit.clamp(1, MAX_OPERATION_CLAIM_BATCH);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin certificate operation claim", error))?;
        let reaped = reap_exhausted_certificate_operations_in_tx(&mut tx, limit).await?;
        if reaped > 0 {
            tracing::warn!(
                reaped,
                failure_code = EXPIRED_OPERATION_FAILURE_CODE,
                "reaped expired certificate operations after their retry budget was exhausted"
            );
        }
        let claimed_rows = sqlx::query(
            "WITH candidates AS (
                SELECT id FROM web_certificate_operation
                WHERE attempt_count < max_attempts
                  AND ((status = 'PENDING' AND next_attempt_at <= NOW())
                    OR (status = 'RUNNING' AND lease_expires_at <= NOW()))
                ORDER BY next_attempt_at ASC, id ASC
                FOR UPDATE SKIP LOCKED LIMIT $1
             )
             UPDATE web_certificate_operation operation
             SET status = 'RUNNING', attempt_count = operation.attempt_count + 1,
                 lease_owner = $2, lease_expires_at = NOW() + ($3 * INTERVAL '1 second'),
                 fencing_token = operation.fencing_token + 1, failure_code = NULL,
                 updated_at = NOW()
             FROM candidates
             WHERE operation.id = candidates.id
             RETURNING operation.id",
        )
        .bind(limit)
        .bind(lease_owner)
        .bind(lease_seconds)
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| store_error("claim certificate operations", error))?;
        if claimed_rows.is_empty() {
            tx.commit()
                .await
                .map_err(|error| store_error("commit empty certificate operation claim", error))?;
            return Ok(Vec::new());
        }
        let claimed_ids = claimed_rows
            .iter()
            .map(|row| row.try_get::<i64, _>("id"))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| store_error("map claimed certificate operation id", error))?;
        sqlx::query(
            "UPDATE web_certificate certificate
             SET renewal_status = 1, updated_at = NOW(), version = version + 1
             FROM web_certificate_operation operation
             WHERE operation.id = ANY($1) AND operation.tenant_id = certificate.tenant_id
               AND operation.certificate_id = certificate.id",
        )
        .bind(&claimed_ids)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("mark claimed certificate operations running", error))?;
        let rows = sqlx::query(
            "SELECT operation.tenant_id, operation.uuid AS operation_uuid,
                    operation.operation_type, operation.attempt_count, operation.max_attempts,
                    operation.lease_owner, operation.fencing_token,
                    certificate.uuid AS certificate_uuid, certificate.cert_type,
                    certificate.cert_name, certificate.preferred_key_algorithm,
                    certificate.auto_renew, identifier.hostnames
             FROM web_certificate_operation operation
             INNER JOIN web_certificate certificate
               ON certificate.tenant_id = operation.tenant_id
              AND certificate.id = operation.certificate_id
             INNER JOIN LATERAL (
                 SELECT CAST(jsonb_agg(identifier.hostname ORDER BY identifier.position) AS TEXT) AS hostnames
                 FROM web_certificate_identifier identifier
                 WHERE identifier.tenant_id = operation.tenant_id
                   AND identifier.certificate_id = operation.certificate_id
             ) identifier ON TRUE
             WHERE operation.id = ANY($1) AND operation.status = 'RUNNING'
               AND operation.lease_owner = $2
             ORDER BY operation.id ASC",
        )
        .bind(&claimed_ids)
        .bind(lease_owner)
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| store_error("load claimed certificate operations", error))?;
        let leases = rows
            .iter()
            .map(map_certificate_operation_lease)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| store_error("map claimed certificate operation", error))?;
        if leases.len() != claimed_ids.len() {
            return Err(WebServiceError::Internal(
                "claimed certificate operation is missing its certificate aggregate".to_string(),
            ));
        }
        tx.commit()
            .await
            .map_err(|error| store_error("commit certificate operation claim", error))?;
        Ok(leases)
    }

    pub(super) async fn fail_certificate_operation_repo(
        &self,
        lease: &CertificateOperationLease,
        failure_code: &str,
        retry_at: &str,
        terminal_retry_at: &str,
    ) -> WebServiceResult<CertificateOperationResponse> {
        validate_failure_code(failure_code)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin fail certificate operation", error))?;
        let operation = sqlx::query(
            "SELECT operation.id, operation.status, operation.attempt_count,
                    operation.max_attempts, operation.lease_owner, operation.fencing_token,
                    operation.lease_expires_at > NOW() AS lease_current,
                    operation.operation_type, certificate.id AS certificate_internal_id,
                    certificate.uuid AS certificate_uuid
             FROM web_certificate_operation operation
             INNER JOIN web_certificate certificate
               ON certificate.tenant_id = operation.tenant_id
              AND certificate.id = operation.certificate_id
             WHERE operation.tenant_id = $1 AND operation.uuid = $2
             FOR UPDATE OF operation, certificate",
        )
        .bind(lease.tenant_id)
        .bind(&lease.operation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| store_error("lock failed certificate operation", error))?
        .ok_or_else(|| WebServiceError::not_found("certificate operation not found"))?;
        validate_operation_lease(&operation, lease)?;
        let attempt_count: i32 = operation
            .try_get("attempt_count")
            .map_err(|error| store_error("map certificate operation attempt", error))?;
        let max_attempts: i32 = operation
            .try_get("max_attempts")
            .map_err(|error| store_error("map certificate operation maximum attempts", error))?;
        let terminal = attempt_count >= max_attempts;
        let operation_type: String = operation
            .try_get("operation_type")
            .map_err(|error| store_error("map certificate operation type", error))?;
        let certificate_internal_id: i64 = operation
            .try_get("certificate_internal_id")
            .map_err(|error| store_error("map failed operation certificate id", error))?;

        sqlx::query(
            "UPDATE web_certificate_operation
             SET status = CASE WHEN $3 THEN 'FAILED' ELSE 'PENDING' END,
                 next_attempt_at = CAST(CASE WHEN $3 THEN $5 ELSE $4 END AS TIMESTAMPTZ),
                 lease_owner = NULL, lease_expires_at = NULL, failure_code = $6,
                 completed_at = CASE WHEN $3 THEN NOW() ELSE NULL END, updated_at = NOW()
             WHERE tenant_id = $1 AND uuid = $2",
        )
        .bind(lease.tenant_id)
        .bind(&lease.operation_id)
        .bind(terminal)
        .bind(retry_at)
        .bind(terminal_retry_at)
        .bind(failure_code)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("persist certificate operation failure", error))?;
        // Atomic JSONB merge: the failure code is added to the existing
        // document so sibling keys (ARI renewal window, listing metadata)
        // are never clobbered by a whole-document replacement.
        let metadata = json!({ "certificateOperationFailureCode": failure_code });
        sqlx::query(
            "UPDATE web_certificate
             SET renewal_status = CASE WHEN $3 THEN 3 ELSE 2 END,
                 status = CASE WHEN $4 = 'ISSUE' AND $3 THEN 0 ELSE status END,
                 metadata = metadata || CAST($5 AS JSONB),
                 updated_at = NOW(), version = version + 1
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(lease.tenant_id)
        .bind(certificate_internal_id)
        .bind(terminal)
        .bind(operation_type)
        .bind(metadata.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("persist certificate aggregate operation failure", error))?;
        tx.commit()
            .await
            .map_err(|error| store_error("commit failed certificate operation", error))?;
        self.retrieve_certificate_operation_repo(lease.tenant_id, None, &lease.operation_id)
            .await
    }

    pub(super) async fn renew_certificate_operation_lease_repo(
        &self,
        lease: &CertificateOperationLease,
        lease_seconds: i64,
    ) -> WebServiceResult<()> {
        if !(MIN_OPERATION_LEASE_SECONDS..=MAX_OPERATION_LEASE_SECONDS).contains(&lease_seconds) {
            return Err(WebServiceError::validation(format!(
                "certificate operation lease must be between {MIN_OPERATION_LEASE_SECONDS} and {MAX_OPERATION_LEASE_SECONDS} seconds"
            )));
        }
        let result = sqlx::query(
            "UPDATE web_certificate_operation
             SET lease_expires_at = NOW() + ($4 * INTERVAL '1 second'),
                 updated_at = NOW()
             WHERE tenant_id = $1 AND uuid = $2 AND status = 'RUNNING'
               AND lease_owner = $3 AND fencing_token = $5",
        )
        .bind(lease.tenant_id)
        .bind(&lease.operation_id)
        .bind(&lease.lease_owner)
        .bind(lease_seconds)
        .bind(lease.fencing_token)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("renew certificate operation lease", error))?;
        if result.rows_affected() == 0 {
            return Err(WebServiceError::conflict(
                "certificate operation lease is no longer current",
            ));
        }
        Ok(())
    }

    async fn find_idempotent_certificate_operation(
        &self,
        tenant_id: i64,
        idempotency_key_hash: &str,
        request_sha256: &str,
    ) -> WebServiceResult<Option<CertificateOperationAcceptedResponse>> {
        let row = sqlx::query(
            "SELECT uuid, status, request_sha256
             FROM web_certificate_operation
             WHERE tenant_id = $1 AND idempotency_key_hash = $2",
        )
        .bind(tenant_id)
        .bind(idempotency_key_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("find idempotent certificate operation", error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let existing = ExistingCertificateOperation::from_row(&row)?;
        ensure_matching_request(&existing, request_sha256)?;
        Ok(Some(accepted_operation(&existing.uuid, &existing.status)))
    }
}

struct ExistingCertificateOperation {
    uuid: String,
    status: String,
    request_sha256: String,
}

impl ExistingCertificateOperation {
    fn from_row(row: &EngineRow) -> WebServiceResult<Self> {
        Ok(Self {
            uuid: row
                .try_get("uuid")
                .map_err(|error| store_error("map certificate operation uuid", error))?,
            status: row
                .try_get("status")
                .map_err(|error| store_error("map certificate operation status", error))?,
            request_sha256: row
                .try_get("request_sha256")
                .map_err(|error| store_error("map certificate operation request hash", error))?,
        })
    }
}

async fn find_idempotent_certificate_operation_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: i64,
    idempotency_key_hash: &str,
) -> WebServiceResult<Option<ExistingCertificateOperation>> {
    let row = sqlx::query(
        "SELECT uuid, status, request_sha256
         FROM web_certificate_operation
         WHERE tenant_id = $1 AND idempotency_key_hash = $2",
    )
    .bind(tenant_id)
    .bind(idempotency_key_hash)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("find idempotent certificate operation", error))?;
    row.as_ref()
        .map(ExistingCertificateOperation::from_row)
        .transpose()
}

async fn find_active_certificate_operation_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: i64,
    certificate_id: i64,
) -> WebServiceResult<Option<ExistingCertificateOperation>> {
    let row = sqlx::query(
        "SELECT uuid, status, request_sha256
         FROM web_certificate_operation
         WHERE tenant_id = $1 AND certificate_id = $2
           AND status IN ('PENDING', 'RUNNING')
         ORDER BY id DESC LIMIT 1",
    )
    .bind(tenant_id)
    .bind(certificate_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("find active certificate operation", error))?;
    row.as_ref()
        .map(ExistingCertificateOperation::from_row)
        .transpose()
}

async fn reap_exhausted_certificate_operations_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    limit: i32,
) -> WebServiceResult<u64> {
    let row = sqlx::query(
        "WITH candidates AS (
            SELECT id
            FROM web_certificate_operation
            WHERE status = 'RUNNING'
              AND lease_expires_at <= NOW()
              AND attempt_count >= max_attempts
            ORDER BY lease_expires_at ASC, id ASC
            FOR UPDATE SKIP LOCKED LIMIT $1
         ), expired AS (
            UPDATE web_certificate_operation operation
            SET status = 'FAILED',
                next_attempt_at = NOW() + ($2 * INTERVAL '1 second'),
                lease_owner = NULL, lease_expires_at = NULL,
                failure_code = $3, completed_at = NOW(), updated_at = NOW()
            FROM candidates
            WHERE operation.id = candidates.id
            RETURNING operation.tenant_id, operation.certificate_id, operation.operation_type
         ), updated_certificates AS (
            UPDATE web_certificate certificate
            SET renewal_status = 3,
                status = CASE WHEN expired.operation_type = 'ISSUE' THEN 0 ELSE certificate.status END,
                metadata = certificate.metadata
                    || jsonb_build_object('certificateOperationFailureCode', $3::text),
                updated_at = NOW(), version = certificate.version + 1
            FROM expired
            WHERE certificate.tenant_id = expired.tenant_id
              AND certificate.id = expired.certificate_id
            RETURNING certificate.id
         ), archived_certificates AS (
            UPDATE web_certificate certificate
            SET deleted_at = NOW(), updated_at = NOW(),
                version = certificate.version + 1
            FROM expired
            WHERE certificate.tenant_id = expired.tenant_id
              AND certificate.id = expired.certificate_id
              AND expired.operation_type = 'ISSUE'
            RETURNING certificate.tenant_id, certificate.id AS certificate_id
         ), removed_identifiers AS (
            DELETE FROM web_certificate_identifier identifier
            USING archived_certificates archived
            WHERE identifier.tenant_id = archived.tenant_id
              AND identifier.certificate_id = archived.certificate_id
         )
         SELECT COUNT(*)::BIGINT AS reaped_count FROM expired",
    )
    .bind(limit)
    .bind(TERMINAL_OPERATION_COOLDOWN_SECONDS)
    .bind(EXPIRED_OPERATION_FAILURE_CODE)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error("reap exhausted certificate operations", error))?;
    let reaped: u64 = row
        .try_get::<i64, _>("reaped_count")
        .map(|count| count as u64)
        .map_err(|error| store_error("map reaped certificate operation count", error))?;
    // Soft-delete ISSUE certificates whose operations already reached terminal
    // FAILED through the manual failure path; they have no active operation and
    // never produced a usable version, so keeping them would accumulate orphan
    // certificate rows that block domain removal.
    sqlx::query(
        "WITH failed_issue_certificates AS (
             SELECT DISTINCT operation.tenant_id, operation.certificate_id
             FROM web_certificate_operation operation
             INNER JOIN web_certificate certificate
               ON certificate.tenant_id = operation.tenant_id
              AND certificate.id = operation.certificate_id
              AND certificate.deleted_at IS NULL
             WHERE operation.operation_type = 'ISSUE'
               AND operation.status = 'FAILED'
               AND operation.completed_at IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1 FROM web_certificate_operation active_operation
                   WHERE active_operation.tenant_id = operation.tenant_id
                     AND active_operation.certificate_id = operation.certificate_id
                     AND active_operation.status IN ('PENDING', 'RUNNING')
               )
             LIMIT $1
         ), archived AS (
             UPDATE web_certificate certificate
             SET deleted_at = NOW(), updated_at = NOW(),
                 version = certificate.version + 1
             FROM failed_issue_certificates failed
             WHERE certificate.tenant_id = failed.tenant_id
               AND certificate.id = failed.certificate_id
             RETURNING certificate.tenant_id, certificate.id AS certificate_id
         )
         DELETE FROM web_certificate_identifier identifier
         USING archived
         WHERE identifier.tenant_id = archived.tenant_id
           AND identifier.certificate_id = archived.certificate_id",
    )
    .bind(limit)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("archive terminal failed ISSUE certificates", error))?;
    Ok(reaped)
}

fn accepted_operation(operation_id: &str, status: &str) -> CertificateOperationAcceptedResponse {
    CertificateOperationAcceptedResponse {
        accepted: true,
        operation_id: operation_id.to_string(),
        status: status.to_ascii_lowercase(),
    }
}

fn ensure_matching_request(
    existing: &ExistingCertificateOperation,
    request_sha256: &str,
) -> WebServiceResult<()> {
    if existing.request_sha256 != request_sha256 {
        return Err(WebServiceError::conflict(
            "idempotency key was already used with a different certificate request",
        ));
    }
    Ok(())
}

fn certificate_operation_idempotency_key_hash(
    tenant_id: i64,
    requested_by: Option<i64>,
    operation_type: &str,
    resource: &str,
    raw_key: Option<&str>,
) -> WebServiceResult<String> {
    let raw_key =
        raw_key.ok_or_else(|| WebServiceError::validation("idempotency key is required"))?;
    if raw_key != raw_key.trim() || !(1..=128).contains(&raw_key.len()) {
        return Err(WebServiceError::validation(
            "idempotency key must contain between 1 and 128 bytes without surrounding whitespace",
        ));
    }
    let scope = format!(
        "v1:{tenant_id}:{}:{operation_type}:{}:{resource}:{}:{raw_key}",
        requested_by.unwrap_or_default(),
        resource.len(),
        raw_key.len()
    );
    Ok(sha256_hex(&scope))
}

fn certificate_issue_request_sha256(request: &IssueCertificateRequest) -> String {
    let mut canonical = format!(
        "v1:ISSUE:{}:{}:{}:",
        request.cert_type,
        request.key_algorithm,
        u8::from(request.auto_renew)
    );
    for domain_id in &request.domain_ids {
        canonical.push_str(&format!("{}:{domain_id}:", domain_id.len()));
    }
    sha256_hex(&canonical)
}

fn certificate_renewal_request_sha256(certificate_uuid: &str) -> String {
    sha256_hex(&format!(
        "v2:RENEW:{}:{certificate_uuid}",
        certificate_uuid.len()
    ))
}

fn validate_certificate_issue_request(request: &IssueCertificateRequest) -> WebServiceResult<()> {
    if request.domain_ids.is_empty() || request.domain_ids.len() > MAX_CERTIFICATE_IDENTIFIERS {
        return Err(WebServiceError::validation(format!(
            "domainIds must contain between 1 and {MAX_CERTIFICATE_IDENTIFIERS} identifiers"
        )));
    }
    if request
        .domain_ids
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len()
        != request.domain_ids.len()
    {
        return Err(WebServiceError::validation("domainIds must be unique"));
    }
    if !matches!(request.cert_type, 1 | 3) {
        return Err(WebServiceError::validation(
            "certType must be 1 (Let's Encrypt) or 3 (self-signed)",
        ));
    }
    if request.cert_type == 3 && request.auto_renew {
        return Err(WebServiceError::validation(
            "automatic renewal is supported only for ACME certificates",
        ));
    }
    if !matches!(request.key_algorithm.as_str(), "ECDSA" | "RSA") {
        return Err(WebServiceError::validation(
            "keyAlgorithm must be ECDSA or RSA",
        ));
    }
    Ok(())
}

fn certificate_ca_profile(cert_type: i32) -> &'static str {
    match cert_type {
        1 => "LETS_ENCRYPT_PRODUCTION",
        3 => "SELF_SIGNED",
        _ => "CUSTOM",
    }
}

fn validate_lease_owner(lease_owner: &str) -> WebServiceResult<()> {
    if lease_owner.is_empty()
        || lease_owner.len() > 128
        || !lease_owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(WebServiceError::validation(
            "certificate operation lease owner is invalid",
        ));
    }
    Ok(())
}

fn validate_failure_code(failure_code: &str) -> WebServiceResult<()> {
    if failure_code.is_empty()
        || failure_code.len() > 64
        || !failure_code
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(WebServiceError::validation(
            "certificate operation failure code is invalid",
        ));
    }
    Ok(())
}

pub(super) fn validate_operation_lease(
    row: &EngineRow,
    lease: &CertificateOperationLease,
) -> WebServiceResult<()> {
    let status: String = row
        .try_get("status")
        .map_err(|error| store_error("map certificate operation lease status", error))?;
    let lease_owner: Option<String> = row
        .try_get("lease_owner")
        .map_err(|error| store_error("map certificate operation lease owner", error))?;
    let fencing_token: i64 = row
        .try_get("fencing_token")
        .map_err(|error| store_error("map certificate operation fencing token", error))?;
    let lease_current: bool = row
        .try_get("lease_current")
        .map_err(|error| store_error("map certificate operation lease expiry", error))?;
    let certificate_uuid: String = row
        .try_get("certificate_uuid")
        .map_err(|error| store_error("map certificate operation certificate", error))?;
    if status != "RUNNING"
        || lease_owner.as_deref() != Some(lease.lease_owner.as_str())
        || fencing_token != lease.fencing_token
        || !lease_current
        || certificate_uuid != lease.certificate_id
    {
        return Err(WebServiceError::conflict(
            "certificate operation lease is no longer current",
        ));
    }
    Ok(())
}

fn map_certificate_operation(row: &EngineRow) -> Result<CertificateOperationResponse, sqlx::Error> {
    Ok(CertificateOperationResponse {
        id: row.try_get("uuid")?,
        certificate_id: row.try_get("certificate_uuid")?,
        operation_type: row.try_get("operation_type")?,
        status: row.try_get("status")?,
        attempt_count: row.try_get("attempt_count")?,
        max_attempts: row.try_get("max_attempts")?,
        next_attempt_at: instant_from_row(row, "next_attempt_at")?,
        failure_code: row.try_get("failure_code")?,
        created_at: instant_from_row(row, "created_at")?,
        updated_at: instant_from_row(row, "updated_at")?,
        completed_at: optional_instant_from_row(row, "completed_at")?,
    })
}

fn map_certificate_operation_lease(
    row: &EngineRow,
) -> Result<CertificateOperationLease, sqlx::Error> {
    let hostnames_json: String = row.try_get("hostnames")?;
    let hostnames = serde_json::from_str::<Vec<String>>(&hostnames_json)
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
    if hostnames.is_empty() {
        return Err(sqlx::Error::Decode(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "certificate operation has no identifiers",
        ))));
    }
    Ok(CertificateOperationLease {
        tenant_id: row.try_get("tenant_id")?,
        operation_id: row.try_get("operation_uuid")?,
        certificate_id: row.try_get("certificate_uuid")?,
        operation_type: row.try_get("operation_type")?,
        cert_type: row.try_get("cert_type")?,
        cert_name: row.try_get("cert_name")?,
        hostnames,
        key_algorithm: row.try_get("preferred_key_algorithm")?,
        auto_renew: bool_from_row(row, "auto_renew")?,
        attempt_count: row.try_get("attempt_count")?,
        max_attempts: row.try_get("max_attempts")?,
        lease_owner: row.try_get("lease_owner")?,
        fencing_token: row.try_get("fencing_token")?,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        certificate_issue_request_sha256, certificate_operation_idempotency_key_hash,
        certificate_renewal_request_sha256, validate_certificate_issue_request,
        validate_failure_code, validate_lease_owner,
    };
    use sdkwork_webserver_contract::IssueCertificateRequest;

    #[test]
    fn request_fingerprint_is_stable_and_order_sensitive() {
        let request = IssueCertificateRequest {
            domain_ids: vec!["domain-a".to_string(), "domain-b".to_string()],
            cert_type: 1,
            key_algorithm: "ECDSA".to_string(),
            auto_renew: true,
        };
        assert_eq!(
            certificate_issue_request_sha256(&request),
            certificate_issue_request_sha256(&request)
        );
        let mut reversed = request.clone();
        reversed.domain_ids.reverse();
        assert_ne!(
            certificate_issue_request_sha256(&request),
            certificate_issue_request_sha256(&reversed)
        );
    }

    #[test]
    fn renewal_fingerprint_is_stable_across_immutable_version_changes() {
        assert_eq!(
            certificate_renewal_request_sha256("certificate-a"),
            certificate_renewal_request_sha256("certificate-a")
        );
        assert_ne!(
            certificate_renewal_request_sha256("certificate-a"),
            certificate_renewal_request_sha256("certificate-b")
        );
    }

    #[test]
    fn self_signed_issue_cannot_enable_automatic_renewal() {
        let request = IssueCertificateRequest {
            domain_ids: vec!["domain-a".to_string()],
            cert_type: 3,
            key_algorithm: "ECDSA".to_string(),
            auto_renew: true,
        };
        assert!(validate_certificate_issue_request(&request).is_err());
    }

    #[test]
    fn raw_idempotency_keys_are_scoped_and_never_persisted() {
        let raw = "logical-command-key";
        let hash = certificate_operation_idempotency_key_hash(
            42,
            Some(7),
            "ISSUE",
            "certificate_collection",
            Some(raw),
        )
        .expect("hash idempotency key");
        assert_eq!(hash.len(), 64);
        assert!(!hash.contains(raw));
        assert!(certificate_operation_idempotency_key_hash(
            42,
            Some(7),
            "ISSUE",
            "certificate_collection",
            None,
        )
        .is_err());
    }

    #[test]
    fn operation_control_values_are_bounded() {
        assert!(validate_lease_owner("worker-1:123").is_ok());
        assert!(validate_lease_owner("worker/1").is_err());
        assert!(validate_failure_code("ACME_ISSUER_FAILED").is_ok());
        assert!(validate_failure_code("provider failed").is_err());
    }
}
