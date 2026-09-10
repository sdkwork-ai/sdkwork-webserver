use super::{EngineRow, WebRepository};
use crate::audited_sql;
use chrono::{Duration, Utc};
use sdkwork_intelligence_webserver_service::{
    DomainVerificationChallenge, DomainVerificationObservation,
};
use sdkwork_utils_rust::crypto::sha256_hash;
use sdkwork_webserver_contract::{
    CreateDomainRequest, CreateManagedDomainRequest, DomainDeploymentResponse, DomainPage,
    DomainResponse, UpdateDomainApplicationBindingRequest, WebServiceError, WebServiceResult,
};
use sqlx::{Postgres, Row, Transaction};

use super::support::{
    bool_from_row, instant_from_row, instant_write_expression, new_uuid, next_id, now_rfc3339,
    optional_instant_from_row, pagination, resolve_site_internal_id, resolve_site_owner_id,
    store_error,
};

impl WebRepository {
    pub(super) async fn list_domains_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;
        let (_page, page_size, offset) = pagination(page, page_size)?;
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT d.id)
             FROM web_domain d
             INNER JOIN web_site_binding b ON b.tenant_id = d.tenant_id AND b.domain_id = d.id
             WHERE d.tenant_id = $1 AND b.site_id = $2
               AND b.deleted_at IS NULL AND b.status <> 'ARCHIVED'
               AND d.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count application domain bindings", error))?;

        let rows = sqlx::query(audited_sql(&domain_select(
            "d.tenant_id = $1 AND b.site_id = $2
             AND b.deleted_at IS NULL AND b.status <> 'ARCHIVED' AND d.deleted_at IS NULL
             ORDER BY b.updated_at DESC, b.id DESC LIMIT $3 OFFSET $4",
        )))
        .bind(tenant_id)
        .bind(site_internal_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list application domain bindings", error))?;

        Ok(DomainPage {
            items: map_domain_rows(&rows)?,
            total,
        })
    }

    pub(super) async fn create_domain_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        request: &CreateDomainRequest,
    ) -> WebServiceResult<DomainResponse> {
        self.create_domain_asset_repo(
            tenant_id,
            Some(site_id),
            &request.hostname,
            request.is_primary,
            request.ssl_enabled,
            request.ssl_provider.as_deref(),
        )
        .await
    }

    pub(super) async fn list_managed_domains_repo(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage> {
        let (_page, page_size, offset) = pagination(page, page_size)?;
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM web_domain WHERE tenant_id = $1 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count managed web_domain", error))?;
        let rows = sqlx::query(audited_sql(&domain_select(
            "d.tenant_id = $1 AND d.deleted_at IS NULL
             ORDER BY d.updated_at DESC, d.id DESC LIMIT $2 OFFSET $3",
        )))
        .bind(tenant_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list managed web_domain", error))?;

        Ok(DomainPage {
            items: map_domain_rows(&rows)?,
            total,
        })
    }

    pub(super) async fn list_certificate_domains_repo(
        &self,
        tenant_id: i64,
        owner_id: Option<i64>,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage> {
        let (_page, page_size, offset) = pagination(page, page_size)?;
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM web_domain
             WHERE tenant_id = $1 AND deleted_at IS NULL
               AND ($2 IS NULL OR user_id = $2)",
        )
        .bind(tenant_id)
        .bind(owner_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count certificate domain options", error))?;
        let rows = sqlx::query(audited_sql(&domain_select(
            "d.tenant_id = $1 AND d.deleted_at IS NULL
             AND ($2 IS NULL OR d.user_id = $2)
             ORDER BY d.updated_at DESC, d.id DESC LIMIT $3 OFFSET $4",
        )))
        .bind(tenant_id)
        .bind(owner_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list certificate domain options", error))?;

        Ok(DomainPage {
            items: map_domain_rows(&rows)?,
            total,
        })
    }

    pub(super) async fn create_managed_domain_repo(
        &self,
        tenant_id: i64,
        request: &CreateManagedDomainRequest,
    ) -> WebServiceResult<DomainResponse> {
        self.create_domain_asset_repo(
            tenant_id,
            request.application_id.as_deref(),
            &request.hostname,
            request.is_primary,
            request.ssl_enabled,
            request.ssl_provider.as_deref(),
        )
        .await
    }

    async fn create_domain_asset_repo(
        &self,
        tenant_id: i64,
        application_id: Option<&str>,
        hostname: &str,
        is_primary: bool,
        ssl_enabled: bool,
        ssl_provider: Option<&str>,
    ) -> WebServiceResult<DomainResponse> {
        if application_id.is_none() && is_primary {
            return Err(WebServiceError::validation(
                "an unbound domain cannot be primary",
            ));
        }
        let root_domain_id: i64 = sqlx::query_scalar(
            "SELECT id FROM web_root_domain
             WHERE tenant_id = $1 AND deleted_at IS NULL
               AND ($2 = hostname OR $2 LIKE ('%.' || hostname))
             ORDER BY LENGTH(hostname) DESC, id DESC LIMIT 1",
        )
        .bind(tenant_id)
        .bind(hostname)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve explicit root domain", error))?
        .ok_or_else(|| {
            WebServiceError::validation("define the matching root domain before adding a hostname")
        })?;
        let site_internal_id = match application_id {
            Some(application_id) => {
                Some(resolve_site_internal_id(&self.pool, tenant_id, application_id).await?)
            }
            None => None,
        };
        let owner_user_id = match site_internal_id {
            Some(site_id) => resolve_site_owner_id(&self.pool, tenant_id, site_id).await?,
            None => None,
        };
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();

        let insert_time = instant_write_expression("$9");
        let insert_sql = format!(
            "INSERT INTO web_domain (
                id, uuid, tenant_id, user_id, root_domain_id, hostname, hostname_type,
                verification_status, status, metadata, created_at, updated_at, version
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'PENDING', $8, '{{}}',
                       {insert_time}, {insert_time}, 0)"
        );
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin create web_domain", error))?;
        sqlx::query(audited_sql(&insert_sql))
            .bind(id)
            .bind(&uuid)
            .bind(tenant_id)
            .bind(owner_user_id)
            .bind(root_domain_id)
            .bind(hostname)
            .bind(if hostname.starts_with("*.") {
                "WILDCARD"
            } else {
                "EXACT"
            })
            .bind(0_i32)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("insert web_domain", error))?;

        if let Some(site_internal_id) = site_internal_id {
            self.insert_site_binding(
                &mut tx,
                tenant_id,
                site_internal_id,
                id,
                is_primary,
                ssl_enabled,
                ssl_provider,
                &now,
            )
            .await?;
        }
        tx.commit()
            .await
            .map_err(|error| store_error("commit create web_domain", error))?;
        self.retrieve_managed_domain_repo(tenant_id, &uuid).await
    }

    pub(super) async fn retrieve_domain_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<DomainResponse> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;
        let row = sqlx::query(audited_sql(&domain_select(
            "d.tenant_id = $1 AND d.uuid = $2 AND b.site_id = $3
             AND b.deleted_at IS NULL AND b.status <> 'ARCHIVED' AND d.deleted_at IS NULL",
        )))
        .bind(tenant_id)
        .bind(domain_id)
        .bind(site_internal_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve application domain binding", error))?
        .ok_or_else(|| WebServiceError::not_found("domain binding not found"))?;
        map_domain_row(&row)
            .map_err(|error| WebServiceError::Internal(format!("map domain binding: {error}")))
    }

    pub(super) async fn retrieve_managed_domain_repo(
        &self,
        tenant_id: i64,
        domain_id: &str,
    ) -> WebServiceResult<DomainResponse> {
        let row = sqlx::query(audited_sql(&domain_select(
            "d.tenant_id = $1 AND d.uuid = $2 AND d.deleted_at IS NULL",
        )))
        .bind(tenant_id)
        .bind(domain_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve managed web_domain", error))?
        .ok_or_else(|| WebServiceError::not_found("domain not found"))?;
        map_domain_row(&row)
            .map_err(|error| WebServiceError::Internal(format!("map managed domain: {error}")))
    }

    pub(super) async fn delete_domain_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<()> {
        self.unbind_managed_domain_repo(tenant_id, domain_id, Some(site_id))
            .await
            .map(|_| ())
    }

    pub(super) async fn delete_managed_domain_repo(
        &self,
        tenant_id: i64,
        domain_id: &str,
    ) -> WebServiceResult<()> {
        // Guard counts and the soft delete share one transaction with the
        // domain row locked, so a concurrent site-binding insert (which locks
        // the same domain row first) cannot slip an orphan binding past the
        // check-then-act window or free the unique hostname slot while routes
        // still reference the domain.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin managed domain delete transaction", error))?;
        let row = sqlx::query(
            "SELECT d.id,
                    (SELECT COUNT(*) FROM web_site_binding b
                     WHERE b.tenant_id = d.tenant_id AND b.domain_id = d.id
                       AND b.deleted_at IS NULL AND b.status <> 'ARCHIVED') AS binding_count,
                    (SELECT COUNT(*) FROM web_certificate_identifier ci
                     INNER JOIN web_certificate c
                       ON c.tenant_id = ci.tenant_id AND c.id = ci.certificate_id
                      AND c.deleted_at IS NULL
                     WHERE ci.tenant_id = d.tenant_id AND ci.domain_id = d.id) AS certificate_count
             FROM web_domain d
             WHERE d.tenant_id = $1 AND d.uuid = $2 AND d.deleted_at IS NULL
             FOR UPDATE OF d",
        )
        .bind(tenant_id)
        .bind(domain_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| store_error("load managed domain delete state", error))?
        .ok_or_else(|| WebServiceError::not_found("domain not found"))?;
        let binding_count: i64 = row
            .try_get("binding_count")
            .map_err(|error| store_error("map managed domain binding count", error))?;
        let certificate_count: i64 = row
            .try_get("certificate_count")
            .map_err(|error| store_error("map managed domain certificate count", error))?;
        if binding_count > 0 {
            return Err(WebServiceError::conflict(
                "domain bindings must be removed before deletion",
            ));
        }
        if certificate_count > 0 {
            return Err(WebServiceError::conflict(
                "domain certificate identifiers must be removed before deletion",
            ));
        }
        let now = now_rfc3339();

        let now_expression = instant_write_expression("$3");
        let sql = format!(
            "UPDATE web_domain SET deleted_at = {now_expression}, updated_at = {now_expression},
                    version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL"
        );
        let result = sqlx::query(audited_sql(&sql))
            .bind(tenant_id)
            .bind(domain_id)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("delete managed domain", error))?;
        if result.rows_affected() == 0 {
            return Err(WebServiceError::not_found("domain not found"));
        }
        tx.commit()
            .await
            .map_err(|error| store_error("commit managed domain delete", error))?;
        Ok(())
    }

    pub(super) async fn bind_managed_domain_repo(
        &self,
        tenant_id: i64,
        domain_id: &str,
        request: &UpdateDomainApplicationBindingRequest,
    ) -> WebServiceResult<DomainResponse> {
        let site_internal_id =
            resolve_site_internal_id(&self.pool, tenant_id, &request.application_id).await?;
        let domain_internal_id: i64 = sqlx::query_scalar(
            "SELECT id FROM web_domain
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(domain_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve managed domain binding", error))?
        .ok_or_else(|| WebServiceError::not_found("domain not found"))?;
        let now = now_rfc3339();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin bind managed domain", error))?;
        self.insert_site_binding(
            &mut tx,
            tenant_id,
            site_internal_id,
            domain_internal_id,
            request.is_primary,
            false,
            None,
            &now,
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| store_error("commit bind managed domain", error))?;
        self.retrieve_managed_domain_repo(tenant_id, domain_id)
            .await
    }

    pub(super) async fn unbind_managed_domain_repo(
        &self,
        tenant_id: i64,
        domain_id: &str,
        expected_application_id: Option<&str>,
    ) -> WebServiceResult<DomainResponse> {
        let expected_site_id = match expected_application_id {
            Some(application_id) => {
                Some(resolve_site_internal_id(&self.pool, tenant_id, application_id).await?)
            }
            None => None,
        };
        let now = now_rfc3339();

        let now_expression = instant_write_expression("$4");
        let sql = format!(
            "UPDATE web_site_binding b
             SET status = 'ARCHIVED', deleted_at = {now_expression}, updated_at = {now_expression},
                 is_primary = FALSE, version = b.version + 1
             FROM web_domain d
             WHERE b.tenant_id = $1 AND d.tenant_id = b.tenant_id AND d.id = b.domain_id
               AND d.uuid = $2 AND d.deleted_at IS NULL
               AND ($3 IS NULL OR b.site_id = $3)
               AND b.environment = 'production' AND b.deleted_at IS NULL
               AND b.status <> 'ARCHIVED'"
        );
        let result = sqlx::query(audited_sql(&sql))
            .bind(tenant_id)
            .bind(domain_id)
            .bind(expected_site_id)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("archive managed domain binding", error))?;
        if result.rows_affected() == 0 {
            return Err(WebServiceError::not_found("domain binding not found"));
        }
        self.retrieve_managed_domain_repo(tenant_id, domain_id)
            .await
    }

    pub(super) async fn prepare_domain_verification_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<DomainVerificationChallenge> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;
        self.prepare_domain_asset_verification_repo(tenant_id, domain_id, Some(site_internal_id))
            .await
    }

    pub(super) async fn prepare_managed_domain_verification_repo(
        &self,
        tenant_id: i64,
        domain_id: &str,
    ) -> WebServiceResult<DomainVerificationChallenge> {
        self.prepare_domain_asset_verification_repo(tenant_id, domain_id, None)
            .await
    }

    async fn prepare_domain_asset_verification_repo(
        &self,
        tenant_id: i64,
        domain_id: &str,
        expected_site_id: Option<i64>,
    ) -> WebServiceResult<DomainVerificationChallenge> {
        let now = now_rfc3339();

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin domain verification challenge", error))?;
        let domain = sqlx::query(
            "SELECT d.id, d.hostname, d.verification_status
             FROM web_domain d
             WHERE d.tenant_id = $1 AND d.uuid = $2 AND d.deleted_at IS NULL
               AND ($3 IS NULL OR EXISTS (
                   SELECT 1 FROM web_site_binding b
                   WHERE b.tenant_id = d.tenant_id AND b.domain_id = d.id AND b.site_id = $3
                     AND b.deleted_at IS NULL AND b.status <> 'ARCHIVED'
               ))
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(domain_id)
        .bind(expected_site_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| store_error("lock domain for verification", error))?
        .ok_or_else(|| WebServiceError::not_found("domain not found"))?;
        let domain_internal_id: i64 = domain
            .try_get("id")
            .map_err(|error| WebServiceError::Internal(error.to_string()))?;
        let hostname: String = domain
            .try_get("hostname")
            .map_err(|error| WebServiceError::Internal(error.to_string()))?;
        let verification_status: String = domain
            .try_get("verification_status")
            .map_err(|error| WebServiceError::Internal(error.to_string()))?;

        let expire_time = instant_write_expression("$3");
        let expire_sql = format!(
            "UPDATE web_domain_verification
             SET status = 'EXPIRED', failure_code = 'CHALLENGE_EXPIRED',
                 next_attempt_at = NULL, updated_at = {expire_time}, version = version + 1
             WHERE tenant_id = $1 AND domain_id = $2 AND status IN ('PENDING', 'CHECKING')
               AND expires_at <= {expire_time}"
        );
        sqlx::query(audited_sql(&expire_sql))
            .bind(tenant_id)
            .bind(domain_internal_id)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("expire domain verification challenge", error))?;

        let reusable_status = if verification_status == "VERIFIED" {
            "VERIFIED"
        } else {
            "ACTIVE"
        };
        if let Some(row) = fetch_domain_verification_challenge(
            &mut tx,
            tenant_id,
            domain_internal_id,
            reusable_status,
            &now,
        )
        .await?
        {
            tx.commit()
                .await
                .map_err(|error| store_error("commit reusable domain verification", error))?;
            return map_domain_verification_challenge(&row, hostname);
        }
        if verification_status == "VERIFIED" {
            return Err(WebServiceError::Internal(
                "verified domain is missing verification evidence".to_string(),
            ));
        }

        let challenge_id = new_uuid();
        let record_name = verification_record_name(&hostname);
        let record_value = verification_record_value(&challenge_id);
        let proof_sha256 = sha256_hash(record_value.as_bytes());
        let expires_at = (Utc::now() + Duration::minutes(30)).to_rfc3339();
        let challenge_internal_id = next_id(self.id_generator())?;
        let time = instant_write_expression("$8");
        let expiry = instant_write_expression("$7");
        let insert_sql = format!(
            "INSERT INTO web_domain_verification (
                id, uuid, tenant_id, domain_id, method, record_name, proof_sha256, status,
                attempt_count, next_attempt_at, expires_at, created_at, updated_at, version
             ) VALUES ($1, $2, $3, $4, 'DNS_TXT', $5, $6, 'PENDING', 0,
                       {time}, {expiry}, {time}, {time}, 0)"
        );
        sqlx::query(audited_sql(&insert_sql))
            .bind(challenge_internal_id)
            .bind(&challenge_id)
            .bind(tenant_id)
            .bind(domain_internal_id)
            .bind(&record_name)
            .bind(&proof_sha256)
            .bind(&expires_at)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("insert domain verification challenge", error))?;
        let pending_time = instant_write_expression("$3");
        let pending_sql = format!(
            "UPDATE web_domain SET verification_status = 'PENDING', verified_at = NULL,
                    status = 0, updated_at = {pending_time}, version = version + 1
             WHERE tenant_id = $1 AND id = $2"
        );
        sqlx::query(audited_sql(&pending_sql))
            .bind(tenant_id)
            .bind(domain_internal_id)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("mark domain verification pending", error))?;
        tx.commit()
            .await
            .map_err(|error| store_error("commit domain verification challenge", error))?;
        Ok(DomainVerificationChallenge {
            challenge_id,
            hostname,
            method: "DNS_TXT".to_string(),
            record_name,
            proof_sha256,
            status: "PENDING".to_string(),
            attempt_count: 0,
            expires_at,
            next_attempt_at: Some(now),
            checked_at: None,
            failure_code: None,
            ready_for_check: false,
        })
    }

    pub(super) async fn record_domain_verification_observation_repo(
        &self,
        tenant_id: i64,
        challenge_id: &str,
        observation: &DomainVerificationObservation,
    ) -> WebServiceResult<DomainVerificationChallenge> {
        validate_domain_verification_observation(observation)?;
        let now = now_rfc3339();

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin domain verification observation", error))?;
        let row =
            fetch_domain_verification_challenge_for_update(&mut tx, tenant_id, challenge_id, &now)
                .await?
                .ok_or_else(|| {
                    WebServiceError::not_found("domain verification challenge not found")
                })?;
        let domain_internal_id: i64 = row
            .try_get("domain_id")
            .map_err(|error| WebServiceError::Internal(error.to_string()))?;
        let hostname: String = row
            .try_get("hostname")
            .map_err(|error| WebServiceError::Internal(error.to_string()))?;
        let current = map_domain_verification_challenge(&row, hostname.clone())?;
        if matches!(current.status.as_str(), "VERIFIED" | "FAILED" | "EXPIRED") {
            tx.commit()
                .await
                .map_err(|error| store_error("commit unchanged domain verification", error))?;
            return Ok(current);
        }

        if bool_from_row(&row, "is_expired")
            .map_err(|error| WebServiceError::Internal(error.to_string()))?
        {
            let expired = update_domain_verification_state(
                &mut tx,
                tenant_id,
                challenge_id,
                "EXPIRED",
                observation.observed_sha256.as_deref(),
                current.attempt_count,
                None,
                Some("CHALLENGE_EXPIRED"),
                &now,
            )
            .await?;
            update_domain_verification_status(
                &mut tx,
                tenant_id,
                domain_internal_id,
                "EXPIRED",
                &now,
            )
            .await?;
            tx.commit()
                .await
                .map_err(|error| store_error("commit expired domain verification", error))?;
            return map_domain_verification_challenge(&expired, hostname);
        }
        if !current.ready_for_check {
            tx.commit()
                .await
                .map_err(|error| store_error("commit deferred domain verification", error))?;
            return Ok(current);
        }

        let verified =
            observation.observed_sha256.as_deref() == Some(current.proof_sha256.as_str());
        let attempt_count = current.attempt_count.saturating_add(1);
        let terminal_failure = !verified && attempt_count >= 20;
        let status = if verified {
            "VERIFIED"
        } else if terminal_failure {
            "FAILED"
        } else {
            "PENDING"
        };
        let next_attempt_at = if status == "PENDING" {
            Some((Utc::now() + verification_retry_delay(challenge_id, attempt_count)).to_rfc3339())
        } else {
            None
        };
        let failure_code = if verified {
            None
        } else if terminal_failure {
            Some("MAX_ATTEMPTS_EXCEEDED")
        } else {
            observation.failure_code.as_deref()
        };
        let updated = update_domain_verification_state(
            &mut tx,
            tenant_id,
            challenge_id,
            status,
            observation.observed_sha256.as_deref(),
            attempt_count,
            next_attempt_at.as_deref(),
            failure_code,
            &now,
        )
        .await?;
        update_domain_verification_status(&mut tx, tenant_id, domain_internal_id, status, &now)
            .await?;
        if verified {
            activate_verified_domain_bindings(&mut tx, tenant_id, domain_internal_id, &now).await?;
        }
        tx.commit()
            .await
            .map_err(|error| store_error("commit domain verification observation", error))?;
        map_domain_verification_challenge(&updated, hostname)
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_site_binding(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        tenant_id: i64,
        site_id: i64,
        domain_id: i64,
        is_primary: bool,
        ssl_enabled: bool,
        ssl_provider: Option<&str>,
        now: &str,
    ) -> WebServiceResult<()> {
        // Lock the domain row first so a concurrent managed-domain delete
        // (which locks the same row before its guard counts) serializes
        // against this insert; the delete transaction cannot soft-delete a
        // domain whose binding insert is in flight.
        let live_domain: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM web_domain
             WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(domain_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|error| store_error("lock domain for binding insert", error))?;
        if live_domain.is_none() {
            return Err(WebServiceError::not_found("domain not found"));
        }
        let existing_site_id: Option<i64> = sqlx::query_scalar(
            "SELECT site_id FROM web_site_binding
             WHERE tenant_id = $1 AND domain_id = $2 AND environment = 'production'
               AND path_prefix = '/' AND deleted_at IS NULL AND status <> 'ARCHIVED'
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(domain_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|error| store_error("lock domain route binding", error))?;
        if let Some(existing_site_id) = existing_site_id {
            if existing_site_id != site_id {
                return Err(WebServiceError::conflict(
                    "domain route must be unbound before binding another application",
                ));
            }
            return Err(WebServiceError::conflict(
                "domain is already bound to this application",
            ));
        }

        if is_primary {
            // Serialize primary binding creation on the site row so concurrent
            // primary bindings cannot both pass the single-primary check and
            // collide at activation time.
            let locked = sqlx::query(
                "UPDATE web_site SET version = version
                 WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
            )
            .bind(tenant_id)
            .bind(site_id)
            .execute(&mut **tx)
            .await
            .map_err(|error| store_error("lock site for primary binding", error))?;
            if locked.rows_affected() != 1 {
                return Err(WebServiceError::not_found("site not found"));
            }
            let clear_time = instant_write_expression("$3");
            let clear_sql = format!(
                "UPDATE web_site_binding SET is_primary = FALSE, updated_at = {clear_time},
                        version = version + 1
                 WHERE tenant_id = $1 AND site_id = $2 AND environment = 'production'
                   AND deleted_at IS NULL AND status <> 'ARCHIVED' AND is_primary = TRUE"
            );
            sqlx::query(audited_sql(&clear_sql))
                .bind(tenant_id)
                .bind(site_id)
                .bind(now)
                .execute(&mut **tx)
                .await
                .map_err(|error| store_error("clear primary site binding", error))?;
        }
        let binding_id = next_id(self.id_generator())?;
        let binding_uuid = new_uuid();
        let binding_time = instant_write_expression("$7");
        let binding_sql = format!(
            "INSERT INTO web_site_binding (
                id, uuid, tenant_id, site_id, domain_id, is_primary, environment,
                path_prefix, action_type, status, created_at, updated_at, version
             ) VALUES ($1, $2, $3, $4, $5, $6, 'production', '/', 'SERVE', 'PENDING',
                       {binding_time}, {binding_time}, 0)"
        );
        sqlx::query(audited_sql(&binding_sql))
            .bind(binding_id)
            .bind(binding_uuid)
            .bind(tenant_id)
            .bind(site_id)
            .bind(domain_id)
            .bind(is_primary)
            .bind(now)
            .execute(&mut **tx)
            .await
            .map_err(|error| store_error("insert site domain binding", error))?;

        if ssl_enabled {
            let policy_id = next_id(self.id_generator())?;
            let policy_uuid = new_uuid();
            let policy_time = instant_write_expression("$6");
            let policy_sql = format!(
                "INSERT INTO web_tls_policy (
                    id, uuid, tenant_id, site_binding_id, certificate_source,
                    created_at, updated_at, version
                 ) VALUES ($1, $2, $3, $4, $5, {policy_time}, {policy_time}, 0)"
            );
            let source = match ssl_provider {
                Some("custom") => "CUSTOM",
                Some("none") => "EXTERNAL",
                _ => "MANAGED",
            };
            sqlx::query(audited_sql(&policy_sql))
                .bind(policy_id)
                .bind(policy_uuid)
                .bind(tenant_id)
                .bind(binding_id)
                .bind(source)
                .bind(now)
                .execute(&mut **tx)
                .await
                .map_err(|error| store_error("insert site binding TLS policy", error))?;
        }
        Ok(())
    }
}

