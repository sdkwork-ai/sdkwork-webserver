//! nginx `slice <size>` (`ngx_http_slice_filter_module`).
//!
//! nginx turns one client request into a sequence of upstream Range exchanges
//! and concatenates the `206` slice responses into the client representation.
//! The operator wires the module with
//!
//! ```nginx
//! slice 1m;
//! proxy_cache cache;
//! proxy_cache_key "$uri|$slice_range";
//! proxy_set_header Range $slice_range;
//! ```
//!
//! This module reproduces the observable pipeline for the proxy adapter:
//!
//! - `$slice_range` renders `bytes=<start>-<start+size-1>` for the slice
//!   currently being fetched; its initial value aligns the client `Range`
//!   start down to a slice boundary exactly like
//!   `ngx_http_slice_get_start` (a multi-range or `If-Range` request starts at
//!   0, a suffix range starts at 0 and jumps later);
//! - every upstream answer must be a `206` whose `Content-Range` starts at the
//!   expected offset and ends at `min(start + size, complete_length)`; the
//!   `ETag` must be identical to the first slice's (nginx's `etag mismatch in
//!   slice response` check), and a mismatch aborts the response mid-stream
//!   exactly like nginx;
//! - each slice is cached under its own cache key (`base key + slice range`),
//!   the structural equivalent of `proxy_cache_key "$uri|$slice_range"`;
//! - the client response is the nginx range-filter result over the assembled
//!   representation: a full `200` (with `Accept-Ranges: bytes`), a single
//!   `206` with `Content-Range`, or the range filter's `416` for an
//!   unsatisfiable request. `single_range` is set like nginx's slice filter,
//!   so a multi-range client request answers the whole representation;
//! - exactly one slice is buffered at a time (`slice size` bounded at
//!   [`RouteConfig::MAXIMUM_SLICE_BYTES`]), so assembly memory never grows
//!   with the representation: full `GET`s stream slice by slice.
//!
//! Deliberate bounds relative to nginx, all fail-closed:
//!
//! - only bodyless `GET`/`HEAD` requests are sliced; a request with a body
//!   cannot replay it per slice, so it keeps the ordinary one-exchange path;
//! - a malformed first slice (invalid `Content-Range`, missing complete
//!   length, or an unexpected start/end) answers `502`; nginx aborts the
//!   connection without a response, which the runtime cannot express through
//!   its HTTP/1 and HTTP/2 writers;
//! - a slice body longer than its declared `Content-Range` span is treated as
//!   an assembly failure instead of being forwarded;
//! - a conditional request (`If-None-Match` / `If-Modified-Since`) is still
//!   forwarded to the upstream and the assembled representation is answered
//!   as-is. nginx additionally runs its `not_modified` filter over the
//!   synthesized `200`, so a matching validator would answer `304`; conditional
//!   handling over assembled slices is outside this round's required semantics
//!   (the whole-object cache path still answers `304`), and the case is not
//!   claimed as differential evidence;
//! - `HEAD` responses are never stored in the slice cache (a stored `HEAD`
//!   slice body is empty, and caching it under the `GET` key would corrupt a
//!   later `GET`); a repeated `HEAD` therefore re-fetches its single slice,
//!   while the response bytes remain identical to nginx.

use std::{net::IpAddr, sync::Arc, time::Duration};

use axum::{
    body::Body,
    http::{
        header::{
            ACCEPT_RANGES, AUTHORIZATION, CACHE_CONTROL, CONNECTION, CONTENT_LENGTH, CONTENT_RANGE,
            ETAG, EXPIRES, IF_RANGE, LAST_MODIFIED, RANGE, SET_COOKIE, TRANSFER_ENCODING,
        },
        HeaderMap, HeaderName, HeaderValue, Method, Request, Response, StatusCode, Uri, Version,
    },
};
use bytes::Bytes;
use futures_util::StreamExt;
use sdkwork_webserver_core::{GeoIpRequestState, ProxyResponsePolicyConfig, RouteConfig};

use super::{
    cache::{CacheDecision, CacheKey, CachedResponse, HttpResponseCache, ResponseMetadata},
    map_variables::MapResults,
    metrics::DataPlaneMetrics,
    proxy::{proxy_request, ProxyRequestContext},
    proxy_body::RequestBodyFailure,
    request_variables::RequestVariables,
    runtime::RuntimeGeneration,
    tunnel::TunnelSupervisor,
};

/// The `$slice_range` value the proxy pipeline resolved for this response.
///
/// nginx registers `$slice_range` in the variable table of
/// `ngx_http_slice_filter_module`, so the headers filter (`add_header`) can
/// reference it. The value is response-scoped because the slice header filter
/// *drops* the slice context when the upstream does not answer with the
/// expected `206` — `$slice_range` then renders as the empty string.
#[derive(Clone)]
pub(crate) struct SliceRangeHeaderValue(pub String);

/// Whether the effective proxy request headers wire the slice module.
///
/// nginx only slices when the operator forwards `$slice_range` to the upstream
/// (`proxy_set_header Range $slice_range`). Without that wiring the range
/// variable is never evaluated, the slice filter has no context, and a `206`
/// from the upstream is passed through untouched. Matching the wiring exactly
/// keeps a `slice` declaration harmless until the operator completes it.
pub(crate) fn slice_wired(request_set_headers: &[String]) -> bool {
    request_set_headers.iter().any(|entry| {
        let trimmed = entry.trim();
        let Some((name, value)) = trimmed.split_once(char::is_whitespace) else {
            return false;
        };
        name.trim().eq_ignore_ascii_case("range") && value.contains("$slice_range")
    })
}

