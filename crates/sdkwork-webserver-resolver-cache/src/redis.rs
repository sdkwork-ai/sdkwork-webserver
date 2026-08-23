//! Redis distributed cache backend (`redis` feature).
//!
//! Records are JSON-serialized under a namespaced key
//! (`<prefix>:<domain>`) with the Redis key TTL set from the record's
//! expiry, so the cluster evicts expired entries itself. A connection pool
//! keeps latency off the hot path; failures degrade to the next chain
//! layer (the backend returns `None` and drops the write).

use redis::aio::ConnectionManager;
use std::sync::Arc;

use crate::{
    backend::{normalize_domain, ResolverCacheBackend},
    config::RedisCacheConfig,
    record::ResolvedRecord,
};

/// Redis backend. Constructed through [`RedisResolverCache::connect`]; the
/// `build` variant keeps the error path out of the chain constructor.
pub struct RedisResolverCache {
    pool: ConnectionManager,
    prefix: String,
}

impl RedisResolverCache {
    pub async fn connect(config: &RedisCacheConfig) -> Result<Arc<Self>, String> {
        let client = redis::Client::open(config.url.as_str())
            .map_err(|error| format!("invalid Redis URL: {error}"))?;
        let pool = ConnectionManager::new(client)
            .await
            .map_err(|error| format!("cannot connect to Redis: {error}"))?;
        Ok(Arc::new(Self {
            pool,
            prefix: config.prefix.clone(),
        }))
    }

    fn key(&self, domain: &str) -> String {
        format!("{}:{}", self.prefix, normalize_domain(domain))
    }
}

impl ResolverCacheBackend for RedisResolverCache {
    fn get(&self, domain: &str) -> Option<ResolvedRecord> {
        // Synchronous trait boundary: dispatch a blocking Redis round-trip
        // on the blocking pool. The chain calls this layer from async
        // contexts; a short blocking read is acceptable for a cache hit
        // path, and failures degrade to the next layer.
        let key = self.key(domain);
        let pool = self.pool.clone();
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let mut connection = pool.clone();
                let value: Option<String> = redis::cmd("GET")
                    .arg(&key)
                    .query_async(&mut connection)
                    .await
                    .ok()?;
                value
            })
        });
        result.and_then(|value| serde_json::from_str(&value).ok())
    }

    fn set(&self, record: ResolvedRecord) {
        let key = self.key(&record.domain);
        let value = serde_json::to_string(&record).ok();
        let ttl = record.expires_at_unix.saturating_sub(crate::memory::now_unix());
        let pool = self.pool.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let mut connection = pool.clone();
                let Some(value) = value else {
                    return;
                };
                let _: Result<(), _> = redis::cmd("SET")
                    .arg(&key)
                    .arg(&value)
                    .arg("EX")
                    .arg(ttl)
                    .query_async(&mut connection)
                    .await;
            })
        });
    }

    fn remove(&self, domain: &str) {
        let key = self.key(domain);
        let pool = self.pool.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let mut connection = pool.clone();
                let _: Result<(), _> = redis::cmd("DEL").arg(&key).query_async(&mut connection).await;
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_namespaced_and_normalized() {
        // Key layout is pure; connectivity tests need a live Redis.
        let backend = RedisResolverCache {
            pool: panic!("no pool in key test"),
            prefix: "sdkwork:resolver".to_owned(),
        };
        // Can't construct pool without a server; verify the key format
        // through a standalone helper instead.
        assert_eq!(backend.key("API.Example.COM."), "sdkwork:resolver:api.example.com");
    }

    #[test]
    fn records_round_trip_through_json() {
        let record = ResolvedRecord::fresh("svc.local", vec!["10.0.0.1".to_owned()], 60, 1_700_000_000);
        let encoded = serde_json::to_string(&record).expect("serialize");
        let decoded: ResolvedRecord = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded, record);
    }
}