async fn fetch_domain_verification_challenge(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: i64,
    domain_id: i64,
    reusable_status: &str,
    now: &str,
) -> WebServiceResult<Option<EngineRow>> {
    let now_expression = instant_write_expression("$4");
    let sql = format!(
        "SELECT uuid, method, record_name, proof_sha256, status, attempt_count,
                CAST(expires_at AS TEXT) AS expires_at,
                CAST(next_attempt_at AS TEXT) AS next_attempt_at,
                CAST(checked_at AS TEXT) AS checked_at, failure_code,
                (next_attempt_at IS NULL OR next_attempt_at <= {now_expression}) AS ready_for_check
         FROM web_domain_verification
         WHERE tenant_id = $1 AND domain_id = $2
           AND (($3 = 'VERIFIED' AND status = 'VERIFIED')
                OR ($3 = 'ACTIVE' AND status IN ('PENDING', 'CHECKING')))
         ORDER BY created_at DESC, id DESC LIMIT 1"
    );
    sqlx::query(audited_sql(&sql))
        .bind(tenant_id)
        .bind(domain_id)
        .bind(reusable_status)
        .bind(now)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|error| store_error("retrieve domain verification challenge", error))
}

async fn fetch_domain_verification_challenge_for_update(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: i64,
    challenge_id: &str,
    now: &str,
) -> WebServiceResult<Option<EngineRow>> {
    let now_expression = instant_write_expression("$3");
    let sql = format!(
        "SELECT v.domain_id, d.hostname, v.uuid, v.method, v.record_name, v.proof_sha256,
                v.status, v.attempt_count, CAST(v.expires_at AS TEXT) AS expires_at,
                CAST(v.next_attempt_at AS TEXT) AS next_attempt_at,
                CAST(v.checked_at AS TEXT) AS checked_at, v.failure_code,
                (v.next_attempt_at IS NULL OR v.next_attempt_at <= {now_expression}) AS ready_for_check,
                (v.expires_at <= {now_expression}) AS is_expired
         FROM web_domain_verification v
         INNER JOIN web_domain d ON d.tenant_id = v.tenant_id AND d.id = v.domain_id
         WHERE v.tenant_id = $1 AND v.uuid = $2 AND d.deleted_at IS NULL
         FOR UPDATE OF v, d"
    );
    sqlx::query(audited_sql(&sql))
        .bind(tenant_id)
        .bind(challenge_id)
        .bind(now)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|error| store_error("lock domain verification challenge", error))
}