/// Build the initial `$slice_range` for a request, or `None` when `slice` is
/// not configured (nginx renders the variable as the empty string then).
pub(crate) fn initial_slice_range(headers: &HeaderMap, slice_size: Option<u64>) -> Option<String> {
    let size = slice_size.filter(|size| *size > 0)?;
    let start = initial_slice_start(headers, size);
    Some(slice_range_value(start, size))
}

/// nginx `ngx_http_slice_get_start` plus the slice-boundary rounding of
/// `ngx_http_slice_range_variable`: the start of the first slice.
fn initial_slice_start(headers: &HeaderMap, size: u64) -> u64 {
    let start = slice_get_start(headers);
    size * (start / size)
}

/// nginx `ngx_http_slice_get_start`: the requested offset before rounding.
fn slice_get_start(headers: &HeaderMap) -> u64 {
    // `If-Range` makes nginx start at the beginning: the range filter decides
    // later whether the validator matches.
    if headers.contains_key(IF_RANGE) {
        return 0;
    }
    let Some(value) = headers.get(RANGE).and_then(|value| value.to_str().ok()) else {
        return 0;
    };
    let Some(spec) = value
        .get(..6)
        .filter(|prefix| prefix.eq_ignore_ascii_case("bytes="))
        .map(|_| &value[6..])
    else {
        return 0;
    };
    // Multi-range requests start at 0 in nginx (`ngx_strchr(p, ',')`).
    if spec.contains(',') {
        return 0;
    }
    let spec = spec.trim_start_matches(' ');
    if spec.starts_with('-') {
        return 0;
    }
    let digits = spec.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return 0;
    }
    spec[..digits].parse::<u64>().unwrap_or(0)
}

/// `bytes=<start>-<start + size - 1>`, saturating like nginx's `off_t`
/// arithmetic.
fn slice_range_value(start: u64, size: u64) -> String {
    format!(
        "bytes={start}-{}",
        start.saturating_add(size).saturating_sub(1)
    )
}

/// nginx's `Content-Range: bytes <start>-<end>/<complete>` response header
/// (`ngx_http_slice_parse_content_range`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ContentRange {
    start: u64,
    /// Exclusive end (`end + 1`, as nginx keeps it).
    end: u64,
    complete_length: u64,
}

fn parse_content_range(value: &str) -> Option<ContentRange> {
    let rest = value.strip_prefix("bytes ")?;
    let rest = rest.trim_start_matches(' ');
    let (range, complete) = rest.split_once('/')?;
    let complete = complete.trim_start_matches(' ');
    if complete == "*" {
        // nginx treats a missing complete length as a slice failure.
        return None;
    }
    let (start, end) = range.split_once('-')?;
    let start = start.trim().parse::<u64>().ok()?;
    let end = end.trim().parse::<u64>().ok()?.checked_add(1)?;
    let complete_length = complete.trim().parse::<u64>().ok()?;
    Some(ContentRange {
        start,
        end,
        complete_length,
    })
}

/// One validated upstream slice.
#[derive(Debug, Clone)]
struct SliceMeta {
    start: u64,
    /// Exclusive end of the slice actually served.
    end: u64,
    complete_length: u64,
    etag: Option<String>,
}

/// Validate one upstream slice against the slice-pipeline invariants.
///
/// `anchor` is the ETag the first slice established. nginx compares every
/// later slice's ETag with it (and adopts a later ETag when the first slice had
/// none); a mismatch aborts the assembly.
fn validate_slice(
    status: StatusCode,
    headers: &HeaderMap,
    expected_start: u64,
    slice_size: u64,
    anchor: Option<&str>,
) -> Result<SliceMeta, ()> {
    if status != StatusCode::PARTIAL_CONTENT {
        return Err(());
    }
    let response_etag = headers
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let effective_etag = match (anchor, response_etag.as_deref()) {
        (Some(anchor), Some(value)) if anchor != value => return Err(()),
        (Some(_), None) => return Err(()),
        (Some(anchor), _) => Some(anchor.to_owned()),
        (None, Some(value)) => Some(value.to_owned()),
        (None, None) => None,
    };
    let content_range = headers
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_content_range)
        .ok_or(())?;
    if content_range.start != expected_start {
        return Err(());
    }
    let expected_end = expected_start
        .saturating_add(slice_size)
        .min(content_range.complete_length);
    if content_range.end != expected_end {
        return Err(());
    }
    Ok(SliceMeta {
        start: content_range.start,
        end: content_range.end,
        complete_length: content_range.complete_length,
        etag: effective_etag,
    })
}

/// The client-facing outcome of nginx's range filter (`single_range = 1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClientRange {
    /// Serve the whole representation with `200` and `Accept-Ranges: bytes`.
    Full,
    /// Serve one range with `206` and `Content-Range`; `end` is exclusive.
    Partial { start: u64, end: u64 },
    /// `416` with `Content-Range: bytes */<complete>`.
    NotSatisfiable,
}

/// nginx `ngx_http_range_parse` with the `single_range` cap the slice filter
/// sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParsedRange {
    Single {
        start: u64,
        end: u64,
    },
    /// nginx `NGX_DECLINED`: fall through to the whole representation.
    Declined,
    /// nginx `NGX_HTTP_RANGE_NOT_SATISFIABLE`.
    NotSatisfiable,
}

