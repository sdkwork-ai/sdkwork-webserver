//! Redis distributed cache backend (`redis` feature).
//!
//! Records are JSON-serialized under a namespaced key
//! (`<prefix>:<domain>`) with the Redis key TTL set from the record's
//! expiry, so the cluster evicts expired entries itself.
//!
//! The connection is established lazily on first asynchronous use and every
//! round-trip is bounded by the configured connect/operation timeouts. The
//! backend never blocks an executor worker and never panics on
//! current-thread runtimes (the former `block_in_place` + `block_on`
//! boundary let one stalled Redis freeze the whole worker pool). A failed
//! initial connect opens a cooldown before retrying; per-operation failures
//! degrade to the upper chain layers (memory, file, database, upstream).

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use redis::aio::ConnectionManager;
use tokio::sync::OnceCell;

use crate::{
    backend::{normalize_domain, ResolverCacheBackend},
    config::RedisCacheConfig,
    record::ResolvedRecord,
};

/// Cooldown before a failed initial connect is retried, so a temporarily
/// unavailable Redis does not turn every request into a connect attempt.
const CONNECT_RETRY_COOLDOWN: Duration = Duration::from_secs(30);

/// Redis backend. Construct without IO through [`RedisResolverCache::new`];
/// the connection is established lazily inside the async trait methods.
pub struct RedisResolverCache {
    config: RedisCacheConfig,
    connection: OnceCell<ConnectionManager>,
    /// Unix millis of the last failed connect attempt (0 = never failed).
    /// The cooldown is advisory (a rare duplicate connect attempt is
    /// harmless), so it is enforced with one atomic read instead of a lock
    /// that would otherwise be held across the connect await.
    last_connect_failure_ms: AtomicU64,
}

impl RedisResolverCache {
    pub fn new(config: RedisCacheConfig) -> Self {
        Self {
            config,
            connection: OnceCell::new(),
            last_connect_failure_ms: AtomicU64::new(0),
        }
    }

    fn key(&self, domain: &str) -> String {
        namespaced_key(&self.config.prefix, domain)
    }

    /// Established connection, or `None` while unavailable. The initial
    /// connect is bounded by `connectTimeoutMs`; failures are retried only
    /// after the cooldown.
    async fn connection(&self) -> Option<ConnectionManager> {
        if let Some(connection) = self.connection.get() {
            return Some(connection.clone());
        }
        let now_ms = now_unix_ms();
        let last_failure = self.last_connect_failure_ms.load(Ordering::Relaxed);
        if last_failure != 0
            && now_ms.saturating_sub(last_failure) < CONNECT_RETRY_COOLDOWN.as_millis() as u64
        {
            return None;
        }
        let connect_timeout = Duration::from_millis(self.config.connect_timeout_ms);
        let attempt = tokio::time::timeout(connect_timeout, async {
            let client = redis::Client::open(self.config.url.as_str()).ok()?;
            client.get_connection_manager().await.ok()
        })
        .await
        .ok()
        .flatten();
        match attempt {
            Some(connection) => {
                self.last_connect_failure_ms.store(0, Ordering::Relaxed);
                let _ = self.connection.set(connection.clone());
                Some(connection)
            }
            None => {
                self.last_connect_failure_ms
                    .store(now_unix_ms(), Ordering::Relaxed);
                None
            }
        }
    }

    async fn bounded_query<T: redis::FromRedisValue>(&self, command: &mut redis::Cmd) -> Option<T> {
        let mut connection = self.connection().await?;
        tokio::time::timeout(
            Duration::from_millis(self.config.operation_timeout_ms),
            command.query_async(&mut connection),
        )
        .await
        .ok()?
        .ok()
    }
}

/// Pure key layout: `<prefix>:<normalized-domain>`. A free function so the
/// key format is testable without a live Redis connection.
fn namespaced_key(prefix: &str, domain: &str) -> String {
    format!("{}:{}", prefix, normalize_domain(domain))
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

#[async_trait]
impl ResolverCacheBackend for RedisResolverCache {
    async fn get(&self, domain: &str) -> Option<ResolvedRecord> {
        let key = self.key(domain);
        let mut command = redis::cmd("GET");
        command.arg(&key);
        let value = self.bounded_query::<Option<String>>(&mut command).await??;
        serde_json::from_str(&value).ok()
    }

    async fn set(&self, record: ResolvedRecord) {
        let key = self.key(&record.domain);
        let Ok(value) = serde_json::to_string(&record) else {
            return;
        };
        let ttl = record
            .expires_at_unix
            .saturating_sub(crate::memory::now_unix());
        let mut command = redis::cmd("SET");
        command.arg(&key).arg(&value).arg("EX").arg(ttl);
        let _: Option<()> = self.bounded_query(&mut command).await;
    }

    async fn remove(&self, domain: &str) {
        let key = self.key(domain);
        let mut command = redis::cmd("DEL");
        command.arg(&key);
        let _: Option<i64> = self.bounded_query(&mut command).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_namespaced_and_normalized() {
        // Key layout is pure; connectivity tests need a live Redis.
        assert_eq!(
            namespaced_key("sdkwork:resolver", "API.Example.COM."),
            "sdkwork:resolver:api.example.com"
        );
    }

    #[test]
    fn records_round_trip_through_json() {
        let record =
            ResolvedRecord::fresh("svc.local", vec!["10.0.0.1".to_owned()], 60, 1_700_000_000);
        let encoded = serde_json::to_string(&record).expect("serialize");
        let decoded: ResolvedRecord = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded, record);
    }

    #[tokio::test]
    async fn unreachable_redis_degrades_without_blocking() {
        // A closed port fails the bounded connect attempt; the backend must
        // return None quickly instead of blocking or panicking, and the
        // cooldown suppresses immediate reconnect storms.
        let config = RedisCacheConfig {
            url: "redis://127.0.0.1:1".to_owned(),
            ttl_seconds: 60,
            prefix: "test:resolver".to_owned(),
            connect_timeout_ms: 250,
            operation_timeout_ms: 250,
        };
        let backend = RedisResolverCache::new(config);
        let started = std::time::Instant::now();
        let first = backend.get("svc.local").await;
        assert!(first.is_none());
        assert!(started.elapsed() < Duration::from_secs(5));
        // Within the cooldown the second lookup fails fast without another
        // connect attempt.
        let second =
            tokio::time::timeout(Duration::from_millis(250), backend.get("svc.local")).await;
        assert_eq!(second, Ok(None));
    }
}
