use std::{io, sync::Arc};

use axum::{
    body::Body,
    http::{
        header::{
            ACCEPT, ACCEPT_LANGUAGE, ACCEPT_RANGES, ALLOW, CACHE_CONTROL, CONTENT_LENGTH,
            CONTENT_LOCATION, CONTENT_RANGE, CONTENT_TYPE, ETAG, IF_MATCH, IF_MODIFIED_SINCE,
            IF_NONE_MATCH, IF_RANGE, IF_UNMODIFIED_SINCE, LAST_MODIFIED, LOCATION, RANGE,
            RETRY_AFTER, USER_AGENT, VARY,
        },
        HeaderMap, HeaderName, HeaderValue, Method, Response, StatusCode,
    },
};
use bytes::Bytes;
use futures_util::stream;
use sdkwork_web_core::{
    new_request_id, resolve_trace_context, trace_id_from_traceparent, REQUEST_ID_HEADER,
};
use sdkwork_webserver_contract::provider::{
    WebsiteByteRange, WebsiteProviderContentStream, WebsiteProviderErrorKind,
    WebsiteRequestConditions,
};
use sdkwork_webserver_core::website_runtime::{
    WebsiteClientClass, WebsiteClientClassificationSource, WebsiteRedirectScheme,
    WebsiteRouteSelectionError,
};
use sdkwork_webserver_delivery_runtime::{
    WebsiteDeliveryContent, WebsiteDeliveryError, WebsiteDeliveryExecutor, WebsiteDeliveryMethod,
    WebsiteDeliveryOutcome, WebsiteDeliveryRedirect, WebsiteDeliveryRequest,
    WebsiteDeliveryRoutingContext, WebsiteDeliveryScheme,
};
use woothee::parser::Parser;

const MAXIMUM_CONDITION_HEADER_BYTES: usize = 4 * 1024;
const MAXIMUM_RANGE_HEADER_BYTES: usize = 128;
const MAXIMUM_ACCEPT_HEADER_BYTES: usize = 2 * 1024;
const MAXIMUM_ACCEPT_LANGUAGE_BYTES: usize = 256;
const MAXIMUM_USER_AGENT_BYTES: usize = 1_024;
const MAXIMUM_LOCALE_BYTES: usize = 35;
const SEC_CH_UA_MOBILE: HeaderName = HeaderName::from_static("sec-ch-ua-mobile");
const SEC_FETCH_MODE: HeaderName = HeaderName::from_static("sec-fetch-mode");
const ACCEPT_CH: HeaderName = HeaderName::from_static("accept-ch");
const X_CONTENT_TYPE_OPTIONS: HeaderName = HeaderName::from_static("x-content-type-options");
const REFERRER_POLICY: HeaderName = HeaderName::from_static("referrer-policy");

pub(crate) async fn serve_website_request(
    executor: Arc<WebsiteDeliveryExecutor>,
    scheme: WebsiteDeliveryScheme,
    authority: String,
    path: String,
    query: Option<String>,
    method: Method,
    headers: HeaderMap,
) -> Response<Body> {
    let request_id = new_request_id();
    let trace_context = resolve_trace_context(&headers, &request_id);
    let trace_id = trace_id_from_traceparent(&trace_context.traceparent)
        .unwrap_or(request_id.as_str())
        .to_owned();
    let suppress_body = method == Method::HEAD;
    let delivery_method = match method {
        Method::GET => WebsiteDeliveryMethod::Get,
        Method::HEAD => WebsiteDeliveryMethod::Head,
        _ => return finalize_response(method_not_allowed(), &request_id, false),
    };
    let request = match delivery_request(
        scheme,
        authority,
        path,
        delivery_method,
        request_id.clone(),
        trace_id,
        &headers,
    ) {
        Ok(request) => request,
        Err(RequestHeaderError::Range) => {
            return finalize_response(range_not_satisfiable(), &request_id, suppress_body)
        }
        Err(RequestHeaderError::Invalid) => {
            return finalize_response(
                text_response(StatusCode::BAD_REQUEST),
                &request_id,
                suppress_body,
            )
        }
    };
    let response = match executor.execute(request).await {
        Ok(outcome) => outcome_response(outcome, query.as_deref()),
        Err(error) => delivery_error_response(error),
    };
    finalize_response(response, &request_id, suppress_body)
}

fn delivery_request(
    scheme: WebsiteDeliveryScheme,
    authority: String,
    path: String,
    method: WebsiteDeliveryMethod,
    request_id: String,
    trace_id: String,
    headers: &HeaderMap,
) -> Result<WebsiteDeliveryRequest, RequestHeaderError> {
    let conditions = WebsiteRequestConditions {
        if_match: bounded_header(headers, IF_MATCH, MAXIMUM_CONDITION_HEADER_BYTES)?,
        if_none_match: bounded_header(headers, IF_NONE_MATCH, MAXIMUM_CONDITION_HEADER_BYTES)?,
        if_modified_since: bounded_header(
            headers,
            IF_MODIFIED_SINCE,
            MAXIMUM_CONDITION_HEADER_BYTES,
        )?,
        if_unmodified_since: bounded_header(
            headers,
            IF_UNMODIFIED_SINCE,
            MAXIMUM_CONDITION_HEADER_BYTES,
        )?,
        if_range: bounded_header(headers, IF_RANGE, MAXIMUM_CONDITION_HEADER_BYTES)?,
    };
    let range = if method == WebsiteDeliveryMethod::Get {
        bounded_header(headers, RANGE, MAXIMUM_RANGE_HEADER_BYTES)?
            .map(|value| parse_range(&value))
            .transpose()?
    } else {
        None
    };
    let locale = bounded_header(headers, ACCEPT_LANGUAGE, MAXIMUM_ACCEPT_LANGUAGE_BYTES)?
        .map(|value| parse_locale(&value))
        .transpose()?
        .flatten();
    let (client_class, client_classification_source) = classify_client(headers)?;
    let spa_fallback_eligible = navigation_request(headers)?;
    Ok(WebsiteDeliveryRequest {
        authority,
        path,
        scheme,
        method,
        request_id,
        trace_id,
        routing: WebsiteDeliveryRoutingContext {
            verified_preferred_variant_uuid: None,
            client_class,
            client_classification_source,
        },
        conditions,
        range,
        locale,
        spa_fallback_eligible,
    })
}

pub(crate) fn bounded_header(
    headers: &HeaderMap,
    name: HeaderName,
    maximum_bytes: usize,
) -> Result<Option<String>, RequestHeaderError> {
    let mut values = headers.get_all(&name).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() || value.as_bytes().len() > maximum_bytes {
        return Err(RequestHeaderError::Invalid);
    }
    let value = value
        .to_str()
        .map_err(|_| RequestHeaderError::Invalid)?
        .trim();
    if value.is_empty() {
        return Err(RequestHeaderError::Invalid);
    }
    Ok(Some(value.to_owned()))
}

pub(crate) fn parse_range(value: &str) -> Result<WebsiteByteRange, RequestHeaderError> {
    let Some(value) = value.strip_prefix("bytes=") else {
        return Err(RequestHeaderError::Range);
    };
    if value.contains(',') {
        return Err(RequestHeaderError::Range);
    }
    let Some((start, end)) = value.split_once('-') else {
        return Err(RequestHeaderError::Range);
    };
    if start.is_empty() {
        // Suffix range (`bytes=-N`): the last N bytes of the representation
        // (RFC 9110 §14.1.2). The executor resolves it to an explicit range
        // once the provider declares the content length.
        if end.is_empty() || !end.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(RequestHeaderError::Range);
        }
        let suffix = end.parse::<u64>().map_err(|_| RequestHeaderError::Range)?;
        if suffix == 0 {
            return Err(RequestHeaderError::Range);
        }
        return Ok(WebsiteByteRange {
            start: 0,
            end_inclusive: None,
            suffix_bytes: Some(suffix),
        });
    }
    if !start.bytes().all(|byte| byte.is_ascii_digit())
        || (!end.is_empty() && !end.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(RequestHeaderError::Range);
    }
    let start = start
        .parse::<u64>()
        .map_err(|_| RequestHeaderError::Range)?;
    let end_inclusive = if end.is_empty() {
        None
    } else {
        let end = end.parse::<u64>().map_err(|_| RequestHeaderError::Range)?;
        if end < start {
            return Err(RequestHeaderError::Range);
        }
        Some(end)
    };
    Ok(WebsiteByteRange {
        start,
        end_inclusive,
        suffix_bytes: None,
    })
}

