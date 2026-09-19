use crate::audited_sql;
use futures_util::TryStreamExt;
use sdkwork_utils_rust::crypto::sha256_hash;
use sdkwork_webserver_contract::{
    AgentCertificateBundle, AgentHeartbeatRequest, AgentHeartbeatResponse, AgentNginxConfigBundle,
    AgentSyncResponse, CertificateDistributionPage, CertificateDistributionResponse,
    WebServiceError, WebServiceResult,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use super::{EngineDatabase, EngineRow, WebRepository};
use sqlx::Row;

use super::support::{
    instant_write_expression, json_from_row, new_agent_token, now_rfc3339,
    pagination, sha256_hex, store_error,
};
use super::certificate_secrets::decrypt_certificate_secret_bundle;

const MAX_NODE_SYNC_ITEMS: usize = 2_048;
const MAX_NODE_SYNC_BUNDLE_BYTES: usize = 12 * 1024 * 1024;
const MAX_NODE_NGINX_CONFIG_BYTES: i64 = 1024 * 1024;

struct NodeSyncBudget {
    items: usize,
    serialized_bytes: usize,
    maximum_items: usize,
    maximum_serialized_bytes: usize,
}

impl NodeSyncBudget {
    fn new() -> Self {
        Self {
            items: 0,
            serialized_bytes: 0,
            maximum_items: MAX_NODE_SYNC_ITEMS,
            maximum_serialized_bytes: MAX_NODE_SYNC_BUNDLE_BYTES,
        }
    }

    fn reserve<T: serde::Serialize>(&mut self, item: &T) -> WebServiceResult<()> {
        self.reserve_with_additional_bytes(item, 0)
    }

    fn reserve_with_additional_bytes<T: serde::Serialize>(
        &mut self,
        item: &T,
        additional_bytes: usize,
    ) -> WebServiceResult<()> {
        if self.items >= self.maximum_items {
            return Err(WebServiceError::Internal(format!(
                "node sync manifest exceeds {} items",
                self.maximum_items
            )));
        }
        let item_bytes = serde_json::to_vec(item)
            .map_err(|error| WebServiceError::Internal(format!("encode node sync item: {error}")))?
            .len()
            .checked_add(additional_bytes)
            .ok_or_else(|| WebServiceError::Internal("node sync item byte overflow".to_string()))?;
        let serialized_bytes = self
            .serialized_bytes
            .checked_add(item_bytes)
            .ok_or_else(|| {
                WebServiceError::Internal("node sync byte budget overflow".to_string())
            })?;
        if serialized_bytes > self.maximum_serialized_bytes {
            return Err(WebServiceError::Internal(format!(
                "node sync manifest exceeds {} serialized bundle bytes",
                self.maximum_serialized_bytes
            )));
        }
        self.items += 1;
        self.serialized_bytes = serialized_bytes;
        Ok(())
    }

    #[cfg(test)]
    fn with_limits(maximum_items: usize, maximum_serialized_bytes: usize) -> Self {
        Self {
            items: 0,
            serialized_bytes: 0,
            maximum_items,
            maximum_serialized_bytes,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AuthenticatedAgent {
    pub server_uuid: String,
    pub tenant_id: i64,
}

/// Lightweight desired-state projection for the heartbeat path: the sync
/// version plus the `(certificate_id, fingerprint)` pairs observations must
/// match. Carries no decrypted key material.
#[derive(Clone, Debug)]
pub(crate) struct AgentSyncFingerprint {
    pub sync_version: String,
    pub certificate_fingerprints: HashSet<(String, String)>,
}

pub(crate) fn hash_agent_token(token: &str) -> String {
    sha256_hash(token.as_bytes())
}

pub(crate) fn generate_agent_token() -> String {
    new_agent_token()
}

impl WebRepository {
    pub(super) async fn authenticate_agent_token_repo(
        &self,
        token: &str,
    ) -> WebServiceResult<AuthenticatedAgent> {
        let token_hash = hash_agent_token(token);
        // Containment (not `->>` extraction) so the metadata GIN index serves
        // the lookup; an extraction predicate would seq-scan the table on
        // every node authentication/heartbeat.
        let sql = "SELECT uuid, tenant_id, name, host
                   FROM webserver_server
                   WHERE metadata @> CAST($1 AS JSONB)";
        let credential = json!({ "agentTokenHash": token_hash }).to_string();
        let row = sqlx::query(sql)
            .bind(credential)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| store_error("authenticate webserver_server agent token", error))?;

        let row = row.ok_or(WebServiceError::Forbidden)?;
        map_authenticated_agent(&row)
            .map_err(|error| WebServiceError::Internal(format!("map authenticated agent: {error}")))
    }

    pub(super) async fn record_agent_heartbeat_repo(
        &self,
        agent: &AuthenticatedAgent,
        request: &AgentHeartbeatRequest,
    ) -> WebServiceResult<AgentHeartbeatResponse> {
        // The heartbeat only needs the desired-state fingerprint (ids +
        // fingerprints + hostnames) to validate and record observations; the
        // full manifest would decrypt every assigned certificate's PEM
        // bundle per heartbeat for data the comparison never reads.
        let desired_fingerprint = if !request.certificate_observations.is_empty() {
            Some(self.build_agent_sync_fingerprint_repo(agent).await?)
        } else {
            None
        };
        let now = now_rfc3339();
        let metadata_patch = json!({
            "lastHeartbeatAt": now,
            "agentVersion": request.agent_version,
            "nginxEnabled": request.nginx_enabled,
            "activeConfigs": request.active_configs,
            "lastAppliedSyncVersion": request.last_sync_version,
        });

        // Atomic JSONB merge so concurrent heartbeats never lose fields.

        let now_expression = instant_write_expression("$3");
        let update_sql = format!(
            "UPDATE webserver_server
             SET status = 1, metadata = metadata || CAST($2 AS JSONB),
                 updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND uuid = $4"
        );

        sqlx::query(audited_sql(&update_sql))
            .bind(agent.tenant_id)
            .bind(metadata_patch.to_string())
            .bind(&now)
            .bind(&agent.server_uuid)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("record webserver_server heartbeat", error))?;

        if !request.certificate_observations.is_empty() {
            let recorded = self
                .record_certificate_observations(
                agent,
                &request.certificate_observations,
                desired_fingerprint.as_ref().ok_or_else(|| {
                    WebServiceError::Internal(
                        "desired node fingerprint missing for certificate observations"
                            .to_string(),
                    )
                })?,
                )
                .await?;
            if recorded {
                self.promote_converged_listener_certificate_bindings(agent.tenant_id)
                    .await?;
            }
        }

        Ok(AgentHeartbeatResponse {
            server_id: agent.server_uuid.clone(),
            status: 1,
            acknowledged_at: now,
        })
    }

    pub(super) async fn build_agent_sync_manifest_repo(
        &self,
        agent: &AuthenticatedAgent,
        if_sync_version: Option<&str>,
    ) -> WebServiceResult<AgentSyncResponse> {
        let mut budget = NodeSyncBudget::new();
        let assigned_site_uuids = self
            .load_current_assigned_site_uuids(agent)
            .await?
            .ok_or_else(|| WebServiceError::conflict("runtime assignment not found for node"))?;
        let nginx_configs = self
            .load_active_nginx_configs_for_sites(
                agent.tenant_id,
                &assigned_site_uuids,
                &mut budget,
            )
            .await?;
        let certificates = self
            .load_active_certificates_for_sites(
                agent.tenant_id,
                &assigned_site_uuids,
                &mut budget,
            )
            .await?;
        let sync_version = compute_agent_sync_version(&nginx_configs, &certificates);

        if if_sync_version.is_some_and(|value| value == sync_version) {
            return Ok(AgentSyncResponse {
                server_id: agent.server_uuid.clone(),
                sync_version,
                unchanged: true,
                nginx_configs: Vec::new(),
                certificates: Vec::new(),
            });
        }

        Ok(AgentSyncResponse {
            server_id: agent.server_uuid.clone(),
            sync_version,
            unchanged: false,
            nginx_configs,
            certificates,
        })
    }

    /// Desired-state fingerprint for heartbeat-validated certificate
    /// observations. Produces the same `sync_version` as the full manifest
    /// without decrypting any certificate secret bundle (the version is
    /// computed from ids, fingerprints, and hostnames only).
    pub(super) async fn build_agent_sync_fingerprint_repo(
        &self,
        agent: &AuthenticatedAgent,
    ) -> WebServiceResult<AgentSyncFingerprint> {
        let mut budget = NodeSyncBudget::new();
        let assigned_site_uuids = self
            .load_current_assigned_site_uuids(agent)
            .await?
            .ok_or_else(|| WebServiceError::conflict("runtime assignment not found for node"))?;
        let nginx_configs = self
            .load_active_nginx_configs_for_sites(
                agent.tenant_id,
                &assigned_site_uuids,
                &mut budget,
            )
            .await?;
        let certificate_fingerprints = self
            .load_active_certificate_fingerprints_for_sites(
                agent.tenant_id,
                &assigned_site_uuids,
                &mut budget,
            )
            .await?;
        let mut parts = Vec::with_capacity(nginx_configs.len() + certificate_fingerprints.len());
        parts.extend(nginx_configs.iter().map(agent_nginx_sync_part));
        for (certificate_id, fingerprint, hostnames) in &certificate_fingerprints {
            parts.push(agent_certificate_sync_part(
                certificate_id,
                fingerprint,
                hostnames,
            ));
        }
        Ok(AgentSyncFingerprint {
            sync_version: compute_agent_sync_version_from_parts(parts),
            certificate_fingerprints: certificate_fingerprints
                .into_iter()
                .map(|(certificate_id, fingerprint, _)| (certificate_id, fingerprint))
                .collect(),
        })
    }

    /// Fingerprint-only projection of the active node certificate set: the
    /// same rows as [`Self::load_active_certificates_for_sites`] minus the
    /// secret-bundle join and decryption.
    async fn load_active_certificate_fingerprints_for_sites(
        &self,
        tenant_id: i64,
        site_uuids: &[String],
        budget: &mut NodeSyncBudget,
    ) -> WebServiceResult<Vec<(String, String, Vec<String>)>> {
        let sql = format!(
            "SELECT DISTINCT c.uuid, v.fingerprint_sha256 AS fingerprint,
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
                            WHERE listener.tenant_id = l.tenant_id
                              AND listener.certificate_id = c.id
                              AND listener.desired_version_id = v.id
                              AND listener.status IN ('PENDING', 'DEPLOYING', 'ACTIVE', 'FAILED')
                              AND listener.deleted_at IS NULL
                              AND listener_site.uuid = ANY($2)
                        ) verification_hostnames
                    ) AS TEXT) AS verification_hostnames
             FROM webserver_listener_certificate_binding l
             INNER JOIN webserver_site_binding b ON b.tenant_id = l.tenant_id
                 AND b.id = l.site_binding_id AND b.status = 'ACTIVE'
                 AND b.deleted_at IS NULL
             INNER JOIN webserver_site s ON s.tenant_id = b.tenant_id AND s.id = b.site_id
                 AND s.deleted_at IS NULL
             INNER JOIN webserver_certificate c ON c.tenant_id = l.tenant_id
                 AND c.id = l.certificate_id AND c.status = 1 AND c.deleted_at IS NULL
             INNER JOIN webserver_certificate_version v ON v.tenant_id = l.tenant_id
                 AND v.id = l.desired_version_id AND v.certificate_id = c.id
                  AND v.status IN ('ACTIVE', 'SUPERSEDED')
             WHERE l.tenant_id = $1 AND s.uuid = ANY($2)
               AND l.status IN ('PENDING', 'DEPLOYING', 'ACTIVE', 'FAILED')
               AND l.deleted_at IS NULL
             ORDER BY c.uuid ASC
             LIMIT {}",
            MAX_NODE_SYNC_ITEMS + 1
        );
        let mut rows = sqlx::query(audited_sql(&sql))
            .bind(tenant_id)
            .bind(site_uuids)
            .fetch(&self.pool);

        let mut items = Vec::new();
        while let Some(row) = rows
            .try_next()
            .await
            .map_err(|error| store_error("stream active certificate fingerprints", error))?
        {
            let certificate_id: String = row.try_get("uuid").map_err(|error| {
                WebServiceError::Internal(format!("agent sync certificate id: {error}"))
            })?;
            let fingerprint: String = row
                .try_get("fingerprint")
                .map_err(|error| store_error("agent sync certificate fingerprint", error))?;
            let verification_hostnames_json: String = row
                .try_get("verification_hostnames")
                .map_err(|error| store_error("agent sync hostnames", error))?;
            let hostnames = serde_json::from_str::<Vec<String>>(&verification_hostnames_json)
                .map_err(|error| {
                    WebServiceError::Internal(format!(
                        "decode agent sync certificate verification hostnames: {error}"
                    ))
                })?;
            if hostnames.is_empty() || hostnames.len() > 128 {
                return Err(WebServiceError::Internal(
                    "agent sync certificate must target between 1 and 128 listener hostnames"
                        .to_string(),
                ));
            }
            budget.reserve(&serde_json::json!({
                "certificate": certificate_id,
                "fingerprint": fingerprint,
            }))?;
            items.push((certificate_id, fingerprint, hostnames));
        }
        Ok(items)
    }

    pub(super) async fn list_certificate_distribution_repo(
        &self,
        tenant_id: i64,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<CertificateDistributionPage> {
        let (page, page_size, offset) = pagination(page, page_size)?;
        let mut transaction = self.pool.begin().await.map_err(|error| {
            store_error(
                "begin certificate distribution read transaction",
                error,
            )
        })?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                store_error(
                    "configure certificate distribution read transaction",
                    error,
                )
            })?;
        let count_row = sqlx::query("SELECT COUNT(*) AS total FROM webserver_server WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| store_error("count webserver_server certificate distribution", error))?;
        let total: i64 = count_row
            .try_get("total")
            .map_err(|error| store_error("map webserver_server sync count", error))?;
        let rows = sqlx::query(
            "SELECT id, uuid, name, host, status, CAST(metadata AS TEXT) AS metadata
             FROM webserver_server
             WHERE tenant_id = $1
             ORDER BY updated_at DESC, id DESC LIMIT $2 OFFSET $3",
        )
        .bind(tenant_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| store_error("list webserver_server certificate distribution", error))?;

        let mut server_ids = Vec::with_capacity(rows.len());
        let mut server_uuids = BTreeMap::new();
        for row in &rows {
            let server_id: i64 = row.try_get("id").map_err(|error| {
                WebServiceError::Internal(format!(
                    "certificate distribution server database id: {error}"
                ))
            })?;
            let server_uuid: String = row.try_get("uuid").map_err(|error| {
                WebServiceError::Internal(format!(
                    "certificate distribution server uuid: {error}"
                ))
            })?;
            server_ids.push(server_id);
            server_uuids.insert(server_id, server_uuid);
        }
        let desired_sync_versions = self
            .load_desired_sync_versions_for_servers(
                &mut transaction,
                tenant_id,
                &server_ids,
                &server_uuids,
            )
            .await?;

        let mut items = Vec::with_capacity(page_size as usize);
        for row in rows {
            let server_id: i64 = row.try_get("id").map_err(|error| {
                WebServiceError::Internal(format!(
                    "certificate distribution server database id: {error}"
                ))
            })?;
            let server_uuid: String = row.try_get("uuid").map_err(|error| {
                WebServiceError::Internal(format!(
                    "certificate distribution server uuid: {error}"
                ))
            })?;
            let desired_sync_version = desired_sync_versions.get(&server_id);
            let assigned = desired_sync_version.is_some();
            let desired_sync_version = desired_sync_version.cloned().unwrap_or_default();
            let metadata = json_from_row(&row, "metadata")
                .map_err(|error| {
                    WebServiceError::Internal(format!(
                        "certificate distribution server metadata: {error}"
                    ))
                })?
                .unwrap_or_else(|| json!({}));
            let applied_sync_version = metadata
                .get("lastAppliedSyncVersion")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let last_heartbeat_at = metadata
                .get("lastHeartbeatAt")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let server_status: i32 = row.try_get("status").map_err(|error| {
                WebServiceError::Internal(format!("certificate distribution server status: {error}"))
            })?;
            let status = if server_status == 0 {
                "OFFLINE"
            } else if !assigned {
                "UNASSIGNED"
            } else if applied_sync_version.as_deref() == Some(desired_sync_version.as_str()) {
                "SYNCED"
            } else {
                "PENDING"
            };
            items.push(CertificateDistributionResponse {
                server_id: server_uuid,
                server_name: row.try_get("name").map_err(|error| {
                    WebServiceError::Internal(format!("certificate distribution server name: {error}"))
                })?,
                host: row.try_get("host").map_err(|error| {
                    WebServiceError::Internal(format!("certificate distribution server host: {error}"))
                })?,
                desired_sync_version: desired_sync_version.clone(),
                applied_sync_version,
                status: status.to_string(),
                last_heartbeat_at,
            });
        }
        transaction.commit().await.map_err(|error| {
            store_error("commit certificate distribution read transaction", error)
        })?;

        Ok(CertificateDistributionPage {
            items,
            total,
            page,
            page_size,
        })
    }

    async fn load_desired_sync_versions_for_servers(
        &self,
        transaction: &mut sqlx::Transaction<'_, EngineDatabase>,
        tenant_id: i64,
        server_ids: &[i64],
        server_uuids: &BTreeMap<i64, String>,
    ) -> WebServiceResult<BTreeMap<i64, String>> {
        if server_ids.is_empty() {
            return Ok(BTreeMap::new());
        }

        let assignment_rows = sqlx::query(
            "WITH current_assignments AS (
                SELECT DISTINCT ON (a.server_id, a.environment)
                       a.server_id, a.environment, a.runtime_set
                FROM webserver_runtime_assignment a
                WHERE a.tenant_id = $1 AND a.server_id = ANY($2)
                ORDER BY a.server_id, a.environment, a.generation DESC
             )
             SELECT server_id, runtime_set ->> 'nodeUuid' AS node_uuid,
                    jsonb_typeof(runtime_set -> 'descriptors') AS descriptors_kind,
                    CASE WHEN jsonb_typeof(runtime_set -> 'descriptors') = 'array'
                         THEN jsonb_array_length(runtime_set -> 'descriptors')
                         ELSE NULL END AS descriptor_count
             FROM current_assignments
             ORDER BY server_id, environment",
        )
        .bind(tenant_id)
        .bind(server_ids)
        .fetch_all(&mut **transaction)
        .await
        .map_err(|error| {
            store_error(
                "validate current certificate distribution assignments",
                error,
            )
        })?;

        let mut assignment_counts = BTreeMap::<i64, usize>::new();
        let mut descriptor_counts = BTreeMap::<i64, usize>::new();
        for row in assignment_rows {
            let server_id: i64 = row.try_get("server_id").map_err(|error| {
                WebServiceError::Internal(format!(
                    "certificate distribution assignment server id: {error}"
                ))
            })?;
            let expected_uuid = server_uuids.get(&server_id).ok_or_else(|| {
                WebServiceError::Internal(
                    "certificate distribution assignment escaped the requested server scope"
                        .to_string(),
                )
            })?;
            let node_uuid: Option<String> = row.try_get("node_uuid").map_err(|error| {
                WebServiceError::Internal(format!(
                    "certificate distribution assignment node identity: {error}"
                ))
            })?;
            if node_uuid.as_deref() != Some(expected_uuid.as_str()) {
                return Err(WebServiceError::Internal(
                    "stored node runtime assignment has an invalid node scope".to_string(),
                ));
            }
            let descriptors_kind: Option<String> =
                row.try_get("descriptors_kind").map_err(|error| {
                    WebServiceError::Internal(format!(
                        "certificate distribution assignment descriptor type: {error}"
                    ))
                })?;
            if descriptors_kind.as_deref() != Some("array") {
                return Err(WebServiceError::Internal(
                    "stored node runtime assignment has no descriptors".to_string(),
                ));
            }
            let descriptor_count: i32 = row.try_get("descriptor_count").map_err(|error| {
                WebServiceError::Internal(format!(
                    "certificate distribution assignment descriptor count: {error}"
                ))
            })?;
            let descriptor_count = usize::try_from(descriptor_count).map_err(|_| {
                WebServiceError::Internal(
                    "stored node runtime assignment has an invalid descriptor count".to_string(),
                )
            })?;
            let assignment_count = assignment_counts.entry(server_id).or_default();
            *assignment_count += 1;
            if *assignment_count > 4 {
                return Err(WebServiceError::Internal(
                    "node has more than four current runtime assignments".to_string(),
                ));
            }
            let total_descriptors = descriptor_counts.entry(server_id).or_default();
            *total_descriptors = total_descriptors
                .checked_add(descriptor_count)
                .ok_or_else(|| {
                    WebServiceError::Internal(
                        "node runtime assignment descriptor count overflow".to_string(),
                    )
                })?;
            if *total_descriptors > MAX_NODE_SYNC_ITEMS {
                return Err(WebServiceError::Internal(format!(
                    "node runtime assignment exceeds {MAX_NODE_SYNC_ITEMS} descriptors"
                )));
            }
        }

        let mut desired_sync_versions = BTreeMap::new();
        for server_id in assignment_counts.keys() {
            desired_sync_versions.insert(*server_id, compute_agent_sync_version_from_parts(Vec::new()));
        }

        let manifest_sql = format!(
            "WITH current_assignments AS (
                SELECT DISTINCT ON (a.server_id, a.environment)
                       a.server_id, a.runtime_set
                FROM webserver_runtime_assignment a
                WHERE a.tenant_id = $1 AND a.server_id = ANY($2)
                ORDER BY a.server_id, a.environment, a.generation DESC
             ),
             expanded_sites AS (
                SELECT a.server_id, descriptor.value AS descriptor,
                       descriptor.value ->> 'siteUuid' AS site_uuid
                FROM current_assignments a
                CROSS JOIN LATERAL jsonb_array_elements(
                    CASE WHEN jsonb_typeof(a.runtime_set -> 'descriptors') = 'array'
                         THEN a.runtime_set -> 'descriptors' ELSE '[]'::jsonb END
                ) AS descriptor(value)
             ),
             invalid_sites AS (
                SELECT DISTINCT server_id, 1::INTEGER AS item_kind,
                       NULL::TEXT AS component, 0::BIGINT AS hostname_count
                FROM expanded_sites
                WHERE jsonb_typeof(descriptor) <> 'object'
                   OR site_uuid IS NULL OR site_uuid = ''
                   OR octet_length(site_uuid) > 128
                   OR site_uuid ~ '[[:cntrl:]]'
             ),
             site_scope AS (
                SELECT DISTINCT server_id, site_uuid
                FROM expanded_sites
                WHERE jsonb_typeof(descriptor) = 'object'
                  AND site_uuid IS NOT NULL AND site_uuid <> ''
                  AND octet_length(site_uuid) <= 128
                  AND site_uuid !~ '[[:cntrl:]]'
             ),
             nginx_candidates AS (
                SELECT DISTINCT scope.server_id, nc.uuid, nc.config_hash, nc.version,
                       OCTET_LENGTH(nc.config_content) AS config_content_bytes,
                       (SELECT d.hostname FROM webserver_site_binding b
                        INNER JOIN webserver_domain d
                            ON d.tenant_id = b.tenant_id AND d.id = b.domain_id
                        WHERE b.tenant_id = nc.tenant_id AND b.site_id = s.id
                          AND b.environment = 'production' AND b.status = 'ACTIVE'
                          AND b.deleted_at IS NULL AND d.deleted_at IS NULL
                        ORDER BY b.is_primary DESC, b.created_at ASC
                        LIMIT 1) AS domain
                FROM site_scope scope
                INNER JOIN webserver_site s
                    ON s.tenant_id = $1 AND s.uuid = scope.site_uuid
                   AND s.deleted_at IS NULL
                INNER JOIN webserver_nginx_config nc
                    ON nc.tenant_id = s.tenant_id AND nc.site_id = s.id
                   AND nc.is_active = TRUE AND nc.status = 1
             ),
             nginx_parts AS (
                SELECT server_id,
                       CASE WHEN config_hash ~ '^[0-9a-f]{{64}}$'
                                  AND config_content_bytes BETWEEN 0 AND {MAX_NODE_NGINX_CONFIG_BYTES}
                                  AND domain IS NOT NULL AND domain <> ''
                            THEN 2 ELSE 5 END AS item_kind,
                       'n:' || uuid || ':' || config_hash || ':' || version::TEXT
                           || ':' || COALESCE(domain, '') AS component,
                       0::BIGINT AS hostname_count
                FROM nginx_candidates
             ),
             certificate_targets AS (
                SELECT DISTINCT scope.server_id, c.id AS certificate_id,
                       c.uuid AS certificate_uuid, v.id AS certificate_version_id,
                       v.uuid AS certificate_version_uuid,
                       v.fingerprint_sha256 AS fingerprint, v.secret_bundle_ref,
                       sb.encryption_algorithm, OCTET_LENGTH(sb.bundle_encrypted) AS encrypted_bytes
                FROM site_scope scope
                INNER JOIN webserver_site s
                    ON s.tenant_id = $1 AND s.uuid = scope.site_uuid
                   AND s.deleted_at IS NULL
                INNER JOIN webserver_site_binding b
                    ON b.tenant_id = s.tenant_id AND b.site_id = s.id
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
                   AND v.certificate_id = c.id
                   AND v.status IN ('ACTIVE', 'SUPERSEDED')
                INNER JOIN webserver_certificate_secret_bundle sb
                    ON sb.tenant_id = v.tenant_id AND sb.certificate_version_id = v.id
             ),
             certificate_hostnames AS (
                SELECT DISTINCT target.server_id, target.certificate_id,
                       target.certificate_uuid, target.certificate_version_id,
                       target.certificate_version_uuid, target.fingerprint,
                       target.secret_bundle_ref, target.encryption_algorithm,
                       target.encrypted_bytes, listener_domain.hostname
                FROM certificate_targets target
                INNER JOIN webserver_listener_certificate_binding listener
                    ON listener.tenant_id = $1
                   AND listener.certificate_id = target.certificate_id
                   AND listener.desired_version_id = target.certificate_version_id
                   AND listener.status IN ('PENDING', 'DEPLOYING', 'ACTIVE', 'FAILED')
                   AND listener.deleted_at IS NULL
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
                INNER JOIN site_scope listener_scope
                    ON listener_scope.server_id = target.server_id
                   AND listener_scope.site_uuid = listener_site.uuid
             ),
             ranked_certificate_hostnames AS (
                SELECT *, ROW_NUMBER() OVER (
                    PARTITION BY server_id, certificate_id, certificate_version_id
                    ORDER BY hostname
                ) AS hostname_rank
                FROM certificate_hostnames
             ),
             certificate_parts AS (
                SELECT server_id,
                       CASE WHEN COUNT(*) > 128 THEN 4
                            WHEN fingerprint ~ '^[0-9a-f]{{64}}$'
                             AND secret_bundle_ref = 'secret:' || certificate_version_uuid
                             AND encryption_algorithm = 'AES_256_GCM_V1'
                             AND encrypted_bytes BETWEEN 64 AND 2097152
                            THEN 3 ELSE 6 END AS item_kind,
                       'c:' || certificate_uuid || ':' || fingerprint || ':'
                           || STRING_AGG(hostname, ',' ORDER BY hostname) AS component,
                       COUNT(*) AS hostname_count
                FROM ranked_certificate_hostnames
                WHERE hostname_rank <= 129
                GROUP BY server_id, certificate_id, certificate_uuid,
                         certificate_version_id, certificate_version_uuid, fingerprint,
                         secret_bundle_ref, encryption_algorithm, encrypted_bytes
             ),
             raw_items AS (
                SELECT * FROM invalid_sites
                UNION ALL SELECT * FROM nginx_parts
                UNION ALL SELECT * FROM certificate_parts
             ),
             ranked_items AS (
                SELECT *, ROW_NUMBER() OVER (
                    PARTITION BY server_id ORDER BY item_kind, component
                ) AS item_rank
                FROM raw_items
             )
             SELECT server_id, item_kind, component, hostname_count
             FROM ranked_items
             WHERE item_rank <= {}
             ORDER BY server_id, item_kind, component",
            MAX_NODE_SYNC_ITEMS + 1
        );
        let mut rows = sqlx::query(audited_sql(&manifest_sql))
            .bind(tenant_id)
            .bind(server_ids)
            .fetch(&mut **transaction);
        let mut current_server_id = None;
        let mut current_parts = Vec::new();
        while let Some(row) = rows.try_next().await.map_err(|error| {
            store_error(
                "stream certificate distribution desired manifests",
                error,
            )
        })? {
            let server_id: i64 = row.try_get("server_id").map_err(|error| {
                WebServiceError::Internal(format!(
                    "certificate distribution manifest server id: {error}"
                ))
            })?;
            if !server_uuids.contains_key(&server_id) {
                return Err(WebServiceError::Internal(
                    "certificate distribution manifest escaped the requested server scope"
                        .to_string(),
                ));
            }
            if current_server_id.is_some_and(|current| current != server_id) {
                let completed_server_id = current_server_id.take().ok_or_else(|| {
                    WebServiceError::Internal(
                        "certificate distribution manifest grouping failed".to_string(),
                    )
                })?;
                desired_sync_versions.insert(
                    completed_server_id,
                    compute_agent_sync_version_from_parts(std::mem::take(&mut current_parts)),
                );
            }
            current_server_id = Some(server_id);
            let item_kind: i32 = row.try_get("item_kind").map_err(|error| {
                WebServiceError::Internal(format!(
                    "certificate distribution manifest item kind: {error}"
                ))
            })?;
            match item_kind {
                1 => {
                    return Err(WebServiceError::Internal(
                        "stored node runtime assignment has an invalid Site identity".to_string(),
                    ));
                }
                2 | 3 => {
                    let component: String = row.try_get("component").map_err(|error| {
                        WebServiceError::Internal(format!(
                            "certificate distribution manifest component: {error}"
                        ))
                    })?;
                    current_parts.push(component);
                    if current_parts.len() > MAX_NODE_SYNC_ITEMS {
                        return Err(WebServiceError::Internal(format!(
                            "node sync manifest exceeds {MAX_NODE_SYNC_ITEMS} items"
                        )));
                    }
                }
                4 => {
                    let hostname_count: i64 = row.try_get("hostname_count").map_err(|error| {
                        WebServiceError::Internal(format!(
                            "certificate distribution manifest hostname count: {error}"
                        ))
                    })?;
                    return Err(WebServiceError::Internal(format!(
                        "agent sync certificate targets {hostname_count} listener hostnames; maximum is 128"
                    )));
                }
                5 => {
                    return Err(WebServiceError::Internal(
                        "active nginx configuration has invalid bounded manifest metadata"
                            .to_string(),
                    ));
                }
                6 => {
                    return Err(WebServiceError::Internal(
                        "active certificate has invalid bounded secret metadata".to_string(),
                    ));
                }
                _ => {
                    return Err(WebServiceError::Internal(
                        "certificate distribution manifest returned an invalid item kind"
                            .to_string(),
                    ));
                }
            }
        }
        drop(rows);
        if let Some(server_id) = current_server_id {
            desired_sync_versions.insert(
                server_id,
                compute_agent_sync_version_from_parts(current_parts),
            );
        }
        Ok(desired_sync_versions)
    }

    async fn load_current_assigned_site_uuids(
        &self,
        agent: &AuthenticatedAgent,
    ) -> WebServiceResult<Option<Vec<String>>> {
        let rows = sqlx::query(
            "SELECT CAST(a.runtime_set AS TEXT) AS runtime_set
             FROM webserver_runtime_assignment a
             INNER JOIN webserver_server s ON s.tenant_id = a.tenant_id AND s.id = a.server_id
             WHERE a.tenant_id = $1 AND s.uuid = $2
               AND NOT EXISTS (
                   SELECT 1 FROM webserver_runtime_assignment newer
                   WHERE newer.tenant_id = a.tenant_id AND newer.server_id = a.server_id
                     AND newer.environment = a.environment AND newer.generation > a.generation
               )
             ORDER BY a.environment ASC
             LIMIT 5",
        )
        .bind(agent.tenant_id)
        .bind(&agent.server_uuid)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("load current node runtime assignments", error))?;
        if rows.is_empty() {
            return Ok(None);
        }
        if rows.len() > 4 {
            return Err(WebServiceError::Internal(
                "node has more than four current runtime assignments".to_string(),
            ));
        }

        let mut site_uuids = BTreeSet::new();
        let mut descriptor_count = 0_usize;
        for row in rows {
            let raw: String = row.try_get("runtime_set").map_err(|error| {
                WebServiceError::Internal(format!("node runtime assignment JSON: {error}"))
            })?;
            let runtime_set: Value = serde_json::from_str(&raw).map_err(|_| {
                WebServiceError::Internal("stored node runtime assignment is invalid".to_string())
            })?;
            if runtime_set.get("nodeUuid").and_then(Value::as_str)
                != Some(agent.server_uuid.as_str())
            {
                return Err(WebServiceError::Internal(
                    "stored node runtime assignment has an invalid node scope".to_string(),
                ));
            }
            let descriptors = runtime_set
                .get("descriptors")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    WebServiceError::Internal(
                        "stored node runtime assignment has no descriptors".to_string(),
                    )
            })?;
            for descriptor in descriptors {
                descriptor_count = descriptor_count.checked_add(1).ok_or_else(|| {
                    WebServiceError::Internal(
                        "node runtime assignment descriptor count overflow".to_string(),
                    )
                })?;
                if descriptor_count > MAX_NODE_SYNC_ITEMS {
                    return Err(WebServiceError::Internal(format!(
                        "node runtime assignment exceeds {MAX_NODE_SYNC_ITEMS} descriptors"
                    )));
                }
                let site_uuid = descriptor
                    .get("siteUuid")
                    .and_then(Value::as_str)
                    .filter(|value| {
                        !value.is_empty()
                            && value.len() <= 128
                            && !value.bytes().any(|byte| byte.is_ascii_control())
                    })
                    .ok_or_else(|| {
                        WebServiceError::Internal(
                            "stored node runtime assignment has an invalid Site identity"
                                .to_string(),
                        )
                    })?;
                site_uuids.insert(site_uuid.to_string());
            }
        }
        Ok(Some(site_uuids.into_iter().collect()))
    }

    async fn load_active_nginx_configs_for_sites(
        &self,
        tenant_id: i64,
        site_uuids: &[String],
        budget: &mut NodeSyncBudget,
    ) -> WebServiceResult<Vec<AgentNginxConfigBundle>> {
        let content_size = "CAST(OCTET_LENGTH(nc.config_content) AS BIGINT)";
        let sql = format!(
            "SELECT nc.uuid, nc.config_hash,
                    CASE WHEN {content_size} <= {MAX_NODE_NGINX_CONFIG_BYTES}
                         THEN nc.config_content ELSE NULL END AS config_content,
                    {content_size} AS config_content_bytes,
                    nc.version,
                    (SELECT d.hostname FROM webserver_site_binding b
                     INNER JOIN webserver_domain d ON d.tenant_id = b.tenant_id AND d.id = b.domain_id
                     WHERE b.tenant_id = nc.tenant_id AND b.site_id = s.id
                       AND b.environment = 'production' AND b.status = 'ACTIVE'
                       AND b.deleted_at IS NULL AND d.deleted_at IS NULL
                     ORDER BY b.is_primary DESC, b.created_at ASC
                     LIMIT 1) AS domain
             FROM webserver_nginx_config nc
             INNER JOIN webserver_site s ON s.id = nc.site_id
             WHERE nc.tenant_id = $1 AND s.uuid = ANY($2)
               AND nc.is_active = TRUE AND nc.status = 1
               AND s.deleted_at IS NULL
             ORDER BY nc.id ASC
             LIMIT {}",
            MAX_NODE_SYNC_ITEMS + 1
        );
        let mut rows = sqlx::query(audited_sql(&sql))
            .bind(tenant_id)
            .bind(site_uuids)
            .fetch(&self.pool);

        let mut items = Vec::new();
        while let Some(row) = rows
            .try_next()
            .await
            .map_err(|error| store_error("stream active nginx configs for agent sync", error))?
        {
            let content_bytes: i64 = row.try_get("config_content_bytes").map_err(|error| {
                WebServiceError::Internal(format!("agent sync nginx content bytes: {error}"))
            })?;
            if !(0..=MAX_NODE_NGINX_CONFIG_BYTES).contains(&content_bytes) {
                return Err(WebServiceError::Internal(format!(
                    "active nginx configuration exceeds {MAX_NODE_NGINX_CONFIG_BYTES} bytes"
                )));
            }
            let config_content: Option<String> =
                row.try_get("config_content").map_err(|error| {
                    WebServiceError::Internal(format!("agent sync nginx content: {error}"))
                })?;
            let config_content = config_content.ok_or_else(|| {
                WebServiceError::Internal("active nginx configuration is unavailable".to_string())
            })?;
            let config_hash: String = row.try_get("config_hash").map_err(|error| {
                WebServiceError::Internal(format!("agent sync nginx config hash: {error}"))
            })?;
            if config_hash != sha256_hex(&config_content) {
                return Err(WebServiceError::Internal(
                    "active nginx configuration hash mismatch".to_string(),
                ));
            }
            let domain: Option<String> = row.try_get("domain").map_err(|error| {
                WebServiceError::Internal(format!("agent sync nginx domain: {error}"))
            })?;
            let domain = domain.filter(|value| !value.is_empty()).ok_or_else(|| {
                WebServiceError::Internal(
                    "active nginx configuration has no deployable domain".to_string(),
                )
            })?;
            let item = AgentNginxConfigBundle {
                config_id: row.try_get("uuid").map_err(|error| {
                    WebServiceError::Internal(format!("agent sync nginx uuid: {error}"))
                })?,
                domain,
                fingerprint: config_hash,
                config_content,
                version: row.try_get("version").map_err(|error| {
                    WebServiceError::Internal(format!("agent sync nginx version: {error}"))
                })?,
            };
            budget.reserve(&item)?;
            items.push(item);
        }
        Ok(items)
    }

    async fn load_active_certificates_for_sites(
        &self,
        tenant_id: i64,
        site_uuids: &[String],
        budget: &mut NodeSyncBudget,
    ) -> WebServiceResult<Vec<AgentCertificateBundle>> {
        let sql = format!(
            "SELECT DISTINCT c.uuid, c.cert_name, v.fingerprint_sha256 AS fingerprint,
                    v.uuid AS certificate_version_uuid, v.secret_bundle_ref,
                    sb.encryption_algorithm, sb.bundle_encrypted,
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
                            WHERE listener.tenant_id = l.tenant_id
                              AND listener.certificate_id = c.id
                              AND listener.desired_version_id = v.id
                              AND listener.status IN ('PENDING', 'DEPLOYING', 'ACTIVE', 'FAILED')
                              AND listener.deleted_at IS NULL
                              AND listener_site.uuid = ANY($2)
                        ) verification_hostnames
                    ) AS TEXT) AS verification_hostnames
             FROM webserver_listener_certificate_binding l
             INNER JOIN webserver_site_binding b ON b.tenant_id = l.tenant_id
                 AND b.id = l.site_binding_id AND b.status = 'ACTIVE'
                 AND b.deleted_at IS NULL
             INNER JOIN webserver_site s ON s.tenant_id = b.tenant_id AND s.id = b.site_id
                 AND s.deleted_at IS NULL
             INNER JOIN webserver_certificate c ON c.tenant_id = l.tenant_id
                 AND c.id = l.certificate_id AND c.status = 1 AND c.deleted_at IS NULL
             INNER JOIN webserver_certificate_version v ON v.tenant_id = l.tenant_id
                 AND v.id = l.desired_version_id AND v.certificate_id = c.id
                  AND v.status IN ('ACTIVE', 'SUPERSEDED')
             INNER JOIN webserver_certificate_secret_bundle sb ON sb.tenant_id = v.tenant_id
                 AND sb.certificate_version_id = v.id
             WHERE l.tenant_id = $1 AND s.uuid = ANY($2)
               AND l.status IN ('PENDING', 'DEPLOYING', 'ACTIVE', 'FAILED')
               AND l.deleted_at IS NULL
             ORDER BY c.uuid ASC
             LIMIT {}",
            MAX_NODE_SYNC_ITEMS + 1
        );
        let mut rows = sqlx::query(audited_sql(&sql))
            .bind(tenant_id)
            .bind(site_uuids)
            .fetch(&self.pool);

        let mut items = Vec::new();
        while let Some(row) = rows
            .try_next()
            .await
            .map_err(|error| store_error("stream active certificates for agent sync", error))?
        {
            let secret_bundle_ref: String = row.try_get("secret_bundle_ref").map_err(|error| {
                WebServiceError::Internal(format!("agent sync certificate secret ref: {error}"))
            })?;
            let certificate_version_uuid: String = row
                .try_get("certificate_version_uuid")
                .map_err(|error| {
                    WebServiceError::Internal(format!(
                        "agent sync certificate version uuid: {error}"
                    ))
                })?;
            let encryption_algorithm: String = row
                .try_get("encryption_algorithm")
                .map_err(|error| {
                    WebServiceError::Internal(format!(
                        "agent sync certificate encryption algorithm: {error}"
                    ))
                })?;
            let bundle_encrypted: String = row.try_get("bundle_encrypted").map_err(|error| {
                WebServiceError::Internal(format!(
                    "agent sync encrypted certificate bundle: {error}"
                ))
            })?;
            let secret_bundle = decrypt_certificate_secret_bundle(
                self.secret_key(),
                tenant_id,
                &certificate_version_uuid,
                &secret_bundle_ref,
                &encryption_algorithm,
                &bundle_encrypted,
            )?;
            let verification_hostnames_json: String = row
                .try_get("verification_hostnames")
                .map_err(|error| {
                    WebServiceError::Internal(format!(
                        "agent sync certificate verification hostnames: {error}"
                    ))
                })?;
            let hostnames = serde_json::from_str::<Vec<String>>(&verification_hostnames_json)
                .map_err(|error| {
                    WebServiceError::Internal(format!(
                        "decode agent sync certificate verification hostnames: {error}"
                    ))
                })?;
            if hostnames.is_empty() || hostnames.len() > 128 {
                return Err(WebServiceError::Internal(
                    "agent sync certificate must target between 1 and 128 listener hostnames"
                        .to_string(),
                ));
            }

            let item = AgentCertificateBundle {
                certificate_id: row.try_get("uuid").map_err(|error| {
                    WebServiceError::Internal(format!("agent sync certificate uuid: {error}"))
                })?,
                cert_name: row.try_get("cert_name").map_err(|error| {
                    WebServiceError::Internal(format!("agent sync certificate name: {error}"))
                })?,
                fingerprint: row.try_get("fingerprint").map_err(|error| {
                    WebServiceError::Internal(format!(
                        "agent sync certificate fingerprint: {error}"
                    ))
                })?,
                hostnames,
                fullchain_pem: secret_bundle.fullchain_pem,
                privkey_pem: secret_bundle.private_key_pem,
            };
            budget.reserve_with_additional_bytes(&item, bundle_encrypted.len())?;
            items.push(item);
        }
        Ok(items)
    }
}

