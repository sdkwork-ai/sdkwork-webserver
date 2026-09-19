//! HTTP response cache component (nginx `proxy_cache` equivalent).
//!
//! Module layout (high cohesion, low coupling):
//!
//! - `key.rs`     — canonical cache keys (host + path + query + Vary input)
//! - `entry.rs`   — cached response entries with freshness metadata
//! - `policy.rs`  — Cache-Control / Expires freshness parsing
//! - `backend.rs` — `CacheBackend` trait + in-memory LRU (L1)
//! - `disk.rs`    — tiered memory+disk backend (`proxy_cache_path` L2 spill)
//! - `mod.rs`     — `HttpResponseCache` facade: lookup / insert / conditional
//!                  revalidation / single-flight coalescing
//!
//! Semantics follow nginx `proxy_cache`: only GET/HEAD, cacheable statuses
//! without Set-Cookie, Vary-aware keys, `Cache-Control` freshness, stale
//! fallback on upstream failure, and one in-flight fill per key.

pub(crate) mod backend;
pub(crate) mod disk;
pub(crate) mod entry;
pub(crate) mod key;
pub(crate) mod policy;

pub(crate) use backend::{CacheBackend, MemoryCacheBackend};
pub(crate) use entry::{decide_cacheability, CacheDecision, CachedResponse, ResponseMetadata};
pub(crate) use key::{parse_vary_header, CacheKey};
pub(crate) use policy::freshness_for;

use std::{path::PathBuf, sync::Arc, time::Duration};

use std::sync::Mutex;

use sdkwork_webserver_core::ProxyCacheConfig;

use crate::data_plane::metrics::DataPlaneMetrics;

use self::disk::TieredCacheBackend;

/// The proxy response cache. Shared across listeners; all operations are
/// concurrency-safe. Single-flight fills are tracked per key with a bounded
/// waiters map so a cache stampede collapses to one upstream request.
///
/// The facade is asynchronous: disk-backed stores run on the blocking pool
/// so a slow disk never stalls a runtime worker, while the memory-only
/// default takes the direct synchronous path with no spawn overhead.
pub(crate) struct HttpResponseCache {
    store: Arc<dyn CacheBackend>,
    disk_backed: bool,
    maximum_object_bytes: u64,
    default_ttl: Duration,
    stale_ttl: Duration,
    in_flight: Arc<Mutex<std::collections::HashMap<CacheKey, Arc<tokio::sync::Notify>>>>,
    metrics: Arc<DataPlaneMetrics>,
}

