use std::{io, ops::RangeInclusive, path::Path};

use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderValue, Method, Response, StatusCode},
};
use bytes::Bytes;
use futures_util::stream;
use httpdate::HttpDate;
use sdkwork_webserver_core::{CachePolicyConfig, IfModifiedSinceMode};
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

use super::static_path::OpenedStaticFile;

const FILE_CHUNK_BYTES: usize = 64 * 1024;

pub(crate) async fn serve_opened_file(
    opened: OpenedStaticFile,
    method: &Method,
    headers: &HeaderMap,
) -> Response<Body> {
    serve_opened_file_with_cache_policy(opened, method, headers, None).await
}

/// `serve_opened_file` with the route's nginx `etag` / `if_modified_since`
/// policy applied.
///
/// `etag off` suppresses the entity tag *generation* (nginx never removes an
/// upstream tag), and with no tag to compare, `If-None-Match` can no longer
/// answer 304 — only the `*` form still matches — while every concrete
/// `If-Match` candidate fails with 412, exactly as
/// `ngx_http_not_modified_filter` behaves. `if_modified_since` selects the
/// comparison `exact` (nginx's default) / `before` / `off`.
///
/// The precondition chain (`If-Unmodified-Since`, `If-Match`), the
/// `If-Modified-Since`-before-`If-None-Match` precedence, the validators kept on
/// a 304 and the `If-Range` gate on range handling all follow nginx's
/// `ngx_http_not_modified_header_filter` / `ngx_http_range_header_filter`.
pub(crate) async fn serve_opened_file_with_cache_policy(
    opened: OpenedStaticFile,
    method: &Method,
    headers: &HeaderMap,
    policy: Option<&CachePolicyConfig>,
) -> Response<Body> {
    let modified = opened.metadata.modified().ok().map(HttpDate::from);
    let etag_enabled = policy.map(|policy| policy.etag).unwrap_or(true);
    let etag = if etag_enabled {
        entity_tag(&opened.metadata)
    } else {
        None
    };
    // `ngx_http_not_modified_header_filter` order and precedence, exactly:
    // the two preconditions first, then If-Modified-Since, then If-None-Match.
    if !if_unmodified_since_passes(headers, modified) || !if_match_passes(headers, etag.as_deref())
    {
        return empty_response(StatusCode::PRECONDITION_FAILED);
    }
    let if_modified_since = policy
        .map(|policy| policy.if_modified_since)
        .unwrap_or_default();
    let ims_present = headers.contains_key(header::IF_MODIFIED_SINCE);
    let inm_present = headers.contains_key(header::IF_NONE_MATCH);
    if ims_present || inm_present {
        // nginx consults If-Modified-Since first: when that vote says the
        // representation is modified it is served and If-None-Match is never
        // examined, so a request carrying both can be answered 200 even though
        // its entity tag matches. Only when the If-Modified-Since vote says
        // "not modified" does a matching If-None-Match decide the 304.
        let serves_on_ims =
            ims_present && if_modified_since_is_modified(headers, modified, if_modified_since);
        let serves_on_inm =
            !serves_on_ims && inm_present && if_none_match_passes(headers, etag.as_deref());
        if !serves_on_ims && !serves_on_inm {
            return not_modified_response(&opened.path_hint, etag.as_deref(), modified);
        }
    }

    let size = opened.metadata.len();
    // `If-Range` gates the range handling: a mismatching validator makes the
    // server ignore `Range` and answer the whole representation with 200.
    let ranges = if if_range_allows_ranges(headers, etag.as_deref(), modified) {
        parse_range(headers, size)
    } else {
        None
    };
    let mime = mime_guess::from_path(&opened.path_hint)
        .first_raw()
        .and_then(|value| HeaderValue::from_str(value).ok())
        .unwrap_or_else(|| HeaderValue::from_static("application/octet-stream"));
    // nginx attaches `Accept-Ranges` from the range filter's fall-through path
    // only, which every non-partial outcome reaches; a 206 or 416 never carries
    // it, so advertising it on the partial response would be a difference the
    // differential battery can see.
    let partial = ranges.is_some();
    let mut builder = Response::builder()
        .header(header::CONTENT_TYPE, mime)
        // Deployment-level freshness policy (ENVIRONMENT_SPEC §13, SDKWORK_DEPLOY_SPEC
        // §8.1): fingerprinted immutable assets get long-lived caching, the public
        // runtime env document never caches, everything else revalidates through
        // ETag/Last-Modified conditional requests.
        .header(
            header::CACHE_CONTROL,
            cache_control_for_path(&opened.path_hint),
        );
    if !partial {
        builder = builder.header(header::ACCEPT_RANGES, "bytes");
    }
    if let Some(modified) = modified {
        builder = builder.header(header::LAST_MODIFIED, modified.to_string());
    }
    if let Some(etag) = etag {
        builder = builder.header(header::ETAG, etag);
    }

    match ranges {
        Some(Ok(ranges)) if ranges.len() == 1 => {
            let range = &ranges[0];
            let range_size = range.end().saturating_sub(*range.start()) + 1;
            let body = if method == Method::HEAD {
                Body::empty()
            } else {
                let mut file = tokio::fs::File::from_std(opened.file);
                if file.seek(SeekFrom::Start(*range.start())).await.is_err() {
                    return empty_response(StatusCode::INTERNAL_SERVER_ERROR);
                }
                file_body(file, range_size)
            };
            finish_response(
                builder
                    .status(StatusCode::PARTIAL_CONTENT)
                    .header(
                        header::CONTENT_RANGE,
                        format!("bytes {}-{}/{}", range.start(), range.end(), size),
                    )
                    .header(header::CONTENT_LENGTH, range_size),
                body,
            )
        }
        Some(Ok(_)) => {
            range_not_satisfiable(size, Body::from("Cannot serve multipart range requests"))
        }
        Some(Err(())) => range_not_satisfiable(size, Body::empty()),
        None => {
            let body = if method == Method::HEAD {
                Body::empty()
            } else {
                file_body(tokio::fs::File::from_std(opened.file), size)
            };
            finish_response(builder.header(header::CONTENT_LENGTH, size), body)
        }
    }
}