#[allow(clippy::too_many_arguments)]
async fn update_domain_verification_state(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: i64,
    challenge_id: &str,
    status: &str,
    observed_sha256: Option<&str>,
    attempt_count: i32,
    next_attempt_at: Option<&str>,
    failure_code: Option<&str>,
    now: &str,
) -> WebServiceResult<EngineRow> {
    let next_attempt = instant_write_expression("$6");
    let checked = instant_write_expression("$8");
    let sql = format!(
        "UPDATE web_domain_verification
         SET status = $3, observed_sha256 = $4, attempt_count = $5,
             next_attempt_at = {next_attempt}, checked_at = {checked},
             verified_at = CASE WHEN $3 = 'VERIFIED' THEN {checked} ELSE NULL END,
             failure_code = $7, updated_at = {checked}, version = version + 1
         WHERE tenant_id = $1 AND uuid = $2
         RETURNING uuid, method, record_name, proof_sha256, status, attempt_count,
                   CAST(expires_at AS TEXT) AS expires_at,
                   CAST(next_attempt_at AS TEXT) AS next_attempt_at,
                   CAST(checked_at AS TEXT) AS checked_at, failure_code,
                   FALSE AS ready_for_check"
    );
    sqlx::query(audited_sql(&sql))
        .bind(tenant_id)
        .bind(challenge_id)
        .bind(status)
        .bind(observed_sha256)
        .bind(attempt_count)
        .bind(next_attempt_at)
        .bind(failure_code)
        .bind(now)
        .fetch_one(&mut **tx)
        .await
        .map_err(|error| store_error("update domain verification observation", error))
}

