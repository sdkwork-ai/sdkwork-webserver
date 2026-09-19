// Node-scoped TLS certificate material projection for the self-hosted TLS
// runtime. The control plane projects every listener certificate binding
// with a desired version across all tenants the node serves into a decrypted,
// bounded material set that the TLS material distributor writes to the node
// material root and publishes as a `tls-runtime` snapshot.

use sdkwork_webserver_contract::{
    TlsCertificateAssignmentMaterial, WebServiceError, WebServiceResult,
};
use sqlx::Row;

use super::certificate_secrets::decrypt_certificate_secret_bundle;
use super::support::store_error;
use super::WebRepository;
use crate::audited_sql;

const MAX_NODE_TLS_ASSIGNMENTS: usize = 256;

impl WebRepository {
    pub(super) async fn load_node_tls_certificate_assignments_repo(
        &self,
        node_uuid: &str,
    ) -> WebServiceResult<Vec<TlsCertificateAssignmentMaterial>> {
        if node_uuid.is_empty() || node_uuid.len() > 128 {
            return Err(WebServiceError::validation(
                "node uuid must contain 1..128 bytes",
            ));
        }
        let sql = format!(
            "WITH node_assignments AS (
                 SELECT a.tenant_id, a.runtime_set
                 FROM webserver_server s
                 INNER JOIN webserver_runtime_assignment a
                     ON a.tenant_id = s.tenant_id AND a.server_id = s.id
                     AND NOT EXISTS (
                         SELECT 1 FROM webserver_runtime_assignment newer
                         WHERE newer.tenant_id = a.tenant_id
                           AND newer.server_id = a.server_id
                           AND newer.environment = a.environment
                           AND newer.generation > a.generation
                     )
                 WHERE s.uuid = $1
             ),
             assigned_sites AS (
                 SELECT DISTINCT a.tenant_id, site.uuid AS site_uuid
                 FROM node_assignments a
                 CROSS JOIN LATERAL jsonb_array_elements(
                     CASE WHEN jsonb_typeof(a.runtime_set -> 'descriptors') = 'array'
                          THEN a.runtime_set -> 'descriptors' ELSE '[]'::jsonb END
                 ) AS descriptor(value)
                 INNER JOIN webserver_site site
                     ON site.tenant_id = a.tenant_id
                     AND site.uuid = descriptor.value ->> 'siteUuid'
                     AND site.deleted_at IS NULL
                 WHERE jsonb_typeof(descriptor.value) = 'object'
                   AND descriptor.value ->> 'siteUuid' IS NOT NULL
             )
             SELECT DISTINCT c.tenant_id, c.uuid AS certificate_uuid, c.cert_name,
                    v.uuid AS version_uuid, v.fingerprint_sha256,
                    to_char(v.not_before AT TIME ZONE 'UTC',
                            'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS not_before,
                    to_char(v.not_after AT TIME ZONE 'UTC',
                            'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS not_after,
                    v.secret_bundle_ref, sb.encryption_algorithm, sb.bundle_encrypted,
                    CAST((
                        SELECT jsonb_agg(hostname ORDER BY hostname)
                        FROM (
                            SELECT DISTINCT listener_domain.hostname
                            FROM webserver_listener_certificate_binding listener
                            INNER JOIN webserver_site_binding listener_route
                                ON listener_route.tenant_id = listener.tenant_id
                                AND listener_route.id = listener.site_binding_id
                                AND listener_route.status = 'ACTIVE'
                                AND listener_route.deleted_at IS NULL
                            INNER JOIN webserver_site listener_site
                                ON listener_site.tenant_id = listener_route.tenant_id
                                AND listener_site.id = listener_route.site_id
                                AND listener_site.deleted_at IS NULL
                            INNER JOIN webserver_domain listener_domain
                                ON listener_domain.tenant_id = listener_route.tenant_id
                                AND listener_domain.id = listener_route.domain_id
                                AND listener_domain.deleted_at IS NULL
                            INNER JOIN assigned_sites asite
                                ON asite.tenant_id = listener.tenant_id
                                AND asite.site_uuid = listener_site.uuid
                            WHERE listener.tenant_id = c.tenant_id
                              AND listener.certificate_id = c.id
                              AND listener.desired_version_id = v.id
                              AND listener.status IN ('PENDING', 'DEPLOYING', 'ACTIVE', 'FAILED')
                              AND listener.deleted_at IS NULL
                        ) hostnames
                    ) AS TEXT) AS hostnames
             FROM assigned_sites assigned
             INNER JOIN webserver_site site
                 ON site.tenant_id = assigned.tenant_id AND site.uuid = assigned.site_uuid
             INNER JOIN webserver_site_binding b
                 ON b.tenant_id = site.tenant_id AND b.site_id = site.id
                 AND b.status = 'ACTIVE' AND b.deleted_at IS NULL
             INNER JOIN webserver_listener_certificate_binding l
                 ON l.tenant_id = b.tenant_id AND l.site_binding_id = b.id
                 AND l.status IN ('PENDING', 'DEPLOYING', 'ACTIVE', 'FAILED')
                 AND l.deleted_at IS NULL
             INNER JOIN webserver_certificate c
                 ON c.tenant_id = l.tenant_id AND c.id = l.certificate_id
                 AND c.status = 1 AND c.deleted_at IS NULL
             INNER JOIN webserver_certificate_version v
                 ON v.tenant_id = l.tenant_id AND v.id = l.desired_version_id
                 AND v.certificate_id = c.id AND v.status IN ('ACTIVE', 'SUPERSEDED')
             INNER JOIN webserver_certificate_secret_bundle sb
                 ON sb.tenant_id = v.tenant_id AND sb.certificate_version_id = v.id
             ORDER BY c.tenant_id ASC, c.uuid ASC
             LIMIT {}",
            MAX_NODE_TLS_ASSIGNMENTS + 1
        );
        let rows = sqlx::query(audited_sql(&sql))
            .bind(node_uuid)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("load node TLS certificate assignments", error))?;
        if rows.len() > MAX_NODE_TLS_ASSIGNMENTS {
            return Err(WebServiceError::Internal(format!(
                "node TLS assignments exceed the maximum of {MAX_NODE_TLS_ASSIGNMENTS}"
            )));
        }
        let mut assignments = Vec::with_capacity(rows.len());
        for row in &rows {
            let tenant_id: i64 = row
                .try_get("tenant_id")
                .map_err(|error| store_error("map TLS assignment tenant", error))?;
            let version_uuid: String = row
                .try_get("version_uuid")
                .map_err(|error| store_error("map TLS assignment version", error))?;
            let secret_bundle_ref: String = row
                .try_get("secret_bundle_ref")
                .map_err(|error| store_error("map TLS assignment secret ref", error))?;
            let encryption_algorithm: String = row
                .try_get("encryption_algorithm")
                .map_err(|error| store_error("map TLS assignment encryption algorithm", error))?;
            let bundle_encrypted: String = row
                .try_get("bundle_encrypted")
                .map_err(|error| store_error("map TLS assignment encrypted bundle", error))?;
            let secret_bundle = decrypt_certificate_secret_bundle(
                self.secret_key(),
                tenant_id,
                &version_uuid,
                &secret_bundle_ref,
                &encryption_algorithm,
                &bundle_encrypted,
            )?;
            let hostnames_json: String = row
                .try_get("hostnames")
                .map_err(|error| store_error("map TLS assignment hostnames", error))?;
            let hostnames: Vec<String> = serde_json::from_str(&hostnames_json)
                .map_err(|error| WebServiceError::Internal(format!(
                    "stored TLS assignment hostnames are invalid: {error}"
                )))?;
            if hostnames.is_empty() {
                return Err(WebServiceError::Internal(
                    "TLS assignment has no listener hostnames".to_string(),
                ));
            }
            assignments.push(TlsCertificateAssignmentMaterial {
                certificate_id: row
                    .try_get("certificate_uuid")
                    .map_err(|error| store_error("map TLS assignment certificate", error))?,
                version_uuid,
                cert_name: row
                    .try_get("cert_name")
                    .map_err(|error| store_error("map TLS assignment certificate name", error))?,
                hostnames,
                fingerprint_sha256: row
                    .try_get("fingerprint_sha256")
                    .map_err(|error| store_error("map TLS assignment fingerprint", error))?,
                not_before: row
                    .try_get("not_before")
                    .map_err(|error| store_error("map TLS assignment not-before", error))?,
                not_after: row
                    .try_get("not_after")
                    .map_err(|error| store_error("map TLS assignment not-after", error))?,
                fullchain_pem: secret_bundle.fullchain_pem,
                private_key_pem: secret_bundle.private_key_pem,
            });
        }
        Ok(assignments)
    }
}
