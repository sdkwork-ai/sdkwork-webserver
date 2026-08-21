//! Bounded in-memory cache store with LRU eviction.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};

use super::entry::CachedResponse;

/// Storage boundary of the cache component. The memory backend is the
/// default; a disk or shared backend can implement this trait and be swapped
/// in without changing callers (component decoupling).
pub(crate) trait CacheBackend: Send + Sync {
    fn get(&self, key: &super::key::CacheKey) -> Option<CachedResponse>;
    fn insert(&self, key: super::key::CacheKey, entry: CachedResponse);
    fn remove(&self, key: &super::key::CacheKey);
}

struct LruEntry {
    key: super::key::CacheKey,
    value: CachedResponse,
    last_used: Instant,
}

/// In-memory LRU store with a bounded entry count.
pub(crate) struct MemoryCacheBackend {
    inner: Mutex<Inner>,
    maximum_entries: usize,
}

struct Inner {
    entries: HashMap<super::key::CacheKey, usize>,
    order: Vec<LruEntry>,
}

impl MemoryCacheBackend {
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

impl CacheBackend for MemoryCacheBackend {
    fn get(&self, key: &super::key::CacheKey) -> Option<CachedResponse> {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let index = *inner.entries.get(key)?;
        let entry = &mut inner.order[index];
        entry.last_used = Instant::now();
        Some(entry.value.clone())
    }

    fn insert(&self, key: super::key::CacheKey, entry: CachedResponse) {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(&index) = inner.entries.get(&key) {
            inner.order[index].value = entry;
            inner.order[index].last_used = Instant::now();
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
                .expect("non-empty order when at capacity");
            let victim_key = inner.order[victim].key.clone();
            inner.entries.remove(&victim_key);
            inner.order.swap_remove(victim);
            // Fix up indices shifted by swap_remove.
            let keys = inner
                .order
                .iter()
                .map(|entry| entry.key.clone())
                .collect::<Vec<_>>();
            for (index, key) in keys.into_iter().enumerate() {
                inner.entries.insert(key, index);
            }
        }
        let index = inner.order.len();
        inner.order.push(LruEntry {
            key: key.clone(),
            value: entry,
            last_used: Instant::now(),
        });
        inner.entries.insert(key, index);
    }

    fn remove(&self, key: &super::key::CacheKey) {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(index) = inner.entries.remove(key) {
            inner.order.swap_remove(index);
            let keys = inner
                .order
                .iter()
                .map(|entry| entry.key.clone())
                .collect::<Vec<_>>();
            for (index, key) in keys.into_iter().enumerate() {
                inner.entries.insert(key, index);
            }
        }
    }
}

impl MemoryCacheBackend {
    pub fn entry_count(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .order
            .len()
    }
}

/// Test seam: construct an in-memory backend directly.
pub(crate) fn memory_backend(maximum_entries: usize) -> Arc<MemoryCacheBackend> {
    Arc::new(MemoryCacheBackend::new(maximum_entries))
}