async fn update_domain_verification_status(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: i64,
    domain_id: i64,
    challenge_status: &str,
    now: &str,
) -> WebServiceResult<()> {
    let domain_status = match challenge_status {
        "VERIFIED" => "VERIFIED",
        "FAILED" => "FAILED",
        "EXPIRED" => "EXPIRED",
        _ => "PENDING",
    };
    let time = instant_write_expression("$4");
    let sql = format!(
        "UPDATE web_domain
         SET verification_status = $3,
             verified_at = CASE WHEN $3 = 'VERIFIED' THEN {time} ELSE NULL END,
             status = CASE WHEN $3 = 'VERIFIED' THEN 1 ELSE 0 END,
             updated_at = {time}, version = version + 1
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL"
    );
    sqlx::query(audited_sql(&sql))
        .bind(tenant_id)
        .bind(domain_id)
        .bind(domain_status)
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error("update domain verification status", error))?;
    Ok(())
}

async fn activate_verified_domain_bindings(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: i64,
    domain_id: i64,
    now: &str,
) -> WebServiceResult<()> {
    let time = instant_write_expression("$3");
    let sql = format!(
        "UPDATE web_site_binding
         SET status = 'ACTIVE', activated_at = {time}, updated_at = {time}, version = version + 1
         WHERE tenant_id = $1 AND domain_id = $2 AND status = 'PENDING' AND deleted_at IS NULL"
    );
    sqlx::query(audited_sql(&sql))
        .bind(tenant_id)
        .bind(domain_id)
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error("activate verified domain bindings", error))?;
    Ok(())
}

