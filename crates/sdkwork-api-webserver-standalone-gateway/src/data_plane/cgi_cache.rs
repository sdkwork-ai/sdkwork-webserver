//! CGI-family response caching on the shared HTTP response cache store.
//!
//! nginx `fastcgi_cache` / `uwsgi_cache` / `scgi_cache` reuse nginx's cache
//! subsystem with module-specific defaults. The runtime maps the resolved
//! [`CgiCacheConfig`] onto the shared [`HttpResponseCache`] (the same store and
//! disk tier as `proxy_cache_path`), with a bounded implementation of:
//!
//! - cache keys from the configured template (default
//!   `$scheme$request_method$host$request_uri`), Vary-aware refinement, and
//!   HEAD→GET key conversion (`*_cache_convert_head`);
//! - `*_cache_valid` freshness by status (defaults 200/302 = 10 m, 301 = 1 h);
//! - `*_cache_bypass` (skip reading) and `*_no_cache` (skip storing);
//! - `*_cache_methods` eligibility and `*_cache_min_uses`;
//! - `*_cache_lock` single-flight with `*_cache_lock_timeout`;
//! - `*_cache_use_stale` on upstream failure and while another fill is in
//!   flight (`updating`), with `*_cache_background_update` serving a stale
//!   entry instead of waiting for an in-progress fill;
//! - `*_cache_revalidate` conditional refresh of an expired entry with its
//!   stored validators.
//!
//! Bounded differences (documented, not faked):
//! - one process store backs every zone; nginx sizes each zone separately;
//! - `*_cache_lock_age` is accepted but the store cannot preempt an
//!   in-progress fill (a second fill starts only after `lock_timeout`);
//! - `*_cache_max_range_offset` is accepted; Range requests always bypass the
//!   cache, which is the conservative form of the limit;
//! - `X-Accel-Expires` is not consulted; freshness comes from the compatible
//!   `Cache-Control`/`Expires` headers when present, otherwise from
//!   `*_cache_valid` (nginx precedence plus `X-Accel-Expires`).

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    body::Body,
    http::{header::HeaderName, HeaderMap, HeaderValue, Method, Response, StatusCode},
};
use http_body::Body as _;
use sdkwork_webserver_core::CgiCacheConfig;

use super::{
    cache::{
        freshness_for, CacheDecision, CacheFillReservation, CacheKey, CachedResponse, FillGuard,
        HttpResponseCache, ResponseMetadata,
    },
    cgi_upstream::CgiRequestFacts,
    runtime::RuntimeGeneration,
};

/// Bounded per-key request counter table used by `*_cache_min_uses`.
#[derive(Default)]
pub(crate) struct CgiMinUsesCounters {
    entries: Mutex<HashMap<String, u32>>,
}

impl CgiMinUsesCounters {
    /// Record one request for `key` and report whether the entry may be
    /// stored (the counter reached `min_uses`).
    pub(crate) fn observe(&self, key: &str, min_uses: u32) -> bool {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if entries.len() >= 65_536 && !entries.contains_key(key) {
            entries.clear();
        }
        let count = entries.entry(key.to_owned()).or_insert(0);
        *count = count.saturating_add(1);
        *count >= min_uses
    }
}

pub(crate) enum CgiCacheLookup {
    /// No resolved policy (or cache store unavailable): call the adapter
    /// directly.
    Disabled,
    /// Fresh cache hit; the response may be the client method's answer.
    Hit(Response<Body>),
    /// A miss that must call the adapter and then hand the response to
    /// [`CgiCachePlan::finish`]. Boxed because the plan carries the store
    /// handle, policy and stale entry while `Disabled`/`Hit` are small.
    Miss(Box<CgiCachePlan>),
}

pub(crate) struct CgiCachePlan {
    cache: Arc<HttpResponseCache>,
    policy: CgiCacheConfig,
    key: CacheKey,
    /// Held (never read) so the single-flight slot lives until the fill
    /// response is stored or dropped.
    _fill_guard: Option<FillGuard>,
    store_allowed: bool,
    conditional_headers: Option<HeaderMap>,
    stale: Option<CachedResponse>,
    client_method: Method,
}

impl CgiCachePlan {
    /// Method the upstream request must use: a HEAD cache miss is filled with
    /// GET when `*_cache_convert_head` is on (nginx `ngx_http_upstream_cache`).
    pub(crate) fn upstream_method(&self) -> Method {
        if self.policy.convert_head && self.client_method == Method::HEAD {
            Method::GET
        } else {
            self.client_method.clone()
        }
    }

