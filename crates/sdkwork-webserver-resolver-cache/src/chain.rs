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
    sync::{Arc, Mutex},
    time::Duration,
};

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

/// Bound on waiter retries after a woken waiter finds no usable record (the
/// winner was cancelled before back-filling). Each retry either becomes the
/// winner or joins a fresh fill.
const SINGLE_FLIGHT_RETRY_LIMIT: u32 = 8;

/// Upper bound for one single-flight wait. Completion and cancellation both
/// notify, so this only fires on a pathological race; it exists so a waiter
/// cannot park forever under any circumstance.
const SINGLE_FLIGHT_WAIT_TIMEOUT: Duration = Duration::from_secs(5);

/// RAII owner of one per-domain in-flight slot. Dropping the guard removes
/// the entry and wakes every waiter, so a cancelled resolution future cannot
/// leave the domain parked on a notification nobody will ever send.
struct InFlightGuard {
    in_flight: Arc<Mutex<HashMap<String, Arc<tokio::sync::Notify>>>>,
    domain: String,
    notify: Arc<tokio::sync::Notify>,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        let mut in_flight = self
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if in_flight.remove(&self.domain).is_some() {
            self.notify.notify_waiters();
        }
    }
}

pub struct ResolutionChain {
    file: Option<Arc<FileResolverSource>>,
    memory: Arc<InMemoryResolverCache>,
    redis: Option<Arc<dyn ResolverCacheBackend>>,
    database: Option<Arc<dyn ResolutionDatabase>>,
    ttl_seconds: u64,
    negative_ttl_seconds: u64,
    /// Per-domain single-flight for the async fallback path. The lock is a
    /// synchronous mutex on purpose: it is only ever held across bounded map
    /// operations, never across an await, and the in-flight RAII guard must
    /// be able to clean up from `Drop` after the winner future is cancelled.
    in_flight: Arc<Mutex<HashMap<String, Arc<tokio::sync::Notify>>>>,
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
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            _config: config.clone(),
        }
    }

    /// Resolve `domain` through the chain, falling back to `upstream`.
    pub async fn resolve(&self, domain: &str, upstream: &UpstreamResolver) -> ResolutionOutcome {
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

        // Layer 3: Redis (distributed). The backend bounds its own
        // round-trips; unavailability degrades to the next layer.
        if let Some(redis) = &self.redis {
            if let Some(record) = redis.get(&domain).await {
                if !record.expired(now_unix()) {
                    self.memory.set(record.clone());
                    return self.outcome_of(record);
                }
            }
        }

        // Layer 4: database (deploy-maintained inventory). Bounded like the
        // write path so a stalled database cannot stall resolution.
        if let Some(database) = &self.database {
            let loaded = tokio::time::timeout(Duration::from_secs(2), database.load(&domain)).await;
            if let Ok(Some(record)) = loaded {
                if !record.expired(now_unix()) {
                    self.backfill(record.clone()).await;
                    return self.outcome_of(record);
                }
            }
        }

        // Upstream fallback with per-domain single-flight. The waiter
        // enables its `Notified` future BEFORE re-reading the memory layer
        // (`tokio::sync::Notify` loses wakeups delivered to a future that
        // was not yet polled): the winner back-fills memory before its
        // guard removes the in-flight entry and calls `notify_waiters`, so
        // a waiter either observes the back-filled record or is registered
        // in time to receive the notification.
        //
        // Every in-flight entry is owned by an RAII [`InFlightGuard`]: if
        // the winner future is cancelled while parked on the upstream call
        // (caller timeout, client disconnect), the guard removes the entry
        // and wakes the waiters from `Drop`, so cancellation can never
        // strand later resolutions on a never-notified `Notify`. A waiter
        // woken without a usable record retries as a fresh candidate, so a
        // cancelled winner costs one bounded retry instead of a fabricated
        // negative result.
        let mut rounds: u32 = 0;
        loop {
            // The borrow ends at this statement's semicolon; the lock is
            // never held across an await in any path below.
            let existing = self
                .in_flight
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get(&domain)
                .cloned();
            let outcome = match existing {
                Some(notify) => self.wait_for_fill(&domain, notify).await,
                None => self.win_fill(&domain, upstream).await,
            };
            if let Some(outcome) = outcome {
                return outcome;
            }
            // Lost the race or the winner was cancelled before back-filling:
            // take another turn. Bounded so a pathological race cannot loop
            // unbounded.
            rounds = rounds.saturating_add(1);
            if rounds > SINGLE_FLIGHT_RETRY_LIMIT {
                return ResolutionOutcome::NegativeHit;
            }
        }
    }

    /// Become the single-flight winner for `domain`: register the in-flight
    /// entry under its RAII guard, run the upstream call, and back-fill the
    /// chain. The guard releases the entry and wakes waiters on drop —
    /// including when this future is cancelled mid-upstream.
    ///
    /// Returns `None` when another caller won the race between the caller's
    /// table check and this lock; the caller should retry as a waiter.
    async fn win_fill(
        &self,
        domain: &str,
        upstream: &UpstreamResolver,
    ) -> Option<ResolutionOutcome> {
        let notify = Arc::new(tokio::sync::Notify::new());
        {
            let mut in_flight = self
                .in_flight
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if in_flight.contains_key(domain) {
                return None;
            }
            in_flight.insert(domain.to_owned(), Arc::clone(&notify));
        }
        // The lock is released and the guard owns the entry before the first
        // await, so every table entry is owned until its future completes or
        // is cancelled.
        let guard = InFlightGuard {
            in_flight: Arc::clone(&self.in_flight),
            domain: domain.to_owned(),
            notify,
        };
        let result = upstream(domain).await;
        let record = match result {
            Ok(addresses) => ResolvedRecord::fresh(
                domain.to_owned(),
                addresses.clone(),
                self.ttl_seconds,
                now_unix(),
            ),
            Err(()) => {
                ResolvedRecord::negative(domain.to_owned(), self.negative_ttl_seconds, now_unix())
            }
        };
        self.backfill(record.clone()).await;
        drop(guard);
        Some(self.outcome_of(record))
    }

    /// Wait for an in-flight fill to complete and serve its back-filled
    /// record. Returns `None` when the winner was cancelled without
    /// back-filling (or the record expired in between), so the caller can
    /// retry as a fresh candidate instead of fabricating a negative result.
    async fn wait_for_fill(
        &self,
        domain: &str,
        notify: Arc<tokio::sync::Notify>,
    ) -> Option<ResolutionOutcome> {
        let notified = notify.notified();
        tokio::pin!(notified);
        // Enable before the re-check so a winner completing between the
        // lookup and the await cannot strand this waiter.
        notified.as_mut().enable();
        if let Some(record) = self.memory.get(domain) {
            if !record.expired(now_unix()) {
                return Some(self.outcome_of(record));
            }
        }
        // Bound the wait even though every completion path now notifies: a
        // timeout only costs one bounded retry instead of a parked future.
        if tokio::time::timeout(SINGLE_FLIGHT_WAIT_TIMEOUT, notified)
            .await
            .is_err()
        {
            tracing::debug!(domain = %domain, "single-flight wait timed out; retrying resolution");
        }
        match self.memory.get(domain) {
            Some(record) if !record.expired(now_unix()) => Some(self.outcome_of(record)),
            _ => None,
        }
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
            redis.set(record.clone()).await;
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
            redis.remove(&domain).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::now_unix;

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
        assert_eq!(
            first,
            ResolutionOutcome::Resolved(vec!["10.0.0.7".to_owned()])
        );
        // Second resolve is served from memory without calling upstream.
        let second = chain.resolve("SVC.LOCAL", &upstream).await;
        assert_eq!(
            second,
            ResolutionOutcome::Resolved(vec!["10.0.0.7".to_owned()])
        );
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
        assert_eq!(
            outcome,
            ResolutionOutcome::Resolved(vec!["10.1.1.1".to_owned()])
        );
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
        let mut expired =
            ResolvedRecord::fresh("exp.local", vec!["10.0.0.1".to_owned()], 0, now_unix());
        expired.expires_at_unix = now_unix().saturating_sub(1);
        chain.memory.set(expired);
        let _ = chain.resolve("exp.local", &upstream).await;
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn concurrent_resolves_collapse_to_one_upstream_call() {
        let chain = Arc::new(ResolutionChain::build(
            &Default::default(),
            None,
            None,
            None,
        ));
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

    /// Regression test for the lost single-flight wakeup family: a waiter
    /// that observes an in-flight entry must serve the back-filled record
    /// through the post-enable memory re-check / notification path instead
    /// of parking forever on a notification it never registered for. The
    /// stale `Notify` here never carries a winner's `notify_waiters` for the
    /// parking future until the test fires it explicitly after back-filling
    /// memory, so a waiter that lost the wakeup would hit the bounded
    /// timeout and fail the test.
    #[tokio::test]
    async fn waiter_serves_backfilled_record_instead_of_parking_on_stale_notify() {
        let chain = Arc::new(ResolutionChain::build(
            &Default::default(),
            None,
            None,
            None,
        ));
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls_clone = calls.clone();
        let upstream: Box<UpstreamResolver> = Box::new(move |_domain| {
            calls_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async move { Err(()) })
        });
        // Pre-register an in-flight entry so the waiter takes the single
        // -flight wait path instead of becoming the winner.
        chain.in_flight.lock().unwrap().insert(
            "race.local".to_owned(),
            Arc::new(tokio::sync::Notify::new()),
        );
        let upstream = Arc::new(upstream);
        let waiter = tokio::spawn({
            let chain = chain.clone();
            let upstream = upstream.clone();
            async move {
                tokio::time::timeout(Duration::from_secs(5), async move {
                    chain.resolve("race.local", upstream.as_ref()).await
                })
                .await
                .expect("waiter must not park on the stale notification")
            }
        });
        // Let the waiter reach the single-flight wait (it registers its
        // enabled `Notified` and misses the still-empty memory layer).
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Winner completes: memory is back-filled, then registered waiters
        // are notified.
        chain.memory.set(ResolvedRecord::fresh(
            "race.local".to_owned(),
            vec!["10.9.9.9".to_owned()],
            60,
            now_unix(),
        ));
        if let Some(notify) = chain.in_flight.lock().unwrap().get("race.local") {
            notify.notify_waiters();
        }

        assert_eq!(
            waiter.await.expect("join"),
            ResolutionOutcome::Resolved(vec!["10.9.9.9".to_owned()])
        );
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "the waiter must be served from the back-filled record"
        );
    }

    /// Regression test for the cancelled-winner leak: a winner future aborted
    /// while parked on the upstream call must release its in-flight slot, and
    /// the next resolution must be able to win instead of parking forever.
    #[tokio::test]
    async fn cancelled_winner_releases_the_in_flight_slot() {
        let chain = Arc::new(ResolutionChain::build(
            &Default::default(),
            None,
            None,
            None,
        ));
        let release = Arc::new(tokio::sync::Notify::new());
        let release_for_task = release.clone();
        let upstream: Box<UpstreamResolver> = Box::new(move |_domain| {
            let release = release_for_task.clone();
            Box::pin(async move {
                release.notified().await;
                Ok(vec!["10.0.0.5".to_owned()])
            })
        });
        let upstream = Arc::new(upstream);

        // Winner parks on the upstream call.
        let winner = tokio::spawn({
            let chain = chain.clone();
            let upstream = upstream.clone();
            async move { chain.resolve("cancel.local", upstream.as_ref()).await }
        });
        while chain.in_flight.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }

        // Abort the winner mid-upstream: the guard must drop the entry.
        winner.abort();
        assert!(winner.await.is_err());
        assert!(
            chain.in_flight.lock().unwrap().is_empty(),
            "a cancelled winner must release its in-flight slot"
        );

        // A fresh resolution must be able to win and complete.
        release.notify_one();
        let outcome = chain.resolve("cancel.local", upstream.as_ref()).await;
        assert_eq!(
            outcome,
            ResolutionOutcome::Resolved(vec!["10.0.0.5".to_owned()])
        );
    }

    /// A waiter joined to a winner that gets cancelled must not report a
    /// fabricated negative result; it retries and is served by a later
    /// winner's back-filled record.
    #[tokio::test]
    async fn waiter_survives_a_cancelled_winner() {
        let chain = Arc::new(ResolutionChain::build(
            &Default::default(),
            None,
            None,
            None,
        ));
        let release = Arc::new(tokio::sync::Notify::new());
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let upstream: Box<UpstreamResolver> = {
            let release = release.clone();
            let calls = calls.clone();
            Box::new(move |_domain| {
                let release = release.clone();
                let calls = calls.clone();
                Box::pin(async move {
                    calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    release.notified().await;
                    Ok(vec!["10.0.0.6".to_owned()])
                })
            })
        };
        let upstream = Arc::new(upstream);

        // Winner parks on the upstream call.
        let winner = tokio::spawn({
            let chain = chain.clone();
            let upstream = upstream.clone();
            async move { chain.resolve("wait.local", upstream.as_ref()).await }
        });
        while chain.in_flight.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        // Waiter joins the in-flight fill.
        let waiter = tokio::spawn({
            let chain = chain.clone();
            let upstream = upstream.clone();
            async move {
                tokio::time::timeout(Duration::from_secs(5), async move {
                    chain.resolve("wait.local", upstream.as_ref()).await
                })
                .await
                .expect("waiter must not park past the winner's cancellation")
            }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Cancel the winner; the waiter must survive and a fresh completion
        // must serve it.
        winner.abort();
        assert!(winner.await.is_err());
        release.notify_one();

        assert_eq!(
            waiter.await.expect("join"),
            ResolutionOutcome::Resolved(vec!["10.0.0.6".to_owned()])
        );
        assert!(
            calls.load(std::sync::atomic::Ordering::SeqCst) >= 1,
            "a fresh upstream resolution must have completed"
        );
    }
}