impl HttpResponseCache {
    pub(crate) fn new(config: &ProxyCacheConfig, metrics: Arc<DataPlaneMetrics>) -> Arc<Self> {
        let mut disk_backed = false;
        let store: Arc<dyn CacheBackend> = match config
            .disk_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            Some(path) => {
                match TieredCacheBackend::with_limits(
                    config.max_entries,
                    config.max_memory_bytes,
                    PathBuf::from(path),
                ) {
                    Ok(backend) => {
                        tracing::info!(disk_path = %path, "proxy cache disk backend enabled");
                        disk_backed = true;
                        Arc::new(backend)
                    }
                    Err(error) => {
                        tracing::warn!(
                            disk_path = %path,
                            error = %error,
                            "proxy cache disk backend unavailable; falling back to memory"
                        );
                        Arc::new(MemoryCacheBackend::with_limits(
                            config.max_entries,
                            config.max_memory_bytes,
                        ))
                    }
                }
            }
            // Memory-only path uses the L1 backend directly (no tier wrapper).
            None => Arc::new(MemoryCacheBackend::with_limits(
                config.max_entries,
                config.max_memory_bytes,
            )),
        };
        Arc::new(Self {
            store,
            disk_backed,
            maximum_object_bytes: config.max_object_bytes,
            default_ttl: Duration::from_secs(config.default_ttl_seconds),
            stale_ttl: Duration::from_secs(config.stale_ttl_seconds),
            in_flight: Arc::new(Mutex::new(std::collections::HashMap::new())),
            metrics,
        })
    }

    /// Read the backing store, off the runtime worker when disk-backed.
    async fn store_get(&self, key: &CacheKey) -> Option<CachedResponse> {
        if !self.disk_backed {
            return self.store.get(key);
        }
        let store = Arc::clone(&self.store);
        let key = key.clone();
        match tokio::task::spawn_blocking(move || store.get(&key)).await {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!(%error, "proxy cache disk read task failed");
                None
            }
        }
    }

    /// Write the backing store, off the runtime worker when disk-backed.
    async fn store_insert(&self, key: CacheKey, entry: CachedResponse) {
        if !self.disk_backed {
            self.store.insert(key, entry);
            return;
        }
        let store = Arc::clone(&self.store);
        let result = tokio::task::spawn_blocking(move || store.insert(key, entry)).await;
        if result.is_err() {
            tracing::warn!("proxy cache disk write task failed");
        }
    }

    /// Remove an entry, off the runtime worker when disk-backed.
    async fn store_remove(&self, key: &CacheKey) {
        if !self.disk_backed {
            self.store.remove(key);
            return;
        }
        let store = Arc::clone(&self.store);
        let key = key.clone();
        let result = tokio::task::spawn_blocking(move || store.remove(&key)).await;
        if result.is_err() {
            tracing::warn!("proxy cache disk remove task failed");
        }
    }

    /// Look up a fresh entry. Returns `None` when the entry is missing,
    /// expired, or not cacheable.
    ///
    /// Expired entries within the stale window are kept so `lookup_stale`
    /// can serve them when the upstream fails (nginx `proxy_cache_use_stale`);
    /// only entries beyond the stale window are evicted here.
    pub(crate) async fn lookup(&self, key: &CacheKey) -> Option<CachedResponse> {
        tracing::debug!(?key, "proxy cache lookup");
        let entry = self.store_get(key).await?;
        if entry.expired() {
            if !entry.stale_available() {
                self.store_remove(key).await;
            }
            self.metrics.record_proxy_cache_miss();
            return None;
        }
        self.metrics.record_proxy_cache_hit();
        Some(entry)
    }

    /// Look up a stale entry (freshness elapsed but within the stale window).
    pub(crate) async fn lookup_stale(&self, key: &CacheKey) -> Option<CachedResponse> {
        self.store_get(key)
            .await
            .filter(|entry| entry.stale_available())
    }

    /// Insert a response into the cache. `CacheDecision::NoStore` responses
    /// are never stored; oversized responses are rejected.
    pub(crate) async fn insert(
        &self,
        key: CacheKey,
        response: ResponseMetadata,
        body: bytes::Bytes,
        decision: CacheDecision,
    ) {
        if !decision.cacheable || body.len() as u64 > self.maximum_object_bytes {
            return;
        }
        let ttl = decision
            .ttl
            .unwrap_or(self.default_ttl)
            .min(self.default_ttl.saturating_mul(4));
        let entry = CachedResponse::new(response, body, ttl, self.stale_ttl);
        tracing::debug!(?key, ttl_seconds = ttl.as_secs(), "proxy cache store");
        self.store_insert(key, entry).await;
        self.metrics.record_proxy_cache_store();
    }

    /// Register for a single-flight fill of `key`.
    ///
    /// Existing fills are always joinable regardless of table size, so a
    /// saturated table only affects new keys: those return
    /// [`CacheFillReservation::Bypass`] and proxy upstream directly (nginx
    /// does not coalesce fills at all) instead of parking on a waiter no
    /// filler will ever notify.
    ///
    /// The returned [`FillGuard`] releases the slot and wakes waiters when
    /// dropped — including when the filling future is cancelled mid-upstream
    /// call — so a dropped fill can never leak its slot.
    pub(crate) fn begin_fill(&self, key: &CacheKey) -> CacheFillReservation {
        let mut in_flight = self
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(waiter) = in_flight.get(key) {
            return CacheFillReservation::Waiter(Arc::clone(waiter));
        }
        if in_flight.len() >= MAXIMUM_IN_FLIGHT_FILLS {
            return CacheFillReservation::Bypass;
        }
        let notify = Arc::new(tokio::sync::Notify::new());
        in_flight.insert(key.clone(), Arc::clone(&notify));
        CacheFillReservation::Filler(FillGuard {
            in_flight: Arc::clone(&self.in_flight),
            key: key.clone(),
            notify,
        })
    }
}

