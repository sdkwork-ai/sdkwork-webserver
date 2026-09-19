use crate::audited_sql;
use super::{EngineRow, WebRepository};
use sdkwork_webserver_contract::{
    CreateRootDomainHostnameRequest, CreateRootDomainRequest, DomainDeploymentResponse, DomainPage,
    DomainResponse, ListRootDomainsQuery, RootDomainPage, RootDomainResponse, WebServiceError,
    WebServiceResult,
};
use sqlx::Row;

use super::support::{
    bool_from_row, instant_from_row, instant_write_expression, new_uuid, next_id, now_rfc3339,
    optional_instant_from_row, pagination, resolve_site_internal_id, resolve_site_owner_id,
    store_error,
};

impl WebRepository {
    pub(super) async fn list_root_domains_repo(
        &self,
        tenant_id: i64,
        query: &ListRootDomainsQuery,
    ) -> WebServiceResult<RootDomainPage> {
        let (_page, page_size, offset) = pagination(query.page, query.page_size)?;
        let keyword = query
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("%{}%", value.to_ascii_lowercase()));

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM webserver_root_domain r
             WHERE r.tenant_id = $1 AND r.deleted_at IS NULL
               AND ($2 IS NULL OR r.status = $2)
               AND ($3 IS NULL OR LOWER(r.hostname) LIKE $3)",
        )
        .bind(tenant_id)
        .bind(query.status)
        .bind(keyword.as_deref())
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count webserver_root_domain", error))?;

        let rows = sqlx::query(
            // One grouped LATERAL pass per page row replaces five separate
            // correlated subqueries per row. Every aggregate counts DISTINCT
            // domain ids so the site-binding/listener joins cannot multiply
            // the counts; only the active-deployment filter keeps a per-row
            // latest-deployment lookup, indexed by (tenant_id, site_id).
            "SELECT r.uuid, r.hostname, r.status,
                    COALESCE(agg.subdomain_count, 0) AS subdomain_count,
                    COALESCE(agg.bound_subdomain_count, 0) AS bound_subdomain_count,
                    COALESCE(agg.verified_subdomain_count, 0) AS verified_subdomain_count,
                    COALESCE(agg.https_subdomain_count, 0) AS https_subdomain_count,
                    COALESCE(agg.active_deployment_count, 0) AS active_deployment_count,
                    CAST(r.created_at AS TEXT) AS created_at,
                    CAST(r.updated_at AS TEXT) AS updated_at
             FROM webserver_root_domain r
             LEFT JOIN LATERAL (
                 SELECT COUNT(DISTINCT d.id) AS subdomain_count,
                        COUNT(DISTINCT d.id) FILTER (WHERE d.verification_status = 'VERIFIED')
                            AS verified_subdomain_count,
                        COUNT(DISTINCT d.id) FILTER (WHERE b.id IS NOT NULL AND b.status <> 'ARCHIVED')
                            AS bound_subdomain_count,
                        COUNT(DISTINCT d.id) FILTER (WHERE l.id IS NOT NULL)
                            AS https_subdomain_count,
                        COUNT(DISTINCT d.id) FILTER (WHERE b.id IS NOT NULL AND b.status = 'ACTIVE'
                            AND (SELECT dep.status FROM webserver_deployment dep
                                 WHERE dep.tenant_id = d.tenant_id AND dep.site_id = b.site_id
                                 ORDER BY dep.created_at DESC, dep.id DESC LIMIT 1) = 2)
                            AS active_deployment_count
                 FROM webserver_domain d
                 LEFT JOIN webserver_site_binding b
                     ON b.tenant_id = d.tenant_id AND b.domain_id = d.id
                     AND b.deleted_at IS NULL
                 LEFT JOIN webserver_listener_certificate_binding l
                     ON l.tenant_id = b.tenant_id AND l.site_binding_id = b.id
                     AND l.status = 'ACTIVE' AND l.deleted_at IS NULL
                 WHERE d.tenant_id = r.tenant_id AND d.root_domain_id = r.id
                   AND d.deleted_at IS NULL
             ) agg ON TRUE
             WHERE r.tenant_id = $1 AND r.deleted_at IS NULL
               AND ($2 IS NULL OR r.status = $2)
               AND ($3 IS NULL OR LOWER(r.hostname) LIKE $3)
             ORDER BY r.updated_at DESC, r.id DESC LIMIT $4 OFFSET $5",
        )
        .bind(tenant_id)
        .bind(query.status)
        .bind(keyword.as_deref())
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list webserver_root_domain", error))?;

        let items = rows
            .iter()
            .map(map_root_domain_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                WebServiceError::Internal(format!("map webserver_root_domain row: {error}"))
            })?;

        Ok(RootDomainPage { items, total })
    }

    pub(super) async fn create_root_domain_repo(
        &self,
        tenant_id: i64,
        request: &CreateRootDomainRequest,
    ) -> WebServiceResult<RootDomainResponse> {
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();

        let now_expression = instant_write_expression("$6");
        let sql = format!(
            "INSERT INTO webserver_root_domain (
                id, uuid, tenant_id, hostname, status, metadata, created_at, updated_at, version
             ) VALUES ($1, $2, $3, $4, $5, '{{}}', {now_expression}, {now_expression}, 0)"
        );
        sqlx::query(audited_sql(&sql))
            .bind(id)
            .bind(&uuid)
            .bind(tenant_id)
            .bind(&request.hostname)
            .bind(1_i32)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("insert webserver_root_domain", error))?;

        self.retrieve_root_domain_repo(tenant_id, &uuid).await
    }

    pub(super) async fn retrieve_root_domain_repo(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
    ) -> WebServiceResult<RootDomainResponse> {
        let row = sqlx::query(
            "SELECT r.uuid, r.hostname, r.status,
                    (SELECT COUNT(*) FROM webserver_domain d
                     WHERE d.tenant_id = r.tenant_id AND d.root_domain_id = r.id
                       AND d.deleted_at IS NULL) AS subdomain_count,
                    (SELECT COUNT(DISTINCT d.id) FROM webserver_domain d
                     INNER JOIN webserver_site_binding b ON b.tenant_id = d.tenant_id
                         AND b.domain_id = d.id AND b.deleted_at IS NULL
                         AND b.status <> 'ARCHIVED'
                     WHERE d.tenant_id = r.tenant_id AND d.root_domain_id = r.id
                       AND d.deleted_at IS NULL)
                        AS bound_subdomain_count,
                    (SELECT COUNT(*) FROM webserver_domain d
                     WHERE d.tenant_id = r.tenant_id AND d.root_domain_id = r.id
                       AND d.verification_status = 'VERIFIED' AND d.deleted_at IS NULL)
                        AS verified_subdomain_count,
                    (SELECT COUNT(DISTINCT d.id) FROM webserver_domain d
                     INNER JOIN webserver_site_binding b ON b.tenant_id = d.tenant_id
                         AND b.domain_id = d.id AND b.deleted_at IS NULL
                     INNER JOIN webserver_listener_certificate_binding l ON l.tenant_id = b.tenant_id
                         AND l.site_binding_id = b.id AND l.status = 'ACTIVE'
                         AND l.deleted_at IS NULL
                     WHERE d.tenant_id = r.tenant_id AND d.root_domain_id = r.id
                       AND d.deleted_at IS NULL)
                        AS https_subdomain_count,
                    (SELECT COUNT(DISTINCT d.id) FROM webserver_domain d
                     INNER JOIN webserver_site_binding b ON b.tenant_id = d.tenant_id
                         AND b.domain_id = d.id AND b.status = 'ACTIVE'
                         AND b.deleted_at IS NULL
                     WHERE d.tenant_id = r.tenant_id AND d.root_domain_id = r.id
                       AND d.deleted_at IS NULL
                       AND (SELECT dep.status FROM webserver_deployment dep
                            WHERE dep.tenant_id = r.tenant_id AND dep.site_id = b.site_id
                            ORDER BY dep.created_at DESC, dep.id DESC LIMIT 1) = 2)
                        AS active_deployment_count,
                    CAST(r.created_at AS TEXT) AS created_at,
                    CAST(r.updated_at AS TEXT) AS updated_at
             FROM webserver_root_domain r
             WHERE r.tenant_id = $1 AND r.uuid = $2 AND r.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(root_domain_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve webserver_root_domain", error))?
        .ok_or_else(|| WebServiceError::not_found("root domain not found"))?;

        map_root_domain_row(&row)
            .map_err(|error| WebServiceError::Internal(format!("map webserver_root_domain: {error}")))
    }

    pub(super) async fn delete_root_domain_repo(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
    ) -> WebServiceResult<()> {
        let row = sqlx::query(
            "SELECT r.id,
                    (SELECT COUNT(*) FROM webserver_domain d
                     WHERE d.tenant_id = r.tenant_id AND d.root_domain_id = r.id
                       AND d.deleted_at IS NULL) AS subdomain_count
             FROM webserver_root_domain r
             WHERE r.tenant_id = $1 AND r.uuid = $2 AND r.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(root_domain_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("load webserver_root_domain delete state", error))?
        .ok_or_else(|| WebServiceError::not_found("root domain not found"))?;
        let subdomain_count: i64 = row
            .try_get("subdomain_count")
            .map_err(|error| store_error("map webserver_root_domain child count", error))?;
        if subdomain_count > 0 {
            return Err(WebServiceError::conflict(
                "root domain hostnames must be removed before deletion",
            ));
        }

        let now = now_rfc3339();

        let now_expression = instant_write_expression("$3");
        let sql = format!(
            "UPDATE webserver_root_domain
             SET deleted_at = {now_expression}, updated_at = {now_expression},
                 version = version + 1
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL"
        );
        let result = sqlx::query(audited_sql(&sql))
            .bind(tenant_id)
            .bind(root_domain_id)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("delete webserver_root_domain", error))?;
        if result.rows_affected() == 0 {
            return Err(WebServiceError::not_found("root domain not found"));
        }
        Ok(())
    }

    pub(super) async fn list_root_domain_hostnames_repo(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainPage> {
        let root_internal_id = self
            .resolve_root_domain_internal_id(tenant_id, root_domain_id)
            .await?;
        let (_page, page_size, offset) = pagination(page, page_size)?;
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM webserver_domain
             WHERE tenant_id = $1 AND root_domain_id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(root_internal_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error("count root domain hostnames", error))?;

        let rows = sqlx::query(audited_sql(&root_domain_hostname_select(
            "d.tenant_id = $1 AND d.root_domain_id = $2 AND d.deleted_at IS NULL
             ORDER BY d.updated_at DESC, d.id DESC LIMIT $3 OFFSET $4",
        )))
        .bind(tenant_id)
        .bind(root_internal_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list root domain hostnames", error))?;

        let items = rows
            .iter()
            .map(map_root_domain_hostname_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                WebServiceError::Internal(format!("map root domain hostname row: {error}"))
            })?;
        Ok(DomainPage { items, total })
    }

    pub(super) async fn create_root_domain_hostname_repo(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
        request: &CreateRootDomainHostnameRequest,
    ) -> WebServiceResult<DomainResponse> {
        let root = sqlx::query(
            "SELECT id, hostname FROM webserver_root_domain
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(root_domain_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("load webserver_root_domain for hostname", error))?
        .ok_or_else(|| WebServiceError::not_found("root domain not found"))?;
        let root_internal_id: i64 = root
            .try_get("id")
            .map_err(|error| store_error("map webserver_root_domain id", error))?;
        let root_hostname: String = root
            .try_get("hostname")
            .map_err(|error| store_error("map webserver_root_domain hostname", error))?;
        let hostname = if request.record_name == "@" {
            root_hostname
        } else {
            format!("{}.{}", request.record_name, root_hostname)
        };
        if hostname.len() > 253 {
            return Err(WebServiceError::validation(
                "hostname must not exceed 253 characters",
            ));
        }
        if request.application_id.is_none() && request.is_primary {
            return Err(WebServiceError::validation(
                "an unbound hostname cannot be primary",
            ));
        }
        let site_internal_id = match request.application_id.as_deref() {
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

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin create root domain hostname", error))?;

        if request.is_primary {
            let site_internal_id =
                site_internal_id.expect("primary hostname has an application binding");
            // Serialize primary binding creation on the site row so concurrent
            // primary hostnames cannot both pass the single-primary check.
            let locked = sqlx::query(
                "UPDATE webserver_site SET version = version
                 WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
            )
            .bind(tenant_id)
            .bind(site_internal_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("lock site for primary hostname", error))?;
            if locked.rows_affected() != 1 {
                return Err(WebServiceError::not_found("site not found"));
            }
            let clear_time = instant_write_expression("$3");
            let clear_sql = format!(
                "UPDATE webserver_site_binding SET is_primary = FALSE, updated_at = {clear_time},
                        version = version + 1
                 WHERE tenant_id = $1 AND site_id = $2 AND environment = 'production'
                   AND deleted_at IS NULL"
            );
            sqlx::query(audited_sql(&clear_sql))
                .bind(tenant_id)
                .bind(site_internal_id)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .map_err(|error| store_error("clear primary domain for root hostname", error))?;
        }

        let insert_time = instant_write_expression("$9");
        let insert_sql = format!(
            "INSERT INTO webserver_domain (
                id, uuid, tenant_id, user_id, root_domain_id, hostname, hostname_type,
                verification_status, status, metadata,
                created_at, updated_at, version
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, 'PENDING', $8, '{{}}',
                {insert_time}, {insert_time}, 0
             )"
        );
        sqlx::query(audited_sql(&insert_sql))
            .bind(id)
            .bind(&uuid)
            .bind(tenant_id)
            .bind(owner_user_id)
            .bind(root_internal_id)
            .bind(&hostname)
            .bind(if hostname.starts_with("*.") { "WILDCARD" } else { "EXACT" })
            .bind(0_i32)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error("insert root domain hostname", error))?;

        if let Some(site_internal_id) = site_internal_id {
            let binding_id = next_id(self.id_generator())?;
            let binding_uuid = new_uuid();
            let binding_time = instant_write_expression("$7");
            let binding_sql = format!(
                "INSERT INTO webserver_site_binding (
                    id, uuid, tenant_id, site_id, domain_id, environment, path_prefix,
                    action_type, is_primary, status, created_at, updated_at, version
                 ) VALUES (
                    $1, $2, $3, $4, $5, 'production', '/', 'SERVE', $6, 'PENDING',
                    {binding_time}, {binding_time}, 0
                 )"
            );
            sqlx::query(audited_sql(&binding_sql))
                .bind(binding_id)
                .bind(binding_uuid)
                .bind(tenant_id)
                .bind(site_internal_id)
                .bind(id)
                .bind(request.is_primary)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .map_err(|error| store_error("insert root hostname application binding", error))?;

            if request.ssl_enabled {
                let policy_id = next_id(self.id_generator())?;
                let policy_uuid = new_uuid();
                let policy_time = instant_write_expression("$6");
                let policy_sql = format!(
                    "INSERT INTO webserver_tls_policy (
                        id, uuid, tenant_id, site_binding_id, certificate_source,
                        created_at, updated_at, version
                     ) VALUES ($1, $2, $3, $4, $5, {policy_time}, {policy_time}, 0)"
                );
                let source = match request.ssl_provider.as_deref() {
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
                    .bind(&now)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| store_error("insert root hostname TLS policy", error))?;
            }
        }
        tx.commit()
            .await
            .map_err(|error| store_error("commit root domain hostname", error))?;

        self.retrieve_root_domain_hostname_repo(tenant_id, root_domain_id, &uuid)
            .await
    }

    async fn retrieve_root_domain_hostname_repo(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
        domain_id: &str,
    ) -> WebServiceResult<DomainResponse> {
        let row = sqlx::query(audited_sql(&root_domain_hostname_select(
            "d.tenant_id = $1 AND r.uuid = $2 AND d.uuid = $3
             AND d.deleted_at IS NULL AND r.deleted_at IS NULL",
        )))
        .bind(tenant_id)
        .bind(root_domain_id)
        .bind(domain_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve root domain hostname", error))?
        .ok_or_else(|| WebServiceError::not_found("root domain hostname not found"))?;
        map_root_domain_hostname_row(&row).map_err(|error| {
            WebServiceError::Internal(format!("map root domain hostname: {error}"))
        })
    }

    async fn resolve_root_domain_internal_id(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
    ) -> WebServiceResult<i64> {
        sqlx::query_scalar(
            "SELECT id FROM webserver_root_domain
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(root_domain_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve webserver_root_domain id", error))?
        .ok_or_else(|| WebServiceError::not_found("root domain not found"))
    }
}

fn root_domain_hostname_select(predicate: &str) -> String {
    format!(
        "SELECT d.uuid, d.hostname, r.uuid AS root_domain_id, r.hostname AS root_hostname,
                s.uuid AS application_id, s.name AS application_name,
                (SELECT COUNT(*) FROM webserver_certificate_identifier ci
                 WHERE ci.tenant_id = d.tenant_id AND ci.domain_id = d.id)
                    AS certificate_count,
                COALESCE(b.is_primary, FALSE) AS is_primary,
                (d.verification_status = 'VERIFIED') AS is_verified,
                (p.id IS NOT NULL) AS ssl_enabled,
                CASE p.certificate_source
                    WHEN 'MANAGED' THEN 'letsencrypt'
                    WHEN 'CUSTOM' THEN 'custom'
                    WHEN 'EXTERNAL' THEN 'none'
                    ELSE NULL
                END AS ssl_provider,
                d.status,
                latest.uuid AS latest_deployment_id,
                latest.status AS latest_deployment_status,
                latest.environment AS latest_deployment_environment,
                latest.version_tag AS latest_deployment_version_tag,
                CAST(latest.completed_at AS TEXT) AS latest_deployment_completed_at,
                CAST(latest.created_at AS TEXT) AS latest_deployment_created_at,
                CAST(d.created_at AS TEXT) AS created_at,
                CAST(d.updated_at AS TEXT) AS updated_at
         FROM webserver_domain d
         INNER JOIN webserver_root_domain r ON r.id = d.root_domain_id
         LEFT JOIN LATERAL (
             SELECT candidate.* FROM webserver_site_binding candidate
             WHERE candidate.tenant_id = d.tenant_id AND candidate.domain_id = d.id
               AND candidate.environment = 'production' AND candidate.deleted_at IS NULL
               AND candidate.status <> 'ARCHIVED'
             ORDER BY (candidate.status = 'ACTIVE') DESC, candidate.updated_at DESC, candidate.id DESC
             LIMIT 1
         ) b ON TRUE
         LEFT JOIN webserver_site s ON s.id = b.site_id
         LEFT JOIN webserver_tls_policy p ON p.tenant_id = b.tenant_id
             AND p.site_binding_id = b.id AND p.status = 'ACTIVE' AND p.deleted_at IS NULL
         LEFT JOIN webserver_deployment latest ON latest.id = (
             SELECT dep.id FROM webserver_deployment dep
             WHERE dep.tenant_id = d.tenant_id AND dep.site_id = b.site_id
             ORDER BY dep.created_at DESC, dep.id DESC LIMIT 1
         )
         WHERE {predicate}"
    )
}

fn map_root_domain_row(row: &EngineRow) -> Result<RootDomainResponse, sqlx::Error> {
    Ok(RootDomainResponse {
        id: row.try_get("uuid")?,
        hostname: row.try_get("hostname")?,
        status: row.try_get("status")?,
        subdomain_count: row.try_get("subdomain_count")?,
        bound_subdomain_count: row.try_get("bound_subdomain_count")?,
        verified_subdomain_count: row.try_get("verified_subdomain_count")?,
        https_subdomain_count: row.try_get("https_subdomain_count")?,
        active_deployment_count: row.try_get("active_deployment_count")?,
        created_at: instant_from_row(row, "created_at")?,
        updated_at: instant_from_row(row, "updated_at")?,
    })
}

fn map_root_domain_hostname_row(row: &EngineRow) -> Result<DomainResponse, sqlx::Error> {
    let hostname: String = row.try_get("hostname")?;
    let root_hostname: String = row.try_get("root_hostname")?;
    let record_name = if hostname == root_hostname {
        "@".to_string()
    } else {
        hostname
            .strip_suffix(&format!(".{root_hostname}"))
            .unwrap_or(&hostname)
            .to_string()
    };
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
        hostname,
        root_domain_id: row.try_get("root_domain_id")?,
        record_name: Some(record_name),
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