fn parse_range(headers: &HeaderMap, size: u64) -> Option<Result<Vec<RangeInclusive<u64>>, ()>> {
    let value = headers.get(header::RANGE)?.to_str().ok()?;
    Some(
        http_range_header::parse_range_header(value)
            .and_then(|range| range.validate(size))
            .map_err(|_| ()),
    )
}

/// Deployment-level `Cache-Control` freshness for a static file.
///
/// - `runtime-env.json`: `no-store` — the public runtime env document varies
///   per deployment (ENVIRONMENT_SPEC §13), so a cached copy can pin a browser
///   to a stale gateway/API origin set.
/// - Fingerprinted assets (`name-HASH.ext`, e.g. Vite `index-CqUZhiZ2.js`):
///   `public, max-age=31536000, immutable` — content-addressed, safe for
///   year-long caches and CDN edge retention.
/// - Everything else (SPA `index.html`, manifests, unfingerprinted files):
///   `public, no-cache` — storable, but revalidated with ETag/Last-Modified
///   before every reuse.
pub(crate) fn cache_control_for_path(path: &Path) -> &'static str {
    if path.file_name().and_then(|name| name.to_str()) == Some("runtime-env.json") {
        return "no-store";
    }
    if is_fingerprinted_asset(path) {
        return "public, max-age=31536000, immutable";
    }
    "public, no-cache"
}

/// Detects content-addressed asset filenames: a final `-<hash>` segment of
/// 8-32 URL-safe characters containing at least one digit (Vite/webpack
/// `name-HASH.ext` output). Requiring a digit keeps descriptive names such as
/// `main-module.js` or `font-awesome.svg` on the revalidating default.
fn is_fingerprinted_asset(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };
    // Accept the common `name-HASH.min` double-extension form as well.
    let stem = stem.strip_suffix(".min").unwrap_or(stem);
    let Some((_, hash)) = stem.rsplit_once('-') else {
        return false;
    };
    let byte_len = hash.len();
    if !(8..=32).contains(&byte_len) {
        return false;
    }
    hash.bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        && hash.bytes().any(|byte| byte.is_ascii_digit())
}

fn if_unmodified_since_passes(headers: &HeaderMap, modified: Option<HttpDate>) -> bool {
    let Some(condition) = parse_http_date_header(headers, header::IF_UNMODIFIED_SINCE) else {
        return true;
    };
    modified.is_some_and(|modified| condition >= modified)
}