/// Parse one decimal run, refusing overflow exactly like nginx's `off_t`
/// cut-off check.
fn parse_digits(bytes: &[u8], index: &mut usize) -> Option<u64> {
    let start = *index;
    while bytes.get(*index).is_some_and(u8::is_ascii_digit) {
        *index += 1;
    }
    if start == *index {
        return None;
    }
    std::str::from_utf8(&bytes[start..*index])
        .ok()?
        .parse::<u64>()
        .ok()
}

/// nginx `ngx_http_range_parse` capped to a single range.
fn parse_client_range(value: &str, length: u64) -> ParsedRange {
    let Some(spec) = value
        .get(..6)
        .filter(|prefix| prefix.eq_ignore_ascii_case("bytes="))
        .map(|_| &value[6..])
    else {
        return ParsedRange::Declined;
    };
    let bytes = spec.as_bytes();
    let mut index = 0usize;
    let mut single: Option<(u64, u64)> = None;
    loop {
        let mut start = 0u64;
        let mut end;
        while bytes.get(index) == Some(&b' ') {
            index += 1;
        }
        let suffix = if bytes.get(index) == Some(&b'-') {
            index += 1;
            true
        } else {
            start = match parse_digits(bytes, &mut index) {
                Some(start) => start,
                None => return ParsedRange::NotSatisfiable,
            };
            while bytes.get(index) == Some(&b' ') {
                index += 1;
            }
            if bytes.get(index) != Some(&b'-') {
                return ParsedRange::NotSatisfiable;
            }
            index += 1;
            while bytes.get(index) == Some(&b' ') {
                index += 1;
            }
            if matches!(bytes.get(index), None | Some(b',')) {
                // `bytes=<start>-`: the range runs to the end.
                if let Some(outcome) = record_range(start, length, &mut single) {
                    return outcome;
                }
                if bytes.get(index) == Some(&b',') {
                    index += 1;
                    continue;
                }
                return single_outcome(single);
            }
            false
        };
        end = match parse_digits(bytes, &mut index) {
            Some(end) => end,
            None => return ParsedRange::NotSatisfiable,
        };
        while bytes.get(index) == Some(&b' ') {
            index += 1;
        }
        if !matches!(bytes.get(index), None | Some(b',')) {
            return ParsedRange::NotSatisfiable;
        }
        if suffix {
            start = length.saturating_sub(end);
            end = length;
        } else if end >= length {
            end = length;
        } else {
            end += 1;
        }
        if let Some(outcome) = record_range(start, end, &mut single) {
            return outcome;
        }
        if bytes.get(index) == Some(&b',') {
            index += 1;
            continue;
        }
        return single_outcome(single);
    }
}

/// nginx's push/decline rule for one parsed range. Returns `Some(Declined)`
/// when a second valid range trips the `single_range` cap or a zero-length
/// range at offset 0 declines; `None` means "keep going".
fn record_range(start: u64, end: u64, single: &mut Option<(u64, u64)>) -> Option<ParsedRange> {
    if start < end {
        if single.is_some() {
            return Some(ParsedRange::Declined);
        }
        *single = Some((start, end));
        None
    } else if start == 0 {
        Some(ParsedRange::Declined)
    } else {
        None
    }
}

/// nginx's end-of-header decision: no pushed range is a `416`, one is the
/// single-part `206`.
fn single_outcome(single: Option<(u64, u64)>) -> ParsedRange {
    match single {
        Some((start, end)) => ParsedRange::Single { start, end },
        None => ParsedRange::NotSatisfiable,
    }
}

/// nginx `ngx_http_range_header_filter` over the assembled representation.
///
/// `max_ranges` participates exactly like nginx: `max_ranges 0` disables range
/// processing entirely, so even a valid `Range` answers the whole
/// representation (`single_range = 1` caps the accepted range count at one, so
/// a positive ceiling never changes the slice outcome).
fn client_range(
    headers: &HeaderMap,
    complete_length: u64,
    response_headers: &HeaderMap,
    max_ranges: Option<u64>,
) -> ClientRange {
    if max_ranges == Some(0) {
        return ClientRange::Full;
    }
    let Some(raw_range) = headers.get(RANGE).and_then(|value| value.to_str().ok()) else {
        return ClientRange::Full;
    };
    // nginx ignores a `Range` value shorter than `bytes=` plus one character.
    if raw_range.len() < 7
        || !raw_range
            .get(..6)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bytes="))
    {
        return ClientRange::Full;
    }
    if let Some(if_range) = headers.get(IF_RANGE).and_then(|value| value.to_str().ok()) {
        if !if_range_allows(if_range, response_headers) {
            return ClientRange::Full;
        }
    }
    match parse_client_range(raw_range, complete_length) {
        ParsedRange::Single { start, end } => ClientRange::Partial { start, end },
        ParsedRange::Declined => ClientRange::Full,
        ParsedRange::NotSatisfiable => ClientRange::NotSatisfiable,
    }
}

/// nginx's `If-Range` gate: a quoted validator is compared with the response
/// `ETag`, an HTTP date with `Last-Modified`.
fn if_range_allows(value: &str, response_headers: &HeaderMap) -> bool {
    if value.ends_with('"') {
        return response_headers
            .get(ETAG)
            .and_then(|etag| etag.to_str().ok())
            .is_some_and(|etag| etag == value);
    }
    let last_modified = response_headers
        .get(LAST_MODIFIED)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| httpdate::parse_http_date(value).ok());
    let if_range = httpdate::parse_http_date(value).ok();
    matches!((last_modified, if_range), (Some(left), Some(right)) if left == right)
}

