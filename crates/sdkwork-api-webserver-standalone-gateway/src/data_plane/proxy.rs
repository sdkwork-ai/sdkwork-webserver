use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    http::{
        header::{
            CONNECTION, CONTENT_LENGTH, EXPECT, HOST, PROXY_AUTHENTICATE, PROXY_AUTHORIZATION,
            RETRY_AFTER, TE, TRANSFER_ENCODING, UPGRADE,
        },
        HeaderMap, HeaderName, HeaderValue, Method, Request, Response, StatusCode, Version,
    },
};
use http_body::Body as HttpBody;
use http_body_util::BodyExt;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use sdkwork_webserver_core::{
    CompiledWebServerApp, RouteConfig, UpstreamActiveHealthConfig, UpstreamActiveHealthMethod,
    UpstreamConfig, UpstreamHashKeyVar, UpstreamLoadBalancingStrategy, UpstreamRetryCondition,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use url::Url;

use super::{
    dns::{BoundedSystemResolver, GuardedDnsResolver},
    http1_wire::Http1UpgradeGuard,
    metrics::{
        DataPlaneMetrics, UpstreamMetricLease, UpstreamRejection, UpstreamResult,
        UpstreamRetryReason,
    },
    proxy_body::{
        validate_trailer_declaration, GuardedProxyBody, ProxyRequestBodyControl,
        ProxyTrailerPolicy, RequestBodyFailure,
    },
    runtime::RuntimeGeneration,
    smooth_weighted::SmoothWeightedState,
    tunnel::TunnelSupervisor,
    upstream_admission::hold_upstream_permit,
    upstream_client::{UpstreamClient, UpstreamResponseBody},
    DataPlaneError,
};

const CANONICAL_PROXY_PATH_ENCODE_SET: &AsciiSet = &CONTROLS.add(b'%');
const ATTEMPTED_TARGET_WORDS: usize = 16;
const SPLITMIX64_GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;
/// Upper bound on the dynamic-upstream cache: variable `proxy_pass` targets
/// keyed by evaluated authority stay bounded under adversarial Host headers.
const DYNAMIC_UPSTREAM_CACHE_MAXIMUM: usize = 4096;
static RANDOM_SEED_SEQUENCE: AtomicU64 = AtomicU64::new(SPLITMIX64_GAMMA);

pub struct ProxyUpstream<T = UpstreamClient> {
    id: String,
    client: T,
    targets: Vec<ProxyTarget>,
    load_balancing: UpstreamLoadBalancingStrategy,
    hash: Option<HashPolicy>,
    smooth_weighted: SmoothWeightedState,
    cursor: AtomicUsize,
    random_state: AtomicU64,
    permits: Arc<Semaphore>,
    max_in_flight_requests: usize,
    retry: RetryPolicy,
    health: PassiveHealthPolicy,
    active_health: Option<ActiveHealthPolicy>,
    epoch: Instant,
}

struct HashPolicy {
    key: UpstreamHashKeyVar,
    consistent: bool,
    points: Vec<hash_lb::ConsistentHashPoint>,
}

impl HashPolicy {
    fn from_config(config: &UpstreamConfig, targets: &[ProxyTarget]) -> Option<Self> {
        let hash = config.hash.as_ref()?;
        if config.load_balancing != UpstreamLoadBalancingStrategy::Hash {
            return None;
        }
        let points = if hash.consistent {
            let entries: Vec<(usize, String, usize)> = targets
                .iter()
                .enumerate()
                .filter(|(_, target)| !target.backup)
                .map(|(index, target)| {
                    let server = target
                        .url
                        .host_str()
                        .map(|host| match target.url.port() {
                            Some(port) => format!("{host}:{port}"),
                            None => host.to_owned(),
                        })
                        .unwrap_or_else(|| target.url.as_str().to_owned());
                    (index, server, target.weight)
                })
                .collect();
            let refs: Vec<(usize, &str, usize)> = entries
                .iter()
                .map(|(index, server, weight)| (*index, server.as_str(), *weight))
                .collect();
            hash_lb::build_consistent_hash_points(&refs)
        } else {
            Vec::new()
        };
        Some(Self {
            key: hash.key,
            consistent: hash.consistent,
            points,
        })
    }

    fn resolve_key(
        &self,
        client_ip: IpAddr,
        request_uri: &str,
        uri: &str,
        host: &str,
    ) -> String {
        match self.key {
            UpstreamHashKeyVar::RequestUri => request_uri.to_owned(),
            UpstreamHashKeyVar::Uri => uri.to_owned(),
            UpstreamHashKeyVar::RemoteAddr => client_ip.to_string(),
            UpstreamHashKeyVar::Host => host.to_owned(),
        }
    }
}

pub(super) struct ProxyTarget {
    url: Url,
    weight: usize,
    pub(super) backup: bool,
    slow_start_duration_ms: u64,
    slow_start_started_ms: AtomicU64,
    active_requests: Arc<AtomicUsize>,
    consecutive_failures: AtomicU32,
    ejected_until_ms: AtomicU64,
    probe_in_flight: AtomicBool,
    active_available: AtomicBool,
    active_failures: AtomicU32,
    active_successes: AtomicU32,
    active_health_url: Option<Url>,
}

struct PassiveHealthPolicy {
    failure_threshold: u32,
    ejection_time_ms: u64,
    failure_statuses: Vec<u16>,
}

#[derive(Clone, Copy)]
struct RetryPolicy {
    enabled: bool,
    maximum_attempts: usize,
    total_timeout: Duration,
    attempt_timeout: Duration,
    transport_failure: bool,
    timeout: bool,
    statuses: [bool; 3],
}

#[derive(Clone, Copy)]
struct RetryTargetContext<'a> {
    client_ip: IpAddr,
    hash_key: &'a str,
    attempts_started: usize,
    maximum_attempts: usize,
    deadline: Instant,
    metrics: &'a DataPlaneMetrics,
    reason: UpstreamRetryReason,
}

impl RetryPolicy {
    fn from_config(config: &UpstreamConfig) -> Self {
        let mut policy = Self {
            enabled: false,
            maximum_attempts: 1,
            total_timeout: Duration::from_millis(config.request_timeout_ms),
            attempt_timeout: Duration::from_millis(config.request_timeout_ms),
            transport_failure: false,
            timeout: false,
            statuses: [false; 3],
        };
        let Some(retry) = &config.retry else {
            return policy;
        };
        policy.enabled = true;
        policy.maximum_attempts = usize::from(retry.max_attempts);
        policy.total_timeout = Duration::from_millis(retry.timeout_ms);
        for condition in &retry.retry_on {
            match condition {
                UpstreamRetryCondition::TransportFailure => policy.transport_failure = true,
                UpstreamRetryCondition::Timeout => policy.timeout = true,
                UpstreamRetryCondition::Http502 => policy.statuses[0] = true,
                UpstreamRetryCondition::Http503 => policy.statuses[1] = true,
                UpstreamRetryCondition::Http504 => policy.statuses[2] = true,
            }
        }
        policy
    }