/// Entity tag derived from modification time and size, in nginx's own format
/// (`"<mtime hex>-<size hex>"`) and with nginx's own strength: a *strong* tag.
///
/// The tag is a faithful validator for a static file, and its strength is
/// observable: `If-Range` requires the candidate to be byte-identical to the
/// served tag, so a weak tag would make every range revalidation fall back to a
/// full 200 (`ngx_http_range_header_filter`). `If-None-Match` stays RFC-correct
/// because the runtime strips a `W/` prefix from both sides before comparing.
fn entity_tag(metadata: &std::fs::Metadata) -> Option<String> {
    let modified = metadata.modified().ok()?;
    let seconds = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(format!("\"{seconds:x}-{:x}\"", metadata.len()))
}

/// RFC 9110 `If-None-Match`: any matching entity tag means the representation
/// is unchanged, so the request must not serve the body (the caller answers
/// 304). A `*` matches when the representation exists (it always does at this
/// point). `true` here means "serve the representation"; `false` means
/// "not modified".
///
/// With `etag` absent (`etag off`) there is no tag to compare against, so only
/// `*` can still match — mirroring `ngx_http_not_modified_filter`.
fn if_none_match_passes(headers: &HeaderMap, etag: Option<&str>) -> bool {
    let Some(condition) = headers.get(header::IF_NONE_MATCH) else {
        return true;
    };
    let Ok(condition) = condition.to_str() else {
        return true;
    };
    if condition.trim() == "*" {
        return false;
    }
    let Some(etag) = etag else {
        return true;
    };
    // Tag list is comma-separated; RFC 9110 weak comparison strips an
    // optional W/ prefix from both sides before comparing opaque tags.
    !condition
        .split(',')
        .any(|candidate| weak_tag_value(candidate) == weak_tag_value(etag))
}

/// Strip the optional RFC 9110 weakness prefix, returning the opaque tag.
fn weak_tag_value(tag: &str) -> &str {
    let trimmed = tag.trim();
    trimmed.strip_prefix("W/").unwrap_or(trimmed)
}

/// nginx `If-Match`, applied *before* the not-modified vote: nothing matching
/// means 412 (`ngx_http_not_modified_header_filter` ->
/// `ngx_http_test_if_match(..., weak = 0)`). The comparison is a whole-value
/// byte compare, so a `W/…` candidate never satisfies the strong tag a static
/// file carries, and a response without an entity tag (`etag off`) fails every
/// concrete candidate. `*` matches because the representation exists.
fn if_match_passes(headers: &HeaderMap, etag: Option<&str>) -> bool {
    let Some(condition) = headers.get(header::IF_MATCH) else {
        return true;
    };
    let Ok(condition) = condition.to_str() else {
        return true;
    };
    if condition.trim() == "*" {
        return true;
    }
    let Some(etag) = etag else {
        return false;
    };
    condition
        .split(',')
        .any(|candidate| candidate.trim() == etag)
}

/// nginx `If-Range`: a mismatch makes the server ignore the `Range` header and
/// serve the whole representation (`ngx_http_range_header_filter` ->
/// `goto next_filter`). The entity-tag form is recognised by its trailing `"`
/// and must be byte-identical to the served tag; the date form must equal
/// `Last-Modified` exactly. An unparsable value is a mismatch, as in nginx.
fn if_range_allows_ranges(
    headers: &HeaderMap,
    etag: Option<&str>,
    modified: Option<HttpDate>,
) -> bool {
    let Some(condition) = headers.get(header::IF_RANGE) else {
        return true;
    };
    let Ok(condition) = condition.to_str() else {
        return true;
    };
    if condition.len() >= 2 && condition.ends_with('"') {
        return etag == Some(condition);
    }
    let Some(modified) = modified else {
        return false;
    };
    httpdate::parse_http_date(condition)
        .ok()
        .map(HttpDate::from)
        == Some(modified)
}

/// A 304 keeps every cache validator: `ngx_http_not_modified_header_filter`
/// clears only `Content-Type`, `Content-Length` and `Accept-Ranges`, leaving
/// `ETag`, `Last-Modified` and `Cache-Control` so a shared cache can refresh the
/// stored metadata (RFC 9110 section 15.4.5). A bare 304 would leave the stored
/// entry without a validator and force a full refetch.
fn not_modified_response(
    path_hint: &Path,
    etag: Option<&str>,
    modified: Option<HttpDate>,
) -> Response<Body> {
    let mut builder = Response::builder()
        .status(StatusCode::NOT_MODIFIED)
        .header(header::CACHE_CONTROL, cache_control_for_path(path_hint));
    if let Some(modified) = modified {
        builder = builder.header(header::LAST_MODIFIED, modified.to_string());
    }
    if let Some(etag) = etag {
        builder = builder.header(header::ETAG, etag);
    }
    finish_response(builder, Body::empty())
}