    /// Extra request headers for conditional revalidation (`If-None-Match` /
    /// `If-Modified-Since`) merged over the client request headers.
    pub(crate) fn upstream_headers(&self) -> Option<&HeaderMap> {
        self.conditional_headers.as_ref()
    }

    pub(crate) async fn finish(mut self, response: Response<Body>) -> Response<Body> {
        self.finish_inner(response)
            .await
            .unwrap_or_else(|| Response::new(Body::empty()))
    }

    async fn finish_inner(&mut self, response: Response<Body>) -> Option<Response<Body>> {
        let status = response.status();
        // `*_cache_revalidate`: the upstream confirmed the cached
        // representation is unchanged; refresh its freshness and serve it.
        if self.conditional_headers.is_some() && status == StatusCode::NOT_MODIFIED {
            if let Some(stale) = self.stale.clone() {
                let ttl = ttl_for_status(&self.policy, stale.metadata.status).unwrap_or(600);
                let decision = CacheDecision {
                    cacheable: true,
                    ttl: Some(Duration::from_secs(ttl)),
                };
                self.cache
                    .insert(
                        self.key.clone(),
                        stale.metadata.clone(),
                        stale.body.clone(),
                        decision,
                    )
                    .await;
                return Some(cached_cgi_response(stale, &self.client_method));
            }
        }
        // `*_cache_use_stale`: serve a stale entry for the configured upstream
        // failure conditions instead of the failure response.
        if let Some(stale) = self.stale.as_ref() {
            if stale_condition_matches(&self.policy.use_stale, status) {
                return Some(cached_cgi_response(stale.clone(), &self.client_method));
            }
        }
        let head_client = self.client_method == Method::HEAD;
        let convert_client_head = head_client && self.policy.convert_head;
        if !self.store_allowed {
            return Some(suppress_body_for_head(response, convert_client_head));
        }
        // The adapter always buffers the bounded upstream body, so the exact
        // size is known before consuming it; oversized bodies pass through
        // uncached (the store's object limit applies).
        let Some(body_size) = response.body().size_hint().exact() else {
            return Some(suppress_body_for_head(response, convert_client_head));
        };
        if body_size > self.cache.maximum_object_bytes() {
            return Some(suppress_body_for_head(response, convert_client_head));
        }
        // Explicit upstream freshness (Cache-Control/Expires) wins over the
        // directive rule, matching nginx precedence for those headers.
        let freshness = freshness_for(
            response
                .headers()
                .get("cache-control")
                .and_then(|value| value.to_str().ok()),
            response
                .headers()
                .get("expires")
                .and_then(|value| value.to_str().ok()),
            response
                .headers()
                .get("date")
                .and_then(|value| value.to_str().ok()),
        );
        let header_freshness = freshness.fresh_seconds.filter(|seconds| *seconds > 0);
        // nginx caches only when a caching time exists: from `*_cache_valid`
        // for the status, or from an explicit upstream freshness header.
        let Some(ttl) = (match ttl_for_status(&self.policy, status.as_u16()) {
            Some(rule_ttl) => Some(header_freshness.unwrap_or(rule_ttl)),
            None => header_freshness.filter(|_| header_cacheable_status(status.as_u16())),
        }) else {
            return Some(suppress_body_for_head(response, convert_client_head));
        };
        if !shared_cacheable_response(response.headers()) {
            return Some(suppress_body_for_head(response, convert_client_head));
        }
        let (parts, body) = response.into_parts();
        let body_bytes = match http_body_util::BodyExt::collect(body).await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => {
                return Some(suppress_body_for_head(
                    Response::from_parts(parts, Body::empty()),
                    convert_client_head,
                ))
            }
        };
        let vary = parts
            .headers
            .get("vary")
            .and_then(|value| value.to_str().ok())
            .map(super::cache::parse_vary_header)
            .unwrap_or_default();
        let headers = parts
            .headers
            .iter()
            .filter(|(name, _)| !is_hop_by_hop_header(name.as_str()))
            .map(|(name, value)| {
                (
                    name.as_str().to_owned(),
                    value.to_str().unwrap_or_default().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        let metadata = ResponseMetadata {
            status: status.as_u16(),
            headers,
            vary,
            fresh_seconds: ttl,
        };
        let store_decision = CacheDecision {
            cacheable: true,
            ttl: Some(Duration::from_secs(ttl)),
        };
        self.cache
            .insert(
                self.key.clone(),
                metadata,
                body_bytes.clone(),
                store_decision,
            )
            .await;
        let rebuilt = Response::from_parts(parts, Body::from(body_bytes));
        Some(suppress_body_for_head(rebuilt, convert_client_head))
    }
}

/// Resolve the cache lookup for one CGI request.
pub(crate) async fn begin(
    generation: &Arc<RuntimeGeneration>,
    policy: Option<&CgiCacheConfig>,
    facts: &CgiRequestFacts<'_>,
    method: &Method,
    headers: &HeaderMap,
) -> CgiCacheLookup {
    let Some(policy) = policy else {
        return CgiCacheLookup::Disabled;
    };
    let Some(cache) = generation.proxy_cache.clone() else {
        return CgiCacheLookup::Disabled;
    };
    let head = method == Method::HEAD;
    let convert_head = head && policy.convert_head;
    let cache_method = if convert_head {
        Method::GET
    } else {
        method.clone()
    };
    let method_eligible = policy
        .methods
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(cache_method.as_str()));
    if !method_eligible {
        return CgiCacheLookup::Disabled;
    }
    if headers.get("authorization").is_some()
        || headers.get("connection").is_some_and(|value| {
            value
                .to_str()
                .map(|value| value.to_ascii_lowercase().contains("upgrade"))
                .unwrap_or(false)
        })
    {
        return CgiCacheLookup::Disabled;
    }
    // nginx `*_cache_max_range_offset`: only ranges whose first byte offset is
    // above the configured value bypass the cache; every other range is cached
    // like a normal GET (and served from the full cached body).
    if let Some(limit) = policy.max_range_offset {
        if let Some(range) = headers.get("range") {
            let start = range_start_offset(range.to_str().unwrap_or_default());
            if start.is_none_or(|start| start > limit) {
                return CgiCacheLookup::Disabled;
            }
        }
    }
    let truthy = |variables: &[String]| {
        variables.iter().any(|variable| {
            let name = variable.trim_start_matches('$');
            facts
                .variable(name)
                .map(|value| !value.is_empty() && value != "0")
                .unwrap_or(false)
        })
    };
    let bypass = truthy(&policy.bypass);
    let no_cache = truthy(&policy.no_cache);
    let base_key = if convert_head {
        let get = Method::GET;
        build_cache_key(&facts.with_overrides(&get, headers), &policy.key, headers)
    } else {
        build_cache_key(facts, &policy.key, headers)
    };
    let mut lookup_hit = None;
    if !bypass {
        if let Some(hit) = cache.lookup(&base_key).await {
            if hit.metadata.vary.is_empty() {
                return CgiCacheLookup::Hit(cached_cgi_response(hit, method));
            }
            let refined = refine_key(&base_key, &hit.metadata.vary, headers);
            if let Some(refined_hit) = cache.lookup(&refined).await {
                return CgiCacheLookup::Hit(cached_cgi_response(refined_hit, method));
            }
        }
        lookup_hit = Some(());
    }
    let _ = lookup_hit;
    // Expired-but-still-stale entry powers revalidation, `use_stale` and
    // `background_update`.
    let stale = cache.lookup_stale(&base_key).await;
    // nginx does not store a bypassed response: the next non-bypassed request
    // is served from the previously cached entry.
    let store_allowed = !no_cache
        && !bypass
        && generation
            .cgi_cache_min_uses
            .observe(&base_key.stable_hash_hex(), policy.min_uses);
    let mut fill_guard = None;
    if policy.lock && !bypass {
        match cache.begin_fill(&base_key) {
            CacheFillReservation::Filler(guard) => fill_guard = Some(guard),
            CacheFillReservation::Waiter(waiter) => {
                let mut notified = std::pin::pin!(waiter.notified());
                notified.as_mut().enable();
                // `*_cache_background_update` / `use_stale updating`: serve
                // the stale representation instead of waiting for the fill.
                if let Some(stale_entry) = stale.clone() {
                    let updating = policy
                        .use_stale
                        .iter()
                        .any(|condition| condition == "updating");
                    if policy.background_update || updating {
                        return CgiCacheLookup::Hit(cached_cgi_response(stale_entry, method));
                    }
                }
                if tokio::time::timeout(
                    Duration::from_millis(policy.lock_timeout_ms.max(1)),
                    notified,
                )
                .await
                .is_ok()
                {
                    if let Some(hit) = cache.lookup(&base_key).await {
                        return CgiCacheLookup::Hit(cached_cgi_response(hit, method));
                    }
                }
            }
            CacheFillReservation::Bypass => {}
        }
    }
    // Conditional revalidation headers from the stored validators.
    let conditional_headers = if policy.revalidate && !bypass {
        stale.as_ref().and_then(|entry| {
            let mut headers = headers.clone();
            let mut added = false;
            for (name, value) in &entry.metadata.headers {
                if name.eq_ignore_ascii_case("etag") {
                    if let Ok(value) = HeaderValue::from_str(value) {
                        headers.insert(HeaderName::from_static("if-none-match"), value);
                        added = true;
                    }
                } else if name.eq_ignore_ascii_case("last-modified") {
                    if let Ok(value) = HeaderValue::from_str(value) {
                        headers.insert(HeaderName::from_static("if-modified-since"), value);
                        added = true;
                    }
                }
            }
            added.then_some(headers)
        })
    } else {
        None
    };
    CgiCacheLookup::Miss(Box::new(CgiCachePlan {
        cache,
        policy: policy.clone(),
        key: base_key,
        _fill_guard: fill_guard,
        store_allowed,
        conditional_headers,
        stale: if bypass { None } else { stale },
        client_method: method.clone(),
    }))
}

fn build_cache_key(facts: &CgiRequestFacts<'_>, template: &str, headers: &HeaderMap) -> CacheKey {
    let expanded = facts.expand(template);
    // The expanded template *is* the nginx cache key; the store's key struct
    // is used as a typed container, so the expanded value becomes its
    // identity field.
    CacheKey::new("", &expanded, "", None, &[], headers)
}

fn refine_key(base: &CacheKey, vary: &[String], headers: &HeaderMap) -> CacheKey {
    CacheKey::new("", &base.host, "", None, vary, headers)
}

/// Statuses nginx stores when the caching time comes from an upstream header
/// (`Cache-Control`/`Expires`/`X-Accel-Expires`) rather than `*_cache_valid`.
fn header_cacheable_status(status: u16) -> bool {
    matches!(
        status,
        200 | 203 | 204 | 206 | 300 | 301 | 302 | 303 | 304 | 307 | 308
    )
}

fn ttl_for_status(policy: &CgiCacheConfig, status: u16) -> Option<u64> {
    for rule in &policy.valid {
        if rule.statuses.is_empty() {
            if matches!(status, 200 | 302) {
                return Some(rule.seconds);
            }
            continue;
        }
        if rule.statuses.contains(&status) {
            return Some(rule.seconds);
        }
    }
    None
}

fn stale_condition_matches(conditions: &[String], status: StatusCode) -> bool {
    let code = status.as_u16();
    conditions.iter().any(|condition| match condition.as_str() {
        "off" => false,
        "error" => code == 502 || code == 504,
        "timeout" => code == 504,
        "invalid_header" => code == 502,
        "updating" => false,
        other => other
            .strip_prefix("http_")
            .and_then(|code| code.parse::<u16>().ok())
            .is_some_and(|condition_code| condition_code == code),
    })
}

/// A response may enter the shared cache unless it forbids it
/// (`Cache-Control: no-store`/`private`) or carries a `Set-Cookie`; nginx's
/// `*_cache_valid` status/time rules decide the freshness of everything else.
fn shared_cacheable_response(headers: &HeaderMap) -> bool {
    let cache_control = headers
        .get("cache-control")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    !cache_control.split(',').any(|token| {
        let token = token.trim();
        token == "no-store" || token == "private"
    }) && headers.get_all("set-cookie").iter().next().is_none()
}

fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection" | "keep-alive" | "transfer-encoding" | "trailer" | "upgrade"
    )
}

