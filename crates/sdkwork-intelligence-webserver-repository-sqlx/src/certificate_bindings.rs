use crate::audited_sql;
use sdkwork_webserver_contract::{
    CertificateIdentifierResponse, CreateListenerCertificateBindingRequest,
    ListenerCertificateBindingPage, ListenerCertificateBindingResponse,
    ListenerCertificateSummaryResponse, WebServiceError, WebServiceResult,
};
use sqlx::Row;

use super::support::{
    bool_from_row, instant_from_row, new_uuid, next_id, now_rfc3339, pagination, store_error,
};
use super::{EngineRow, WebRepository};

impl WebRepository {
    pub(super) async fn list_listener_certificate_bindings_repo(
        &self,
        tenant_id: i64,
        site_uuid: &str,
        domain_uuid: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<ListenerCertificateBindingPage> {
        let (_, page_size, offset) = pagination(page, page_size)?;
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total
             FROM web_listener_certificate_binding l
             INNER JOIN web_site_binding b ON b.tenant_id = l.tenant_id
                 AND b.id = l.site_binding_id
             INNER JOIN web_site s ON s.tenant_id = b.tenant_id AND s.id = b.site_id
             INNER JOIN web_domain d ON d.tenant_id = b.tenant_id AND d.id = b.domain_id
             WHERE l.tenant_id = $1 AND s.uuid = $2 AND d.uuid = $3
               AND l.deleted_at IS NULL AND l.status <> 'ARCHIVED'",
        )
        .bind(tenant_id)
        .bind(site_uuid)
        .bind(domain_uuid)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count listener certificate bindings", error))?;
        let total: i64 = count_row
            .try_get("total")
            .map_err(|error| store_error("map listener certificate binding count", error))?;
        let rows = sqlx::query(audited_sql(&listener_binding_select(
            "l.tenant_id = $1 AND s.uuid = $2 AND d.uuid = $3
             AND l.deleted_at IS NULL AND l.status <> 'ARCHIVED'
             ORDER BY l.is_default DESC, l.priority ASC, l.id ASC
             LIMIT $4 OFFSET $5",
        )))
        .bind(tenant_id)
        .bind(site_uuid)
        .bind(domain_uuid)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list listener certificate bindings", error))?;
        let items = rows
            .iter()
            .map(map_listener_binding_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                WebServiceError::Internal(format!("map listener certificate binding: {error}"))
            })?;
        Ok(ListenerCertificateBindingPage {
            total,
            items,
        })
    }

    pub(super) async fn bind_listener_certificate_repo(
        &self,
        tenant_id: i64,
        site_uuid: &str,
        domain_uuid: &str,
        request: &CreateListenerCertificateBindingRequest,
    ) -> WebServiceResult<ListenerCertificateBindingResponse> {
        if !(0..=10_000).contains(&request.priority) {
            return Err(WebServiceError::validation(
                "priority must be between 0 and 10000",
            ));
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin listener certificate binding", error))?;
        let route = sqlx::query(
            "SELECT b.id AS site_binding_id, d.id AS domain_id
             FROM web_site_binding b
             INNER JOIN web_site s ON s.tenant_id = b.tenant_id AND s.id = b.site_id
             INNER JOIN web_domain d ON d.tenant_id = b.tenant_id AND d.id = b.domain_id
             WHERE b.tenant_id = $1 AND s.uuid = $2 AND d.uuid = $3
               AND b.environment = 'production' AND b.status = 'ACTIVE'
               AND b.deleted_at IS NULL AND s.deleted_at IS NULL AND d.deleted_at IS NULL
               AND d.verification_status = 'VERIFIED'
             FOR UPDATE OF b",
        )
        .bind(tenant_id)
        .bind(site_uuid)
        .bind(domain_uuid)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| store_error("resolve active listener route", error))?
        .ok_or_else(|| WebServiceError::validation("active verified domain binding not found"))?;
        let site_binding_id: i64 = route
            .try_get("site_binding_id")
            .map_err(|error| store_error("map listener route id", error))?;
        let domain_id: i64 = route
            .try_get("domain_id")
            .map_err(|error| store_error("map listener domain id", error))?;

        let version = sqlx::query(
            "SELECT c.id AS certificate_id, v.id AS certificate_version_id,
                    v.uuid AS certificate_version_uuid, v.key_algorithm
             FROM web_certificate c
             INNER JOIN web_certificate_identifier ci ON ci.tenant_id = c.tenant_id
                 AND ci.certificate_id = c.id AND ci.domain_id = $3
             INNER JOIN web_certificate_version v ON v.tenant_id = c.tenant_id
                 AND v.certificate_id = c.id
                 AND (($4 IS NULL AND v.id = c.current_version_id)
                      OR ($4 IS NOT NULL AND v.uuid = $4))
             WHERE c.tenant_id = $1 AND c.uuid = $2 AND c.status = 1
               AND c.deleted_at IS NULL
               AND (($4 IS NULL AND v.status = 'ACTIVE')
                    OR ($4 IS NOT NULL AND v.status IN ('ACTIVE', 'SUPERSEDED')))
               AND v.not_after > NOW()",
        )
        .bind(tenant_id)
        .bind(&request.certificate_id)
        .bind(domain_id)
        .bind(request.certificate_version_id.as_deref())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| store_error("resolve listener certificate version", error))?
        .ok_or_else(|| {
            WebServiceError::validation(
                "certificate does not cover the domain or has no deployable version",
            )
        })?;
        let certificate_id: i64 = version
            .try_get("certificate_id")
            .map_err(|error| store_error("map listener certificate id", error))?;
        let certificate_version_id: i64 = version
            .try_get("certificate_version_id")
            .map_err(|error| store_error("map listener certificate version id", error))?;
        let key_algorithm: String = version
            .try_get("key_algorithm")
            .map_err(|error| store_error("map listener certificate key algorithm", error))?;

        let algorithm_occupied = sqlx::query(
            "SELECT 1
             FROM web_listener_certificate_binding
             WHERE tenant_id = $1 AND site_binding_id = $2 AND key_algorithm = $3
               AND certificate_id <> $4 AND status <> 'ARCHIVED' AND deleted_at IS NULL
             LIMIT 1",
        )
        .bind(tenant_id)
        .bind(site_binding_id)
        .bind(&key_algorithm)
        .bind(certificate_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| store_error("check listener certificate key algorithm", error))?
        .is_some();
        if algorithm_occupied {
            return Err(WebServiceError::conflict(format!(
                "listener already has a certificate binding for key algorithm {key_algorithm}"
            )));
        }

        let now = now_rfc3339();
        if request.is_default {
            sqlx::query(
                "UPDATE web_listener_certificate_binding
                 SET is_default = FALSE, updated_at = CAST($3 AS TIMESTAMPTZ),
                     version = version + 1
                 WHERE tenant_id = $1 AND site_binding_id = $2 AND is_default = TRUE
                   AND status <> 'ARCHIVED' AND deleted_at IS NULL",
            )
            .bind(tenant_id)
            .bind(site_binding_id)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("clear listener default certificate", error))?;
        }
        let binding_uuid = new_uuid();
        let binding_id = next_id(self.id_generator())?;
        let row = sqlx::query(
            "INSERT INTO web_listener_certificate_binding (
                id, uuid, tenant_id, site_binding_id, certificate_id,
                desired_version_id, current_version_id, key_algorithm,
                priority, is_default, status,
                activated_at, created_at, updated_at, version, deleted_at
             ) VALUES (
                $1, $2, $3, $4, $5, $6, NULL, $7, $8, $9, 'PENDING',
                NULL, CAST($10 AS TIMESTAMPTZ),
                CAST($10 AS TIMESTAMPTZ), 0, NULL
             )
             ON CONFLICT ON CONSTRAINT uk_web_listener_certificate_binding_certificate
             DO UPDATE SET desired_version_id = EXCLUDED.desired_version_id,
                 current_version_id = CASE
                     WHEN web_listener_certificate_binding.status = 'ARCHIVED'
                          OR web_listener_certificate_binding.deleted_at IS NOT NULL
                         THEN NULL
                     ELSE web_listener_certificate_binding.current_version_id
                 END,
                 key_algorithm = EXCLUDED.key_algorithm, priority = EXCLUDED.priority,
                 is_default = EXCLUDED.is_default,
                 status = CASE
                     WHEN web_listener_certificate_binding.status = 'ARCHIVED'
                          OR web_listener_certificate_binding.deleted_at IS NOT NULL
                         THEN 'PENDING'
                     WHEN web_listener_certificate_binding.desired_version_id = EXCLUDED.desired_version_id
                         THEN web_listener_certificate_binding.status
                     WHEN web_listener_certificate_binding.status = 'PAUSED'
                         THEN 'PAUSED'
                     WHEN web_listener_certificate_binding.current_version_id = EXCLUDED.desired_version_id
                         THEN 'ACTIVE'
                     ELSE 'PENDING'
                 END,
                 activated_at = CASE
                     WHEN web_listener_certificate_binding.status = 'ARCHIVED'
                          OR web_listener_certificate_binding.deleted_at IS NOT NULL
                         THEN NULL
                     ELSE web_listener_certificate_binding.activated_at
                 END,
                 updated_at = EXCLUDED.updated_at,
                 deleted_at = NULL,
                 version = web_listener_certificate_binding.version + 1
             RETURNING uuid, status",
        )
        .bind(binding_id)
        .bind(&binding_uuid)
        .bind(tenant_id)
        .bind(site_binding_id)
        .bind(certificate_id)
        .bind(certificate_version_id)
        .bind(&key_algorithm)
        .bind(request.priority)
        .bind(request.is_default)
        .bind(&now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| store_error("bind listener certificate", error))?;
        let binding_uuid: String = row
            .try_get("uuid")
            .map_err(|error| store_error("map listener certificate binding id", error))?;
        let binding_status: String = row
            .try_get("status")
            .map_err(|error| store_error("map listener certificate rollout status", error))?;
        if binding_status != "ACTIVE" {
            sqlx::query(
                "DELETE FROM web_certificate_node_state
                 WHERE tenant_id = $1 AND certificate_id = $2
                   AND certificate_version_id = $3",
            )
            .bind(tenant_id)
            .bind(certificate_id)
            .bind(certificate_version_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("invalidate listener certificate observations", error))?;
        }
        tx.commit()
            .await
            .map_err(|error| store_error("commit listener certificate binding", error))?;
        self.retrieve_listener_certificate_binding(tenant_id, &binding_uuid)
            .await
    }

    pub(super) async fn unbind_listener_certificate_repo(
        &self,
        tenant_id: i64,
        site_uuid: &str,
        domain_uuid: &str,
        binding_uuid: &str,
    ) -> WebServiceResult<()> {
        let now = now_rfc3339();
        let result = sqlx::query(
            "UPDATE web_listener_certificate_binding l
             SET status = 'ARCHIVED', is_default = FALSE,
                 deleted_at = CAST($5 AS TIMESTAMPTZ), updated_at = CAST($5 AS TIMESTAMPTZ),
                 version = l.version + 1
             FROM web_site_binding b
             INNER JOIN web_site s ON s.tenant_id = b.tenant_id AND s.id = b.site_id
             INNER JOIN web_domain d ON d.tenant_id = b.tenant_id AND d.id = b.domain_id
             WHERE l.tenant_id = $1 AND l.uuid = $2 AND s.uuid = $3 AND d.uuid = $4
               AND l.site_binding_id = b.id AND l.deleted_at IS NULL
               AND l.status <> 'ARCHIVED'",
        )
        .bind(tenant_id)
        .bind(binding_uuid)
        .bind(site_uuid)
        .bind(domain_uuid)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|error| store_error("unbind listener certificate", error))?;
        if result.rows_affected() == 0 {
            return Err(WebServiceError::not_found(
                "listener certificate binding not found",
            ));
        }
        Ok(())
    }

    async fn retrieve_listener_certificate_binding(
        &self,
        tenant_id: i64,
        binding_uuid: &str,
    ) -> WebServiceResult<ListenerCertificateBindingResponse> {
        let row = sqlx::query(audited_sql(&listener_binding_select(
            "l.tenant_id = $1 AND l.uuid = $2 AND l.deleted_at IS NULL",
        )))
        .bind(tenant_id)
        .bind(binding_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve listener certificate binding", error))?
        .ok_or_else(|| {
            WebServiceError::not_found("listener certificate binding not found")
        })?;
        map_listener_binding_row(&row).map_err(|error| {
            WebServiceError::Internal(format!("map listener certificate binding: {error}"))
        })
    }
}