fn map_authenticated_agent(row: &EngineRow) -> Result<AuthenticatedAgent, sqlx::Error> {
    Ok(AuthenticatedAgent {
        server_uuid: row.try_get("uuid")?,
        tenant_id: row.try_get("tenant_id")?,
    })
}

pub(crate) fn parse_last_heartbeat_at(metadata_raw: &str) -> Option<String> {
    let metadata: Value = serde_json::from_str(metadata_raw).ok()?;
    metadata
        .get("lastHeartbeatAt")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Canonical sync-version part for one nginx config bundle.
fn agent_nginx_sync_part(config: &AgentNginxConfigBundle) -> String {
    format!(
        "n:{}:{}:{}:{}",
        config.config_id, config.fingerprint, config.version, config.domain
    )
}

/// Canonical sync-version part for one certificate bundle. The fingerprint
/// projection ([`Self::build_agent_sync_fingerprint_repo`]) must produce the
/// identical string so heartbeat-validated sync versions match the full
/// manifest path byte for byte.
fn agent_certificate_sync_part(
    certificate_id: &str,
    fingerprint: &str,
    hostnames: &[String],
) -> String {
    format!("c:{certificate_id}:{fingerprint}:{}", hostnames.join(","))
}

pub(crate) fn compute_agent_sync_version(
    nginx_configs: &[AgentNginxConfigBundle],
    certificates: &[AgentCertificateBundle],
) -> String {
    let mut parts = Vec::with_capacity(nginx_configs.len() + certificates.len());
    parts.extend(nginx_configs.iter().map(agent_nginx_sync_part));
    parts.extend(certificates.iter().map(|certificate| {
        agent_certificate_sync_part(
            &certificate.certificate_id,
            &certificate.fingerprint,
            &certificate.hostnames,
        )
    }));
    compute_agent_sync_version_from_parts(parts)
}

fn compute_agent_sync_version_from_parts(mut parts: Vec<String>) -> String {
    parts.sort_unstable();
    format!("sv1:{}", sha256_hash(parts.join("\n").as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_version_is_stable_for_same_manifest() {
        let nginx = vec![AgentNginxConfigBundle {
            config_id: "cfg-1".to_string(),
            domain: "example.com".to_string(),
            config_content: "server {}".to_string(),
            fingerprint: sha256_hex("server {}"),
            version: 2,
        }];
        let certs = vec![AgentCertificateBundle {
            certificate_id: "cert-1".to_string(),
            cert_name: "example.com".to_string(),
            fingerprint: "abc123".to_string(),
            hostnames: vec!["example.com".to_string()],
            fullchain_pem: String::new(),
            privkey_pem: String::new(),
        }];

        let first = compute_agent_sync_version(&nginx, &certs);
        let second = compute_agent_sync_version(&nginx, &certs);
        assert_eq!(first, second);
        assert!(first.starts_with("sv1:"));
    }

    #[test]
    fn sync_version_changes_when_certificate_fingerprint_changes() {
        let nginx = Vec::new();
        let certs_a = vec![AgentCertificateBundle {
            certificate_id: "cert-1".to_string(),
            cert_name: "example.com".to_string(),
            fingerprint: "abc123".to_string(),
            hostnames: vec!["example.com".to_string()],
            fullchain_pem: String::new(),
            privkey_pem: String::new(),
        }];
        let mut certs_b = certs_a.clone();
        certs_b[0].fingerprint = "def456".to_string();

        assert_ne!(
            compute_agent_sync_version(&nginx, &certs_a),
            compute_agent_sync_version(&nginx, &certs_b)
        );
    }

    #[test]
    fn sync_version_changes_when_nginx_domain_changes() {
        let mut nginx_a = vec![AgentNginxConfigBundle {
            config_id: "cfg-1".to_string(),
            domain: "example.com".to_string(),
            config_content: "server {}".to_string(),
            fingerprint: sha256_hex("server {}"),
            version: 2,
        }];
        let version_a = compute_agent_sync_version(&nginx_a, &[]);
        nginx_a[0].domain = "www.example.com".to_string();
        assert_ne!(version_a, compute_agent_sync_version(&nginx_a, &[]));
    }

    #[test]
    fn sync_budget_rejects_item_and_serialized_byte_overflow() {
        let mut item_budget = NodeSyncBudget::with_limits(1, 1024);
        item_budget.reserve(&serde_json::json!({"id": 1})).unwrap();
        assert!(item_budget.reserve(&serde_json::json!({"id": 2})).is_err());

        let mut byte_budget = NodeSyncBudget::with_limits(2, 8);
        assert!(byte_budget
            .reserve(&serde_json::json!({"content": "too-large"}))
            .is_err());
    }
}