/// Bounded reader for one upstream slice body.
async fn read_slice_body(body: Body, limit: u64) -> Result<Bytes, ()> {
    let mut stream = body.into_data_stream();
    let mut buffer = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ())?;
        if buffer.len() as u64 + chunk.len() as u64 > limit {
            return Err(());
        }
        buffer.extend_from_slice(&chunk);
    }
    Ok(Bytes::from(buffer))
}

/// Everything the spawned assembly task needs, owned so the task is `'static`.
struct SliceFetcher {
    generation: Arc<RuntimeGeneration>,
    upstream_ref: String,
    strip_prefix: bool,
    target_uri: Option<String>,
    request_set_headers: Vec<String>,
    proxy_pass_request_headers: bool,
    route: RouteConfig,
    client_ip: IpAddr,
    external_scheme: String,
    external_authority: String,
    listener_port: u16,
    normalized_path: String,
    path_rewritten: bool,
    request_failure: RequestBodyFailure,
    tunnel_supervisor: Arc<TunnelSupervisor>,
    metrics: Arc<DataPlaneMetrics>,
    cache: Option<Arc<HttpResponseCache>>,
    map_results: MapResults,
    uid_got: String,
    uid_set: String,
    modern_browser: String,
    ancient_browser: String,
    geoip: Option<GeoIpRequestState>,
    response_policy: Option<ProxyResponsePolicyConfig>,
    proxy_method: Option<String>,
    variables: Option<RequestVariables>,
    method: Method,
    uri: Uri,
    version: Version,
    request_headers: HeaderMap,
}

impl SliceFetcher {
    fn new(context: &ProxyRequestContext<'_>, request: &axum::http::request::Parts) -> Self {
        Self {
            generation: Arc::clone(context.generation),
            upstream_ref: context.upstream_ref.to_owned(),
            strip_prefix: context.strip_prefix,
            target_uri: context.target_uri.map(str::to_owned),
            request_set_headers: context.request_set_headers.to_vec(),
            proxy_pass_request_headers: context.proxy_pass_request_headers,
            route: context.route.clone(),
            client_ip: context.client_ip,
            external_scheme: context.external_scheme.to_owned(),
            external_authority: context.external_authority.to_owned(),
            listener_port: context.listener_port,
            normalized_path: context.normalized_path.to_owned(),
            path_rewritten: context.path_rewritten,
            request_failure: context.request_failure.clone(),
            tunnel_supervisor: Arc::clone(context.tunnel_supervisor),
            metrics: Arc::clone(context.metrics),
            cache: context.cache.clone(),
            map_results: context.map_results.clone(),
            uid_got: context.uid_got.to_owned(),
            uid_set: context.uid_set.to_owned(),
            modern_browser: context.modern_browser.to_owned(),
            ancient_browser: context.ancient_browser.to_owned(),
            geoip: context.geoip.cloned(),
            response_policy: context.response_policy.cloned(),
            proxy_method: context.proxy_method.map(str::to_owned),
            variables: context.variables.cloned(),
            method: request.method.clone(),
            uri: request.uri.clone(),
            version: request.version,
            request_headers: request.headers.clone(),
        }
    }

    fn context<'a>(&'a self, slice_range: &'a str) -> ProxyRequestContext<'a> {
        ProxyRequestContext {
            generation: &self.generation,
            upstream_ref: &self.upstream_ref,
            strip_prefix: self.strip_prefix,
            target_uri: self.target_uri.as_deref(),
            request_set_headers: &self.request_set_headers,
            dynamic_target: None,
            proxy_pass_request_headers: self.proxy_pass_request_headers,
            route: &self.route,
            client_ip: self.client_ip,
            external_scheme: &self.external_scheme,
            external_authority: &self.external_authority,
            listener_port: self.listener_port,
            normalized_path: &self.normalized_path,
            path_rewritten: self.path_rewritten,
            request_failure: self.request_failure.clone(),
            tunnel_supervisor: &self.tunnel_supervisor,
            metrics: &self.metrics,
            cache: self.cache.clone(),
            map_results: &self.map_results,
            uid_got: &self.uid_got,
            uid_set: &self.uid_set,
            modern_browser: &self.modern_browser,
            ancient_browser: &self.ancient_browser,
            geoip: self.geoip.as_ref(),
            response_policy: self.response_policy.as_ref(),
            proxy_method: self.proxy_method.as_deref(),
            variables: self.variables.as_ref(),
            slice_range: Some(slice_range),
        }
    }

    /// Issue one upstream slice exchange.
    async fn fetch(&self, slice_range: &str) -> Response<Body> {
        let mut request = Request::builder()
            .method(self.method.clone())
            .uri(self.uri.clone())
            .version(self.version)
            .body(Body::empty())
            .unwrap_or_else(|_| Request::new(Body::empty()));
        *request.headers_mut() = self.request_headers.clone();
        proxy_request(self.context(slice_range), request).await
    }

    /// Whether a slice may be cached: only `GET` without authorization (a
    /// cached `HEAD` slice would store an empty body).
    fn cache_enabled(&self) -> bool {
        self.cache.is_some()
            && self.route.proxy_cache != Some(false)
            && self.method == Method::GET
            && !self.request_headers.contains_key(AUTHORIZATION)
    }