    fn status_reason(self, status: StatusCode) -> Option<UpstreamRetryReason> {
        match status.as_u16() {
            502 if self.statuses[0] => Some(UpstreamRetryReason::Http502),
            503 if self.statuses[1] => Some(UpstreamRetryReason::Http503),
            504 if self.statuses[2] => Some(UpstreamRetryReason::Http504),
            _ => None,
        }
    }
}

#[derive(Default)]
pub(super) struct AttemptedTargets {
    words: [u64; ATTEMPTED_TARGET_WORDS],
}

impl AttemptedTargets {
    pub(super) fn contains(&self, index: usize) -> bool {
        let word = index / u64::BITS as usize;
        let bit = index % u64::BITS as usize;
        self.words
            .get(word)
            .is_some_and(|value| value & (1_u64 << bit) != 0)
    }

    fn insert(&mut self, index: usize) {
        let word = index / u64::BITS as usize;
        let bit = index % u64::BITS as usize;
        if let Some(value) = self.words.get_mut(word) {
            *value |= 1_u64 << bit;
        }
    }
}

struct ActiveHealthPolicy {
    method: UpstreamActiveHealthMethod,
    interval: Duration,
    timeout: Duration,
    unhealthy_threshold: u32,
    healthy_threshold: u32,
    success_status_min: u16,
    success_status_max: u16,
    max_response_body_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ActiveHealthTransition {
    Unchanged,
    BecameHealthy,
    BecameUnhealthy,
}

impl ActiveHealthTransition {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Unchanged => "unchanged",
            Self::BecameHealthy => "healthy",
            Self::BecameUnhealthy => "unhealthy",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct SelectedTarget<'a> {
    index: usize,
    url: &'a Url,
    probe: bool,
    ejection_deadline_ms: u64,
}

/// Stream proxy selection handle that can report success/failure into the
/// shared upstream health state.
#[derive(Debug, Clone)]
pub(crate) struct StreamEndpoint {
    pub host: String,
    pub port: u16,
    index: usize,
    probe: bool,
    ejection_deadline_ms: u64,
}

struct ProbeClaimLease<'a> {
    flag: Option<&'a AtomicBool>,
}

pub(crate) struct TargetActivityLease {
    counter: Arc<AtomicUsize>,
    acquired: bool,
}

impl TargetActivityLease {
    fn claim(counter: &Arc<AtomicUsize>) -> Self {
        let acquired = counter
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_add(1)
            })
            .is_ok();
        Self {
            counter: counter.clone(),
            acquired,
        }
    }
}

impl Drop for TargetActivityLease {
    fn drop(&mut self) {
        if self.acquired {
            let _ = self
                .counter
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                    current.checked_sub(1)
                });
        }
    }
}

impl Drop for ProbeClaimLease<'_> {
    fn drop(&mut self) {
        if let Some(flag) = self.flag {
            flag.store(false, Ordering::Release);
        }
    }
}

pub(super) struct ProxyRequestContext<'a> {
    pub generation: &'a Arc<RuntimeGeneration>,
    pub upstream_ref: &'a str,
    pub strip_prefix: bool,
    /// nginx `proxy_pass` URI part (`http://backend/api/` → `/api/`): the
    /// route's matched prefix is replaced with this URI before forwarding.
    pub target_uri: Option<&'a str>,
    pub request_set_headers: &'a [String],
    /// Variable `proxy_pass` template evaluated per request.
    pub dynamic_target: Option<&'a str>,
    /// nginx `proxy_pass_request_headers`; `false` strips client headers.
    pub proxy_pass_request_headers: bool,
    pub route: &'a RouteConfig,
    pub client_ip: IpAddr,
    pub external_scheme: &'a str,
    pub external_authority: &'a str,
    /// Listener port for `$server_port` expansion (nginx semantics: the
    /// port the request was accepted on, not the Host header's port).
    pub listener_port: u16,
    pub normalized_path: &'a str,
    pub request_failure: RequestBodyFailure,
    pub tunnel_supervisor: &'a Arc<TunnelSupervisor>,
    pub metrics: &'a Arc<DataPlaneMetrics>,
    /// Shared HTTP response cache for proxied surfaces, when enabled.
    pub cache: Option<Arc<super::cache::HttpResponseCache>>,
}

