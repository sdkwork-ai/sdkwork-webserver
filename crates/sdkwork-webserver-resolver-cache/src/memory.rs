//! In-process memory cache backend: bounded TTL entries with LRU eviction.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

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

struct Entry {
    record: ResolvedRecord,
    last_used: u64,
}

/// Bounded in-process cache. `get` drops expired entries; `set` evicts the
/// least-recently-used entry when the entry cap is reached.
pub struct InMemoryResolverCache {
    inner: Mutex<Inner>,
    maximum_entries: usize,
}

struct Inner {
    entries: HashMap<String, usize>,
    order: Vec<Entry>,
}

impl InMemoryResolverCache {
    pub fn new(maximum_entries: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                entries: HashMap::new(),
                order: Vec::new(),
            }),
            maximum_entries: maximum_entries.max(1),
        }
    }
}

impl ResolverCacheBackend for InMemoryResolverCache {
    fn get(&self, domain: &str) -> Option<ResolvedRecord> {
        let domain = normalize_domain(domain);
        let now = now_unix();
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let index = *inner.entries.get(&domain)?;
        if inner.order[index].record.expired(now) {
            remove_at(&mut inner, index);
            return None;
        }
        inner.order[index].last_used = now;
        Some(inner.order[index].record.clone())
    }

    fn set(&self, record: ResolvedRecord) {
        let now = now_unix();
        let domain = normalize_domain(&record.domain);
        let mut record = record;
        record.domain = domain.clone();
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(&index) = inner.entries.get(&domain) {
            inner.order[index].record = record;
            inner.order[index].last_used = now;
            return;
        }
        if inner.order.len() >= self.maximum_entries {
            // Evict the least recently used entry.
            let victim = inner
                .order
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(index, _)| index)
                .expect("non-empty");
            remove_at(&mut inner, victim);
        }
        let index = inner.order.len();
        inner.entries.insert(domain, index);
        inner.order.push(Entry {
            record,
            last_used: now,
        });
    }

    fn remove(&self, domain: &str) {
        let domain = normalize_domain(domain);
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(&index) = inner.entries.get(&domain) else {
            return;
        };
        remove_at(&mut inner, index);
    }
}

/// Remove the entry at `index` (dropping its index mapping) and re-index
/// everything after it.
fn remove_at(inner: &mut Inner, index: usize) {
    let domain = inner.order[index].record.domain.clone();
    inner.order.remove(index);
    inner.entries.remove(&domain);
    for (offset, entry) in inner.order.iter().enumerate().skip(index) {
        *inner
            .entries
            .get_mut(&entry.record.domain)
            .expect("indexed") = offset;
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
        // Age b's last-use beyond a's (the clock is second-granular).
        std::thread::sleep(std::time::Duration::from_millis(1_100));
        let _ = cache.get("a.local"); // a is now the most recent
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