    fn cache_key(&self, slice_range: &str, vary_names: &[String]) -> CacheKey {
        CacheKey::new(
            self.method.as_str(),
            &self.external_authority,
            &self.normalized_path,
            self.uri.query(),
            vary_names,
            &self.request_headers,
        )
        .with_variant(slice_range)
    }

    /// Look up one slice. A `Vary`-bearing stored slice is re-keyed with the
    /// request header values before the final lookup, exactly like the
    /// whole-object cache path.
    async fn cache_lookup(&self, slice_range: &str) -> Option<CachedResponse> {
        if !self.cache_enabled() {
            return None;
        }
        let cache = self.cache.as_ref()?;
        let base = self.cache_key(slice_range, &[]);
        let hit = cache.lookup(&base).await?;
        if hit.metadata.vary.is_empty() {
            return Some(hit);
        }
        let refined = self.cache_key(slice_range, &hit.metadata.vary);
        cache.lookup(&refined).await
    }

    /// Store one slice body under its slice-specific key. A refusal to store
    /// never fails the client response.
    async fn cache_store(&self, slice_range: &str, response_headers: &HeaderMap, body: &Bytes) {
        if !self.cache_enabled() {
            return;
        }
        let Some(cache) = self.cache.as_ref() else {
            return;
        };
        if response_headers.get_all(SET_COOKIE).iter().next().is_some() {
            return;
        }
        let cache_control = response_headers
            .get(CACHE_CONTROL)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if cache_control.split(',').any(|token| {
            let token = token.trim();
            token == "no-store" || token == "private"
        }) {
            return;
        }
        let freshness = super::cache::freshness_for(
            response_headers
                .get(CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            response_headers
                .get(EXPIRES)
                .and_then(|value| value.to_str().ok()),
            response_headers
                .get("date")
                .and_then(|value| value.to_str().ok()),
        );
        let vary = response_headers
            .get_all("vary")
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(super::cache::parse_vary_header)
            .collect::<Vec<_>>();
        let metadata = ResponseMetadata {
            status: StatusCode::PARTIAL_CONTENT.as_u16(),
            headers: response_headers
                .iter()
                .filter(|(name, _)| {
                    !matches!(*name, &CONTENT_LENGTH | &TRANSFER_ENCODING | &CONNECTION)
                })
                .map(|(name, value)| {
                    (
                        name.as_str().to_owned(),
                        value.to_str().unwrap_or_default().to_owned(),
                    )
                })
                .collect(),
            vary: vary.clone(),
            fresh_seconds: freshness.fresh_seconds.unwrap_or(0),
        };
        let key = self.cache_key(slice_range, &vary);
        cache
            .insert(
                key,
                metadata,
                body.clone(),
                CacheDecision {
                    cacheable: true,
                    ttl: freshness.fresh_seconds.map(Duration::from_secs),
                },
            )
            .await;
    }
}

/// Rebuild the stored slice headers for re-validation.
fn cached_slice_headers(entry: &CachedResponse) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in &entry.metadata.headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            headers.append(name, value);
        }
    }
    headers
}

/// Forward the portion of one slice body the client still needs.
async fn send_slice_portion(
    tx: &tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
    body: &Bytes,
    slice_start: u64,
    range_start: u64,
    range_end: u64,
) -> Result<(), ()> {
    let body_end = slice_start.saturating_add(body.len() as u64);
    let from = range_start.max(slice_start).min(body_end);
    let to = range_end.min(body_end).max(from);
    if from >= to {
        return Ok(());
    }
    let start = usize::try_from(from - slice_start).map_err(|_| ())?;
    let end = usize::try_from(to - slice_start).map_err(|_| ())?;
    tx.send(Ok(body.slice(start..end))).await.map_err(|_| ())
}

/// Read one complete slice body and enforce the declared span.
async fn read_exact_slice(body: Body, meta: &SliceMeta) -> Result<Bytes, ()> {
    let limit = meta.end.saturating_sub(meta.start);
    let body = read_slice_body(body, limit).await?;
    if body.len() as u64 == limit {
        Ok(body)
    } else {
        Err(())
    }
}

