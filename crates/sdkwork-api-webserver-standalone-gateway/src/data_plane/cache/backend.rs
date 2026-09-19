//! Bounded in-memory cache store with LRU eviction.

use std::sync::Mutex;

use hashlink::LinkedHashMap;

use super::entry::CachedResponse;

/// Storage boundary of the cache component. The memory backend is the
/// default; a disk or shared backend can implement this trait and be swapped
/// in without changing callers (component decoupling).
pub(crate) trait CacheBackend: Send + Sync {
    fn get(&self, key: &super::key::CacheKey) -> Option<CachedResponse>;
    fn insert(&self, key: super::key::CacheKey, entry: CachedResponse);
    fn remove(&self, key: &super::key::CacheKey);
}

/// In-memory LRU store bounded by entry count AND total buffered bytes. The
/// linked map keeps entries in recency order, so lookup, insert, and eviction
/// are all O(1) under the mutex (no scans, no index rebuilds). The byte
/// budget bounds resident body memory independently of how many entries the
/// operator configured (nginx `proxy_cache_path max_size` analog for the
/// memory tier).
pub(crate) struct MemoryCacheBackend {
    inner: Mutex<Inner>,
    maximum_entries: usize,
    maximum_bytes: u64,
}

struct Inner {
    entries: LinkedHashMap<super::key::CacheKey, CachedResponse>,
    buffered_bytes: u64,
}

impl MemoryCacheBackend {
    pub fn with_limits(maximum_entries: usize, maximum_bytes: u64) -> Self {
        Self {
            inner: Mutex::new(Inner {
                entries: LinkedHashMap::new(),
                buffered_bytes: 0,
            }),
            maximum_entries: maximum_entries.max(1),
            maximum_bytes: maximum_bytes.max(1),
        }
    }
}

impl CacheBackend for MemoryCacheBackend {
    fn get(&self, key: &super::key::CacheKey) -> Option<CachedResponse> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // LRU touch: remove+reinsert moves the entry to the most recently
        // used end in O(1).
        let value = inner.entries.remove(key)?;
        inner.entries.insert(key.clone(), value.clone());
        Some(value)
    }

    fn insert(&self, key: super::key::CacheKey, entry: CachedResponse) {
        let entry_bytes = entry.body.len() as u64;
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // An object larger than the whole byte budget can never be resident.
        if entry_bytes > self.maximum_bytes {
            return;
        }
        // Replacing an existing entry releases its bytes before eviction
        // accounting begins.
        if let Some(existing) = inner.entries.remove(&key) {
            inner.buffered_bytes = inner
                .buffered_bytes
                .saturating_sub(existing.body.len() as u64);
        }
        // Evict least-recently-used entries (front of the map) until both
        // budgets admit the new object.
        while inner.entries.len() >= self.maximum_entries
            || inner.buffered_bytes + entry_bytes > self.maximum_bytes
        {
            let Some((_, victim)) = inner.entries.pop_front() else {
                break;
            };
            inner.buffered_bytes = inner
                .buffered_bytes
                .saturating_sub(victim.body.len() as u64);
        }
        inner.buffered_bytes += entry_bytes;
        inner.entries.insert(key, entry);
    }

    fn remove(&self, key: &super::key::CacheKey) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(existing) = inner.entries.remove(key) {
            inner.buffered_bytes = inner
                .buffered_bytes
                .saturating_sub(existing.body.len() as u64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::entry::ResponseMetadata;
    use axum::http::HeaderMap;
    use bytes::Bytes;
    use std::time::Duration;

    use super::*;

    fn cache_key(id: u8) -> super::super::key::CacheKey {
        super::super::key::CacheKey::new(
            "GET",
            "example.com",
            &format!("/{id}"),
            None,
            &[],
            &HeaderMap::new(),
        )
    }

    fn cache_entry(status: u16) -> CachedResponse {
        CachedResponse::new(
            ResponseMetadata {
                status,
                headers: Vec::new(),
                vary: Vec::new(),
                fresh_seconds: 60,
            },
            Bytes::from_static(b"body"),
            Duration::from_secs(60),
            Duration::from_secs(60),
        )
    }

    fn entry_with_body(status: u16, body: &'static [u8]) -> CachedResponse {
        CachedResponse::new(
            ResponseMetadata {
                status,
                headers: Vec::new(),
                vary: Vec::new(),
                fresh_seconds: 60,
            },
            Bytes::from_static(body),
            Duration::from_secs(60),
            Duration::from_secs(60),
        )
    }

    #[test]
    fn byte_budget_evicts_least_recent_entries_first() {
        let backend = MemoryCacheBackend::with_limits(16, 8);
        // Three 4-byte bodies exceed the 8-byte budget: inserting the third
        // must evict the least recently used entry. Touching key 1 makes
        // key 2 the LRU victim.
        backend.insert(cache_key(1), entry_with_body(200, b"aaaa"));
        backend.insert(cache_key(2), entry_with_body(200, b"bbbb"));
        assert!(backend.get(&cache_key(1)).is_some(), "touch key 1");
        backend.insert(cache_key(3), entry_with_body(200, b"cccc"));
        assert!(backend.get(&cache_key(2)).is_none(), "key 2 (LRU) evicted");
        assert!(backend.get(&cache_key(1)).is_some());
        assert!(backend.get(&cache_key(3)).is_some());

        // An object larger than the whole budget is never resident.
        backend.insert(cache_key(4), entry_with_body(200, b"0123456789"));
        assert!(backend.get(&cache_key(4)).is_none());
        // Removing an entry releases its bytes for future inserts.
        backend.remove(&cache_key(2));
        backend.insert(cache_key(5), entry_with_body(200, b"dddd"));
        assert!(backend.get(&cache_key(5)).is_some());
    }

    #[test]
    fn lru_eviction_is_bounded_and_keeps_recent_entries() {
        let backend = MemoryCacheBackend::with_limits(2, u64::MAX);
        backend.insert(cache_key(1), cache_entry(200));
        backend.insert(cache_key(2), cache_entry(200));
        // Touch key 1 so key 2 becomes the least recently used entry.
        assert!(backend.get(&cache_key(1)).is_some());
        backend.insert(cache_key(3), cache_entry(200));
        assert!(
            backend.get(&cache_key(2)).is_none(),
            "LRU victim must be evicted"
        );
        assert!(backend.get(&cache_key(1)).is_some());
        assert!(backend.get(&cache_key(3)).is_some());
        backend.remove(&cache_key(1));
        assert!(backend.get(&cache_key(1)).is_none());
    }
}
