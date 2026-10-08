// The synced DNS resolution-record snapshot of a root-domain Zone.
//
// One sync run replaces the Zone's whole snapshot in one transaction, so the
// table always answers with a single provider answer. Every read here is
// store-only: the resolution page renders this snapshot, and only the sync
// operation talks to the provider.

use crate::audited_sql;
use sdkwork_intelligence_webserver_service::{
    DomainDnsRecordFilter, DomainDnsSnapshotWrite, DomainHostnameAsset, RootDomainDnsSyncTarget,
};
use sdkwork_webserver_contract::{
    DomainDnsRecordPage, DomainDnsRecordResponse, WebServiceError, WebServiceResult,
};
use sqlx::Row;

use super::support::{
    instant_from_row, instant_write_expression, new_uuid, next_id, pagination, store_error,
};
use super::{EngineRow, WebRepository};

impl WebRepository {
    pub(super) async fn root_domain_dns_sync_target_repo(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
    ) -> WebServiceResult<Option<RootDomainDnsSyncTarget>> {
        let row = sqlx::query(
            "SELECT id, hostname, cloud_account_id FROM webserver_root_domain
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(root_domain_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("resolve webserver_root_domain sync target", error))?;
        row.map(|row| {
            Ok(RootDomainDnsSyncTarget {
                root_domain_id: row
                    .try_get("id")
                    .map_err(|error| store_error("map webserver_root_domain id", error))?,
                hostname: row
                    .try_get("hostname")
                    .map_err(|error| store_error("map webserver_root_domain hostname", error))?,
                cloud_account_id: row
                    .try_get("cloud_account_id")
                    .map_err(|error| store_error("map webserver_root_domain account", error))?,
            })
        })
        .transpose()
    }

    pub(super) async fn list_root_domain_hostname_assets_repo(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
    ) -> WebServiceResult<Vec<DomainHostnameAsset>> {
        let root_internal_id = self
            .resolve_root_domain_internal_id(tenant_id, root_domain_id)
            .await?;
        let rows = sqlx::query(
            "SELECT id, hostname, hostname_type FROM webserver_domain
             WHERE tenant_id = $1 AND root_domain_id = $2 AND deleted_at IS NULL
             ORDER BY id",
        )
        .bind(tenant_id)
        .bind(root_internal_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list webserver_domain assets", error))?;
        let assets = rows
            .iter()
            .map(|row| {
                Ok(DomainHostnameAsset {
                    domain_id: row.try_get("id")?,
                    hostname: row.try_get("hostname")?,
                    hostname_type: row.try_get("hostname_type")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(|error| WebServiceError::Internal(format!("map hostname asset: {error}")))?;
        Ok(assets)
    }

    /// Replaces the Zone's snapshot under the same zone-row lock the delete
    /// path takes: whichever of a delete and a sync serializes second sees the
    /// other's outcome, so a snapshot can never outlive its zone or a delete
    /// silently strand a snapshot a read would still serve.
    pub(super) async fn replace_root_domain_dns_records_repo(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
        snapshot: &DomainDnsSnapshotWrite,
    ) -> WebServiceResult<i64> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin dns snapshot replace", error))?;
        let root = sqlx::query(
            "SELECT id FROM webserver_root_domain
             WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(root_domain_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| store_error("lock webserver_root_domain for dns snapshot", error))?
        .ok_or_else(|| WebServiceError::not_found("root domain not found"))?;
        let root_internal_id: i64 = root
            .try_get("id")
            .map_err(|error| store_error("map webserver_root_domain id", error))?;

        // Whole-snapshot replace, not a merge: the truth is what the provider
        // answered at `synced_at`, and stale rows that merely *look* plausible
        // are worse than absent ones.
        sqlx::query(
            "DELETE FROM webserver_domain_dns_record
             WHERE tenant_id = $1 AND root_domain_id = $2",
        )
        .bind(tenant_id)
        .bind(root_internal_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error("clear webserver_domain_dns_record", error))?;

        let synced_expression = instant_write_expression("$15");
        for row in &snapshot.records {
            let id = next_id(self.id_generator())?;
            let uuid = new_uuid();
            let sql = format!(
                "INSERT INTO webserver_domain_dns_record (
                    id, uuid, tenant_id, root_domain_id, domain_id,
                    record_name, record_type, record_value, ttl_seconds, priority,
                    record_line, dns_provider, cloud_account_id, provider_record_ref,
                    synced_at, created_at, updated_at, version
                 ) VALUES (
                    $1, $2, $3, $4, $5,
                    $6, $7, $8, $9, $10,
                    $11, $12, $13, $14,
                    {synced_expression}, {synced_expression}, {synced_expression}, 0
                 )"
            );
            sqlx::query(audited_sql(&sql))
                .bind(id)
                .bind(&uuid)
                .bind(tenant_id)
                .bind(root_internal_id)
                .bind(row.domain_id)
                .bind(&row.record_name)
                .bind(&row.record_type)
                .bind(&row.record_value)
                .bind(row.ttl_seconds)
                .bind(row.priority)
                .bind(row.record_line.as_deref())
                .bind(&snapshot.dns_provider)
                .bind(&snapshot.cloud_account_id)
                .bind(row.provider_record_ref.as_deref())
                .bind(&snapshot.synced_at)
                .execute(&mut *tx)
                .await
                .map_err(|error| store_error("insert webserver_domain_dns_record", error))?;
        }
        let stored = i64::try_from(snapshot.records.len()).unwrap_or(i64::MAX);
        tx.commit()
            .await
            .map_err(|error| store_error("commit dns snapshot replace", error))?;
        Ok(stored)
    }

    pub(super) async fn list_root_domain_dns_records_repo(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
        filter: &DomainDnsRecordFilter,
        page: i32,
        page_size: i32,
    ) -> WebServiceResult<DomainDnsRecordPage> {
        let root_internal_id = self
            .resolve_root_domain_internal_id(tenant_id, root_domain_id)
            .await?;
        let (_page, page_size, offset) = pagination(page, page_size)?;

        // The subdomain restriction is the uuid the sync stamped on the rows,
        // so the filter is one equality against the joined hostname row — no
        // owner-string parsing, and the wildcard semantics were already
        // applied when the match was made.
        let domain_uuid = filter
            .domain_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        // A record matched to a hostname that has since been deleted leaves
        // the Zone view with the hostname: it would resolve nothing the edge
        // registers, so the read hides it rather than showing a stale pairing.
        let predicate = "FROM webserver_domain_dns_record rec
             LEFT JOIN webserver_domain d ON d.tenant_id = rec.tenant_id AND d.id = rec.domain_id
             WHERE rec.tenant_id = $1 AND rec.root_domain_id = $2
               AND rec.deleted_at IS NULL
               AND (rec.domain_id IS NULL OR d.deleted_at IS NULL)
               AND ($3::text IS NULL OR d.uuid = $3)";

        let total: i64 = sqlx::query_scalar(audited_sql(&format!("SELECT COUNT(*) {predicate}")))
            .bind(tenant_id)
            .bind(root_internal_id)
            .bind(domain_uuid)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count webserver_domain_dns_record", error))?;

        let rows = sqlx::query(audited_sql(&format!(
            "SELECT rec.uuid, rec.record_name, rec.record_type, rec.record_value,
                    rec.ttl_seconds, rec.priority, rec.record_line, rec.dns_provider,
                    rec.cloud_account_id, rec.provider_record_ref, d.uuid AS domain_uuid,
                    CAST(rec.synced_at AS TEXT) AS synced_at
             {predicate}
             ORDER BY rec.synced_at DESC, rec.id DESC LIMIT $4 OFFSET $5"
        )))
        .bind(tenant_id)
        .bind(root_internal_id)
        .bind(domain_uuid)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error("list webserver_domain_dns_record", error))?;

        let items = rows
            .iter()
            .map(map_dns_record_row)
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(|error| {
                WebServiceError::Internal(format!("map webserver_domain_dns_record row: {error}"))
            })?;
        Ok(DomainDnsRecordPage { items, total })
    }
}

fn map_dns_record_row(row: &EngineRow) -> Result<DomainDnsRecordResponse, sqlx::Error> {
    Ok(DomainDnsRecordResponse {
        id: row.try_get("uuid")?,
        record_name: row.try_get("record_name")?,
        record_type: row.try_get("record_type")?,
        record_value: row.try_get("record_value")?,
        ttl_seconds: row.try_get("ttl_seconds")?,
        priority: row.try_get("priority")?,
        record_line: row.try_get("record_line")?,
        domain_id: row.try_get("domain_uuid")?,
        dns_provider: row.try_get("dns_provider")?,
        cloud_account_id: row.try_get("cloud_account_id")?,
        provider_record_ref: row.try_get("provider_record_ref")?,
        synced_at: instant_from_row(row, "synced_at")?,
    })
}