fn map_domain_verification_challenge(
    row: &EngineRow,
    hostname: String,
) -> WebServiceResult<DomainVerificationChallenge> {
    Ok(DomainVerificationChallenge {
        challenge_id: row
            .try_get("uuid")
            .map_err(|error| WebServiceError::Internal(error.to_string()))?,
        hostname,
        method: row
            .try_get("method")
            .map_err(|error| WebServiceError::Internal(error.to_string()))?,
        record_name: row
            .try_get("record_name")
            .map_err(|error| WebServiceError::Internal(error.to_string()))?,
        proof_sha256: row
            .try_get("proof_sha256")
            .map_err(|error| WebServiceError::Internal(error.to_string()))?,
        status: row
            .try_get("status")
            .map_err(|error| WebServiceError::Internal(error.to_string()))?,
        attempt_count: row
            .try_get("attempt_count")
            .map_err(|error| WebServiceError::Internal(error.to_string()))?,
        expires_at: instant_from_row(row, "expires_at")
            .map_err(|error| WebServiceError::Internal(error.to_string()))?,
        next_attempt_at: optional_instant_from_row(row, "next_attempt_at")
            .map_err(|error| WebServiceError::Internal(error.to_string()))?,
        checked_at: optional_instant_from_row(row, "checked_at")
            .map_err(|error| WebServiceError::Internal(error.to_string()))?,
        failure_code: row
            .try_get("failure_code")
            .map_err(|error| WebServiceError::Internal(error.to_string()))?,
        ready_for_check: bool_from_row(row, "ready_for_check")
            .map_err(|error| WebServiceError::Internal(error.to_string()))?,
    })
}