pub(crate) fn parse_locale(value: &str) -> Result<Option<String>, RequestHeaderError> {
    let locale = value
        .split(',')
        .next()
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim();
    if locale == "*" {
        return Ok(None);
    }
    if locale.is_empty()
        || locale.len() > MAXIMUM_LOCALE_BYTES
        || !locale
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(RequestHeaderError::Invalid);
    }
    Ok(Some(locale.to_owned()))
}

fn classify_client(
    headers: &HeaderMap,
) -> Result<
    (
        Option<WebsiteClientClass>,
        Option<WebsiteClientClassificationSource>,
    ),
    RequestHeaderError,
> {
    let user_agent = header_value(headers, USER_AGENT, MAXIMUM_USER_AGENT_BYTES)?;
    if let Some(user_agent) = user_agent {
        let lower = user_agent.to_ascii_lowercase();
        let parsed = Parser::new().parse(user_agent);
        if parsed.is_some_and(|value| value.category == "crawler")
            || lower.contains("bot")
            || lower.contains("crawler")
            || lower.contains("spider")
        {
            return Ok((
                Some(WebsiteClientClass::Bot),
                Some(WebsiteClientClassificationSource::Bot),
            ));
        }
        if is_tv_user_agent(&lower) {
            return Ok((
                Some(WebsiteClientClass::Tv),
                Some(WebsiteClientClassificationSource::UserAgent),
            ));
        }
    }
    if let Some(mobile) = header_value(headers, SEC_CH_UA_MOBILE, 8)? {
        match mobile.trim() {
            "?1" => {
                return Ok((
                    Some(WebsiteClientClass::Mobile),
                    Some(WebsiteClientClassificationSource::ClientHint),
                ));
            }
            // Continue to UA so iPad with Sec-CH-UA-Mobile=?0 stays Tablet
            // (SDKWORK_DEPLOY_SPEC.md §8 / shared Adaptive Web detection order).
            "?0" => {}
            _ => return Err(RequestHeaderError::Invalid),
        }
    }
    let Some(user_agent) = user_agent else {
        // Coarse non-mobile Client Hint without UA defaults to desktop.
        if header_value(headers, SEC_CH_UA_MOBILE, 8)? == Some("?0") {
            return Ok((
                Some(WebsiteClientClass::Desktop),
                Some(WebsiteClientClassificationSource::ClientHint),
            ));
        }
        return Ok((None, None));
    };
    let lower = user_agent.to_ascii_lowercase();
    // Tablet before the shared mobile UA regex so iPad stays on the PC surface
    // (SDKWORK_DEPLOY_SPEC.md §8 / NGINX_SPEC.md §7.1).
    if lower.contains("ipad") || lower.contains("tablet") {
        return Ok((
            Some(WebsiteClientClass::Tablet),
            Some(WebsiteClientClassificationSource::UserAgent),
        ));
    }
    // Shared Adaptive Web mobile UA regex (SDKWORK_DEPLOY_SPEC.md §8).
    if matches_adaptive_mobile_user_agent(user_agent) {
        return Ok((
            Some(WebsiteClientClass::Mobile),
            Some(WebsiteClientClassificationSource::UserAgent),
        ));
    }
    let class = match Parser::new().parse(user_agent).map(|value| value.category) {
        Some("pc") => WebsiteClientClass::Desktop,
        Some("smartphone" | "mobilephone" | "appliance") => WebsiteClientClass::Mobile,
        _ => WebsiteClientClass::Other,
    };
    Ok((
        Some(class),
        Some(WebsiteClientClassificationSource::UserAgent),
    ))
}