/// Outcome of registering for a single-flight cache fill.
pub(crate) enum CacheFillReservation {
    /// This caller owns the fill for `key`. The in-flight slot is released
    /// and waiters are notified when the guard drops, even on cancellation.
    Filler(FillGuard),
    /// Another request is filling the key: enable the notified future, then
    /// re-lookup and await.
    Waiter(Arc<tokio::sync::Notify>),
    /// The in-flight table is saturated; proxy upstream without coalescing.
    Bypass,
}

/// RAII owner of one single-flight slot. Dropping the guard removes the key
/// from the in-flight table and notifies every waiter, so cancellation of the
/// filling future cannot strand waiters on a never-notified `Notify`.
pub(crate) struct FillGuard {
    in_flight: Arc<Mutex<std::collections::HashMap<CacheKey, Arc<tokio::sync::Notify>>>>,
    key: CacheKey,
    notify: Arc<tokio::sync::Notify>,
}

impl Drop for FillGuard {
    fn drop(&mut self) {
        let mut in_flight = self
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if in_flight.remove(&self.key).is_some() {
            self.notify.notify_waiters();
        }
    }
}

const MAXIMUM_IN_FLIGHT_FILLS: usize = 4_096;

impl HttpResponseCache {
    pub(crate) fn maximum_object_bytes(&self) -> u64 {
        self.maximum_object_bytes
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::http::HeaderMap;
    use bytes::Bytes;
    use sdkwork_webserver_core::ProxyCacheConfig;

    use super::*;
    use crate::data_plane::cache::entry::{decide_cacheability, ResponseMetadata};
    use crate::data_plane::metrics::DataPlaneMetrics;

    fn config() -> ProxyCacheConfig {
        ProxyCacheConfig {
            enabled: true,
            max_entries: 4,
            max_object_bytes: 1024,
            max_memory_bytes: 4096,
            default_ttl_seconds: 60,
            stale_ttl_seconds: 60,
            disk_path: None,
        }
    }

    fn metrics() -> Arc<DataPlaneMetrics> {
        DataPlaneMetrics::new(Default::default())
    }

    fn key(path: &str) -> CacheKey {
        CacheKey::new("GET", "example.com", path, None, &[], &HeaderMap::new())
    }

    fn response(etag: &str) -> ResponseMetadata {
        ResponseMetadata {
            status: 200,
            headers: vec![
                ("content-type".to_owned(), "text/plain".to_owned()),
                ("etag".to_owned(), etag.to_owned()),
            ],
            vary: Vec::new(),
            fresh_seconds: 60,
        }
    }

    #[tokio::test]
    async fn cache_round_trip_hit_miss_and_eviction() {
        let cache = HttpResponseCache::new(&config(), metrics());
        let k = key("/one");
        assert!(cache.lookup(&k).await.is_none());
        cache
            .insert(
                k.clone(),
                response("\"v1\""),
                Bytes::from_static(b"one"),
                CacheDecision {
                    cacheable: true,
                    ttl: Some(std::time::Duration::from_secs(60)),
                },
            )
            .await;
        let hit = cache.lookup(&k).await.expect("cached");
        assert_eq!(hit.body.as_ref(), b"one");

        // Fill more entries than capacity; LRU evicts the oldest.
        for i in 0..8 {
            let k = key(&format!("/evict-{i}"));
            cache
                .insert(
                    k,
                    response("\"v\""),
                    Bytes::from(vec![b'x'; 10]),
                    CacheDecision {
                        cacheable: true,
                        ttl: None,
                    },
                )
                .await;
        }
        assert!(cache.lookup(&k).await.is_none(), "LRU must evict /one");
    }

    #[tokio::test]
    async fn single_flight_coalesces_concurrent_fills() {
        let cache = HttpResponseCache::new(&config(), metrics());
        let k = key("/hot");
        let filler = match cache.begin_fill(&k) {
            CacheFillReservation::Filler(guard) => guard,
            _ => panic!("first caller must own the fill"),
        };
        let waiter = match cache.begin_fill(&k) {
            CacheFillReservation::Waiter(waiter) => waiter,
            _ => panic!("second caller must wait"),
        };
        // Enable before releasing the filler so the wakeup cannot be lost
        // between the notify and the waiter's first poll.
        let mut notified = std::pin::pin!(waiter.notified());
        notified.as_mut().enable();
        drop(filler);
        tokio::time::timeout(std::time::Duration::from_secs(1), notified)
            .await
            .expect("waiter must be woken by the fill guard");
        // The slot was released: a fresh fill can start again.
        assert!(matches!(
            cache.begin_fill(&k),
            CacheFillReservation::Filler(_)
        ));
    }

    #[tokio::test]
    async fn cancelled_fill_releases_its_slot() {
        let cache = HttpResponseCache::new(&config(), metrics());
        let k = key("/cancelled");
        let task_cache = Arc::clone(&cache);
        let task_key = k.clone();
        let filler = tokio::spawn(async move {
            let _guard = match task_cache.begin_fill(&task_key) {
                CacheFillReservation::Filler(guard) => guard,
                _ => panic!("first caller must own the fill"),
            };
            // Hold the slot until the task is aborted mid-"upstream call".
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        });
        // Wait until the fill is registered before cancelling it.
        while !matches!(cache.begin_fill(&k), CacheFillReservation::Waiter(_)) {
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        filler.abort();
        assert!(filler.await.is_err(), "the filler task was aborted");
        assert!(
            matches!(cache.begin_fill(&k), CacheFillReservation::Filler(_)),
            "a cancelled fill must release its slot instead of leaking it"
        );
    }

    #[test]
    fn saturated_table_bypasses_instead_of_dead_waiter() {
        let cache = HttpResponseCache::new(&config(), metrics());
        // The guards must stay alive or each slot would be released on drop.
        let guards: Vec<FillGuard> = (0..MAXIMUM_IN_FLIGHT_FILLS)
            .map(
                |i| match cache.begin_fill(&key(&format!("/saturation-{i}"))) {
                    CacheFillReservation::Filler(guard) => guard,
                    _ => panic!("each fresh key must own a fill"),
                },
            )
            .collect();
        // A new key past capacity bypasses coalescing instead of receiving a
        // waiter nobody will ever notify.
        assert!(matches!(
            cache.begin_fill(&key("/overflow")),
            CacheFillReservation::Bypass
        ));
        // Already-filling keys stay joinable regardless of table size.
        assert!(matches!(
            cache.begin_fill(&key("/saturation-0")),
            CacheFillReservation::Waiter(_)
        ));
        drop(guards);
        assert!(matches!(
            cache.begin_fill(&key("/overflow")),
            CacheFillReservation::Filler(_)
        ));
    }

    #[test]
    fn cacheability_respects_status_set_cookie_and_no_store() {
        let mut headers = HeaderMap::new();
        let decision = decide_cacheability(200, &headers, Some(60));
        assert!(decision.cacheable);
        headers.insert("set-cookie", "session=1".parse().unwrap());
        assert!(!decide_cacheability(200, &headers, Some(60)).cacheable);
        headers.remove("set-cookie");
        headers.insert("cache-control", "no-store".parse().unwrap());
        assert!(!decide_cacheability(200, &headers, Some(60)).cacheable);
        assert!(!decide_cacheability(404, &HeaderMap::new(), Some(60)).cacheable);
    }

    #[test]
    fn vary_parsing_and_keys_are_header_sensitive() {
        let vary = parse_vary_header("Accept-Encoding, User-Agent");
        assert_eq!(vary, vec!["Accept-Encoding", "User-Agent"]);
        let mut headers = HeaderMap::new();
        headers.insert("accept-encoding", "gzip".parse().unwrap());
        let gzip = CacheKey::new("GET", "example.com", "/", None, &vary, &headers);
        let identity = CacheKey::new("GET", "example.com", "/", None, &vary, &HeaderMap::new());
        assert_ne!(gzip, identity);
        assert_eq!(
            gzip.vary_input.get("accept-encoding").map(String::as_str),
            Some("gzip")
        );
    }
}