mod hash_lb;
mod upstream;
#[cfg(test)]
use upstream::{advance_ip_hash, weighted_load_cmp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpgradeDisposition {
    None,
    WebSocket,
    Unsupported,
}

/// Proxy entry point with HTTP response caching (nginx `proxy_cache`
/// equivalent). Non-cacheable requests (non-GET/HEAD, WebSocket upgrades,
/// Authorization/Range) bypass the cache entirely and keep streaming
/// semantics. Cacheable responses are buffered up to the configured object
/// limit; larger responses pass through uncached.
pub(super) async fn proxy_request_cached(
    context: ProxyRequestContext<'_>,
    request: Request<Body>,
) -> Response<Body> {
    if context.dynamic_target.is_some() {
        // Dynamic targets vary per request; caching would mix hosts.
        return proxy_request(context, request).await;
    }
    let Some(cache) = context.cache.clone() else {
        tracing::debug!("proxy cache disabled for {}", context.external_authority);
        return proxy_request(context, request).await;
    };
    tracing::debug!("proxy cache enabled for {}", context.external_authority);
    let cacheable = matches!(request.method(), &Method::GET | &Method::HEAD)
        && request.headers().get("authorization").is_none()
        && request.headers().get("range").is_none()
        && request.headers().get("connection").is_none_or(|value| {
            value
                .to_str()
                .map(|value| !value.to_ascii_lowercase().contains("upgrade"))
                .unwrap_or(false)
        });
    if !cacheable {
        return proxy_request(context, request).await;
    }

    let base_key = super::cache::CacheKey::new(
        request.method().as_str(),
        context.external_authority,
        context.normalized_path,
        request.uri().query(),
        &[],
        request.headers(),
    );
    if let Some(hit) = cache.lookup(&base_key) {
        if hit.metadata.vary.is_empty() {
            return cached_proxy_response(hit, request.headers());
        }
        let refined = super::cache::CacheKey::new(
            request.method().as_str(),
            context.external_authority,
            context.normalized_path,
            request.uri().query(),
            &hit.metadata.vary,
            request.headers(),
        );
        if let Some(refined_hit) = cache.lookup(&refined) {
            return cached_proxy_response(refined_hit, request.headers());
        }
    }

    // Single-flight: concurrent requests for the same key wait for the fill.
    if let Some(waiter) = cache.begin_fill(&base_key) {
        waiter.notified().await;
        if let Some(hit) = cache.lookup(&base_key) {
            return cached_proxy_response(hit, request.headers());
        }
        return proxy_request(context, request).await;
    }

    let request_headers = request.headers().clone();
    let response = proxy_request(context, request).await;
    // Upstream failure: fall back to a stale entry when one exists (nginx
    // `proxy_cache_use_stale`), keeping the origin available under outages.
    if response.status().is_server_error() {
        if let Some(stale) = cache.lookup_stale(&base_key) {
            cache.finish_fill(&base_key);
            return cached_proxy_response(stale, &request_headers);
        }
    }
    let response = store_proxy_response(&cache, &base_key, &request_headers, response).await;
    cache.finish_fill(&base_key);
    response
}

/// Buffer and store a cacheable upstream response. Oversized or
/// non-cacheable responses are left untouched.
async fn store_proxy_response(
    cache: &Arc<super::cache::HttpResponseCache>,
    base_key: &super::cache::CacheKey,
    request_headers: &axum::http::HeaderMap,
    response: Response<Body>,
) -> Response<Body> {
    let maximum_bytes = cache_maximum_object_bytes(cache);
    let freshness = super::cache::freshness_for(
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
    let decision = super::cache::decide_cacheability(
        response.status().as_u16(),
        response.headers(),
        freshness.fresh_seconds,
    );
    if !decision.cacheable {
        return response;
    }
    let status = response.status();
    let response_headers = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_owned(),
                value.to_str().unwrap_or_default().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    let vary = response_headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("vary"))
        .map(|(_, value)| super::cache::parse_vary_header(value))
        .unwrap_or_default();
    let (parts, body) = response.into_parts();
    let Ok(bytes) = collect_body_limited(body, maximum_bytes).await else {
        return Response::from_parts(parts, Body::empty());
    };
    let key = super::cache::CacheKey::new(
        &base_key.method,
        &base_key.host,
        &base_key.path,
        Some(&base_key.query),
        &vary,
        request_headers,
    );
    let metadata = super::cache::ResponseMetadata {
        status: status.as_u16(),
        headers: response_headers,
        vary,
        fresh_seconds: freshness.fresh_seconds.unwrap_or(0),
    };
    cache.insert(key, metadata, bytes.clone(), decision);
    Response::from_parts(parts, Body::from(bytes))
}

async fn collect_body_limited(body: Body, maximum_bytes: u64) -> Result<bytes::Bytes, ()> {
    use futures_util::StreamExt;
    let mut stream = body.into_data_stream();
    let mut collected = Vec::new();
    while let Some(frame) = stream.next().await {
        let chunk = frame.map_err(|_| ())?;
        if collected.len() as u64 + chunk.len() as u64 > maximum_bytes {
            return Err(());
        }
        collected.extend_from_slice(&chunk);
    }
    Ok(bytes::Bytes::from(collected))
}

fn cache_maximum_object_bytes(cache: &Arc<super::cache::HttpResponseCache>) -> u64 {
    // The facade clamps inserts; read the configured limit through a helper.
    cache.maximum_object_bytes()
}

/// Serve a cached entry, answering conditional requests with 304.
fn cached_proxy_response(
    entry: super::cache::CachedResponse,
    request_headers: &axum::http::HeaderMap,
) -> Response<Body> {
    let etag = entry
        .metadata
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("etag"))
        .map(|(_, value)| value.clone());
    if let Some(etag) = etag {
        if let Some(condition) = request_headers.get("if-none-match").and_then(|value| value.to_str().ok())
        {
            if condition
                .split(',')
                .map(str::trim)
                .any(|candidate| candidate == etag || candidate == "*")
            {
                return Response::builder()
                    .status(StatusCode::NOT_MODIFIED)
                    .header("etag", etag)
                    .body(Body::empty())
                    .unwrap_or_else(|_| Response::new(Body::empty()));
            }
        }
    }
    let mut builder = Response::builder().status(entry.metadata.status);
    for (name, value) in &entry.metadata.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    builder
        .body(Body::from(entry.body))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

pub(super) async fn proxy_request(
    context: ProxyRequestContext<'_>,
    request: Request<Body>,
) -> Response<Body> {
    let upgrade = match classify_upgrade_request(&request) {
        Ok(upgrade) => upgrade,
        Err(()) => return text_response(StatusCode::BAD_REQUEST, "invalid protocol upgrade\n"),
    };
    if upgrade == UpgradeDisposition::Unsupported {
        return text_response(
            StatusCode::NOT_IMPLEMENTED,
            "protocol upgrade is unsupported\n",
        );
    }
    if upgrade == UpgradeDisposition::WebSocket {
        if let Some(guard) = request.extensions().get::<Http1UpgradeGuard>() {
            guard.activate();
        }
    }
    if let Some(template) = context.dynamic_target {
        return proxy_request_dynamic(context, request, upgrade, template).await;
    }
    let Some(upstream) = context.generation.upstreams.get(context.upstream_ref) else {
        context
            .metrics
            .record_upstream_rejection(UpstreamRejection::MissingUpstream);
        return upgrade_failure_response(
            upgrade,
            text_response(StatusCode::BAD_GATEWAY, "upstream is unavailable\n"),
        );
    };

    let upstream_permit = match upstream.try_admit() {
        Ok(permit) => permit,
        Err(()) => {
            context
                .metrics
                .record_upstream_rejection(UpstreamRejection::RequestCapacity);
            return upgrade_failure_response(
                upgrade,
                upstream_unavailable_response("upstream is saturated\n"),
            );
        }
    };
    let request_path = request.uri().path();
    let request_query = request.uri().query();
    let request_uri = match request_query {
        Some(query) => format!("{request_path}?{query}"),
        None => request_path.to_owned(),
    };
    let hash_key = upstream
        .hash
        .as_ref()
        .map(|policy| {
            policy.resolve_key(
                context.client_ip,
                &request_uri,
                request_path,
                context.external_authority,
            )
        })
        .unwrap_or_default();
    let Some(selected) =
        upstream.select_target_observed(context.client_ip, &hash_key, Some(context.metrics))
    else {
        context
            .metrics
            .record_upstream_rejection(UpstreamRejection::NoEligibleTarget);
        return upgrade_failure_response(
            upgrade,
            upstream_unavailable_response("all upstream targets are unavailable\n"),
        );
    };
    let target_activity = upstream.claim_target_activity(selected.index);

    let target_url = match build_target_url(
        selected.url,
        context.strip_prefix,
        context.target_uri,
        &context.route.route_match.path,
        request.uri().path(),
        context.normalized_path,
        request.uri().query(),
    ) {
        Ok(url) => url,
        Err(()) => {
            upstream.abandon_probe(selected);
            return upgrade_failure_response(
                upgrade,
                text_response(StatusCode::BAD_GATEWAY, "invalid upstream target\n"),
            );
        }
    };

    if upgrade == UpgradeDisposition::WebSocket {
        return proxy_websocket_request(
            &context,
            upstream,
            selected,
            target_activity,
            upstream_permit,
            target_url,
            request,
        )
        .await;
    }
    proxy_http_request(
        &context,
        upstream,
        selected,
        target_activity,
        target_url,
        upstream_permit,
        &hash_key,
        request,
    )
    .await
}

/// Variable `proxy_pass`: evaluate the template per request, cache a
/// single-target upstream per resolved authority, and reuse the regular
/// proxy pipeline (no retries or health checks, matching nginx's dynamic
/// `proxy_pass` semantics).
async fn proxy_request_dynamic(
    context: ProxyRequestContext<'_>,
    request: Request<Body>,
    upgrade: UpgradeDisposition,
    template: &str,
) -> Response<Body> {
    let request_path = request.uri().path();
    let request_query = request.uri().query();
    let request_uri = match request_query {
        Some(query) => format!("{request_path}?{query}"),
        None => request_path.to_owned(),
    };
    let (host, server_port) = split_authority(context.external_authority);
    let headers = request
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_ascii_lowercase(),
                value.to_str().unwrap_or_default().to_owned(),
            )
        })
        .collect::<HashMap<_, _>>();
    let evaluated =
        match sdkwork_webserver_core::expand_proxy_pass_template(
            template,
            &host,
            server_port,
            request_path,
            &request_uri,
            &headers,
        ) {
            Ok(url) => url,
            Err(()) => {
                return upgrade_failure_response(
                    upgrade,
                    text_response(StatusCode::BAD_GATEWAY, "invalid proxy_pass target
"),
                );
            }
        };
    // A template without `$uri`/`$request_uri` forwards the full request URI
    // (nginx `proxy_pass http://host;` semantics).
    let has_uri_variable = template.contains("$uri") || template.contains("$request_uri");
    let target_url = if has_uri_variable {
        evaluated
    } else {
        format!("{evaluated}{request_uri}")
    };
    let authority = match target_url.parse::<Url>() {
        Ok(url) => match url.host_str() {
            Some(host) => format!(
                "{host}:{}",
                url.port_or_known_default().unwrap_or(80)
            ),
            None => {
                return upgrade_failure_response(
                    upgrade,
                    text_response(StatusCode::BAD_GATEWAY, "invalid proxy_pass target
"),
                );
            }
        },
        Err(_) => {
            tracing::debug!("dynamic proxy_pass target failed to parse");
            return upgrade_failure_response(
                upgrade,
                text_response(StatusCode::BAD_GATEWAY, "invalid proxy_pass target
"),
            );
        }
    };
    // Dynamic upstreams are cached per evaluated authority so repeated
    // variable `proxy_pass` targets reuse one connection pool. The cache is
    // bounded (an attacker-controlled Host header must not grow memory
    // without limit), and the (synchronous) upstream build runs OUTSIDE the
    // lock so one slow build never serializes every dynamic-proxy request.
    let upstream = {
        let cached = context
            .generation
            .dynamic_upstreams
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&authority)
            .cloned();
        if let Some(upstream) = cached {
            upstream
        } else {
            let synthetic = synthetic_dynamic_upstream_config(&target_url);
            let built = match ProxyUpstream::build(
                &context.generation.app,
                &synthetic,
                context.generation.resolver.clone(),
                context.metrics.clone(),
                context.generation.resolution_chain.clone(),
            ) {
                Ok(upstream) => Arc::new(upstream),
                Err(_) => {
                    return upgrade_failure_response(
                        upgrade,
                        text_response(
                            StatusCode::BAD_GATEWAY,
                            "upstream is unavailable
",
                        ),
                    );
                }
            };
            let mut cache = context
                .generation
                .dynamic_upstreams
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(existing) = cache.get(&authority) {
                existing.clone()
            } else {
                if cache.len() >= DYNAMIC_UPSTREAM_CACHE_MAXIMUM {
                    if let Some(victim) = cache.keys().next().cloned() {
                        cache.remove(&victim);
                    }
                }
                cache.insert(authority.clone(), built.clone());
                built
            }
        }
    };
    let upstream_permit = match upstream.try_admit() {
        Ok(permit) => permit,
        Err(()) => {
            return upgrade_failure_response(
                upgrade,
                upstream_unavailable_response("upstream is saturated
"),
            );
        }
    };
    let hash_key = String::new();
    let Some(selected) =
        upstream.select_target_observed(context.client_ip, &hash_key, Some(context.metrics))
    else {
        return upgrade_failure_response(
            upgrade,
            upstream_unavailable_response("all upstream targets are unavailable
"),
        );
    };
    let target_activity = upstream.claim_target_activity(selected.index);
    let target_url = match Url::parse(&target_url) {
        Ok(url) => url,
        Err(_) => {
            upstream.abandon_probe(selected);
            return upgrade_failure_response(
                upgrade,
                text_response(StatusCode::BAD_GATEWAY, "invalid upstream target
"),
            );
        }
    };
    if upgrade == UpgradeDisposition::WebSocket {
        return proxy_websocket_request(
            &context,
            &upstream,
            selected,
            target_activity,
            upstream_permit,
            target_url,
            request,
        )
        .await;
    }
    proxy_http_request(
        &context,
        &upstream,
        selected,
        target_activity,
        target_url,
        upstream_permit,
        &hash_key,
        request,
    )
    .await
}

fn synthetic_dynamic_upstream_config(target_url: &str) -> sdkwork_webserver_core::UpstreamConfig {
    // The `proxy_pass` template is the operator's authorization for
    // per-request hosts: authorize the standard restricted networks while
    // the hard-forbidden ranges (cloud metadata, documentation, multicast)
    // stay blocked by the SSRF guard.
    serde_json::from_value(serde_json::json!({
        "id": "dynamic",
        "targets": [{ "url": target_url }],
        "addressPolicy": {
            "allowedCidrs": [
                "10.0.0.0/8",
                "100.64.0.0/10",
                "127.0.0.0/8",
                "169.254.0.0/16",
                "172.16.0.0/12",
                "192.168.0.0/16",
                "::1/128",
                "fc00::/7",
                "fe80::/10",
            ]
        },
    }))
    .expect("synthetic upstream config is valid")
}

/// Split an external authority into host and port with safe defaults.
fn split_authority(authority: &str) -> (String, u16) {
    if let Some((host, port)) = authority.rsplit_once(':') {
        if let Ok(port) = port.parse::<u16>() {
            return (host.to_owned(), port);
        }
    }
    (authority.to_owned(), 80)
}

async fn proxy_http_request(
    context: &ProxyRequestContext<'_>,
    upstream: &ProxyUpstream,
    mut selected: SelectedTarget<'_>,
    mut target_activity: TargetActivityLease,
    mut target_url: Url,
    upstream_permit: OwnedSemaphorePermit,
    hash_key: &str,
    request: Request<Body>,
) -> Response<Body> {
    let request_version = request.version();
    let retryable_request = is_bodyless_idempotent_request(&request);
    let (request_parts, body) = request.into_parts();
    let maximum_body_bytes = context
        .generation
        .app
        .config()
        .limits
        .max_request_body_bytes;
    let maximum_response_body_bytes = context
        .generation
        .app
        .config()
        .limits
        .max_response_body_bytes;
    let maximum_trailer_bytes = context.generation.app.config().limits.max_trailer_bytes;
    let maximum_trailers = context.generation.app.config().limits.max_trailers;
    let (mut headers, forbidden_request_trailers, declared_request_trailers) =
        match forwarded_request_headers(
            &request_parts.headers,
            context.client_ip,
            context.external_scheme,
            context.external_authority,
            maximum_trailer_bytes,
            maximum_trailers,
            context.proxy_pass_request_headers,
        ) {
            Ok(result) => result,
            Err(()) => {
                upstream.abandon_probe(selected);
                return text_response(StatusCode::BAD_REQUEST, "invalid Trailer declaration\n");
            }
        };
    if apply_proxy_set_headers(
        &mut headers,
        context.request_set_headers,
        &request_parts.headers,
        context.client_ip,
        context.external_scheme,
        context.external_authority,
        context.listener_port,
    )
    .is_err()
    {
        upstream.abandon_probe(selected);
        return text_response(StatusCode::BAD_GATEWAY, "invalid proxySetHeader value\n");
    }
    let mut request_body = Some(body);
    let mut request_trailer_policy = Some((forbidden_request_trailers, declared_request_trailers));
    let retry_enabled = retryable_request && upstream.retry.enabled;
    let maximum_attempts = if retry_enabled {
        upstream.retry.maximum_attempts
    } else {
        1
    };
    let retry_deadline = Instant::now() + upstream.retry.total_timeout;
    let mut attempted = AttemptedTargets::default();
    attempted.insert(selected.index);
    let mut attempts_started = 0usize;

    loop {
        attempts_started = attempts_started.saturating_add(1);
        let _probe_claim_lease = upstream.probe_claim_lease(selected.index, selected.probe);
        let target_uri = match target_url.as_str().parse() {
            Ok(uri) => uri,
            Err(_) => {
                upstream.abandon_probe(selected);
                return text_response(StatusCode::BAD_GATEWAY, "invalid upstream target\n");
            }
        };
        let request_control = if retryable_request {
            ProxyRequestBodyControl::completed()
        } else {
            ProxyRequestBodyControl::default()
        };
        let upstream_body = if retryable_request {
            Body::empty()
        } else {
            let (forbidden, declared) = request_trailer_policy
                .take()
                .expect("non-replayed request owns one Trailer policy");
            Body::new(GuardedProxyBody::request(
                request_body
                    .take()
                    .expect("non-replayed request owns one Body"),
                maximum_body_bytes,
                ProxyTrailerPolicy::new(
                    maximum_trailer_bytes,
                    maximum_trailers,
                    declared,
                    forbidden,
                ),
                context.request_failure.clone(),
                request_control.clone(),
            ))
        };
        let mut upstream_request = Request::new(upstream_body);
        *upstream_request.method_mut() = request_parts.method.clone();
        *upstream_request.uri_mut() = target_uri;
        *upstream_request.headers_mut() = headers.clone();

        let attempt = context.metrics.begin_upstream_attempt();
        let result = if retry_enabled {
            let remaining = retry_deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                attempt.finish(UpstreamResult::Timeout);
                upstream.abandon_probe(selected);
                return text_response(StatusCode::GATEWAY_TIMEOUT, "upstream timed out\n");
            }
            upstream
                .client
                .execute_with_timeout(
                    upstream_request,
                    upstream.retry.attempt_timeout.min(remaining),
                )
                .await
        } else {
            upstream.client.execute(upstream_request).await
        };
        let response = match result {
            Ok(response) => response,
            Err(_) if context.request_failure.timed_out() => {
                attempt.finish(UpstreamResult::RequestFailure);
                upstream.abandon_probe(selected);
                return request_body_timeout_response(request_version);
            }
            Err(_) if context.request_failure.body_too_large() => {
                attempt.finish(UpstreamResult::RequestFailure);
                upstream.abandon_probe(selected);
                return text_response(StatusCode::PAYLOAD_TOO_LARGE, "request body is too large\n");
            }
            Err(_) if context.request_failure.invalid_body() => {
                attempt.finish(UpstreamResult::RequestFailure);
                upstream.abandon_probe(selected);
                return text_response(StatusCode::BAD_REQUEST, "request body framing is invalid\n");
            }
            Err(error) if error.is_connection_saturated() => {
                context
                    .metrics
                    .record_upstream_rejection(UpstreamRejection::ConnectionCapacity);
                attempt.finish(UpstreamResult::RequestFailure);
                upstream.abandon_probe(selected);
                return upstream_unavailable_response(
                    "upstream connection capacity is saturated\n",
                );
            }
            Err(error) if error.is_timeout() => {
                attempt.finish(UpstreamResult::Timeout);
                upstream.record_failure(selected);
                if upstream.retry.timeout {
                    if let Some(next) = upstream.next_retry_target(
                        &mut attempted,
                        RetryTargetContext {
                            client_ip: context.client_ip,
                            hash_key,
                            attempts_started,
                            maximum_attempts,
                            deadline: retry_deadline,
                            metrics: context.metrics,
                            reason: UpstreamRetryReason::Timeout,
                        },
                    ) {
                        selected = next;
                        target_activity = upstream.claim_target_activity(selected.index);
                        target_url = match build_retry_target_url(context, selected, &request_parts)
                        {
                            Ok(url) => url,
                            Err(()) => {
                                upstream.abandon_probe(selected);
                                return text_response(
                                    StatusCode::BAD_GATEWAY,
                                    "invalid upstream target\n",
                                );
                            }
                        };
                        continue;
                    }
                }
                return text_response(StatusCode::GATEWAY_TIMEOUT, "upstream timed out\n");
            }
            Err(_) => {
                attempt.finish(UpstreamResult::TransportFailure);
                upstream.record_failure(selected);
                if upstream.retry.transport_failure {
                    if let Some(next) = upstream.next_retry_target(
                        &mut attempted,
                        RetryTargetContext {
                            client_ip: context.client_ip,
                            hash_key,
                            attempts_started,
                            maximum_attempts,
                            deadline: retry_deadline,
                            metrics: context.metrics,
                            reason: UpstreamRetryReason::TransportFailure,
                        },
                    ) {
                        selected = next;
                        target_activity = upstream.claim_target_activity(selected.index);
                        target_url = match build_retry_target_url(context, selected, &request_parts)
                        {
                            Ok(url) => url,
                            Err(()) => {
                                upstream.abandon_probe(selected);
                                return text_response(
                                    StatusCode::BAD_GATEWAY,
                                    "invalid upstream target\n",
                                );
                            }
                        };
                        continue;
                    }
                }
                return text_response(StatusCode::BAD_GATEWAY, "upstream failed\n");
            }
        };

        let upstream_responded_early = request_control.pause_if_incomplete();
        let (mut response_parts, response_body) = response.into_parts();
        let (response_headers, forbidden_response_trailers, declared_response_trailers) =
            match forwarded_response_headers(
                &response_parts.headers,
                maximum_trailer_bytes,
                maximum_trailers,
            ) {
                Ok(result) => result,
                Err(()) => {
                    attempt.finish(UpstreamResult::InvalidResponse);
                    upstream.record_failure(selected);
                    request_control.cancel_if_incomplete();
                    return text_response(
                        StatusCode::BAD_GATEWAY,
                        "upstream Trailer declaration is invalid\n",
                    );
                }
            };
        response_parts.headers = response_headers;
        if upstream.status_is_failure(response_parts.status) {
            upstream.record_failure(selected);
        } else {
            upstream.record_success(selected);
        }
        attempt.finish(UpstreamResult::Response);

        if retryable_request {
            if let Some(reason) = upstream.retry.status_reason(response_parts.status) {
                if let Some(next) = upstream.next_retry_target(
                    &mut attempted,
                    RetryTargetContext {
                        client_ip: context.client_ip,
                        hash_key,
                        attempts_started,
                        maximum_attempts,
                        deadline: retry_deadline,
                        metrics: context.metrics,
                        reason,
                    },
                ) {
                    drop(response_body);
                    selected = next;
                    target_activity = upstream.claim_target_activity(selected.index);
                    target_url = match build_retry_target_url(context, selected, &request_parts) {
                        Ok(url) => url,
                        Err(()) => {
                            upstream.abandon_probe(selected);
                            return text_response(
                                StatusCode::BAD_GATEWAY,
                                "invalid upstream target\n",
                            );
                        }
                    };
                    continue;
                }
            }
        }

        if upstream_responded_early && request_version != Version::HTTP_2 {
            response_parts
                .headers
                .insert(CONNECTION, HeaderValue::from_static("close"));
        }
        let response_trailer_policy = ProxyTrailerPolicy::new(
            maximum_trailer_bytes,
            maximum_trailers,
            declared_response_trailers,
            forbidden_response_trailers,
        );
        let guarded_body = if upstream_responded_early {
            GuardedProxyBody::response_with_request_cancellation(
                response_body,
                response_trailer_policy,
                request_control,
                Some(maximum_response_body_bytes),
            )
        } else {
            GuardedProxyBody::response(
                response_body,
                response_trailer_policy,
                Some(maximum_response_body_bytes),
            )
        };
        return hold_upstream_permit(
            Response::from_parts(response_parts, Body::new(guarded_body)),
            upstream_permit,
            (context.generation.clone(), target_activity),
        );
    }
}

fn is_bodyless_idempotent_request(request: &Request<Body>) -> bool {
    request.body().is_end_stream()
        && matches!(
            *request.method(),
            Method::GET
                | Method::HEAD
                | Method::OPTIONS
                | Method::TRACE
                | Method::PUT
                | Method::DELETE
        )
}

fn build_retry_target_url(
    context: &ProxyRequestContext<'_>,
    selected: SelectedTarget<'_>,
    request_parts: &axum::http::request::Parts,
) -> Result<Url, ()> {
    build_target_url(
        selected.url,
        context.strip_prefix,
        context.target_uri,
        &context.route.route_match.path,
        request_parts.uri.path(),
        context.normalized_path,
        request_parts.uri.query(),
    )
}

async fn proxy_websocket_request(
    context: &ProxyRequestContext<'_>,
    upstream: &ProxyUpstream,
    selected: SelectedTarget<'_>,
    target_activity: TargetActivityLease,
    upstream_permit: OwnedSemaphorePermit,
    target_url: Url,
    mut request: Request<Body>,
) -> Response<Body> {
    let downstream_upgrade = hyper::upgrade::on(&mut request);
    let (parts, _body) = request.into_parts();
    let maximum_trailer_bytes = context.generation.app.config().limits.max_trailer_bytes;
    let maximum_trailers = context.generation.app.config().limits.max_trailers;
    let (mut headers, _, _) = match forwarded_request_headers(
        &parts.headers,
        context.client_ip,
        context.external_scheme,
        context.external_authority,
        maximum_trailer_bytes,
        maximum_trailers,
        context.proxy_pass_request_headers,
    ) {
        Ok(headers) => headers,
        Err(()) => {
            upstream.abandon_probe(selected);
            return websocket_failure_response(text_response(
                StatusCode::BAD_REQUEST,
                "invalid WebSocket handshake\n",
            ));
        }
    };
    if apply_proxy_set_headers(
        &mut headers,
        context.request_set_headers,
        &parts.headers,
        context.client_ip,
        context.external_scheme,
        context.external_authority,
        context.listener_port,
    )
    .is_err()
    {
        upstream.abandon_probe(selected);
        return websocket_failure_response(text_response(
            StatusCode::BAD_GATEWAY,
            "invalid proxySetHeader value\n",
        ));
    }
    headers.remove(TE);
    headers.insert(CONNECTION, HeaderValue::from_static("upgrade"));
    headers.insert(UPGRADE, HeaderValue::from_static("websocket"));

    let target_uri = match target_url.as_str().parse() {
        Ok(uri) => uri,
        Err(_) => {
            upstream.abandon_probe(selected);
            return websocket_failure_response(text_response(
                StatusCode::BAD_GATEWAY,
                "invalid upstream target\n",
            ));
        }
    };
    let mut upstream_request = Request::new(Body::empty());
    *upstream_request.method_mut() = Method::GET;
    *upstream_request.version_mut() = Version::HTTP_11;
    *upstream_request.uri_mut() = target_uri;
    *upstream_request.headers_mut() = headers;

    let attempt = context.metrics.begin_upstream_attempt();
    let mut response = match upstream.client.execute(upstream_request).await {
        Ok(response) => response,
        Err(error) if error.is_connection_saturated() => {
            context
                .metrics
                .record_upstream_rejection(UpstreamRejection::ConnectionCapacity);
            attempt.finish(UpstreamResult::RequestFailure);
            upstream.abandon_probe(selected);
            return websocket_failure_response(upstream_unavailable_response(
                "upstream connection capacity is saturated\n",
            ));
        }
        Err(error) if error.is_timeout() => {
            attempt.finish(UpstreamResult::Timeout);
            upstream.record_failure(selected);
            return websocket_failure_response(text_response(
                StatusCode::GATEWAY_TIMEOUT,
                "upstream timed out\n",
            ));
        }
        Err(_) => {
            attempt.finish(UpstreamResult::TransportFailure);
            upstream.record_failure(selected);
            return websocket_failure_response(text_response(
                StatusCode::BAD_GATEWAY,
                "upstream failed\n",
            ));
        }
    };

    if response.status() != StatusCode::SWITCHING_PROTOCOLS {
        return forward_websocket_rejection(
            context,
            upstream,
            selected,
            target_activity,
            upstream_permit,
            response,
            attempt,
        );
    }
    if !valid_websocket_upgrade_response(&response) {
        attempt.finish(UpstreamResult::InvalidResponse);
        upstream.record_failure(selected);
        return websocket_failure_response(text_response(
            StatusCode::BAD_GATEWAY,
            "upstream failed\n",
        ));
    }

    let upstream_upgrade = hyper::upgrade::on(&mut response);
    let (mut parts, _body) = response.into_parts();
    let (mut headers, _, _) =
        match forwarded_response_headers(&parts.headers, maximum_trailer_bytes, maximum_trailers) {
            Ok(headers) => headers,
            Err(()) => {
                attempt.finish(UpstreamResult::InvalidResponse);
                upstream.record_failure(selected);
                return websocket_failure_response(text_response(
                    StatusCode::BAD_GATEWAY,
                    "upstream failed\n",
                ));
            }
        };
    headers.insert(CONNECTION, HeaderValue::from_static("upgrade"));
    headers.insert(UPGRADE, HeaderValue::from_static("websocket"));
    parts.headers = headers;
    upstream.record_success(selected);
    attempt.finish(UpstreamResult::Response);

    if context
        .tunnel_supervisor
        .try_spawn(
            downstream_upgrade,
            upstream_upgrade,
            upstream_permit,
            context.generation.clone(),
            target_activity,
        )
        .is_err()
    {
        return websocket_failure_response(upstream_unavailable_response(
            "server is shutting down\n",
        ));
    }
    Response::from_parts(parts, Body::empty())
}

fn forward_websocket_rejection(
    context: &ProxyRequestContext<'_>,
    upstream: &ProxyUpstream,
    selected: SelectedTarget<'_>,
    target_activity: TargetActivityLease,
    upstream_permit: OwnedSemaphorePermit,
    response: Response<UpstreamResponseBody>,
    attempt: UpstreamMetricLease,
) -> Response<Body> {
    let maximum_trailer_bytes = context.generation.app.config().limits.max_trailer_bytes;
    let maximum_trailers = context.generation.app.config().limits.max_trailers;
    let (mut parts, body) = response.into_parts();
    let (headers, forbidden_trailers, declared_trailers) =
        match forwarded_response_headers(&parts.headers, maximum_trailer_bytes, maximum_trailers) {
            Ok(headers) => headers,
            Err(()) => {
                attempt.finish(UpstreamResult::InvalidResponse);
                upstream.record_failure(selected);
                return websocket_failure_response(text_response(
                    StatusCode::BAD_GATEWAY,
                    "upstream failed\n",
                ));
            }
        };
    parts.headers = headers;
    parts
        .headers
        .insert(CONNECTION, HeaderValue::from_static("close"));
    let body = GuardedProxyBody::response(
        body,
        ProxyTrailerPolicy::new(
            maximum_trailer_bytes,
            maximum_trailers,
            declared_trailers,
            forbidden_trailers,
        ),
        None,
    );
    if upstream.status_is_failure(parts.status) {
        upstream.record_failure(selected);
    } else {
        upstream.record_success(selected);
    }
    attempt.finish(UpstreamResult::Response);
    hold_upstream_permit(
        Response::from_parts(parts, Body::new(body)),
        upstream_permit,
        (context.generation.clone(), target_activity),
    )
}

fn upgrade_failure_response(
    upgrade: UpgradeDisposition,
    response: Response<Body>,
) -> Response<Body> {
    if upgrade == UpgradeDisposition::WebSocket {
        websocket_failure_response(response)
    } else {
        response
    }
}

fn websocket_failure_response(mut response: Response<Body>) -> Response<Body> {
    response
        .headers_mut()
        .insert(CONNECTION, HeaderValue::from_static("close"));
    response
}

fn classify_upgrade_request(request: &Request<Body>) -> Result<UpgradeDisposition, ()> {
    let upgrade = single_upgrade_protocol(request.headers())?;
    let connection_upgrade = connection_contains_upgrade(request.headers())?;
    let Some(upgrade) = upgrade else {
        return if connection_upgrade {
            Err(())
        } else {
            Ok(UpgradeDisposition::None)
        };
    };
    if !connection_upgrade
        || request.version() != Version::HTTP_11
        || request.method() != Method::GET
        || request.headers().contains_key(CONTENT_LENGTH)
        || request.headers().contains_key(TRANSFER_ENCODING)
        || request.headers().contains_key(EXPECT)
    {
        return Err(());
    }
    if upgrade.as_str().eq_ignore_ascii_case("websocket") {
        Ok(UpgradeDisposition::WebSocket)
    } else {
        Ok(UpgradeDisposition::Unsupported)
    }
}

fn single_upgrade_protocol(headers: &HeaderMap) -> Result<Option<HeaderName>, ()> {
    let mut values = headers.get_all(UPGRADE).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(());
    }
    let value = trim_ascii_whitespace(value.as_bytes());
    if value.is_empty() || value.contains(&b',') {
        return Err(());
    }
    HeaderName::from_bytes(value).map(Some).map_err(|_| ())
}

fn connection_contains_upgrade(headers: &HeaderMap) -> Result<bool, ()> {
    let mut found = false;
    for value in headers.get_all(CONNECTION) {
        let value = value.to_str().map_err(|_| ())?;
        for token in value.split(',') {
            let token = token.trim();
            if token.is_empty() || HeaderName::from_bytes(token.as_bytes()).is_err() {
                return Err(());
            }
            found |= token.eq_ignore_ascii_case("upgrade");
        }
    }
    Ok(found)
}

fn valid_websocket_upgrade_response(response: &Response<UpstreamResponseBody>) -> bool {
    response.version() == Version::HTTP_11
        && matches!(connection_contains_upgrade(response.headers()), Ok(true))
        && matches!(single_upgrade_protocol(response.headers()), Ok(Some(protocol)) if protocol.as_str().eq_ignore_ascii_case("websocket"))
        && !response.headers().contains_key(CONTENT_LENGTH)
        && !response.headers().contains_key(TRANSFER_ENCODING)
}

fn trim_ascii_whitespace(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(u8::is_ascii_whitespace) {
        value = &value[1..];
    }
    while value.last().is_some_and(u8::is_ascii_whitespace) {
        value = &value[..value.len() - 1];
    }
    value
}

fn build_target_url(
    target: &Url,
    strip_prefix: bool,
    target_uri: Option<&str>,
    route_path: &str,
    request_path: &str,
    normalized_path: &str,
    query: Option<&str>,
) -> Result<Url, ()> {
    // nginx `proxy_pass` URI replacement: when the target has a URI part,
    // the location-matched prefix is replaced with that URI and the
    // forwarded path is appended. Legacy `stripPrefix` resources (no URI
    // part) keep the upstream target's own path as the base.
    if strip_prefix || target_uri.is_some() {
        let forwarded_path = normalized_path
            .strip_prefix(route_path)
            .unwrap_or(normalized_path);
        let mut rewritten = target.clone();
        let replacement = match target_uri {
            Some(uri) => uri.to_owned(),
            None => rewritten.path().to_owned(),
        };
        let base_path = replacement.trim_end_matches('/');
        let combined_path = if forwarded_path.is_empty() {
            if base_path.is_empty() {
                "/".to_owned()
            } else {
                base_path.to_owned()
            }
        } else {
            format!("{base_path}/{}", forwarded_path.trim_start_matches('/'))
        };
        let encoded_path =
            utf8_percent_encode(&combined_path, CANONICAL_PROXY_PATH_ENCODE_SET).to_string();
        rewritten.set_path(&encoded_path);
        rewritten.set_query(query);
        return Ok(rewritten);
    }
    let forwarded_path = request_path;
    let base = target.as_str().trim_end_matches('/');
    let path = if forwarded_path.is_empty() {
        "/"
    } else {
        forwarded_path
    };
    let mut combined = format!("{base}/{}", path.trim_start_matches('/'));
    if let Some(query) = query {
        combined.push('?');
        combined.push_str(query);
    }
    Url::parse(&combined).map_err(|_| ())
}

fn forwarded_request_headers(
    source: &HeaderMap,
    client_ip: IpAddr,
    external_scheme: &str,
    external_authority: &str,
    maximum_trailer_bytes: usize,
    maximum_trailers: usize,
    proxy_pass_request_headers: bool,
) -> Result<(HeaderMap, HashSet<HeaderName>, HashSet<HeaderName>), ()> {
    let hop_by_hop = hop_by_hop_headers(source);
    let declared_trailers =
        validate_trailer_declaration(source, maximum_trailer_bytes, maximum_trailers, &hop_by_hop)?;
    let mut target = HeaderMap::new();
    // nginx `proxy_pass_request_headers off`: the client's request header
    // fields are not forwarded; only the fixed safe defaults below are set.
    if proxy_pass_request_headers {
        for (name, value) in source {
            if name != HOST && name != CONTENT_LENGTH && name != EXPECT && !hop_by_hop.contains(name)
            {
                target.append(name.clone(), value.clone());
            }
        }
    }
    if let Ok(value) = HeaderValue::from_str(&client_ip.to_string()) {
        target.insert(HeaderName::from_static("x-forwarded-for"), value);
    }
    if let Ok(value) = HeaderValue::from_str(external_scheme) {
        target.insert(HeaderName::from_static("x-forwarded-proto"), value);
    }
    if let Ok(value) = HeaderValue::from_str(external_authority) {
        target.insert(HeaderName::from_static("x-forwarded-host"), value);
    }
    target.insert(TE, HeaderValue::from_static("trailers"));
    Ok((target, hop_by_hop, declared_trailers))
}

/// Apply nginx-style `proxy_set_header` entries onto the upstream request map.
/// Empty list keeps the fixed safe forwarding defaults from
/// [`forwarded_request_headers`].
fn apply_proxy_set_headers(
    target: &mut HeaderMap,
    entries: &[String],
    source: &HeaderMap,
    client_ip: IpAddr,
    external_scheme: &str,
    external_authority: &str,
    listener_port: u16,
) -> Result<(), ()> {
    for entry in entries {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (name, value_template) = trimmed
            .split_once(char::is_whitespace)
            .map(|(n, v)| (n.trim(), v.trim()))
            .unwrap_or((trimmed, ""));
        let header_name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| ())?;
        let expanded = expand_proxy_header_value(
            value_template,
            source,
            client_ip,
            external_scheme,
            external_authority,
            listener_port,
            target,
        )?;
        if expanded.is_empty() {
            target.remove(header_name);
            continue;
        }
        let header_value = HeaderValue::from_str(&expanded).map_err(|_| ())?;
        target.insert(header_name, header_value);
    }
    Ok(())
}

fn expand_proxy_header_value(
    template: &str,
    source: &HeaderMap,
    client_ip: IpAddr,
    external_scheme: &str,
    external_authority: &str,
    listener_port: u16,
    current: &HeaderMap,
) -> Result<String, ()> {
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'$' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        i += 1;
        if i < bytes.len() && bytes[i] == b'$' {
            out.push('$');
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        let var = std::str::from_utf8(&bytes[start..i]).map_err(|_| ())?;
        match var {
            "host" => {
                let host = external_authority
                    .split(':')
                    .next()
                    .unwrap_or(external_authority);
                out.push_str(host);
            }
            "scheme" => out.push_str(external_scheme),
            "remote_addr" => out.push_str(&client_ip.to_string()),
            "proxy_add_x_forwarded_for" => {
                let existing = current
                    .get("x-forwarded-for")
                    .and_then(|v| v.to_str().ok())
                    .or_else(|| source.get("x-forwarded-for").and_then(|v| v.to_str().ok()));
                match existing {
                    Some(prior) if !prior.is_empty() => {
                        out.push_str(prior);
                        out.push_str(", ");
                        out.push_str(&client_ip.to_string());
                    }
                    _ => out.push_str(&client_ip.to_string()),
                }
            }
            "http_upgrade" => {
                if let Some(value) = source.get(UPGRADE).and_then(|v| v.to_str().ok()) {
                    out.push_str(value);
                }
            }
            "server_port" => {
                // nginx `$server_port`: the listener port, not the Host
                // header's port.
                if listener_port != 0 {
                    out.push_str(&listener_port.to_string());
                } else {
                    let port = external_authority
                        .rsplit_once(':')
                        .and_then(|(_, port)| {
                            if port.parse::<u16>().is_ok() {
                                Some(port)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(match external_scheme {
                            "https" => "443",
                            _ => "80",
                        });
                    out.push_str(port);
                }
            }
            _ => return Err(()),
        }
    }
    Ok(out)
}

fn forwarded_response_headers(
    source: &HeaderMap,
    maximum_trailer_bytes: usize,
    maximum_trailers: usize,
) -> Result<(HeaderMap, HashSet<HeaderName>, HashSet<HeaderName>), ()> {
    let hop_by_hop = hop_by_hop_headers(source);
    let declared_trailers =
        validate_trailer_declaration(source, maximum_trailer_bytes, maximum_trailers, &hop_by_hop)?;
    let mut target = HeaderMap::new();
    for (name, value) in source {
        if !hop_by_hop.contains(name) {
            target.append(name.clone(), value.clone());
        }
    }
    Ok((target, hop_by_hop, declared_trailers))
}

fn hop_by_hop_headers(headers: &HeaderMap) -> HashSet<HeaderName> {
    let mut names = HashSet::from([
        CONNECTION,
        HeaderName::from_static("keep-alive"),
        PROXY_AUTHENTICATE,
        PROXY_AUTHORIZATION,
        TE,
        TRANSFER_ENCODING,
        UPGRADE,
        HeaderName::from_static("proxy-connection"),
    ]);
    for value in headers.get_all(CONNECTION) {
        if let Ok(value) = value.to_str() {
            for token in value.split(',').map(str::trim) {
                if let Ok(name) = HeaderName::from_bytes(token.as_bytes()) {
                    names.insert(name);
                }
            }
        }
    }
    names
}

pub(crate) fn text_response(status: StatusCode, body: &'static str) -> Response<Body> {
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

fn upstream_unavailable_response(body: &'static str) -> Response<Body> {
    let mut response = text_response(StatusCode::SERVICE_UNAVAILABLE, body);
    response
        .headers_mut()
        .insert(RETRY_AFTER, HeaderValue::from_static("1"));
    response
}

pub(super) fn request_body_timeout_response(version: Version) -> Response<Body> {
    let mut response = text_response(
        StatusCode::REQUEST_TIMEOUT,
        "request Body progress timed out\n",
    );
    if version != Version::HTTP_2 {
        response
            .headers_mut()
            .insert(CONNECTION, HeaderValue::from_static("close"));
    }
    response
}

#[cfg(test)]
mod tests;