/// Shared Adaptive Web mobile User-Agent markers from `SDKWORK_DEPLOY_SPEC.md` §8.
/// Same marker set as stock nginx Adaptive Web maps (other modules) and process
/// `AdaptiveAppShell` classification for this product.
fn matches_adaptive_mobile_user_agent(user_agent: &str) -> bool {
    const MOBILE_MARKERS: [&str; 13] = [
        "mobile",
        "android",
        "iphone",
        "ipod",
        "webos",
        "blackberry",
        "iemobile",
        "opera mini",
        "micromessenger",
        "huaweibrowser",
        "harmonyos",
        "ucbrowser",
        "quark",
    ];
    let lower = user_agent.to_ascii_lowercase();
    MOBILE_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

fn is_tv_user_agent(lower_user_agent: &str) -> bool {
    const TV_SIGNATURES: [&str; 10] = [
        "smart-tv", "smarttv", "hbbtv", "netcast", "webos", "web0s", "tizen", "roku", "crkey",
        "viera",
    ];
    TV_SIGNATURES
        .iter()
        .any(|signature| lower_user_agent.contains(signature))
}

pub(crate) fn navigation_request(headers: &HeaderMap) -> Result<bool, RequestHeaderError> {
    if header_value(headers, SEC_FETCH_MODE, 32)? == Some("navigate") {
        return Ok(true);
    }
    Ok(header_value(headers, ACCEPT, MAXIMUM_ACCEPT_HEADER_BYTES)?
        .is_some_and(|accept| accept.to_ascii_lowercase().contains("text/html")))
}

pub(crate) fn header_value(
    headers: &HeaderMap,
    name: HeaderName,
    maximum_bytes: usize,
) -> Result<Option<&str>, RequestHeaderError> {
    let mut values = headers.get_all(&name).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() || value.as_bytes().len() > maximum_bytes {
        return Err(RequestHeaderError::Invalid);
    }
    value
        .to_str()
        .map(Some)
        .map_err(|_| RequestHeaderError::Invalid)
}

pub(crate) fn outcome_response(
    outcome: WebsiteDeliveryOutcome,
    query: Option<&str>,
) -> Response<Body> {
    match outcome {
        WebsiteDeliveryOutcome::NotFound => text_response(StatusCode::NOT_FOUND),
        WebsiteDeliveryOutcome::NotModified => empty_response(StatusCode::NOT_MODIFIED),
        WebsiteDeliveryOutcome::Redirect(redirect) => redirect_response(redirect, query),
        WebsiteDeliveryOutcome::Content(content) => content_response(*content),
    }
}

pub(crate) fn redirect_response(
    redirect: WebsiteDeliveryRedirect,
    query: Option<&str>,
) -> Response<Body> {
    let (status_code, location, preserve_query) = match redirect {
        WebsiteDeliveryRedirect::Binding {
            status_code,
            scheme,
            hostname,
            path,
            preserve_query,
        } => {
            let scheme = match scheme {
                WebsiteRedirectScheme::Http => "http",
                WebsiteRedirectScheme::Https => "https",
            };
            (
                status_code,
                format!("{scheme}://{hostname}{path}"),
                preserve_query,
            )
        }
        WebsiteDeliveryRedirect::Wiki {
            status_code,
            canonical_route,
            preserve_query,
            ..
        } => (status_code, canonical_route, preserve_query),
    };
    let location = append_query(location, preserve_query.then_some(query).flatten());
    let Ok(status) = StatusCode::from_u16(status_code) else {
        return text_response(StatusCode::BAD_GATEWAY);
    };
    let Ok(location) = HeaderValue::from_str(&location) else {
        return text_response(StatusCode::BAD_GATEWAY);
    };
    let mut response = empty_response(status);
    response.headers_mut().insert(LOCATION, location);
    response
}

pub(crate) fn append_query(mut location: String, query: Option<&str>) -> String {
    if let Some(query) = query.filter(|query| !query.is_empty()) {
        location.push('?');
        location.push_str(query);
    }
    location
}

pub(crate) fn content_response(mut content: WebsiteDeliveryContent) -> Response<Body> {
    let status = if content.content_range.is_some() {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    let body = content.body.take().map_or_else(Body::empty, stream_body);
    let mut response = Response::new(body);
    *response.status_mut() = status;
    let headers = response.headers_mut();
    let required = [
        (CONTENT_TYPE, content.metadata.content_type.as_str()),
        (ETAG, content.metadata.etag.as_str()),
        (LAST_MODIFIED, content.metadata.last_modified.as_str()),
    ];
    for (name, value) in required {
        let Ok(value) = HeaderValue::from_str(value) else {
            return text_response(StatusCode::BAD_GATEWAY);
        };
        headers.insert(name, value);
    }
    let Ok(content_length) = HeaderValue::from_str(&content.response_content_length.to_string())
    else {
        return text_response(StatusCode::BAD_GATEWAY);
    };
    headers.insert(CONTENT_LENGTH, content_length);
    headers.insert(
        ACCEPT_RANGES,
        HeaderValue::from_static(if content.metadata.range_supported {
            "bytes"
        } else {
            "none"
        }),
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("public, no-cache"));
    if let Some(range) = content.content_range {
        let value = format!(
            "bytes {}-{}/{}",
            range.start, range.end_inclusive, range.complete_length
        );
        let Ok(value) = HeaderValue::from_str(&value) else {
            return text_response(StatusCode::BAD_GATEWAY);
        };
        headers.insert(CONTENT_RANGE, value);
    }
    if let Some(canonical_route) = content.canonical_route {
        let Ok(value) = HeaderValue::from_str(&canonical_route) else {
            return text_response(StatusCode::BAD_GATEWAY);
        };
        headers.insert(CONTENT_LOCATION, value);
    }
    response
}

pub(crate) fn stream_body(stream: Box<dyn WebsiteProviderContentStream>) -> Body {
    let stream = stream::unfold(Some(stream), |state| async move {
        let mut stream = state?;
        match stream.next_chunk().await {
            Ok(Some(chunk)) => Some((Ok::<Bytes, io::Error>(Bytes::from(chunk)), Some(stream))),
            Ok(None) => None,
            Err(_) => Some((Err(io::Error::other("website content stream failed")), None)),
        }
    });
    Body::from_stream(stream)
}

pub(crate) fn delivery_error_response(error: WebsiteDeliveryError) -> Response<Body> {
    match error {
        WebsiteDeliveryError::RouteSelection(WebsiteRouteSelectionError::InvalidHost)
        | WebsiteDeliveryError::RouteSelection(WebsiteRouteSelectionError::InvalidPath)
        | WebsiteDeliveryError::InvalidRequestIdentity => text_response(StatusCode::BAD_REQUEST),
        WebsiteDeliveryError::RouteSelection(WebsiteRouteSelectionError::DeniedPath) => {
            text_response(StatusCode::NOT_FOUND)
        }
        WebsiteDeliveryError::RuntimeUnavailable
        | WebsiteDeliveryError::ProviderNotRegistered { .. } => {
            retry_response(StatusCode::SERVICE_UNAVAILABLE, Some(1_000))
        }
        WebsiteDeliveryError::HandlerNotSupported { .. }
        | WebsiteDeliveryError::ContentTooLarge { .. } => text_response(StatusCode::BAD_GATEWAY),
        WebsiteDeliveryError::RangeNotSupported => range_not_satisfiable(),
        WebsiteDeliveryError::Provider(error) => match error.kind {
            WebsiteProviderErrorKind::NotFound
            | WebsiteProviderErrorKind::NotPublic
            | WebsiteProviderErrorKind::InvalidPath
            | WebsiteProviderErrorKind::Revoked => text_response(StatusCode::NOT_FOUND),
            WebsiteProviderErrorKind::NotModified => empty_response(StatusCode::NOT_MODIFIED),
            WebsiteProviderErrorKind::PreconditionFailed => {
                empty_response(StatusCode::PRECONDITION_FAILED)
            }
            WebsiteProviderErrorKind::RangeNotSatisfiable => range_not_satisfiable(),
            WebsiteProviderErrorKind::RateLimited => {
                retry_response(StatusCode::TOO_MANY_REQUESTS, error.retry_after_ms)
            }
            WebsiteProviderErrorKind::DeadlineExceeded | WebsiteProviderErrorKind::Unavailable => {
                retry_response(StatusCode::SERVICE_UNAVAILABLE, error.retry_after_ms)
            }
            WebsiteProviderErrorKind::ContractMismatch => text_response(StatusCode::BAD_GATEWAY),
        },
    }
}

pub(crate) fn retry_response(status: StatusCode, retry_after_ms: Option<u64>) -> Response<Body> {
    let mut response = text_response(status);
    let seconds = retry_after_ms
        .unwrap_or(1_000)
        .div_ceil(1_000)
        .clamp(1, 86_400);
    if let Ok(value) = HeaderValue::from_str(&seconds.to_string()) {
        response.headers_mut().insert(RETRY_AFTER, value);
    }
    response
}

pub(crate) fn method_not_allowed() -> Response<Body> {
    let mut response = text_response(StatusCode::METHOD_NOT_ALLOWED);
    response
        .headers_mut()
        .insert(ALLOW, HeaderValue::from_static("GET, HEAD"));
    response
}

pub(crate) fn range_not_satisfiable() -> Response<Body> {
    let mut response = text_response(StatusCode::RANGE_NOT_SATISFIABLE);
    response
        .headers_mut()
        .insert(ACCEPT_RANGES, HeaderValue::from_static("none"));
    response
}

pub(crate) fn empty_response(status: StatusCode) -> Response<Body> {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = status;
    response
}

pub(crate) fn text_response(status: StatusCode) -> Response<Body> {
    let mut response = Response::new(Body::from(status.canonical_reason().unwrap_or("error")));
    *response.status_mut() = status;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

pub(crate) fn finalize_response(
    mut response: Response<Body>,
    request_id: &str,
    suppress_body: bool,
) -> Response<Body> {
    if suppress_body {
        *response.body_mut() = Body::empty();
    }
    if let Ok(value) = HeaderValue::from_str(request_id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    response.headers_mut().insert(
        VARY,
        HeaderValue::from_static("Accept-Language, Sec-CH-UA-Mobile, User-Agent"),
    );
    response
        .headers_mut()
        .insert(ACCEPT_CH, HeaderValue::from_static("Sec-CH-UA-Mobile"));
    response
        .headers_mut()
        .insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    response.headers_mut().insert(
        REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    response
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RequestHeaderError {
    Invalid,
    Range,
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            atomic::{AtomicU16, AtomicU64, AtomicUsize, Ordering},
            Arc, Mutex,
        },
        time::Duration,
    };

    use async_trait::async_trait;
    use http_body_util::BodyExt;
    use sdkwork_drive_internal_sdk::{
        models::{DriveResourceResolution, ResolveDriveResourceRequest, WebsiteRoot},
        SdkworkError as DriveSdkworkError,
    };
    use sdkwork_knowledgebase_internal_sdk::{
        models::{
            ResolveWikiRouteRequest, WikiPublicPageListData, WikiPublicPageMetadata,
            WikiPublication, WikiRouteResolution,
        },
        SdkworkError,
    };
    use sdkwork_webserver_contract::provider::{
        OpenWebsiteContentRequest, OpenedWebsiteContent, ResolveWebsiteStaticPathRequest,
        ResolvedWebsiteContent, ValidateWebsiteResourceRequest, ValidatedWebsiteResource,
        WebsiteContentMetadata, WebsiteContentResolution, WebsiteProviderContentHandle,
        WebsiteProviderResult, WebsiteResourceProvider, WebsiteStaticContentProvider,
    };
    use sdkwork_webserver_core::{
        website_runtime::{
            website_runtime_descriptor_sha256, website_runtime_set_snapshot_sha256,
            WebsiteProviderType, WebsiteRuntimeDescriptor, WebsiteRuntimeEnvironment,
            WebsiteRuntimeRegistry, WebsiteRuntimeSetSnapshot,
        },
        TrustedProxyConfig, TrustedProxyHeader,
    };
    use sdkwork_webserver_delivery_runtime::WebsiteProviderRegistry;
    use sdkwork_webserver_drive_provider::{
        DriveContentChunkStream, DriveWebsiteProvider, DriveWebsiteSdkClient,
        FixedDriveWebsiteSdkClientResolver, DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION,
    };
    use sdkwork_webserver_knowledgebase_provider::{
        FixedKnowledgebaseWikiSdkClientResolver, KnowledgebaseWikiSdkClient,
        KnowledgebaseWikiWebsiteProvider, OpenedWikiContentStream, WikiContentChunkStream,
        KNOWLEDGEBASE_WIKI_PROVIDER_CONTRACT_VERSION,
    };
    use serde_json::{json, Value};

    use super::*;
    use crate::data_plane::forwarded_scheme::resolve_request_scheme;

    const NODE_UUID: &str = "gateway-node-0001";
    const TENANT_SCOPE_HASH: &str =
        "1111111111111111111111111111111111111111111111111111111111111111";
    const PUBLICATION_UUID: &str = "11111111-1111-4111-8111-111111111501";
    const PROJECTION_UUID: &str = "11111111-1111-4111-8111-111111111601";
    const CONTENT_SHA256: &str =
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const DRIVE_WEBSITE_ROOT_UUID: &str = "11111111-1111-4111-8111-111111111701";

    #[test]
    fn parses_only_single_byte_ranges_and_suffix_ranges() {
        assert_eq!(
            parse_range("bytes=10-19"),
            Ok(WebsiteByteRange {
                start: 10,
                end_inclusive: Some(19),
                suffix_bytes: None,
            })
        );
        assert_eq!(
            parse_range("bytes=10-"),
            Ok(WebsiteByteRange {
                start: 10,
                end_inclusive: None,
                suffix_bytes: None,
            })
        );
        assert_eq!(
            parse_range("bytes=-4"),
            Ok(WebsiteByteRange {
                start: 0,
                end_inclusive: None,
                suffix_bytes: Some(4),
            })
        );
        for invalid in [
            "items=1-2",
            "bytes=-20-",
            "bytes=20-10",
            "bytes=1-2,4-5",
            "bytes=-",
            "bytes=-0",
            "bytes=--4",
        ] {
            assert_eq!(parse_range(invalid), Err(RequestHeaderError::Range));
        }
    }

    #[test]
    fn client_hints_take_precedence_and_bot_classification_is_bounded() {
        let mut headers = HeaderMap::new();
        headers.insert(SEC_CH_UA_MOBILE, HeaderValue::from_static("?1"));
        headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0"));
        assert_eq!(
            classify_client(&headers).unwrap(),
            (
                Some(WebsiteClientClass::Mobile),
                Some(WebsiteClientClassificationSource::ClientHint),
            )
        );
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("ExampleSearchBot/1.0 crawler"),
        );
        assert_eq!(
            classify_client(&headers).unwrap(),
            (
                Some(WebsiteClientClass::Bot),
                Some(WebsiteClientClassificationSource::Bot),
            )
        );
    }

    #[test]
    fn adaptive_mobile_user_agent_markers_match_deploy_spec() {
        for user_agent in [
            "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)",
            "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36",
            "Mozilla/5.0 MicroMessenger/8.0.0",
            "Mozilla/5.0 HuaweiBrowser/14.0.0",
            "Mozilla/5.0 Quark/6.0.0",
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(USER_AGENT, HeaderValue::from_str(user_agent).unwrap());
            assert_eq!(
                classify_client(&headers).unwrap(),
                (
                    Some(WebsiteClientClass::Mobile),
                    Some(WebsiteClientClassificationSource::UserAgent),
                ),
                "{user_agent}"
            );
        }

        let mut desktop = HeaderMap::new();
        desktop.insert(
            USER_AGENT,
            HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120.0.0.0",
            ),
        );
        assert_eq!(
            classify_client(&desktop).unwrap().0,
            Some(WebsiteClientClass::Desktop),
        );

        let mut ipad = HeaderMap::new();
        ipad.insert(
            USER_AGENT,
            HeaderValue::from_static(
                "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
            ),
        );
        assert_eq!(
            classify_client(&ipad).unwrap().0,
            Some(WebsiteClientClass::Tablet),
            "iPad must win over Mobile substring in the shared adaptive UA markers",
        );

        let mut ipad_ch = HeaderMap::new();
        ipad_ch.insert(SEC_CH_UA_MOBILE, HeaderValue::from_static("?0"));
        ipad_ch.insert(
            USER_AGENT,
            HeaderValue::from_static(
                "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
            ),
        );
        assert_eq!(
            classify_client(&ipad_ch).unwrap().0,
            Some(WebsiteClientClass::Tablet),
            "Sec-CH-UA-Mobile=?0 must not skip tablet UA classification",
        );
    }

    #[test]
    fn tv_signatures_override_coarse_non_mobile_client_hints() {
        for user_agent in [
            "Mozilla/5.0 (SMART-TV; Linux; Tizen 6.0)",
            "Mozilla/5.0 (Web0S; Linux/SmartTV)",
            "Mozilla/5.0 (X11; HbbTV/1.5.1)",
            "Roku/DVP-12.5 (519.50E04154A)",
            "Mozilla/5.0 (CrKey armv7l)",
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(SEC_CH_UA_MOBILE, HeaderValue::from_static("?0"));
            headers.insert(USER_AGENT, HeaderValue::from_str(user_agent).unwrap());
            assert_eq!(
                classify_client(&headers).unwrap(),
                (
                    Some(WebsiteClientClass::Tv),
                    Some(WebsiteClientClassificationSource::UserAgent),
                )
            );
        }
    }

    #[test]
    fn redirects_preserve_query_only_when_the_compiled_outcome_allows_it() {
        let response = redirect_response(
            WebsiteDeliveryRedirect::Binding {
                status_code: 308,
                scheme: WebsiteRedirectScheme::Https,
                hostname: "example.com".to_owned(),
                path: "/docs".to_owned(),
                preserve_query: true,
            },
            Some("page=2"),
        );
        assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(
            response.headers().get(LOCATION).unwrap(),
            "https://example.com/docs?page=2"
        );
    }

    #[tokio::test]
    async fn browser_wiki_request_maps_generated_sdk_adapter_to_http_stream() {
        let (executor, sdk, _) = website_executor(2_500, false);
        let response = serve_website_request(
            executor.clone(),
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/guide/".to_owned(),
            None,
            Method::GET,
            HeaderMap::new(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "text/markdown"
        );
        assert_eq!(response.headers().get(CONTENT_LENGTH).unwrap(), "6");
        assert_eq!(
            response.headers().get(ETAG).unwrap().to_str().unwrap(),
            format!("\"{CONTENT_SHA256}-v7\"")
        );
        assert_eq!(response.headers().get(CONTENT_LOCATION).unwrap(), "/guide/");
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            Bytes::from_static(b"# Wiki")
        );
        assert_eq!(sdk.content_calls.load(Ordering::Acquire), 1);

        let response = serve_website_request(
            executor,
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/guide/".to_owned(),
            None,
            Method::HEAD,
            HeaderMap::new(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get(CONTENT_LENGTH).unwrap(), "6");
        assert!(response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .is_empty());
        assert_eq!(sdk.content_calls.load(Ordering::Acquire), 1);
    }

    #[tokio::test]
    async fn browser_wiki_conditions_redirects_visibility_and_range_fail_closed() {
        let (executor, _, _) = website_executor(2_500, false);
        let mut headers = HeaderMap::new();
        headers.insert(
            IF_NONE_MATCH,
            HeaderValue::from_str(&format!("\"{CONTENT_SHA256}-v7\"")).unwrap(),
        );
        let response = serve_website_request(
            executor.clone(),
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/guide/".to_owned(),
            None,
            Method::GET,
            headers,
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);

        let response = serve_website_request(
            executor.clone(),
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/old".to_owned(),
            Some("from=legacy".to_owned()),
            Method::GET,
            HeaderMap::new(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(
            response.headers().get(LOCATION).unwrap(),
            "/guide/?from=legacy"
        );

        let response = serve_website_request(
            executor.clone(),
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/private".to_owned(),
            None,
            Method::GET,
            HeaderMap::new(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = serve_website_request(
            executor.clone(),
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/private".to_owned(),
            None,
            Method::HEAD,
            HeaderMap::new(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .is_empty());

        let mut headers = HeaderMap::new();
        headers.insert(RANGE, HeaderValue::from_static("bytes=1-2,4-5"));
        let response = serve_website_request(
            executor,
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/guide/".to_owned(),
            None,
            Method::GET,
            headers,
        )
        .await;
        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    }

    #[tokio::test]
    async fn force_https_and_mobile_client_hint_select_the_compiled_variant() {
        let (executor, sdk, variant_static) = website_executor(2_500, true);
        let response = serve_website_request(
            executor.clone(),
            WebsiteDeliveryScheme::Http,
            "example.com".to_owned(),
            "/guide/".to_owned(),
            Some("lang=zh".to_owned()),
            Method::GET,
            HeaderMap::new(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(
            response.headers().get(LOCATION).unwrap(),
            "https://example.com/guide/?lang=zh"
        );
        assert_eq!(sdk.route_calls.load(Ordering::Acquire), 0);

        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
        headers.insert(SEC_CH_UA_MOBILE, HeaderValue::from_static("?1"));
        let trusted_proxy = TrustedProxyConfig {
            trusted_cidrs: vec!["10.0.0.0/8".parse().unwrap()],
            header: TrustedProxyHeader::XForwardedFor,
            recursive: true,
            max_hops: 16,
            max_header_bytes: 4_096,
        };
        let scheme = resolve_request_scheme(
            "10.0.0.8".parse().unwrap(),
            &headers,
            Some(&trusted_proxy),
            false,
        )
        .unwrap();
        let response = serve_website_request(
            executor,
            scheme.website_delivery_scheme(),
            "example.com".to_owned(),
            "/".to_owned(),
            None,
            Method::GET,
            headers,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            Bytes::from_static(b"mobile")
        );
        assert_eq!(
            variant_static.resolve_paths.lock().unwrap().as_slice(),
            ["/mobile/index.html"]
        );
    }

    #[tokio::test]
    async fn tv_user_agent_selects_the_compiled_tv_variant_end_to_end() {
        let (executor, _, variant_static) = website_executor(2_500, false);
        let mut headers = HeaderMap::new();
        headers.insert(SEC_CH_UA_MOBILE, HeaderValue::from_static("?0"));
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("Mozilla/5.0 (SMART-TV; Linux; Tizen 6.0)"),
        );

        let response = serve_website_request(
            executor,
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/".to_owned(),
            None,
            Method::GET,
            headers,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            Bytes::from_static(b"tv")
        );
        assert_eq!(
            variant_static.resolve_paths.lock().unwrap().as_slice(),
            ["/tv/index.html"]
        );
    }

    #[tokio::test]
    async fn provider_deadline_maps_to_non_disclosing_service_unavailable() {
        let (executor, _, _) = website_executor(5, false);
        let response = serve_website_request(
            executor,
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/slow".to_owned(),
            None,
            Method::GET,
            HeaderMap::new(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers().get(RETRY_AFTER).unwrap(), "1");
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            Bytes::from_static(b"Service Unavailable")
        );
    }

    #[tokio::test]
    async fn browser_drive_get_head_and_conditions_use_the_generated_sdk_adapter() {
        let (executor, sdk) = drive_executor(2_500);
        let response = serve_website_request(
            executor.clone(),
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/".to_owned(),
            None,
            Method::GET,
            mobile_headers(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );
        assert_eq!(response.headers().get(CONTENT_LENGTH).unwrap(), "10");
        assert_eq!(
            response.headers().get(ETAG).unwrap().to_str().unwrap(),
            format!("\"{CONTENT_SHA256}\"")
        );
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            Bytes::from_static(b"0123456789")
        );
        assert_eq!(sdk.content_calls.load(Ordering::Acquire), 1);

        let response = serve_website_request(
            executor.clone(),
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/".to_owned(),
            None,
            Method::HEAD,
            mobile_headers(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get(CONTENT_LENGTH).unwrap(), "10");
        assert!(response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .is_empty());
        assert_eq!(sdk.content_calls.load(Ordering::Acquire), 1);

        let mut headers = mobile_headers();
        headers.insert(
            IF_NONE_MATCH,
            HeaderValue::from_str(&format!("\"{CONTENT_SHA256}\"")).unwrap(),
        );
        let response = serve_website_request(
            executor,
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/".to_owned(),
            None,
            Method::GET,
            headers,
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(sdk.content_calls.load(Ordering::Acquire), 1);
        assert!(sdk
            .resolve_requests
            .lock()
            .unwrap()
            .iter()
            .all(|request| request.relative_path == "mobile/index.html"));
    }

    #[tokio::test]
    async fn browser_drive_ranges_if_range_and_unsatisfiable_ranges_map_to_http() {
        let (executor, _) = drive_executor(2_500);
        let mut headers = mobile_headers();
        headers.insert(RANGE, HeaderValue::from_static("bytes=2-5"));
        let response = serve_website_request(
            executor.clone(),
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/".to_owned(),
            None,
            Method::GET,
            headers,
        )
        .await;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers().get(CONTENT_LENGTH).unwrap(), "4");
        assert_eq!(
            response.headers().get(CONTENT_RANGE).unwrap(),
            "bytes 2-5/10"
        );
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            Bytes::from_static(b"2345")
        );

        let mut headers = mobile_headers();
        headers.insert(RANGE, HeaderValue::from_static("bytes=2-5"));
        headers.insert(IF_RANGE, HeaderValue::from_static("\"different\""));
        let response = serve_website_request(
            executor.clone(),
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/".to_owned(),
            None,
            Method::GET,
            headers,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get(CONTENT_LENGTH).unwrap(), "10");
        assert!(response.headers().get(CONTENT_RANGE).is_none());

        let mut headers = mobile_headers();
        headers.insert(RANGE, HeaderValue::from_static("bytes=10-"));
        let response = serve_website_request(
            executor,
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/".to_owned(),
            None,
            Method::GET,
            headers,
        )
        .await;
        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(response.headers().get(ACCEPT_RANGES).unwrap(), "none");
    }

    #[tokio::test]
    async fn browser_drive_suffix_ranges_normalize_to_explicit_ranges() {
        let (executor, _) = drive_executor(2_500);
        let mut headers = mobile_headers();
        headers.insert(RANGE, HeaderValue::from_static("bytes=-4"));
        let response = serve_website_request(
            executor.clone(),
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/".to_owned(),
            None,
            Method::GET,
            headers,
        )
        .await;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers().get(CONTENT_LENGTH).unwrap(), "4");
        assert_eq!(
            response.headers().get(CONTENT_RANGE).unwrap(),
            "bytes 6-9/10"
        );
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            Bytes::from_static(b"6789")
        );

        // A suffix longer than the representation serves the whole file.
        let mut headers = mobile_headers();
        headers.insert(RANGE, HeaderValue::from_static("bytes=-999"));
        let response = serve_website_request(
            executor.clone(),
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/".to_owned(),
            None,
            Method::GET,
            headers,
        )
        .await;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            response.headers().get(CONTENT_RANGE).unwrap(),
            "bytes 0-9/10"
        );

        // Zero-length and malformed suffixes stay unsatisfiable.
        for invalid in ["bytes=-0", "bytes=-"] {
            let mut headers = mobile_headers();
            headers.insert(RANGE, HeaderValue::from_str(invalid).unwrap());
            let response = serve_website_request(
                executor.clone(),
                WebsiteDeliveryScheme::Https,
                "example.com".to_owned(),
                "/".to_owned(),
                None,
                Method::GET,
                headers,
            )
            .await;
            assert_eq!(
                response.status(),
                StatusCode::RANGE_NOT_SATISFIABLE,
                "range={invalid}"
            );
        }
    }

    #[tokio::test]
    async fn browser_drive_hidden_resources_and_timeouts_fail_closed() {
        let (executor, sdk) = drive_executor(2_500);
        sdk.next_resolve_status.store(404, Ordering::Release);
        let response = serve_website_request(
            executor,
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/".to_owned(),
            None,
            Method::GET,
            mobile_headers(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let (executor, sdk) = drive_executor(5);
        sdk.resolve_delay_ms.store(50, Ordering::Release);
        let response = serve_website_request(
            executor,
            WebsiteDeliveryScheme::Https,
            "example.com".to_owned(),
            "/".to_owned(),
            None,
            Method::GET,
            mobile_headers(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers().get(RETRY_AFTER).unwrap(), "1");
    }

    fn mobile_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(SEC_CH_UA_MOBILE, HeaderValue::from_static("?1"));
        headers
    }

    struct FakeKnowledgebaseSdk {
        route_calls: AtomicUsize,
        content_calls: AtomicUsize,
    }

    #[async_trait]
    impl KnowledgebaseWikiSdkClient for FakeKnowledgebaseSdk {
        async fn retrieve_publication(
            &self,
            _publication_uuid: &str,
        ) -> Result<WikiPublication, SdkworkError> {
            Ok(wiki_publication())
        }

        async fn resolve_route(
            &self,
            _publication_uuid: &str,
            request: &ResolveWikiRouteRequest,
        ) -> Result<WikiRouteResolution, SdkworkError> {
            self.route_calls.fetch_add(1, Ordering::AcqRel);
            match request.route.as_str() {
                "/slow" => tokio::time::sleep(Duration::from_millis(50)).await,
                "/private" => {
                    return Err(SdkworkError::HttpStatus {
                        status: 404,
                        body: "{}".to_owned(),
                    })
                }
                _ => {}
            }
            if request.route == "/old" {
                return Ok(WikiRouteResolution {
                    disposition: "REDIRECT".to_owned(),
                    page: None,
                    content_handle: None,
                    requested_route: Some("/old".to_owned()),
                    canonical_route: Some("/guide/".to_owned()),
                    status: Some(308),
                    page_public_version: Some("7".to_owned()),
                });
            }
            Ok(WikiRouteResolution {
                disposition: "PAGE".to_owned(),
                page: Some(public_wiki_page()),
                content_handle: Some("opaque-content-handle".to_owned()),
                requested_route: None,
                canonical_route: None,
                status: None,
                page_public_version: None,
            })
        }

        async fn retrieve_content(
            &self,
            _publication_uuid: &str,
            _content_handle: &str,
        ) -> Result<Vec<u8>, SdkworkError> {
            self.content_calls.fetch_add(1, Ordering::AcqRel);
            Ok(b"# Wiki".to_vec())
        }

        async fn retrieve_content_stream(
            &self,
            _publication_uuid: &str,
            _content_handle: &str,
        ) -> Result<OpenedWikiContentStream, SdkworkError> {
            self.content_calls.fetch_add(1, Ordering::AcqRel);
            Ok(OpenedWikiContentStream {
                content_length: Some(6),
                stream: Box::new(MemoryWikiChunkStream {
                    chunks: vec![b"# Wiki".to_vec()].into(),
                }),
            })
        }

        async fn list_navigation(
            &self,
            _publication_uuid: &str,
            _locale: Option<&str>,
            _cursor: Option<&str>,
            _page_size: i64,
        ) -> Result<WikiPublicPageListData, SdkworkError> {
            unreachable!("page delivery does not list navigation")
        }

        async fn search_pages(
            &self,
            _publication_uuid: &str,
            _query: &str,
            _locale: Option<&str>,
            _cursor: Option<&str>,
            _page_size: i64,
        ) -> Result<WikiPublicPageListData, SdkworkError> {
            unreachable!("page delivery does not search")
        }
    }

    struct FakeDriveSdk {
        content_calls: AtomicUsize,
        next_resolve_status: AtomicU16,
        resolve_delay_ms: AtomicU64,
        resolve_requests: Mutex<Vec<ResolveDriveResourceRequest>>,
    }

    #[async_trait]
    impl DriveWebsiteSdkClient for FakeDriveSdk {
        async fn retrieve_website_root(
            &self,
            _website_root_uuid: &str,
        ) -> Result<WebsiteRoot, DriveSdkworkError> {
            Ok(WebsiteRoot {
                uuid: DRIVE_WEBSITE_ROOT_UUID.to_owned(),
                space_id: "space-mobile".to_owned(),
                source_root_mode: "SPACE_ROOT".to_owned(),
                content_mode: "LIVE_TREE".to_owned(),
                active_generation: "3".to_owned(),
                root_status: "ACTIVE".to_owned(),
                capabilities: vec![
                    "STATIC_CONTENT".to_owned(),
                    "BYTE_RANGE".to_owned(),
                    "CONDITIONAL_REQUESTS".to_owned(),
                ],
                version: "7".to_owned(),
                updated_at: "2026-07-21T00:00:00Z".to_owned(),
            })
        }

        async fn resolve_resource(
            &self,
            request: &ResolveDriveResourceRequest,
        ) -> Result<DriveResourceResolution, DriveSdkworkError> {
            self.resolve_requests.lock().unwrap().push(request.clone());
            let delay_ms = self.resolve_delay_ms.load(Ordering::Acquire);
            if delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
            let status = self.next_resolve_status.swap(0, Ordering::AcqRel);
            if status > 0 {
                return Err(DriveSdkworkError::HttpStatus {
                    status,
                    body: "{}".to_owned(),
                });
            }
            Ok(DriveResourceResolution {
                scope_type: "WEBSITE_ROOT".to_owned(),
                scope_uuid: DRIVE_WEBSITE_ROOT_UUID.to_owned(),
                scope_generation: "3".to_owned(),
                normalized_relative_path: "mobile/index.html".to_owned(),
                resource_type: "FILE".to_owned(),
                node_id: "node-mobile-index".to_owned(),
                logical_node_version_id: "version-mobile-index".to_owned(),
                version_no: "5".to_owned(),
                checksum_sha256_hex: CONTENT_SHA256.to_owned(),
                etag: format!("\"{CONTENT_SHA256}\""),
                content_type: "text/html; charset=utf-8".to_owned(),
                content_length: "10".to_owned(),
                last_modified: "2026-07-21T00:00:00Z".to_owned(),
                scope_status: "ACTIVE".to_owned(),
                node_status: "ACTIVE".to_owned(),
                eligibility: "ELIGIBLE".to_owned(),
            })
        }

        async fn retrieve_content(
            &self,
            _node_version_id: &str,
            _scope_type: &str,
            _scope_uuid: &str,
            _relative_path: &str,
            _pinned_generation: Option<&str>,
            range: Option<&str>,
            _if_match: Option<&str>,
            _if_none_match: Option<&str>,
            _if_range: Option<&str>,
            _if_modified_since: Option<&str>,
            _if_unmodified_since: Option<&str>,
        ) -> Result<Vec<u8>, DriveSdkworkError> {
            self.content_calls.fetch_add(1, Ordering::AcqRel);
            match range {
                Some("bytes=2-5") => Ok(b"2345".to_vec()),
                Some("bytes=6-9") => Ok(b"6789".to_vec()),
                Some("bytes=0-9") => Ok(b"0123456789".to_vec()),
                Some(other) => panic!("unexpected test range {other}"),
                None => Ok(b"0123456789".to_vec()),
            }
        }

        async fn retrieve_content_stream(
            &self,
            _node_version_id: &str,
            _scope_type: &str,
            _scope_uuid: &str,
            _relative_path: &str,
            _pinned_generation: Option<&str>,
            range: Option<&str>,
            _if_match: Option<&str>,
            _if_none_match: Option<&str>,
            _if_range: Option<&str>,
            _if_modified_since: Option<&str>,
            _if_unmodified_since: Option<&str>,
        ) -> Result<Box<dyn DriveContentChunkStream>, DriveSdkworkError> {
            self.content_calls.fetch_add(1, Ordering::AcqRel);
            let content: &[u8] = match range {
                Some("bytes=2-5") => b"2345",
                Some("bytes=6-9") => b"6789",
                Some("bytes=0-9") => b"0123456789",
                Some(other) => panic!("unexpected test range {other}"),
                None => b"0123456789",
            };
            let chunks = content
                .chunks(4)
                .map(<[u8]>::to_vec)
                .collect::<VecDeque<_>>();
            Ok(Box::new(MemoryChunkStream { chunks }))
        }
    }

    struct MemoryStream {
        chunks: VecDeque<Vec<u8>>,
    }

    #[async_trait]
    impl WebsiteProviderContentStream for MemoryStream {
        async fn next_chunk(&mut self) -> WebsiteProviderResult<Option<Vec<u8>>> {
            Ok(self.chunks.pop_front())
        }
    }

    struct MemoryChunkStream {
        chunks: VecDeque<Vec<u8>>,
    }

    #[async_trait]
    impl DriveContentChunkStream for MemoryChunkStream {
        async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, DriveSdkworkError> {
            Ok(self.chunks.pop_front())
        }
    }

    struct MemoryWikiChunkStream {
        chunks: VecDeque<Vec<u8>>,
    }

    #[async_trait]
    impl WikiContentChunkStream for MemoryWikiChunkStream {
        async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, SdkworkError> {
            Ok(self.chunks.pop_front())
        }
    }

    struct FakeVariantStaticProvider {
        resolve_paths: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl WebsiteResourceProvider for FakeVariantStaticProvider {
        fn maximum_content_bytes(&self) -> u64 {
            1024
        }

        async fn validate_resource(
            &self,
            request: &ValidateWebsiteResourceRequest,
        ) -> WebsiteProviderResult<ValidatedWebsiteResource> {
            Ok(ValidatedWebsiteResource {
                provider_resource_uuid: request.provider.provider_resource_uuid.clone(),
                provider_generation: "1".to_owned(),
                public_generation: "1".to_owned(),
                capabilities: request.required_capabilities.clone(),
            })
        }
    }

    #[async_trait]
    impl WebsiteStaticContentProvider for FakeVariantStaticProvider {
        async fn resolve_static_path(
            &self,
            request: &ResolveWebsiteStaticPathRequest,
        ) -> WebsiteProviderResult<WebsiteContentResolution> {
            self.resolve_paths
                .lock()
                .unwrap()
                .push(request.provider_relative_path.clone());
            let (content_length, etag) = match request.provider_relative_path.as_str() {
                "/mobile/index.html" => (6, "\"mobile-v1\""),
                "/tv/index.html" => (2, "\"tv-v1\""),
                _ => {
                    return Err(
                        sdkwork_webserver_contract::provider::WebsiteProviderError::new(
                            WebsiteProviderErrorKind::NotFound,
                        ),
                    )
                }
            };
            Ok(WebsiteContentResolution::Found(ResolvedWebsiteContent {
                content_handle: WebsiteProviderContentHandle::new(
                    request.provider_relative_path.clone(),
                )
                .unwrap(),
                metadata: WebsiteContentMetadata {
                    content_type: "text/html; charset=utf-8".to_owned(),
                    content_length,
                    etag: etag.to_owned(),
                    last_modified: "Tue, 21 Jul 2026 00:00:00 GMT".to_owned(),
                    content_version: "1".to_owned(),
                    provider_generation: "1".to_owned(),
                    range_supported: false,
                },
            }))
        }

        async fn open_static_content(
            &self,
            request: &OpenWebsiteContentRequest,
        ) -> WebsiteProviderResult<OpenedWebsiteContent> {
            let content = match request.content_handle.as_str() {
                "/mobile/index.html" => b"mobile".as_slice(),
                "/tv/index.html" => b"tv".as_slice(),
                _ => {
                    return Err(
                        sdkwork_webserver_contract::provider::WebsiteProviderError::new(
                            WebsiteProviderErrorKind::NotFound,
                        ),
                    )
                }
            };
            Ok(OpenedWebsiteContent {
                stream: Box::new(MemoryStream {
                    chunks: VecDeque::from([content.to_vec()]),
                }),
                content_length: content.len() as u64,
                content_range: None,
            })
        }
    }

    fn website_executor(
        provider_timeout_ms: u64,
        force_https: bool,
    ) -> (
        Arc<WebsiteDeliveryExecutor>,
        Arc<FakeKnowledgebaseSdk>,
        Arc<FakeVariantStaticProvider>,
    ) {
        let runtime = active_runtime(provider_timeout_ms, force_https);
        let sdk = Arc::new(FakeKnowledgebaseSdk {
            route_calls: AtomicUsize::new(0),
            content_calls: AtomicUsize::new(0),
        });
        let sdk_client: Arc<dyn KnowledgebaseWikiSdkClient> = sdk.clone();
        let resolver =
            FixedKnowledgebaseWikiSdkClientResolver::new(TENANT_SCOPE_HASH, sdk_client).unwrap();
        let knowledgebase = Arc::new(KnowledgebaseWikiWebsiteProvider::new(Arc::new(resolver)));
        let variant_static = Arc::new(FakeVariantStaticProvider {
            resolve_paths: Mutex::new(Vec::new()),
        });
        let mut providers = WebsiteProviderRegistry::new();
        providers
            .register_wiki(WebsiteProviderType::Knowledgebase, knowledgebase)
            .unwrap();
        providers
            .register_static(WebsiteProviderType::Drive, variant_static.clone())
            .unwrap();
        (
            Arc::new(WebsiteDeliveryExecutor::new(runtime, Arc::new(providers))),
            sdk,
            variant_static,
        )
    }

    fn drive_executor(
        provider_timeout_ms: u64,
    ) -> (Arc<WebsiteDeliveryExecutor>, Arc<FakeDriveSdk>) {
        let runtime = active_runtime(provider_timeout_ms, false);
        let sdk = Arc::new(FakeDriveSdk {
            content_calls: AtomicUsize::new(0),
            next_resolve_status: AtomicU16::new(0),
            resolve_delay_ms: AtomicU64::new(0),
            resolve_requests: Mutex::new(Vec::new()),
        });
        let sdk_client: Arc<dyn DriveWebsiteSdkClient> = sdk.clone();
        let resolver =
            FixedDriveWebsiteSdkClientResolver::new(TENANT_SCOPE_HASH, sdk_client).unwrap();
        let drive = Arc::new(DriveWebsiteProvider::new(Arc::new(resolver)));
        let mut providers = WebsiteProviderRegistry::new();
        providers
            .register_static(WebsiteProviderType::Drive, drive)
            .unwrap();
        (
            Arc::new(WebsiteDeliveryExecutor::new(runtime, Arc::new(providers))),
            sdk,
        )
    }

    fn active_runtime(provider_timeout_ms: u64, force_https: bool) -> Arc<WebsiteRuntimeRegistry> {
        let registry = Arc::new(WebsiteRuntimeRegistry::new(
            NODE_UUID,
            WebsiteRuntimeEnvironment::Production,
        ));
        registry
            .compile_and_activate(&runtime_set(provider_timeout_ms, force_https))
            .unwrap();
        registry
    }

    fn runtime_set(provider_timeout_ms: u64, force_https: bool) -> Vec<u8> {
        let mut descriptor = json!({
            "schemaVersion": "sdkwork.website-runtime.v1",
            "kind": "sdkwork.website-runtime.descriptor",
            "revisionUuid": "revision-0001",
            "siteUuid": "site-0001",
            "tenantScopeHash": TENANT_SCOPE_HASH,
            "environment": "production",
            "generatedAt": "2026-07-21T00:00:00Z",
            "compilerVersion": "deploy-descriptor-compiler/1",
            "descriptorSha256": "0".repeat(64),
            "siteDefaultVariantUuid": "variant-desktop",
            "bindings": [{
                "bindingUuid": "binding-root",
                "hostname": "example.com",
                "pathPrefix": "/",
                "action": {"type": "SERVE"}
            }],
            "variants": [
                {"variantUuid": "variant-desktop", "label": "Desktop"},
                {"variantUuid": "variant-mobile", "label": "Mobile"},
                {"variantUuid": "variant-tv", "label": "TV"}
            ],
            "variantRules": [
                {
                    "ruleUuid": "rule-mobile-client",
                    "variantUuid": "variant-mobile",
                    "priority": 100,
                    "match": {"type": "CLIENT_CLASS", "clientClass": "MOBILE"}
                },
                {
                    "ruleUuid": "rule-tv-client",
                    "variantUuid": "variant-tv",
                    "priority": 100,
                    "match": {"type": "CLIENT_CLASS", "clientClass": "TV"}
                }
            ],
            "resources": [
                {
                    "resourceUuid": "resource-mobile",
                    "provider": {
                        "providerType": "DRIVE",
                        "providerResourceUuid": DRIVE_WEBSITE_ROOT_UUID,
                        "providerContractVersion": DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION
                    },
                    "capabilities": {
                        "staticContent": true,
                        "wikiRoutes": false,
                        "wikiSearch": false,
                        "rangeRequests": false
                    }
                },
                {
                    "resourceUuid": "resource-wiki",
                    "provider": {
                        "providerType": "KNOWLEDGEBASE",
                        "providerResourceUuid": PUBLICATION_UUID,
                        "providerContractVersion": KNOWLEDGEBASE_WIKI_PROVIDER_CONTRACT_VERSION
                    },
                    "capabilities": {
                        "staticContent": true,
                        "wikiRoutes": true,
                        "wikiSearch": true,
                        "rangeRequests": false
                    }
                }
            ],
            "mounts": [
                {
                    "mountUuid": "mount-desktop",
                    "variantUuid": "variant-desktop",
                    "pathPrefix": "/",
                    "resourceUuid": "resource-wiki",
                    "handler": "WIKI",
                    "translation": {"mode": "ROOT", "resourceSubpath": "/"},
                    "indexFiles": []
                },
                {
                    "mountUuid": "mount-mobile",
                    "variantUuid": "variant-mobile",
                    "pathPrefix": "/",
                    "resourceUuid": "resource-mobile",
                    "handler": "SPA",
                    "translation": {"mode": "ROOT", "resourceSubpath": "/mobile"},
                    "indexFiles": ["index.html"],
                    "spaFallback": "/mobile/index.html"
                },
                {
                    "mountUuid": "mount-tv",
                    "variantUuid": "variant-tv",
                    "pathPrefix": "/",
                    "resourceUuid": "resource-mobile",
                    "handler": "SPA",
                    "translation": {"mode": "ROOT", "resourceSubpath": "/tv"},
                    "indexFiles": ["index.html"],
                    "spaFallback": "/tv/index.html"
                }
            ],
            "deliveryPolicy": {
                "providerTimeoutMs": provider_timeout_ms,
                "metadataCacheTtlSeconds": 60,
                "negativeCacheTtlSeconds": 5,
                "staleWhileRevalidateSeconds": 30,
                "maximumObjectBytes": 1024
            },
            "securityPolicy": {
                "forceHttps": force_https,
                "denyDotFiles": true,
                "deniedPathPrefixes": []
            },
            "limits": {
                "maximumBindings": 8,
                "maximumVariants": 8,
                "maximumVariantRules": 8,
                "maximumResources": 8,
                "maximumMounts": 8,
                "maximumIndexFilesPerMount": 8,
                "maximumPathBytes": 2048,
                "maximumPathSegments": 64
            },
            "observabilityPolicy": {
                "accessLogEnabled": true,
                "usageMeteringEnabled": true,
                "traceSampleRatePerMille": 10
            }
        });
        let parsed: WebsiteRuntimeDescriptor = serde_json::from_value(descriptor.clone()).unwrap();
        descriptor["descriptorSha256"] =
            Value::String(website_runtime_descriptor_sha256(&parsed).unwrap());
        let mut snapshot = json!({
            "schemaVersion": "sdkwork.website-runtime-set.v1",
            "kind": "sdkwork.website-runtime-set.snapshot",
            "snapshotUuid": "snapshot-0001",
            "nodeUuid": NODE_UUID,
            "environment": "production",
            "generation": 1,
            "generatedAt": "2026-07-21T00:00:00Z",
            "compilerVersion": "deploy-runtime-set-compiler/1",
            "snapshotSha256": "0".repeat(64),
            "maximumSites": 8,
            "descriptors": [descriptor]
        });
        let parsed: WebsiteRuntimeSetSnapshot = serde_json::from_value(snapshot.clone()).unwrap();
        snapshot["snapshotSha256"] =
            Value::String(website_runtime_set_snapshot_sha256(&parsed).unwrap());
        serde_json::to_vec(&snapshot).unwrap()
    }

    fn wiki_publication() -> WikiPublication {
        WikiPublication {
            publication_uuid: PUBLICATION_UUID.to_owned(),
            title: "SDKWork Wiki".to_owned(),
            description: None,
            homepage_source_path: "README.md".to_owned(),
            default_locale: "zh-CN".to_owned(),
            supported_locales: vec!["zh-CN".to_owned()],
            navigation_mode: "DIRECTORY".to_owned(),
            theme_key: "sdkwork-wiki".to_owned(),
            theme_version: "theme-v1".to_owned(),
            renderer_policy_version: "renderer-v2".to_owned(),
            search_enabled: true,
            robots_policy: "INDEX_FOLLOW".to_owned(),
            sitemap_enabled: true,
            provider_generation: "3".to_owned(),
            navigation_generation: "4".to_owned(),
            search_generation: "5".to_owned(),
        }
    }

    fn public_wiki_page() -> WikiPublicPageMetadata {
        WikiPublicPageMetadata {
            projection_uuid: PROJECTION_UUID.to_owned(),
            canonical_route: "/guide/".to_owned(),
            file_kind: "PAGE".to_owned(),
            media_type: "text/markdown".to_owned(),
            size_bytes: "6".to_owned(),
            content_sha256: CONTENT_SHA256.to_owned(),
            title: Some("Guide".to_owned()),
            description: Some("Wiki guide".to_owned()),
            locale: Some("zh-CN".to_owned()),
            nav_order: Some(1),
            page_public_version: "7".to_owned(),
            public_updated_at: "2026-07-21T00:00:00Z".to_owned(),
        }
    }
}