/// nginx `if_modified_since`: `exact` (the default) requires the condition to
/// equal `Last-Modified`; `before` accepts a `Last-Modified` that is not newer
/// than the condition; `off` ignores the request header entirely.
fn if_modified_since_is_modified(
    headers: &HeaderMap,
    modified: Option<HttpDate>,
    mode: IfModifiedSinceMode,
) -> bool {
    if mode == IfModifiedSinceMode::Off {
        return true;
    }
    let Some(condition) = parse_http_date_header(headers, header::IF_MODIFIED_SINCE) else {
        return true;
    };
    modified.is_none_or(|modified| match mode {
        IfModifiedSinceMode::Exact => condition != modified,
        // `before` and `off` are separated above; `off` already returned.
        _ => condition < modified,
    })
}

fn parse_http_date_header(headers: &HeaderMap, name: header::HeaderName) -> Option<HttpDate> {
    headers
        .get(name)?
        .to_str()
        .ok()
        .and_then(|value| httpdate::parse_http_date(value).ok())
        .map(HttpDate::from)
}

fn file_body(file: tokio::fs::File, size: u64) -> Body {
    let chunks = stream::try_unfold((file, size), |(mut file, remaining)| async move {
        if remaining == 0 {
            return Ok(None);
        }
        let length = remaining.min(FILE_CHUNK_BYTES as u64) as usize;
        let mut buffer = vec![0_u8; length];
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "static file changed while streaming",
            ));
        }
        buffer.truncate(read);
        Ok(Some((
            Bytes::from(buffer),
            (file, remaining.saturating_sub(read as u64)),
        )))
    });
    Body::from_stream(chunks)
}

/// nginx answers an unsatisfiable range through its special-response handler:
/// the 416 keeps `Content-Range: bytes */<size>` and drops the matrix of
/// validators the content handler had prepared (`ngx_http_range_not_satisfiable`
/// clears the entity headers), because a validator on an error response cannot
/// be acted on. The body is the runtime's own rather than nginx's built-in page.
fn range_not_satisfiable(size: u64, body: Body) -> Response<Body> {
    finish_response(
        Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{size}")),
        body,
    )
}

fn empty_response(status: StatusCode) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Body::empty())
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

fn finish_response(builder: http::response::Builder, body: Body) -> Response<Body> {
    builder
        .body(body)
        .unwrap_or_else(|_| empty_response(StatusCode::INTERNAL_SERVER_ERROR))
}

#[cfg(test)]
mod tests {
    use std::{io::Write, path::PathBuf};

    use axum::body::to_bytes;
    use tempfile::NamedTempFile;

    use super::*;