/// Proxy one client request through the nginx slice pipeline.
pub(super) async fn proxy_sliced(
    context: ProxyRequestContext<'_>,
    request: Request<Body>,
    slice_size: u64,
) -> Response<Body> {
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        // A body-carrying request cannot replay its body for every slice.
        return proxy_request(context, request).await;
    }
    let (parts, body) = request.into_parts();
    drop(body);
    let client_headers = parts.headers.clone();
    let initial_start = initial_slice_start(&client_headers, slice_size);
    let initial_range = initial_slice_range(&client_headers, Some(slice_size))
        .expect("a non-zero slice size always yields an initial range");
    let fetcher = SliceFetcher::new(&context, &parts);
    // The first slice is a cache lookup like every other slice: a warm entry
    // must serve it without a new upstream exchange (nginx's first subrequest
    // is cached under the same per-range key).
    let first_cached = match fetcher.cache_lookup(&initial_range).await {
        Some(entry) => {
            let headers = cached_slice_headers(&entry);
            if validate_slice(
                StatusCode::PARTIAL_CONTENT,
                &headers,
                initial_start,
                slice_size,
                None,
            )
            .is_ok()
            {
                let mut response = Response::new(Body::from(entry.body));
                *response.status_mut() = StatusCode::PARTIAL_CONTENT;
                *response.headers_mut() = headers;
                Some(response)
            } else {
                None
            }
        }
        None => None,
    };
    let first_from_cache = first_cached.is_some();
    let first_response = match first_cached {
        Some(response) => response,
        None => fetcher.fetch(&initial_range).await,
    };
    // nginx's slice header filter passes a non-206 first answer through
    // untouched (and drops the slice context, so `$slice_range` stays empty).
    if first_response.status() != StatusCode::PARTIAL_CONTENT {
        return first_response;
    }
    let first_meta = match validate_slice(
        first_response.status(),
        first_response.headers(),
        initial_start,
        slice_size,
        None,
    ) {
        Ok(meta) => meta,
        Err(()) => return bad_gateway("invalid slice response\n"),
    };
    let first_headers = first_response.headers().clone();
    let outcome = client_range(
        &client_headers,
        first_meta.complete_length,
        first_response.headers(),
        context.route.max_ranges,
    );
    let (mut response_parts, first_body) = first_response.into_parts();
    remove_slice_hop_headers(&mut response_parts.headers);
    response_parts
        .extensions
        .insert(SliceRangeHeaderValue(initial_range.clone()));
    let (range_start, range_end) = match outcome {
        ClientRange::NotSatisfiable => {
            drop(first_body);
            response_parts.status = StatusCode::RANGE_NOT_SATISFIABLE;
            response_parts.headers.insert(
                CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes */{}", first_meta.complete_length))
                    .unwrap_or_else(|_| HeaderValue::from_static("bytes */0")),
            );
            response_parts.headers.remove(CONTENT_LENGTH);
            return Response::from_parts(response_parts, Body::empty());
        }
        ClientRange::Full => {
            response_parts.status = StatusCode::OK;
            set_content_length(&mut response_parts.headers, first_meta.complete_length);
            response_parts
                .headers
                .insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
            (0, first_meta.complete_length)
        }
        ClientRange::Partial { start, end } => {
            response_parts.status = StatusCode::PARTIAL_CONTENT;
            set_content_length(&mut response_parts.headers, end.saturating_sub(start));
            response_parts.headers.insert(
                CONTENT_RANGE,
                HeaderValue::from_str(&format!(
                    "bytes {start}-{}/{total}",
                    end.saturating_sub(1),
                    total = first_meta.complete_length
                ))
                .unwrap_or_else(|_| HeaderValue::from_static("bytes */0")),
            );
            (start, end)
        }
    };
    // HEAD assembles headers only: nginx fetches exactly one slice and never
    // runs the body-filter subrequest loop for a bodyless request.
    if parts.method == Method::HEAD {
        drop(first_body);
        return Response::from_parts(response_parts, Body::empty());
    }
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(2);
    let mut anchor = first_meta.etag.clone();
    tokio::spawn(async move {
        // The first slice is already in hand; the assembly loop starts at the
        // offset nginx's slice header filter records, including the range
        // filter's jump for a suffix range.
        let mut start = first_meta.end;
        if matches!(outcome, ClientRange::Partial { .. })
            && start.saturating_add(slice_size) <= range_start
        {
            start = slice_size * (range_start / slice_size);
        }
        let first_meta_for_anchor = SliceMeta {
            start: first_meta.start,
            end: first_meta.end,
            complete_length: first_meta.complete_length,
            etag: anchor.clone(),
        };
        let first_body = match read_exact_slice(first_body, &first_meta_for_anchor).await {
            Ok(body) => body,
            Err(()) => {
                let _ = tx
                    .send(Err(std::io::Error::other("invalid slice body")))
                    .await;
                return;
            }
        };
        if !first_from_cache {
            fetcher
                .cache_store(&initial_range, &first_headers, &first_body)
                .await;
        }
        if send_slice_portion(
            &tx,
            &first_body,
            first_meta_for_anchor.start,
            range_start,
            range_end,
        )
        .await
        .is_err()
        {
            return;
        }
        while start < range_end {
            let range = slice_range_value(start, slice_size);
            // A stale/invalid stored slice must not poison the assembly: it is
            // validated here and a mismatch falls through to a fresh upstream
            // exchange (never an unbounded re-read of the same entry).
            let cached = match fetcher.cache_lookup(&range).await {
                Some(entry) => {
                    let headers = cached_slice_headers(&entry);
                    match validate_slice(
                        StatusCode::PARTIAL_CONTENT,
                        &headers,
                        start,
                        slice_size,
                        anchor.as_deref(),
                    ) {
                        Ok(meta)
                            if entry.body.len() as u64 == meta.end.saturating_sub(meta.start) =>
                        {
                            Some((meta, entry.body))
                        }
                        _ => None,
                    }
                }
                None => None,
            };
            let (meta, body) = match cached {
                Some(pair) => pair,
                None => {
                    let response = fetcher.fetch(&range).await;
                    let meta = match validate_slice(
                        response.status(),
                        response.headers(),
                        start,
                        slice_size,
                        anchor.as_deref(),
                    ) {
                        Ok(meta) => meta,
                        Err(()) => {
                            let _ = tx
                                .send(Err(std::io::Error::other(
                                    "upstream slice failed validation",
                                )))
                                .await;
                            return;
                        }
                    };
                    let response_headers = response.headers().clone();
                    let (_, upstream_body) = response.into_parts();
                    let body = match read_exact_slice(upstream_body, &meta).await {
                        Ok(body) => body,
                        Err(()) => {
                            let _ = tx
                                .send(Err(std::io::Error::other("upstream slice body invalid")))
                                .await;
                            return;
                        }
                    };
                    fetcher.cache_store(&range, &response_headers, &body).await;
                    (meta, body)
                }
            };
            // nginx adopts a later slice's ETag when the first slice had none,
            // and every following slice must match it.
            if anchor.is_none() {
                anchor = meta.etag.clone();
            }
            if send_slice_portion(&tx, &body, meta.start, range_start, range_end)
                .await
                .is_err()
            {
                return;
            }
            let next = meta.end;
            if next <= start {
                // No forward progress: never spin on a degenerate slice.
                let _ = tx
                    .send(Err(std::io::Error::other("slice assembly stalled")))
                    .await;
                return;
            }
            start = next;
        }
    });
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|frame| (frame, rx))
    });
    Response::from_parts(response_parts, Body::from_stream(stream))
}

