//! In-process memory cache backend: bounded TTL entries with LRU eviction.

use std::{
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use hashlink::LinkedHashMap;

use crate::{
    backend::{normalize_domain, ResolverCacheBackend},
    record::ResolvedRecord,
};

/// Seconds granularity clock shared by the memory backend.
pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// Bounded in-process cache. `get` drops expired entries; `set` evicts the
/// least-recently-used entry when the entry cap is reached. The linked map
/// keeps entries in recency order, so lookup, insert, and eviction are all
/// O(1) under the mutex (no scans, no index rebuilds).
pub struct InMemoryResolverCache {
    inner: Mutex<Inner>,
    maximum_entries: usize,
}

struct Inner {
    entries: LinkedHashMap<String, ResolvedRecord>,
}

impl InMemoryResolverCache {
    pub fn new(maximum_entries: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                entries: LinkedHashMap::new(),
            }),
            maximum_entries: maximum_entries.max(1),
        }
    }

    /// Synchronous lookup on the resolution chain's hot path; the async trait
    /// surface delegates here because a local map lookup never blocks.
    pub fn get(&self, domain: &str) -> Option<ResolvedRecord> {
        let domain = normalize_domain(domain);
        let now = now_unix();
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.entries.get(&domain)?.expired(now) {
            inner.entries.remove(&domain);
            return None;
        }
        // LRU touch: remove+reinsert moves the entry to the most recently
        // used end in O(1).
        let record = inner.entries.remove(&domain)?;
        inner.entries.insert(domain, record.clone());
        Some(record)
    }

    /// Synchronous write into the bounded map.
    pub fn set(&self, record: ResolvedRecord) {
        let domain = normalize_domain(&record.domain);
        let mut record = record;
        record.domain = domain.clone();
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.entries.len() >= self.maximum_entries && !inner.entries.contains_key(&domain) {
            // Evict the least recently used entry (front of the map).
            inner.entries.pop_front();
        }
        inner.entries.insert(domain, record);
    }

    /// Synchronous explicit invalidation.
    pub fn remove(&self, domain: &str) {
        let domain = normalize_domain(domain);
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entries
            .remove(&domain);
    }
}

#[async_trait]
impl ResolverCacheBackend for InMemoryResolverCache {
    async fn get(&self, domain: &str) -> Option<ResolvedRecord> {
        InMemoryResolverCache::get(self, domain)
    }

    async fn set(&self, record: ResolvedRecord) {
        InMemoryResolverCache::set(self, record);
    }

    async fn remove(&self, domain: &str) {
        InMemoryResolverCache::remove(self, domain);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(domain: &str, ttl: u64, negative: bool) -> ResolvedRecord {
        if negative {
            ResolvedRecord::negative(domain, ttl, now_unix())
        } else {
            ResolvedRecord::fresh(domain, vec!["127.0.0.1".to_owned()], ttl, now_unix())
        }
    }

    #[test]
    fn stores_and_reads_until_ttl() {
        let cache = InMemoryResolverCache::new(16);
        cache.set(record("api.local", 100, false));
        let hit = cache.get("API.LOCAL").expect("hit");
        assert_eq!(hit.addresses, vec!["127.0.0.1"]);
        assert!(!hit.negative);
    }

    #[test]
    fn expired_entries_are_dropped_on_read() {
        let cache = InMemoryResolverCache::new(16);
        let mut expired = record("gone.local", 1, false);
        expired.expires_at_unix = now_unix().saturating_sub(1);
        cache.set(expired);
        assert!(cache.get("gone.local").is_none());
    }

    #[test]
    fn evicts_least_recently_used_when_full() {
        let cache = InMemoryResolverCache::new(2);
        cache.set(record("a.local", 100, false));
        cache.set(record("b.local", 100, false));
        // Touch a so b becomes the least recently used entry.
        let _ = cache.get("a.local");
        cache.set(record("c.local", 100, false)); // evicts b
        assert!(cache.get("b.local").is_none());
        assert!(cache.get("a.local").is_some());
        assert!(cache.get("c.local").is_some());
    }

    #[test]
    fn remove_drops_the_entry() {
        let cache = InMemoryResolverCache::new(16);
        cache.set(record("x.local", 100, false));
        cache.set(record("y.local", 100, false));
        cache.remove("x.local");
        assert!(cache.get("x.local").is_none());
        assert!(cache.get("y.local").is_some());
    }
}