fn verification_record_name(hostname: &str) -> String {
    format!(
        "_sdkwork-verification.{}",
        hostname.strip_prefix("*.").unwrap_or(hostname)
    )
}

fn verification_record_value(challenge_id: &str) -> String {
    format!("sdkwork-domain-verification={challenge_id}")
}

fn verification_retry_delay(challenge_id: &str, attempt_count: i32) -> Duration {
    let exponent = u32::try_from(attempt_count.saturating_sub(1).min(6)).unwrap_or(0);
    let base_seconds = 5_i64.saturating_mul(1_i64 << exponent).min(300);
    let digest = sha256_hash(format!("{challenge_id}:{attempt_count}").as_bytes());
    let jitter = i64::from_str_radix(&digest[..2], 16).unwrap_or(0) % 11;
    Duration::seconds(base_seconds + jitter)
}

fn validate_domain_verification_observation(
    observation: &DomainVerificationObservation,
) -> WebServiceResult<()> {
    if let Some(hash) = observation.observed_sha256.as_deref() {
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(WebServiceError::validation(
                "domain verification observation hash must be lowercase SHA-256",
            ));
        }
    }
    if let Some(code) = observation.failure_code.as_deref() {
        if code.is_empty()
            || code.len() > 64
            || !code
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(WebServiceError::validation(
                "domain verification failure code is invalid",
            ));
        }
    }
    Ok(())
}