fn remove_slice_hop_headers(headers: &mut HeaderMap) {
    headers.remove(CONTENT_LENGTH);
    headers.remove(CONTENT_RANGE);
    headers.remove(ACCEPT_RANGES);
    headers.remove(TRANSFER_ENCODING);
    headers.remove(CONNECTION);
}

fn set_content_length(headers: &mut HeaderMap, length: u64) {
    headers.insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&length.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );
}

fn bad_gateway(body: &'static str) -> Response<Body> {
    super::proxy::text_response(StatusCode::BAD_GATEWAY, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(entries: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in entries {
            headers.insert(
                HeaderName::from_bytes(name.as_bytes()).expect("name"),
                HeaderValue::from_str(value).expect("value"),
            );
        }
        headers
    }

    fn slice_headers(start: u64, end: u64, complete: u64, etag: &str) -> HeaderMap {
        let mut headers = headers(&[("content-range", "")]);
        headers.insert(
            CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {start}-{end}/{complete}")).expect("range"),
        );
        headers.insert(ETAG, HeaderValue::from_str(etag).expect("etag"));
        headers
    }

    #[test]
    fn initial_slice_start_matches_ngx_http_slice_get_start() {
        assert_eq!(slice_get_start(&HeaderMap::new()), 0);
        assert_eq!(
            initial_slice_start(&headers(&[("range", "bytes=1-2")]), 4),
            0
        );
        assert_eq!(
            initial_slice_start(&headers(&[("range", "bytes=10-13")]), 4),
            8
        );
        assert_eq!(
            initial_slice_start(&headers(&[("range", "bytes=1-2,5-6")]), 4),
            0,
            "a multi-range request starts at 0"
        );
        assert_eq!(
            initial_slice_start(&headers(&[("range", "bytes=-4")]), 4),
            0,
            "a suffix range starts at 0"
        );
        assert_eq!(
            initial_slice_start(
                &headers(&[("range", "bytes=10-13"), ("if-range", "\"v1\"")]),
                4
            ),
            0,
            "If-Range makes nginx start at 0"
        );
        assert_eq!(
            initial_slice_start(&headers(&[("range", "bytes=abc")]), 4),
            0
        );
        assert_eq!(slice_range_value(8, 4), "bytes=8-11");
        assert_eq!(slice_range_value(0, 4), "bytes=0-3");
    }

    #[test]
    fn client_range_parse_reproduces_ngx_http_range_parse() {
        assert_eq!(
            parse_client_range("bytes=1-2", 30),
            ParsedRange::Single { start: 1, end: 3 }
        );
        assert_eq!(
            parse_client_range("bytes=10-13", 30),
            ParsedRange::Single { start: 10, end: 14 }
        );
        assert_eq!(
            parse_client_range("bytes=20-", 30),
            ParsedRange::Single { start: 20, end: 30 }
        );
        assert_eq!(
            parse_client_range("bytes=-4", 30),
            ParsedRange::Single { start: 26, end: 30 }
        );
        assert_eq!(
            parse_client_range("bytes=0-100", 30),
            ParsedRange::Single { start: 0, end: 30 },
            "an end beyond the representation is clamped"
        );
        assert_eq!(
            parse_client_range("bytes=100-", 30),
            ParsedRange::NotSatisfiable
        );
        assert_eq!(
            parse_client_range("bytes=abc", 30),
            ParsedRange::NotSatisfiable
        );
        assert_eq!(
            parse_client_range("bytes=1-2,5-6", 30),
            ParsedRange::Declined,
            "single_range makes a multi-range request serve the whole body"
        );
        assert_eq!(
            parse_client_range("bytes=1-2,abc", 30),
            ParsedRange::NotSatisfiable,
            "an invalid second range is a 416 before the cap applies"
        );
        assert_eq!(
            parse_client_range("bytes=5-2", 30),
            ParsedRange::NotSatisfiable
        );
        assert_eq!(
            parse_client_range("bytes=0-0", 30),
            ParsedRange::Single { start: 0, end: 1 }
        );
        assert_eq!(
            parse_client_range("bytes=", 30),
            ParsedRange::NotSatisfiable
        );
    }

    #[test]
    fn content_range_is_parsed_like_ngx_http_slice_parse_content_range() {
        assert_eq!(
            parse_content_range("bytes 8-11/30"),
            Some(ContentRange {
                start: 8,
                end: 12,
                complete_length: 30
            })
        );
        assert_eq!(parse_content_range("bytes 0-3/*"), None);
        assert_eq!(parse_content_range("bytes 0-3"), None);
        assert_eq!(parse_content_range("items 0-3/4"), None);
    }

    #[test]
    fn slice_validation_pins_start_end_and_etag() {
        let ok = validate_slice(
            StatusCode::PARTIAL_CONTENT,
            &slice_headers(8, 11, 30, "\"v1\""),
            8,
            4,
            Some("\"v1\""),
        )
        .expect("valid slice");
        assert_eq!(ok.end, 12);
        assert_eq!(ok.complete_length, 30);

        assert!(
            validate_slice(
                StatusCode::OK,
                &slice_headers(8, 11, 30, "\"v1\""),
                8,
                4,
                None
            )
            .is_err(),
            "only 206 is a slice response"
        );
        assert!(
            validate_slice(
                StatusCode::PARTIAL_CONTENT,
                &slice_headers(8, 11, 30, "\"v2\""),
                8,
                4,
                Some("\"v1\"")
            )
            .is_err(),
            "a changed ETag aborts the assembly"
        );
        assert!(
            validate_slice(
                StatusCode::PARTIAL_CONTENT,
                &headers(&[("content-range", "bytes 8-11/30")]),
                8,
                4,
                Some("\"v1\"")
            )
            .is_err(),
            "a slice that drops the anchored ETag aborts the assembly"
        );
        assert!(
            validate_slice(
                StatusCode::PARTIAL_CONTENT,
                &slice_headers(4, 7, 30, "\"v1\""),
                8,
                4,
                None
            )
            .is_err(),
            "an unexpected start aborts the assembly"
        );
        assert!(
            validate_slice(
                StatusCode::PARTIAL_CONTENT,
                &slice_headers(28, 29, 31, "\"v1\""),
                28,
                4,
                None
            )
            .is_err(),
            "an end that disagrees with the complete length aborts the assembly"
        );
        // The first slice without an ETag adopts a later one (nginx only
        // compares once an anchor exists).
        let adopted = validate_slice(
            StatusCode::PARTIAL_CONTENT,
            &slice_headers(4, 7, 30, "\"v2\""),
            4,
            4,
            None,
        )
        .expect("valid");
        assert_eq!(adopted.etag.as_deref(), Some("\"v2\""));
    }

    #[test]
    fn client_range_reproduces_the_nginx_range_filter() {
        assert_eq!(
            client_range(
                &HeaderMap::new(),
                30,
                &slice_headers(0, 3, 30, "\"v1\""),
                None
            ),
            ClientRange::Full
        );
        assert_eq!(
            client_range(
                &headers(&[("range", "bytes=10-13")]),
                30,
                &slice_headers(0, 3, 30, "\"v1\""),
                None
            ),
            ClientRange::Partial { start: 10, end: 14 }
        );
        assert_eq!(
            client_range(
                &headers(&[("range", "bytes=1-2,5-6")]),
                30,
                &slice_headers(0, 3, 30, "\"v1\""),
                None
            ),
            ClientRange::Full,
            "single_range declines a multi-range request"
        );
        assert_eq!(
            client_range(
                &headers(&[("range", "bytes=100-")]),
                30,
                &slice_headers(0, 3, 30, "\"v1\""),
                None
            ),
            ClientRange::NotSatisfiable
        );
        assert_eq!(
            client_range(
                &headers(&[("range", "bytes=10-13"), ("if-range", "\"nope\"")]),
                30,
                &slice_headers(0, 3, 30, "\"v1\""),
                None
            ),
            ClientRange::Full,
            "a mismatching If-Range serves the whole representation"
        );
        assert_eq!(
            client_range(
                &headers(&[("range", "bytes=10-13"), ("if-range", "\"v1\"")]),
                30,
                &slice_headers(0, 3, 30, "\"v1\""),
                None
            ),
            ClientRange::Partial { start: 10, end: 14 }
        );
        assert_eq!(
            client_range(
                &headers(&[("range", "bytes=10-13")]),
                30,
                &slice_headers(0, 3, 30, "\"v1\""),
                Some(0)
            ),
            ClientRange::Full,
            "max_ranges 0 disables range processing even for a sliced response"
        );
        assert_eq!(
            client_range(
                &headers(&[("range", "bytes=")]),
                30,
                &slice_headers(0, 3, 30, "\"v1\""),
                None
            ),
            ClientRange::Full,
            "a Range value shorter than the prefix plus one byte is ignored"
        );
    }

    #[test]
    fn slice_wiring_requires_the_range_header_to_reference_slice_range() {
        assert!(slice_wired(&["Range $slice_range".to_owned()]));
        assert!(slice_wired(&["range $slice_range".to_owned()]));
        assert!(slice_wired(&[
            "Host $host".to_owned(),
            "Range $slice_range".to_owned()
        ]));
        assert!(!slice_wired(&["X-Foo $slice_range".to_owned()]));
        assert!(!slice_wired(&["Range bytes=0-3".to_owned()]));
        assert!(!slice_wired(&[]));
    }

    #[test]
    fn slice_cache_keys_carry_the_slice_variant() {
        let empty = HeaderMap::new();
        let first = CacheKey::new("GET", "h", "/p", None, &[], &empty).with_variant("bytes=0-3");
        let second = CacheKey::new("GET", "h", "/p", None, &[], &empty).with_variant("bytes=4-7");
        let whole = CacheKey::new("GET", "h", "/p", None, &[], &empty);
        assert_ne!(first, second);
        assert_ne!(first, whole);
        assert_ne!(first.stable_hash_hex(), second.stable_hash_hex());
        assert_ne!(first.stable_hash_hex(), whole.stable_hash_hex());
    }
}