fn listener_binding_select(predicate: &str) -> String {
    format!(
        "SELECT l.uuid, s.uuid AS site_id, d.uuid AS domain_id,
                c.uuid AS certificate_id,
                desired.uuid AS desired_certificate_version_id,
                current.uuid AS current_certificate_version_id,
                c.cert_name AS certificate_name,
                desired.issuer AS desired_certificate_issuer,
                desired.fingerprint_sha256 AS desired_certificate_fingerprint,
                CAST(desired.not_after AS TEXT) AS desired_certificate_not_after,
                current.issuer AS current_certificate_issuer,
                current.fingerprint_sha256 AS current_certificate_fingerprint,
                CAST(current.not_after AS TEXT) AS current_certificate_not_after,
                c.status AS certificate_status,
                c.renewal_status AS certificate_renewal_status,
                CAST(COALESCE((
                    SELECT jsonb_agg(jsonb_build_object(
                        'domainId', identifier_domain.uuid,
                        'hostname', ci.hostname,
                        'identifierType', ci.identifier_type,
                        'position', ci.position
                    ) ORDER BY ci.position)
                    FROM web_certificate_identifier ci
                    INNER JOIN web_domain identifier_domain
                        ON identifier_domain.tenant_id = ci.tenant_id
                        AND identifier_domain.id = ci.domain_id
                    WHERE ci.tenant_id = c.tenant_id AND ci.certificate_id = c.id
                ), '[]'::jsonb) AS TEXT) AS certificate_identifiers,
                l.key_algorithm, l.priority, l.is_default, l.status,
                CAST(l.activated_at AS TEXT) AS activated_at,
                CAST(l.created_at AS TEXT) AS created_at,
                CAST(l.updated_at AS TEXT) AS updated_at
         FROM web_listener_certificate_binding l
         INNER JOIN web_site_binding b ON b.tenant_id = l.tenant_id
             AND b.id = l.site_binding_id
         INNER JOIN web_site s ON s.tenant_id = b.tenant_id AND s.id = b.site_id
         INNER JOIN web_domain d ON d.tenant_id = b.tenant_id AND d.id = b.domain_id
         INNER JOIN web_certificate c ON c.tenant_id = l.tenant_id
             AND c.id = l.certificate_id
         INNER JOIN web_certificate_version desired ON desired.tenant_id = l.tenant_id
             AND desired.id = l.desired_version_id AND desired.certificate_id = c.id
         LEFT JOIN web_certificate_version current ON current.tenant_id = l.tenant_id
             AND current.id = l.current_version_id AND current.certificate_id = c.id
         WHERE {predicate}"
    )
}

