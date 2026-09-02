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
pub(crate) struct HttpResponseCache {
    store: Arc<dyn CacheBackend>,
    maximum_object_bytes: u64,
    default_ttl: Duration,
    stale_ttl: Duration,
    in_flight: Mutex<std::collections::HashMap<CacheKey, Arc<tokio::sync::Notify>>>,
    metrics: Arc<DataPlaneMetrics>,
}

impl HttpResponseCache {
    pub(crate) fn new(config: &ProxyCacheConfig, metrics: Arc<DataPlaneMetrics>) -> Arc<Self> {
        let store: Arc<dyn CacheBackend> = match config
            .disk_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            Some(path) => {
                match TieredCacheBackend::with_disk(config.max_entries, PathBuf::from(path)) {
                    Ok(backend) => {
                        tracing::info!(disk_path = %path, "proxy cache disk backend enabled");
                        Arc::new(backend)
                    }
                    Err(error) => {
                        tracing::warn!(
                            disk_path = %path,
                            error = %error,
                            "proxy cache disk backend unavailable; falling back to memory"
                        );
                        Arc::new(MemoryCacheBackend::new(config.max_entries))
                    }
                }
            }
            // Memory-only path uses the L1 backend directly (no tier wrapper).
            None => Arc::new(MemoryCacheBackend::new(config.max_entries)),
        };
        Arc::new(Self {
            store,
            maximum_object_bytes: config.max_object_bytes,
            default_ttl: Duration::from_secs(config.default_ttl_seconds),
            stale_ttl: Duration::from_secs(config.stale_ttl_seconds),
            in_flight: Mutex::new(std::collections::HashMap::new()),
            metrics,
        })
    }

    /// Look up a fresh entry. Returns `None` when the entry is missing,
    /// expired, or not cacheable. Synchronous: the operation never awaits.
    ///
    /// Expired entries within the stale window are kept so `lookup_stale`
    /// can serve them when the upstream fails (nginx `proxy_cache_use_stale`);
    /// only entries beyond the stale window are evicted here.
    pub(crate) fn lookup(&self, key: &CacheKey) -> Option<CachedResponse> {
        tracing::debug!(?key, "proxy cache lookup");
        let entry = self.store.get(key)?;
        if entry.expired() {
            if !entry.stale_available() {
                self.store.remove(key);
            }
            self.metrics.record_proxy_cache_miss();
            return None;
        }
        self.metrics.record_proxy_cache_hit();
        Some(entry)
    }

    /// Look up a stale entry (freshness elapsed but within the stale window).
    pub(crate) fn lookup_stale(&self, key: &CacheKey) -> Option<CachedResponse> {
        self.store.get(key).filter(|entry| entry.stale_available())
    }

    /// Insert a response into the cache. `CacheDecision::NoStore` responses
    /// are never stored; oversized responses are rejected.
    pub(crate) fn insert(
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
        self.store.insert(key, entry);
        self.metrics.record_proxy_cache_store();
    }

    /// Register a single-flight fill for `key`. Returns `Some(waiter)` when
    /// another request is already filling the key; the caller awaits the
    /// waiter and re-looks-up. Returns `None` when this caller is the fill.
    pub(crate) fn begin_fill(&self, key: &CacheKey) -> Option<Arc<tokio::sync::Notify>> {
        let mut in_flight = self
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if in_flight.len() >= MAXIMUM_IN_FLIGHT_FILLS {
            return Some(Arc::new(tokio::sync::Notify::new()));
        }
        if let Some(waiter) = in_flight.get(key) {
            return Some(Arc::clone(waiter));
        }
        let notify = Arc::new(tokio::sync::Notify::new());
        in_flight.insert(key.clone(), Arc::clone(&notify));
        None
    }

    /// Complete a single-flight fill, notifying waiters.
    pub(crate) fn finish_fill(&self, key: &CacheKey) {
        if let Some(notify) = self
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(key)
        {
            notify.notify_waiters();
        }
    }
}

const MAXIMUM_IN_FLIGHT_FILLS: usize = 4_096;

impl HttpResponseCache {
    pub(crate) fn maximum_object_bytes(&self) -> u64 {
        self.maximum_object_bytes
    }
}

/// Cache hit outcome for the proxy path.
pub(crate) enum ProxyCacheLookup {
    /// Fresh entry that can be served directly (or as a 304 when the client
    /// sent a matching conditional header).
    Hit(CachedResponse),
    /// No usable entry: fill from upstream.
    Miss,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::http::HeaderMap;
    use bytes::Bytes;
    use sdkwork_webserver_core::ProxyCacheConfig;
    use tokio::sync::Notify;

    use super::*;
    use crate::data_plane::cache::entry::{decide_cacheability, ResponseMetadata};
    use crate::data_plane::metrics::DataPlaneMetrics;

    fn config() -> ProxyCacheConfig {
        ProxyCacheConfig {
            enabled: true,
            max_entries: 4,
            max_object_bytes: 1024,
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
        assert!(cache.lookup(&k).is_none());
        cache.insert(
            k.clone(),
            response("\"v1\""),
            Bytes::from_static(b"one"),
            CacheDecision {
                cacheable: true,
                ttl: Some(std::time::Duration::from_secs(60)),
            },
        );
        let hit = cache.lookup(&k).expect("cached");
        assert_eq!(hit.body.as_ref(), b"one");

        // Fill more entries than capacity; LRU evicts the oldest.
        for i in 0..8 {
            let k = key(&format!("/evict-{i}"));
            cache.insert(
                k,
                response("\"v\""),
                Bytes::from(vec![b'x'; 10]),
                CacheDecision {
                    cacheable: true,
                    ttl: None,
                },
            );
        }
        assert!(cache.lookup(&k).is_none(), "LRU must evict /one");
    }

    #[tokio::test]
    async fn single_flight_coalesces_concurrent_fills() {
        let cache = HttpResponseCache::new(&config(), metrics());
        let k = key("/hot");
        assert!(cache.begin_fill(&k).is_none(), "first caller fills");
        let waiter = cache.begin_fill(&k).expect("second caller waits");
        let notify = Arc::new(Notify::new());
        let waiter_task = tokio::spawn(async move {
            notify.notified().await;
        });
        cache.finish_fill(&k);
        // The registered waiter was notified; a fresh fill can start again.
        assert!(cache.begin_fill(&k).is_none());
        let _ = waiter;
        waiter_task.abort();
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
