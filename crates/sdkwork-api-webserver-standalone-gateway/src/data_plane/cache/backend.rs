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

/// In-memory LRU store with a bounded entry count. The linked map keeps
/// entries in recency order, so lookup, insert, and eviction are all O(1)
/// under the mutex (no scans, no index rebuilds).
pub(crate) struct MemoryCacheBackend {
    inner: Mutex<Inner>,
    maximum_entries: usize,
}

struct Inner {
    entries: LinkedHashMap<super::key::CacheKey, CachedResponse>,
}

impl MemoryCacheBackend {
    pub fn new(maximum_entries: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                entries: LinkedHashMap::new(),
            }),
            maximum_entries: maximum_entries.max(1),
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
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.entries.len() >= self.maximum_entries && !inner.entries.contains_key(&key) {
            // Evict the least recently used entry (front of the map).
            inner.entries.pop_front();
        }
        inner.entries.insert(key, entry);
    }

    fn remove(&self, key: &super::key::CacheKey) {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entries
            .remove(key);
    }
}

impl MemoryCacheBackend {
    pub fn entry_count(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entries
            .len()
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

    #[test]
    fn lru_eviction_is_bounded_and_keeps_recent_entries() {
        let backend = MemoryCacheBackend::new(2);
        backend.insert(cache_key(1), cache_entry(200));
        backend.insert(cache_key(2), cache_entry(200));
        // Touch key 1 so key 2 becomes the least recently used entry.
        assert!(backend.get(&cache_key(1)).is_some());
        backend.insert(cache_key(3), cache_entry(200));
        assert_eq!(backend.entry_count(), 2);
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