fn map_listener_binding_row(
    row: &EngineRow,
) -> Result<ListenerCertificateBindingResponse, sqlx::Error> {
    let identifiers_json: String = row.try_get("certificate_identifiers")?;
    let identifiers = serde_json::from_str::<Vec<CertificateIdentifierResponse>>(&identifiers_json)
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
    let certificate_name: String = row.try_get("certificate_name")?;
    let certificate_status: i32 = row.try_get("certificate_status")?;
    let certificate_renewal_status: i32 = row.try_get("certificate_renewal_status")?;
    let desired_not_after =
        super::support::optional_instant_from_row(row, "desired_certificate_not_after")?;
    let current_not_after =
        super::support::optional_instant_from_row(row, "current_certificate_not_after")?;
    let current_certificate_version_id: Option<String> =
        row.try_get("current_certificate_version_id")?;
    let current_certificate = if current_certificate_version_id.is_some() {
        Some(ListenerCertificateSummaryResponse {
            cert_name: certificate_name.clone(),
            identifiers: identifiers.clone(),
            issuer: row.try_get("current_certificate_issuer")?,
            fingerprint: row.try_get("current_certificate_fingerprint")?,
            not_after: current_not_after.clone(),
            status: super::certificates::certificate_asset_status(
                certificate_status,
                certificate_renewal_status,
                current_not_after.as_deref(),
            ),
        })
    } else {
        None
    };
    Ok(ListenerCertificateBindingResponse {
        id: row.try_get("uuid")?,
        application_id: row.try_get("site_id")?,
        domain_id: row.try_get("domain_id")?,
        certificate_id: row.try_get("certificate_id")?,
        desired_certificate_version_id: row.try_get("desired_certificate_version_id")?,
        current_certificate_version_id,
        desired_certificate: ListenerCertificateSummaryResponse {
            cert_name: certificate_name,
            identifiers,
            issuer: row.try_get("desired_certificate_issuer")?,
            fingerprint: row.try_get("desired_certificate_fingerprint")?,
            not_after: desired_not_after.clone(),
            status: super::certificates::certificate_asset_status(
                certificate_status,
                certificate_renewal_status,
                desired_not_after.as_deref(),
            ),
        },
        current_certificate,
        key_algorithm: row.try_get("key_algorithm")?,
        priority: row.try_get("priority")?,
        is_default: bool_from_row(row, "is_default")?,
        status: row.try_get("status")?,
        activated_at: super::support::optional_instant_from_row(row, "activated_at")?,
        created_at: instant_from_row(row, "created_at")?,
        updated_at: instant_from_row(row, "updated_at")?,
    })
}