fn suppress_body_for_head(mut response: Response<Body>, suppress: bool) -> Response<Body> {
    if suppress {
        *response.body_mut() = Body::empty();
    }
    response
}

/// Serve a cached entry for one client request (HEAD keeps the stored headers
/// and drops the body, like nginx `*_cache_convert_head`).
fn cached_cgi_response(entry: CachedResponse, client_method: &Method) -> Response<Body> {
    let mut builder = Response::builder().status(entry.metadata.status);
    for (name, value) in &entry.metadata.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    let body = if *client_method == Method::HEAD {
        Body::empty()
    } else {
        Body::from(entry.body)
    };
    builder
        .body(body)
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

/// First-byte offset of a `Range: bytes=<start>-...` request; `None` for
/// malformed/suffix ranges (which conservatively bypass the cache).
fn range_start_offset(value: &str) -> Option<u64> {
    let (unit, ranges) = value.split_once('=')?;
    if !unit.trim().eq_ignore_ascii_case("bytes") {
        return None;
    }
    let first = ranges.split(',').next()?.trim();
    let start = first.split_once('-')?.0.trim();
    if start.is_empty() {
        return None;
    }
    start.parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_webserver_core::CgiCacheValidRule;

    fn policy(valid: Vec<CgiCacheValidRule>) -> CgiCacheConfig {
        CgiCacheConfig {
            zone: "shared".to_owned(),
            key: "$scheme$request_method$host$request_uri".to_owned(),
            valid,
            bypass: Vec::new(),
            no_cache: Vec::new(),
            revalidate: false,
            use_stale: Vec::new(),
            methods: vec!["GET".to_owned(), "HEAD".to_owned()],
            min_uses: 1,
            lock: true,
            lock_timeout_ms: 5_000,
            lock_age_ms: 5_000,
            background_update: false,
            convert_head: true,
            max_range_offset: None,
        }
    }

    #[test]
    fn min_uses_counter_stores_only_after_the_configured_number_of_uses() {
        let counters = CgiMinUsesCounters::default();
        assert!(!counters.observe("key-a", 2));
        assert!(counters.observe("key-a", 2));
        assert!(counters.observe("key-a", 2));
        // A different key has its own count.
        assert!(!counters.observe("key-b", 2));
        assert!(counters.observe("key-complete", 1));
    }

    #[test]
    fn valid_rules_are_the_only_default_freshness_source() {
        let empty = policy(Vec::new());
        assert_eq!(ttl_for_status(&empty, 200), None);
        let rules = policy(vec![
            CgiCacheValidRule {
                statuses: vec![200, 302],
                seconds: 600,
            },
            CgiCacheValidRule {
                statuses: vec![301],
                seconds: 3_600,
            },
        ]);
        assert_eq!(ttl_for_status(&rules, 200), Some(600));
        assert_eq!(ttl_for_status(&rules, 302), Some(600));
        assert_eq!(ttl_for_status(&rules, 301), Some(3_600));
        assert_eq!(ttl_for_status(&rules, 404), None);
        // An empty status list is the default 200/302 rule.
        let defaulted = policy(vec![CgiCacheValidRule {
            statuses: Vec::new(),
            seconds: 30,
        }]);
        assert_eq!(ttl_for_status(&defaulted, 200), Some(30));
        assert_eq!(ttl_for_status(&defaulted, 301), None);
    }

    #[test]
    fn range_offset_parsing_selects_the_configured_limit() {
        assert_eq!(range_start_offset("bytes=0-4"), Some(0));
        assert_eq!(range_start_offset("bytes=100-104"), Some(100));
        assert_eq!(range_start_offset("bytes=100-"), Some(100));
        // Suffix ranges and malformed headers conservatively bypass.
        assert_eq!(range_start_offset("bytes=-5"), None);
        assert_eq!(range_start_offset("items=0-4"), None);
        assert_eq!(range_start_offset("bytes=abc"), None);
    }

    #[test]
    fn stale_conditions_map_to_upstream_failures() {
        let conditions = vec!["error".to_owned(), "http_500".to_owned()];
        assert!(stale_condition_matches(
            &conditions,
            StatusCode::BAD_GATEWAY
        ));
        assert!(stale_condition_matches(
            &conditions,
            StatusCode::INTERNAL_SERVER_ERROR
        ));
        assert!(!stale_condition_matches(&conditions, StatusCode::NOT_FOUND));
        let timeout = vec!["timeout".to_owned()];
        assert!(stale_condition_matches(
            &timeout,
            StatusCode::GATEWAY_TIMEOUT
        ));
        assert!(!stale_condition_matches(&timeout, StatusCode::BAD_GATEWAY));
        // `updating` is handled while another fill is in flight, not from a
        // failure status.
        let updating = vec!["updating".to_owned()];
        assert!(!stale_condition_matches(
            &updating,
            StatusCode::INTERNAL_SERVER_ERROR
        ));
    }
}
