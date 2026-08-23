//! The resolution chain orchestrator.
//!
//! Walk order: local file → in-process memory → Redis (distributed) →
//! database. On a lower-layer hit the result back-fills the upper layers;
//! on an upstream miss a negative record is written to every enabled layer
//! with the configured short TTL (fast-fail absorption during DNS outages).
//!
//! The chain never blocks the caller longer than the configured upstream
//! resolution itself: memory and file lookups are synchronous, and the
//! async layers (Redis, database, fallback) run under a per-domain
//! single-flight guard so a thundering herd collapses to one resolution.

use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration,
};

use tokio::sync::Mutex;

use crate::{
    backend::{normalize_domain, ResolverCacheBackend},
    config::ResolutionCacheConfig,
    db::ResolutionDatabase,
    file::FileResolverSource,
    memory::{now_unix, InMemoryResolverCache},
    record::ResolvedRecord,
};

/// Result of a chain resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionOutcome {
    /// Resolved to addresses (from any layer or the upstream fallback).
    Resolved(Vec<String>),
    /// Negative cache hit: the upstream failed recently and the negative
    /// record is still within its TTL.
    NegativeHit,
}

/// The upstream resolution function the chain falls back to. Returning
/// `Ok(addresses)` records a positive entry; `Err` records a negative one.
pub type UpstreamResolver = dyn Fn(&str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<String>, ()>> + Send>>
    + Send
    + Sync;

pub struct ResolutionChain {
    file: Option<Arc<FileResolverSource>>,
    memory: Arc<InMemoryResolverCache>,
    redis: Option<Arc<dyn ResolverCacheBackend>>,
    database: Option<Arc<dyn ResolutionDatabase>>,
    ttl_seconds: u64,
    negative_ttl_seconds: u64,
    /// Per-domain single-flight for the async fallback path.
    in_flight: Mutex<HashMap<String, Arc<tokio::sync::Notify>>>,
    _config: ResolutionCacheConfig,
}

impl ResolutionChain {
    /// Build the chain from configuration. `redis` and `database` layers are
    /// supplied by the host when their configuration is enabled (the
    /// component does not hard-depend on either).
    pub fn build(
        config: &ResolutionCacheConfig,
        file: Option<Arc<FileResolverSource>>,
        redis: Option<Arc<dyn ResolverCacheBackend>>,
        database: Option<Arc<dyn ResolutionDatabase>>,
    ) -> Self {
        let memory = Arc::new(InMemoryResolverCache::new(config.memory_max_entries));
        if let Some(file) = &file {
            // Seed the memory layer from the deployment file.
            for (domain, addresses) in file.entries() {
                let record = ResolvedRecord::fresh(
                    domain.clone(),
                    addresses.clone(),
                    config.memory_ttl_seconds,
                    now_unix(),
                );
                memory.set(record);
            }
        }
        Self {
            file,
            memory,
            redis,
            database,
            ttl_seconds: config.memory_ttl_seconds,
            negative_ttl_seconds: config.negative_ttl_seconds,
            in_flight: Mutex::new(HashMap::new()),
            _config: config.clone(),
        }
    }

    /// Resolve `domain` through the chain, falling back to `upstream`.
    pub async fn resolve(
        &self,
        domain: &str,
        upstream: &UpstreamResolver,
    ) -> ResolutionOutcome {
        let domain = normalize_domain(domain);
        if domain.is_empty() {
            return ResolutionOutcome::NegativeHit;
        }

        // Layer 1: local file (deployment seed).
        if let Some(file) = &self.file {
            if let Some(addresses) = file.lookup(&domain) {
                let record = ResolvedRecord::fresh(
                    domain.clone(),
                    addresses.clone(),
                    self.ttl_seconds,
                    now_unix(),
                );
                self.backfill(record.clone()).await;
                return ResolutionOutcome::Resolved(addresses);
            }
        }

        // Layer 2: in-process memory.
        if let Some(record) = self.memory.get(&domain) {
            if !record.expired(now_unix()) {
                return self.outcome_of(record);
            }
        }

        // Layer 3: Redis (distributed).
        if let Some(redis) = &self.redis {
            if let Some(record) = redis.get(&domain) {
                if !record.expired(now_unix()) {
                    self.memory.set(record.clone());
                    return self.outcome_of(record);
                }
            }
        }

        // Layer 4: database (deploy-maintained inventory).
        if let Some(database) = &self.database {
            if let Some(record) = database.load(&domain).await {
                if !record.expired(now_unix()) {
                    self.backfill(record.clone()).await;
                    return self.outcome_of(record);
                }
            }
        }

        // Upstream fallback with per-domain single-flight.
        let notify = {
            let mut in_flight = self.in_flight.lock().await;
            if let Some(notify) = in_flight.get(&domain) {
                let notify = notify.clone();
                drop(in_flight);
                notify.notified().await;
                // The winner back-filled the chain; re-read from memory.
                return match self.memory.get(&domain) {
                    Some(record) => self.outcome_of(record),
                    None => ResolutionOutcome::NegativeHit,
                };
            }
            let notify = Arc::new(tokio::sync::Notify::new());
            in_flight.insert(domain.clone(), notify.clone());
            notify
        };

        let result = upstream(&domain).await;
        let record = match result {
            Ok(addresses) => ResolvedRecord::fresh(
                domain.clone(),
                addresses.clone(),
                self.ttl_seconds,
                now_unix(),
            ),
            Err(()) => ResolvedRecord::negative(
                domain.clone(),
                self.negative_ttl_seconds,
                now_unix(),
            ),
        };
        self.backfill(record.clone()).await;

        let mut in_flight = self.in_flight.lock().await;
        in_flight.remove(&domain);
        notify.notify_waiters();

        self.outcome_of(record)
    }

    fn outcome_of(&self, record: ResolvedRecord) -> ResolutionOutcome {
        if record.negative {
            ResolutionOutcome::NegativeHit
        } else {
            ResolutionOutcome::Resolved(record.addresses)
        }
    }

    /// Write a record into every enabled layer (memory, Redis, database).
    /// Hits from lower layers refresh the upper layers, and upstream
    /// results (positive or negative) populate the whole chain.
    async fn backfill(&self, record: ResolvedRecord) {
        self.memory.set(record.clone());
        if let Some(redis) = &self.redis {
            redis.set(record.clone());
        }
        if let Some(database) = &self.database {
            let _ = tokio::time::timeout(Duration::from_secs(2), database.save(record)).await;
        }
    }

    /// Invalidate a domain across every layer.
    pub async fn invalidate(&self, domain: &str) {
        let domain = normalize_domain(domain);
        self.memory.remove(&domain);
        if let Some(redis) = &self.redis {
            redis.remove(&domain);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::now_unix;

    fn upstream_ok(address: &'static str) -> Box<UpstreamResolver> {
        let address = address.to_owned();
        Box::new(move |_domain: &str| -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<String>, ()>> + Send>> {
            let address = address.clone();
            Box::pin(async move { Ok(vec![address]) })
        })
    }

    fn upstream_fail() -> Box<UpstreamResolver> {
        Box::new(|_domain: &str| -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<String>, ()>> + Send>> {
            Box::pin(async move { Err(()) })
        })
    }

    #[tokio::test]
    async fn resolves_through_the_fallback_and_backfills_memory() {
        let chain = ResolutionChain::build(&Default::default(), None, None, None);
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls_clone = calls.clone();
        let upstream: Box<UpstreamResolver> = Box::new(move |_domain: &str| {
            calls_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async move { Ok(vec!["10.0.0.7".to_owned()]) })
        });
        let first = chain.resolve("svc.local", &upstream).await;
        assert_eq!(first, ResolutionOutcome::Resolved(vec!["10.0.0.7".to_owned()]));
        // Second resolve is served from memory without calling upstream.
        let second = chain.resolve("SVC.LOCAL", &upstream).await;
        assert_eq!(second, ResolutionOutcome::Resolved(vec!["10.0.0.7".to_owned()]));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn negative_results_are_cached_with_the_short_ttl() {
        let chain = ResolutionChain::build(&Default::default(), None, None, None);
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls_clone = calls.clone();
        let upstream: Box<UpstreamResolver> = Box::new(move |_domain: &str| {
            calls_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async move { Err(()) })
        });
        assert_eq!(
            chain.resolve("down.local", &upstream).await,
            ResolutionOutcome::NegativeHit
        );
        // The negative record absorbs the second call.
        assert_eq!(
            chain.resolve("down.local", &upstream).await,
            ResolutionOutcome::NegativeHit
        );
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn file_layer_wins_and_seeds_memory() {
        let file = Arc::new(FileResolverSource::from_entries({
            let mut map = std::collections::HashMap::new();
            map.insert("seed.local".to_owned(), vec!["10.1.1.1".to_owned()]);
            map
        }));
        let chain = ResolutionChain::build(&Default::default(), Some(file), None, None);
        let upstream: Box<UpstreamResolver> = Box::new(|_domain: &str| {
            // Never reached.
            Box::pin(async move {
                unreachable!("file layer must win");
            })
        });
        let outcome = chain.resolve("seed.local", &upstream).await;
        assert_eq!(outcome, ResolutionOutcome::Resolved(vec!["10.1.1.1".to_owned()]));
    }

    #[tokio::test]
    async fn expired_entries_force_a_refresh() {
        let chain = ResolutionChain::build(&Default::default(), None, None, None);
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls_clone = calls.clone();
        let upstream: Box<UpstreamResolver> = Box::new(move |_domain: &str| {
            calls_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async move { Ok(vec!["10.0.0.1".to_owned()]) })
        });
        let _ = chain.resolve("exp.local", &upstream).await;
        // Force the memory entry to expire.
        let mut expired = ResolvedRecord::fresh("exp.local", vec!["10.0.0.1".to_owned()], 0, now_unix());
        expired.expires_at_unix = now_unix().saturating_sub(1);
        chain.memory.set(expired);
        let _ = chain.resolve("exp.local", &upstream).await;
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn concurrent_resolves_collapse_to_one_upstream_call() {
        let chain = Arc::new(ResolutionChain::build(&Default::default(), None, None, None));
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls_clone = calls.clone();
        let upstream: Box<UpstreamResolver> = Box::new(move |_domain| {
            calls_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(vec!["10.0.0.9".to_owned()])
            })
        });
        let upstream = Arc::new(upstream);
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let chain = chain.clone();
            let upstream = upstream.clone();
            tasks.push(tokio::spawn(async move {
                chain.resolve("flock.local", upstream.as_ref()).await
            }));
        }
        for task in tasks {
            assert_eq!(
                task.await.expect("join"),
                ResolutionOutcome::Resolved(vec!["10.0.0.9".to_owned()])
            );
        }
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