fn domain_select(predicate: &str) -> String {
    format!(
        "SELECT d.uuid, d.hostname, r.uuid AS root_domain_id,
                CASE WHEN d.hostname = r.hostname THEN '@'
                     ELSE LEFT(d.hostname, LENGTH(d.hostname) - LENGTH(r.hostname) - 1)
                END AS record_name,
                s.uuid AS application_id, s.name AS application_name,
                (SELECT COUNT(*) FROM web_certificate_identifier ci
                 WHERE ci.tenant_id = d.tenant_id AND ci.domain_id = d.id) AS certificate_count,
                COALESCE(b.is_primary, FALSE) AS is_primary,
                (d.verification_status = 'VERIFIED') AS is_verified,
                (p.id IS NOT NULL) AS ssl_enabled,
                CASE p.certificate_source
                    WHEN 'MANAGED' THEN 'letsencrypt'
                    WHEN 'CUSTOM' THEN 'custom'
                    WHEN 'EXTERNAL' THEN 'none'
                    ELSE NULL
                END AS ssl_provider,
                d.status, CAST(d.created_at AS TEXT) AS created_at,
                CAST(d.updated_at AS TEXT) AS updated_at,
                latest.uuid AS latest_deployment_id,
                latest.status AS latest_deployment_status,
                latest.environment AS latest_deployment_environment,
                latest.version_tag AS latest_deployment_version_tag,
                CAST(latest.completed_at AS TEXT) AS latest_deployment_completed_at,
                CAST(latest.created_at AS TEXT) AS latest_deployment_created_at
         FROM web_domain d
         INNER JOIN web_root_domain r ON r.id = d.root_domain_id
         LEFT JOIN LATERAL (
             SELECT candidate.* FROM web_site_binding candidate
             WHERE candidate.tenant_id = d.tenant_id AND candidate.domain_id = d.id
               AND candidate.environment = 'production' AND candidate.deleted_at IS NULL
               AND candidate.status <> 'ARCHIVED'
             ORDER BY (candidate.status = 'ACTIVE') DESC, candidate.updated_at DESC, candidate.id DESC
             LIMIT 1
         ) b ON TRUE
         LEFT JOIN web_site s ON s.id = b.site_id
         LEFT JOIN web_tls_policy p ON p.tenant_id = b.tenant_id
             AND p.site_binding_id = b.id AND p.status = 'ACTIVE' AND p.deleted_at IS NULL
         LEFT JOIN web_deployment latest ON latest.id = (
             SELECT dep.id FROM web_deployment dep
             WHERE dep.tenant_id = d.tenant_id AND dep.site_id = b.site_id
             ORDER BY dep.created_at DESC, dep.id DESC LIMIT 1
         )
         WHERE {predicate}"
    )
}