    #[tokio::test]
    async fn streams_head_range_and_conditional_responses_from_open_handle() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"0123456789").unwrap();
        let opened_file = opened(&temp);
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=2-5"));
        let response = serve_opened_file(opened_file, &Method::GET, &headers).await;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes 2-5/10");
        assert_eq!(to_bytes(response.into_body(), 16).await.unwrap(), "2345");

        let opened_file = opened(&temp);
        let response = serve_opened_file(opened_file, &Method::HEAD, &HeaderMap::new()).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_LENGTH], "10");
        assert!(to_bytes(response.into_body(), 1).await.unwrap().is_empty());

        // nginx `if_modified_since exact` (the default) answers 304 only when
        // the condition equals `Last-Modified`: `ngx_http_test_if_modified`
        // returns "modified" for every non-equal condition, so a *newer*
        // condition still serves the representation. Verified against
        // nginx 1.29.6 by the differential battery.
        let opened_file = opened(&temp);
        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_MODIFIED_SINCE,
            HeaderValue::from_static("Fri, 31 Dec 9999 23:59:59 GMT"),
        );
        assert_eq!(
            serve_opened_file(opened_file, &Method::GET, &headers)
                .await
                .status(),
            StatusCode::OK
        );

        let last_modified =
            HttpDate::from(temp.as_file().metadata().unwrap().modified().unwrap()).to_string();
        let opened_file = opened(&temp);
        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_MODIFIED_SINCE,
            HeaderValue::from_str(&last_modified).unwrap(),
        );
        assert_eq!(
            serve_opened_file(opened_file, &Method::GET, &headers)
                .await
                .status(),
            StatusCode::NOT_MODIFIED
        );
    }

    #[tokio::test]
    async fn if_match_is_a_precondition_with_412() {
        let temp = NamedTempFile::new().unwrap();
        temp.as_file().write_all(b"0123456789").unwrap();
        let _etag = entity_tag(&temp.as_file().metadata().unwrap()).unwrap();
        let call = |value: &'static str| {
            let opened_file = opened(&temp);
            async move {
                let mut headers = HeaderMap::new();
                headers.insert(header::IF_MATCH, HeaderValue::from_static(value));
                serve_opened_file(opened_file, &Method::GET, &headers).await
            }
        };
        // A mismatching candidate is a failed precondition, not a served body.
        let mismatching = call("\"nope\"").await;
        assert_eq!(mismatching.status(), StatusCode::PRECONDITION_FAILED);
        // `*` matches because the representation exists.
        assert_eq!(call("*").await.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn if_modified_since_votes_before_if_none_match() {
        let temp = NamedTempFile::new().unwrap();
        temp.as_file().write_all(b"0123456789").unwrap();
        let metadata = temp.as_file().metadata().unwrap();
        let etag = entity_tag(&metadata).unwrap();
        let last_modified = HttpDate::from(metadata.modified().unwrap()).to_string();
        let older =
            HttpDate::from(metadata.modified().unwrap() - std::time::Duration::from_secs(3_600))
                .to_string();

        let send = |etag: &str, ims: &str| {
            let opened_file = opened(&temp);
            let etag = etag.to_owned();
            let ims = ims.to_owned();
            async move {
                let mut headers = HeaderMap::new();
                headers.insert(header::IF_NONE_MATCH, HeaderValue::from_str(&etag).unwrap());
                headers.insert(
                    header::IF_MODIFIED_SINCE,
                    HeaderValue::from_str(&ims).unwrap(),
                );
                serve_opened_file(opened_file, &Method::GET, &headers).await
            }
        };

        // A matching If-None-Match combined with a non-equal If-Modified-Since
        // is served: nginx never reaches the If-None-Match vote.
        assert_eq!(send(&etag, &older).await.status(), StatusCode::OK);
        // Equal conditions let the If-None-Match vote through, so the matching
        // tag decides the 304.
        assert_eq!(
            send(&etag, &last_modified).await.status(),
            StatusCode::NOT_MODIFIED
        );
        // With If-Modified-Since removed the tag alone still decides.
        let opened_file = opened(&temp);
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_str(&etag).unwrap());
        assert_eq!(
            serve_opened_file(opened_file, &Method::GET, &headers)
                .await
                .status(),
            StatusCode::NOT_MODIFIED
        );
    }

    #[tokio::test]
    async fn not_modified_keeps_the_cache_validators() {
        let temp = NamedTempFile::new().unwrap();
        temp.as_file().write_all(b"0123456789").unwrap();
        let metadata = temp.as_file().metadata().unwrap();
        let etag = entity_tag(&metadata).unwrap();
        let opened_file = opened(&temp);
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_static("*"));
        let response = serve_opened_file(opened_file, &Method::GET, &headers).await;
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        // nginx clears Content-Type / Content-Length / Accept-Ranges on a 304
        // but keeps every validator.
        assert_eq!(response.headers()[header::ETAG], etag.as_str());
        assert!(response.headers().contains_key(header::LAST_MODIFIED));
        assert!(response.headers().contains_key(header::CACHE_CONTROL));
        assert!(!response.headers().contains_key(header::CONTENT_TYPE));
        assert!(!response.headers().contains_key(header::ACCEPT_RANGES));
    }

    #[tokio::test]
    async fn if_range_ignores_the_range_when_the_validator_differs() {
        let temp = NamedTempFile::new().unwrap();
        temp.as_file().write_all(b"0123456789").unwrap();
        let etag = entity_tag(&temp.as_file().metadata().unwrap()).unwrap();
        let with = |if_range: String| {
            let opened_file = opened(&temp);
            async move {
                let mut headers = HeaderMap::new();
                headers.insert(header::RANGE, HeaderValue::from_static("bytes=0-3"));
                headers.insert(header::IF_RANGE, HeaderValue::from_str(&if_range).unwrap());
                serve_opened_file(opened_file, &Method::GET, &headers).await
            }
        };
        // Matching validator: the range is honoured.
        let matched = with(etag.clone()).await;
        assert_eq!(matched.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(matched.headers()[header::CONTENT_RANGE], "bytes 0-3/10");
        // Mismatching validator: nginx drops the range and serves the whole
        // representation, so an unchanged file is never partly served.
        let mismatched = with("\"nope\"".to_owned()).await;
        assert_eq!(mismatched.status(), StatusCode::OK);
        assert_eq!(mismatched.headers()[header::CONTENT_LENGTH], "10");
    }

    #[tokio::test]
    async fn unsatisfiable_range_drops_the_validators() {
        let temp = NamedTempFile::new().unwrap();
        temp.as_file().write_all(b"0123456789").unwrap();
        let opened_file = opened(&temp);
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=99-100"));
        let response = serve_opened_file(opened_file, &Method::GET, &headers).await;
        // nginx answers through `ngx_http_range_not_satisfiable`: the special
        // response path keeps only `Content-Range: bytes */<size>` and drops the
        // validators the content handler had already prepared.
        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes */10");
        assert!(!response.headers().contains_key(header::ETAG));
        assert!(!response.headers().contains_key(header::LAST_MODIFIED));
        assert!(!response.headers().contains_key(header::CONTENT_TYPE));
        assert!(!response.headers().contains_key(header::ACCEPT_RANGES));
    }

    #[test]
    fn if_none_match_pure_polarity() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_NONE_MATCH,
            HeaderValue::from_static("W/\"abc-1\""),
        );
        assert!(!if_none_match_passes(&headers, Some("W/\"abc-1\"")));
        assert!(if_none_match_passes(&headers, Some("\"zzz-9\"")));
        assert!(if_none_match_passes(&HeaderMap::new(), Some("W/\"abc-1\"")));
    }

    #[tokio::test]
    async fn if_none_match_conditional_matrix() {
        let temp = NamedTempFile::new().unwrap();
        temp.as_file().write_all(b"0123456789").unwrap();
        let etag = entity_tag(&temp.as_file().metadata().unwrap()).unwrap();
        let echo = |tag: String| {
            let opened_file = opened(&temp);
            let mut headers = HeaderMap::new();
            headers.insert(header::IF_NONE_MATCH, HeaderValue::from_str(&tag).unwrap());
            async move { serve_opened_file(opened_file, &Method::GET, &headers).await }
        };
        // Echoing the served weak tag (and its weak-equivalent bare form)
        // means the representation is unchanged → 304.
        assert_eq!(echo(etag.clone()).await.status(), StatusCode::NOT_MODIFIED);
        let bare = etag.trim_start_matches("W/").to_owned();
        assert_eq!(echo(bare).await.status(), StatusCode::NOT_MODIFIED);
        // A different tag must still serve the representation.
        assert_eq!(
            echo("\"deadbeef-1\"".to_owned()).await.status(),
            StatusCode::OK
        );
        // `*` matches the existing representation → not modified.
        assert_eq!(
            echo("*".to_owned()).await.status(),
            StatusCode::NOT_MODIFIED
        );
    }

    #[test]
    fn runtime_env_document_is_never_cacheable() {
        assert_eq!(
            cache_control_for_path(&Path::new("/srv/web/h5/runtime-env.json")),
            "no-store"
        );
    }

    #[test]
    fn fingerprinted_assets_are_immutable() {
        for name in [
            "index-CqUZhiZ2.js",
            "vendor-a1B2c3D4e5F6.css",
            "font-inter-7BaC9D2E.woff2",
            "chunk-2d0e4b2f.min.js",
            "logo-2024abCD.png",
        ] {
            assert_eq!(
                cache_control_for_path(&Path::new("/srv/web/pc/assets").join(name)),
                "public, max-age=31536000, immutable",
                "{name} should be immutable"
            );
        }
    }

    #[test]
    fn descriptive_names_stay_on_revalidating_default() {
        for name in [
            "index.html",
            "manifest.webmanifest",
            "main-module.js",
            "font-awesome.svg",
            "sw.js",
            "i18n-en-US.json",
            "app-2024-01-02.js",
            "favicon.ico",
        ] {
            assert_eq!(
                cache_control_for_path(&Path::new("/srv/web/pc").join(name)),
                "public, no-cache",
                "{name} should revalidate"
            );
        }
    }

    fn opened(temp: &NamedTempFile) -> OpenedStaticFile {
        let file = temp.reopen().unwrap();
        let metadata = file.metadata().unwrap();
        OpenedStaticFile {
            file,
            metadata,
            path_hint: PathBuf::from("asset.txt"),
        }
    }
}
