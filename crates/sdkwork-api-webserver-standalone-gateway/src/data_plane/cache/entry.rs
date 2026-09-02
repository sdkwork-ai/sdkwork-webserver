//! Cached response entries with freshness metadata.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// Response header metadata retained in the cache entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ResponseMetadata {
    pub status: u16,
    /// Content-Type, ETag, Last-Modified, and any Vary headers needed to
    /// serve the cached body and answer conditional requests.
    pub headers: Vec<(String, String)>,
    /// Effective `Vary` field list of the cached representation.
    pub vary: Vec<String>,
    /// Seconds the entry remains fresh after insertion (from
    /// Cache-Control/Expires, clamped).
    pub fresh_seconds: u64,
}

/// A fully cached response: headers plus body.
#[derive(Clone, Debug)]
pub(crate) struct CachedResponse {
    pub metadata: ResponseMetadata,
    pub body: Bytes,
    inserted_at: Instant,
    fresh_until: Instant,
    stale_until: Instant,
    /// Wall-clock anchors for durable backends (disk / shared).
    inserted_unix_ms: u64,
    fresh_until_unix_ms: u64,
    stale_until_unix_ms: u64,
}

/// Durable framing for disk / shared cache backends.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct DurableCachedResponse {
    pub metadata: ResponseMetadata,
    pub body: Vec<u8>,
    pub inserted_unix_ms: u64,
    pub fresh_until_unix_ms: u64,
    #[serde(default)]
    pub stale_until_unix_ms: u64,
}

impl CachedResponse {
    pub fn new(
        metadata: ResponseMetadata,
        body: Bytes,
        ttl: Duration,
        stale_ttl: Duration,
    ) -> Self {
        let inserted_at = Instant::now();
        let fresh_until = inserted_at + ttl;
        let stale_until = fresh_until + stale_ttl;
        let now_ms = unix_now_ms();
        let fresh_ms = ttl.as_millis() as u64;
        let stale_ms = stale_ttl.as_millis() as u64;
        Self {
            metadata,
            body,
            inserted_at,
            fresh_until,
            stale_until,
            inserted_unix_ms: now_ms,
            fresh_until_unix_ms: now_ms.saturating_add(fresh_ms),
            stale_until_unix_ms: now_ms.saturating_add(fresh_ms).saturating_add(stale_ms),
        }
    }

    pub fn expired(&self) -> bool {
        Instant::now() >= self.fresh_until || unix_now_ms() >= self.fresh_until_unix_ms
    }

    /// Stale entries remain usable within the configured stale window
    /// (nginx `proxy_cache_use_stale` on upstream failure).
    pub fn stale_available(&self) -> bool {
        let now = Instant::now();
        let now_ms = unix_now_ms();
        (now >= self.fresh_until && now < self.stale_until)
            || (now_ms >= self.fresh_until_unix_ms && now_ms < self.stale_until_unix_ms)
    }

    pub fn age_seconds(&self) -> u64 {
        self.inserted_at.elapsed().as_secs()
    }

    pub fn to_durable(&self) -> DurableCachedResponse {
        DurableCachedResponse {
            metadata: self.metadata.clone(),
            body: self.body.to_vec(),
            inserted_unix_ms: self.inserted_unix_ms,
            fresh_until_unix_ms: self.fresh_until_unix_ms,
            stale_until_unix_ms: self.stale_until_unix_ms,
        }
    }

    pub fn from_durable(durable: DurableCachedResponse) -> Self {
        let now_ms = unix_now_ms();
        let remaining_ms = durable.fresh_until_unix_ms.saturating_sub(now_ms);
        let age_ms = now_ms.saturating_sub(durable.inserted_unix_ms);
        let stale_remaining_ms = if durable.stale_until_unix_ms == 0 {
            // Legacy disk entries without stale anchor: allow one TTL of stale.
            remaining_ms.saturating_add(
                durable
                    .fresh_until_unix_ms
                    .saturating_sub(durable.inserted_unix_ms),
            )
        } else {
            durable.stale_until_unix_ms.saturating_sub(now_ms)
        };
        let inserted_at = Instant::now()
            .checked_sub(Duration::from_millis(age_ms))
            .unwrap_or_else(Instant::now);
        let fresh_until = Instant::now() + Duration::from_millis(remaining_ms);
        let stale_until =
            Instant::now() + Duration::from_millis(stale_remaining_ms.max(remaining_ms));
        Self {
            metadata: durable.metadata,
            body: Bytes::from(durable.body),
            inserted_at,
            fresh_until,
            stale_until,
            inserted_unix_ms: durable.inserted_unix_ms,
            fresh_until_unix_ms: durable.fresh_until_unix_ms,
            stale_until_unix_ms: if durable.stale_until_unix_ms == 0 {
                durable.fresh_until_unix_ms.saturating_add(
                    durable
                        .fresh_until_unix_ms
                        .saturating_sub(durable.inserted_unix_ms),
                )
            } else {
                durable.stale_until_unix_ms
            },
        }
    }
}

fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// What to do with an upstream response.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CacheDecision {
    pub cacheable: bool,
    /// Explicit freshness from the response, when declared.
    pub ttl: Option<Duration>,
}

/// Decide cacheability of an upstream response.
/// Cacheable: cacheable status, no Set-Cookie, no `Cache-Control: no-store`,
/// no `private` for shared caching.
pub(crate) fn decide_cacheability(
    status: u16,
    headers: &axum::http::HeaderMap,
    fresh_seconds: Option<u64>,
) -> CacheDecision {
    let cacheable_status = matches!(status, 200 | 203 | 301 | 302 | 307 | 308);
    let has_set_cookie = headers.get_all("set-cookie").iter().next().is_some();
    let cache_control = headers
        .get("cache-control")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let no_store = cache_control.split(',').any(|token| {
        let token = token.trim();
        token == "no-store" || token == "private"
    });
    let cacheable = cacheable_status && !has_set_cookie && !no_store;
    CacheDecision {
        cacheable,
        ttl: fresh_seconds.map(Duration::from_secs),
    }
}