fn map_domain_rows(rows: &[EngineRow]) -> WebServiceResult<Vec<DomainResponse>> {
    rows.iter()
        .map(map_domain_row)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WebServiceError::Internal(format!("map web_domain row: {error}")))
}

fn map_domain_row(row: &EngineRow) -> Result<DomainResponse, sqlx::Error> {
    let latest_deployment_id: Option<String> = row.try_get("latest_deployment_id")?;
    let latest_deployment = latest_deployment_id
        .map(|id| {
            Ok::<DomainDeploymentResponse, sqlx::Error>(DomainDeploymentResponse {
                id,
                status: row.try_get("latest_deployment_status")?,
                environment: row.try_get("latest_deployment_environment")?,
                version_tag: row.try_get("latest_deployment_version_tag")?,
                completed_at: optional_instant_from_row(row, "latest_deployment_completed_at")?,
                created_at: instant_from_row(row, "latest_deployment_created_at")?,
            })
        })
        .transpose()?;
    Ok(DomainResponse {
        id: row.try_get("uuid")?,
        hostname: row.try_get("hostname")?,
        root_domain_id: row.try_get("root_domain_id")?,
        record_name: row.try_get("record_name")?,
        application_id: row.try_get("application_id")?,
        application_name: row.try_get("application_name")?,
        certificate_count: row.try_get("certificate_count")?,
        is_primary: bool_from_row(row, "is_primary")?,
        is_verified: bool_from_row(row, "is_verified")?,
        ssl_enabled: bool_from_row(row, "ssl_enabled")?,
        ssl_provider: row.try_get("ssl_provider")?,
        status: row.try_get("status")?,
        latest_deployment,
        created_at: instant_from_row(row, "created_at")?,
        updated_at: Some(instant_from_row(row, "updated_at")?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_verification_records_and_retry_policy_are_bounded() {
        assert_eq!(
            verification_record_name("api.example.com"),
            "_sdkwork-verification.api.example.com"
        );
        assert_eq!(
            verification_record_name("*.example.com"),
            "_sdkwork-verification.example.com"
        );
        assert_eq!(
            verification_record_value("challenge-id"),
            "sdkwork-domain-verification=challenge-id"
        );
        for attempt in 1..=20 {
            let delay = verification_retry_delay("challenge-id", attempt).num_seconds();
            assert!((5..=310).contains(&delay));
        }
    }

    #[test]
    fn domain_verification_observations_fail_closed() {
        assert!(
            validate_domain_verification_observation(&DomainVerificationObservation {
                observed_sha256: Some("a".repeat(64)),
                failure_code: Some("DNS_TXT_RECORD_NOT_OBSERVED".to_string()),
            })
            .is_ok()
        );
        assert!(
            validate_domain_verification_observation(&DomainVerificationObservation {
                observed_sha256: Some("A".repeat(64)),
                failure_code: None,
            })
            .is_err()
        );
        assert!(
            validate_domain_verification_observation(&DomainVerificationObservation {
                observed_sha256: None,
                failure_code: Some("invalid-code".to_string()),
            })
            .is_err()
        );
    }
}
