//! `web_resolution_cache` persistence for the multi-layer resolution cache.
//!
//! The deploy control plane (`sdkwork-deployments`) seeds and maintains
//! domain/IP inventory in this table; the data plane reads it as the
//! database layer of `ResolutionChain` and back-fills resolution results.

use std::sync::Arc;

use sdkwork_database_sqlx::{process_shared_database_pool, DatabasePool};
use sdkwork_webserver_resolver_cache::{ResolutionDatabase, ResolvedRecord};

/// SQL-backed resolution cache over the process-shared database pool.
pub struct SqlxResolutionCache {
    pool: DatabasePool,
}

/// Build the database layer from the process-shared pool, when one is
/// active (management feature deployments). Returns `None` when no pool is
/// available so the chain falls back to the upper layers.
pub fn resolution_cache_from_shared_pool() -> Option<Arc<dyn ResolutionDatabase>> {
    process_shared_database_pool()
        .map(|pool| Arc::new(SqlxResolutionCache { pool }) as Arc<dyn ResolutionDatabase>)
}

#[async_trait::async_trait]
impl ResolutionDatabase for SqlxResolutionCache {
    async fn load(&self, domain: &str) -> Option<ResolvedRecord> {
        let pool = self.pool.as_postgres()?;
        let row = sqlx::query_as::<_, (String, String, bool, chrono::DateTime<chrono::Utc>)>(
            "SELECT domain, addresses::text, negative, expires_at \
             FROM web_resolution_cache \
             WHERE domain = $1 AND expires_at > now()",
        )
        .bind(domain)
        .fetch_optional(pool)
        .await
        .ok()?;
        let (domain, addresses, negative, expires_at) = row?;
        let addresses = serde_json::from_str::<serde_json::Value>(&addresses)
            .ok()
            .and_then(|value| value.as_array().cloned())
            .map(|items| {
                items
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Some(ResolvedRecord {
            domain,
            addresses,
            negative,
            expires_at_unix: expires_at.timestamp().max(0) as u64,
        })
    }

    async fn save(&self, record: ResolvedRecord) {
        let addresses =
            serde_json::to_string(&record.addresses).unwrap_or_else(|_| "[]".to_owned());
        let expires_at = chrono::DateTime::from_timestamp(record.expires_at_unix as i64, 0)
            .unwrap_or_else(chrono::Utc::now);
        let Some(pool) = self.pool.as_postgres() else {
            return;
        };
        let _ = sqlx::query(
            "INSERT INTO web_resolution_cache (domain, addresses, negative, expires_at, updated_at) \
             VALUES ($1, $2, $3, $4, now()) \
             ON CONFLICT (domain) DO UPDATE SET \
               addresses = EXCLUDED.addresses, \
               negative = EXCLUDED.negative, \
               expires_at = EXCLUDED.expires_at, \
               updated_at = now()",
        )
        .bind(&record.domain)
        .bind(&addresses)
        .bind(record.negative)
        .bind(expires_at)
        .execute(pool)
        .await;
    }
}
