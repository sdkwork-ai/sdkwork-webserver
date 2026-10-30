//! Bounded nginx subrequest engine (`addition`, `auth_request`, `mirror`).
//!
//! A subrequest is an internal dispatch of an absolute URI (`add_before_body`,
//! `add_after_body`, `auth_request`, `mirror`) against the same listener and
//! configuration generation as the main request. The bounded policy is:
//!
//! - one level only: subrequest targets never execute their own
//!   `addition`/`auth_request`/`mirror` filters (nginx's subrequest-of-subrequest
//!   behavior is not reproduced);
//! - routing re-enters [`CompiledWebServerApp::select_route`] with the main
//!   request's authority, so the target location (including `internal`
//!   locations) is selected exactly like an nginx internal subrequest;
//! - executable target content handlers are `return` (`Respond`), `empty_gif`
//!   and `proxy_pass` (`Proxy`); any other resource fails closed;
//! - subrequests do not consume the connection's request-admission permit and
//!   are not metered (bounded, documented difference from nginx accounting);
//! - response bodies read by a filter are capped (`max_body_bytes`); a larger
//!   body makes the filter skip the splice/assignment instead of buffering
//!   without bound.

use std::net::IpAddr;
use std::time::Duration;

use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, HeaderMap, HeaderValue, Method, Request, Response, StatusCode},
};
use sdkwork_webserver_core::{
    AdditionConfig, AuthRequestConfig, MirrorConfig, ResourceConfig, RouteConfig,
};

use super::{
    delivery_policy::MimePolicy,
    handler::empty_gif_response,
    map_variables::MapResults,
    proxy::{proxy_request_cached, ProxyRequestContext},
    proxy_body::RequestBodyFailure,
    runtime::RuntimeGeneration,
    ListenerState,
};

/// Per-request facts a subrequest dispatch needs.
pub(crate) struct SubrequestEnv<'a> {
    pub(crate) state: &'a ListenerState,
    pub(crate) generation: &'a std::sync::Arc<RuntimeGeneration>,
    pub(crate) listener_id: &'a str,
    pub(crate) authority: &'a str,
    pub(crate) client_ip: IpAddr,
    pub(crate) scheme: &'a str,
    pub(crate) listener_port: u16,
    pub(crate) method: &'a Method,
    pub(crate) headers: &'a HeaderMap,
    pub(crate) map_results: &'a MapResults,
    /// Upper bound on a response body a filter is willing to buffer.
    pub(crate) max_body_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubrequestFailure {
    NoRoute,
    UnsupportedResource,
    TooLarge,
}

/// Serve one internal subrequest URI.
pub(crate) async fn serve(
    env: &SubrequestEnv<'_>,
    uri: &str,
    method: &Method,
) -> Result<Response<Body>, SubrequestFailure> {
    let (path, query) = uri.split_once('?').unwrap_or((uri, ""));
    if !path.starts_with('/') {
        return Err(SubrequestFailure::UnsupportedResource);
    }
    let selected = env
        .generation
        .app
        .select_route(env.listener_id, env.authority, path, method.as_str())
        .ok_or(SubrequestFailure::NoRoute)?;
    match selected.resource {
        ResourceConfig::Respond {
            status,
            content_type,
            content_type_from_uri,
            body,
            ..
        } => {
            let body = env.map_results.expand_tokens(body);
            let content_type = if *content_type_from_uri {
                MimePolicy::resolve(
                    env.generation.app.config(),
                    selected.virtual_host,
                    selected.route,
                )
                .content_type_for_uri(path)
            } else {
                HeaderValue::from_str(content_type).unwrap_or(HeaderValue::from_static("text/plain"))
            };
            let mut response = Response::new(Body::from(body));
            *response.status_mut() = StatusCode::from_u16(*status)
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            response
                .headers_mut()
                .insert(CONTENT_TYPE, content_type);
            Ok(response)
        }
        ResourceConfig::EmptyGif { .. } => Ok(empty_gif_response(*method == Method::HEAD)),
        ResourceConfig::Proxy {
            upstream_ref,
            strip_prefix,
            target_uri,
            request_set_headers,
            dynamic_target,
            proxy_pass_request_headers,
            proxy_method,
            proxy_response_policy,
            ..
        } => {
            let context = ProxyRequestContext {
                generation: env.generation,
                upstream_ref,
                strip_prefix: *strip_prefix,
                target_uri: target_uri.as_deref(),
                request_set_headers,
                dynamic_target: dynamic_target.as_deref(),
                proxy_pass_request_headers: *proxy_pass_request_headers,
                route: selected.route,
                client_ip: env.client_ip,
                external_scheme: env.scheme,
                external_authority: env.authority,
                listener_port: env.listener_port,
                normalized_path: path,
                path_rewritten: false,
                request_failure: RequestBodyFailure::default(),
                tunnel_supervisor: &env.state.runtime.tunnel_supervisor,
                metrics: &env.state.runtime.metrics,
                cache: env.generation.proxy_cache.clone(),
                map_results: env.map_results,
                response_policy: proxy_response_policy.as_ref(),
                proxy_method: proxy_method.as_deref(),
                variables: None,
            };
            let request_uri = if query.is_empty() {
                path.to_owned()
            } else {
                format!("{path}?{query}")
            };
            let mut request = Request::builder()
                .method(method.clone())
                .uri(request_uri)
                .body(Body::empty())
                .map_err(|_| SubrequestFailure::UnsupportedResource)?;
            merge_request_headers(request.headers_mut(), env);
            Ok(proxy_request_cached(context, request).await)
        }
        _ => Err(SubrequestFailure::UnsupportedResource),
    }
}

fn merge_request_headers(target: &mut HeaderMap, env: &SubrequestEnv<'_>) {
    for (name, value) in env.headers {
        target.append(name.clone(), value.clone());
    }
    if !target.contains_key(axum::http::header::HOST) {
        if let Ok(value) = HeaderValue::from_str(env.authority) {
            target.insert(axum::http::header::HOST, value);
        }
    }
}

/// Read a bounded response body; `None` when the body is larger than the cap.
async fn read_bounded_body(
    body: Body,
    maximum: u64,
) -> Option<bytes::Bytes> {
    if let Some(size) = http_body::Body::size_hint(&body).exact() {
        if size > maximum {
            return None;
        }
    }
    let limit = usize::try_from(maximum).unwrap_or(usize::MAX);
    let limited = http_body_util::Limited::new(body, limit);
    match http_body_util::BodyExt::collect(limited).await {
        Ok(collected) => Some(collected.to_bytes()),
        Err(_) => None,
    }
}

// ---------------------------------------------------------------------------
// auth_request
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub(crate) enum AuthOutcome {
    Allow,
    Reject(Response<Body>),
}

/// Run the `auth_request` subrequest and map its status to nginx's decision.
///
/// nginx `ngx_http_auth_request_module`: 2xx allows; 401/403 deny with the
/// subrequest's status (the `WWW-Authenticate` field is propagated); any other
/// status is an internal error (500). `auth_request_set` values are taken from
/// the subrequest response before the body is dropped.
pub(crate) async fn authorize(
    env: &SubrequestEnv<'_>,
    config: &AuthRequestConfig,
) -> AuthOutcome {
    let Ok(response) = serve(env, &config.uri, env.method).await else {
        return AuthOutcome::Reject(empty_status(StatusCode::INTERNAL_SERVER_ERROR));
    };
    let status = response.status();
    for assignment in &config.set {
        let value = expand_upstream_value(&assignment.value, status, response.headers());
        env.map_results.insert(&assignment.variable, value);
    }
    if status.is_success() {
        return AuthOutcome::Allow;
    }
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        let mut rejected = empty_status(status);
        for value in response.headers().get_all("www-authenticate") {
            rejected
                .headers_mut()
                .append(axum::http::header::WWW_AUTHENTICATE, value.clone());
        }
        return AuthOutcome::Reject(rejected);
    }
    AuthOutcome::Reject(empty_status(StatusCode::INTERNAL_SERVER_ERROR))
}

fn empty_status(status: StatusCode) -> Response<Body> {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = status;
    response
}

/// Expand the bounded `auth_request_set` value vocabulary: literals,
/// `$upstream_status`, and `$upstream_http_<name>` fields of the subrequest
/// response (repeated fields joined like nginx `$upstream_http_…`).
fn expand_upstream_value(template: &str, status: StatusCode, headers: &HeaderMap) -> String {
    let mut output = String::with_capacity(template.len() + 8);
    let mut remainder = template;
    while let Some(index) = remainder.find('$') {
        output.push_str(&remainder[..index]);
        let after = &remainder[index + 1..];
        if let Some(literal) = after.strip_prefix('$') {
            output.push('$');
            remainder = literal;
            continue;
        }
        let end = after
            .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .unwrap_or(after.len());
        if end == 0 {
            output.push('$');
            remainder = after;
            continue;
        }
        let name = &after[..end];
        if name == "upstream_status" {
            output.push_str(&status.as_u16().to_string());
        } else if let Some(header) = name.strip_prefix("upstream_http_") {
            let header = header.replace('_', "-");
            output.push_str(&joined_header(headers, &header));
        }
        remainder = &after[end..];
    }
    output.push_str(remainder);
    output
}

fn joined_header(headers: &HeaderMap, name: &str) -> String {
    let mut output = String::new();
    for value in headers.get_all(name) {
        if !output.is_empty() {
            output.push_str(", ");
        }
        output.push_str(value.to_str().unwrap_or_default().trim());
    }
    output
}

// ---------------------------------------------------------------------------
// addition
// ---------------------------------------------------------------------------

/// Splice the configured `add_before_body` / `add_after_body` subrequest bodies
/// around the main response body when the response content type matches
/// `addition_types`.
pub(crate) async fn apply_addition(
    response: Response<Body>,
    env: &SubrequestEnv<'_>,
    config: &AdditionConfig,
) -> Response<Body> {
    if env.method == Method::HEAD {
        return response;
    }
    if !content_type_matches(response.headers(), &config.types) {
        return response;
    }
    // The main body must be buffered; a streaming/oversized body is passed
    // through unchanged (bounded difference, documented).
    let Some(main_size) = http_body::Body::size_hint(response.body()).exact() else {
        return response;
    };
    if main_size > env.max_body_bytes {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let Some(main) = read_bounded_body(body, env.max_body_bytes).await else {
        return Response::from_parts(parts, Body::empty());
    };
    let mut output = Vec::with_capacity(main.len());
    for uri in [config.before.as_deref(), Some("__main__")] {
        if uri == Some("__main__") {
            output.extend_from_slice(&main);
            continue;
        }
        let Some(uri) = uri else { continue };
        if let Ok(pre) = serve(env, uri, &Method::GET).await {
            let (pre_parts, pre_body) = pre.into_parts();
            if pre_parts.status.is_success() {
                if let Some(bytes) = read_bounded_body(pre_body, env.max_body_bytes).await {
                    output.extend_from_slice(&bytes);
                }
            }
        }
    }
    if let Some(uri) = config.after.as_deref() {
        if let Ok(post) = serve(env, uri, &Method::GET).await {
            let (post_parts, post_body) = post.into_parts();
            if post_parts.status.is_success() {
                if let Some(bytes) = read_bounded_body(post_body, env.max_body_bytes).await {
                    output.extend_from_slice(&bytes);
                }
            }
        }
    }
    parts.headers.remove(axum::http::header::CONTENT_LENGTH);
    Response::from_parts(parts, Body::from(output))
}

fn content_type_matches(headers: &HeaderMap, types: &[String]) -> bool {
    let Some(value) = headers.get(CONTENT_TYPE).and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let media_type = value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    types
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(&media_type))
}

// ---------------------------------------------------------------------------
// mirror
// ---------------------------------------------------------------------------

/// Fire the configured mirror subrequests without affecting the main response.
/// The tasks are detached with a bounded timeout; a mirror failure is ignored
/// exactly like nginx. `mirror_request_body on` is rejected at materialization,
/// so the subrequests carry no body.
pub(crate) fn fire_mirror(env: &SubrequestEnv<'_>, config: &MirrorConfig) {
    if config.request_body || config.targets.is_empty() {
        return;
    }
    let state = env.state.clone();
    let generation = env.generation.clone();
    let listener_id = env.listener_id.to_owned();
    let authority = env.authority.to_owned();
    let client_ip = env.client_ip;
    let scheme = env.scheme.to_owned();
    let listener_port = env.listener_port;
    let method = env.method.clone();
    let headers = env.headers.clone();
    let map_results = env.map_results.clone();
    let max_body_bytes = env.max_body_bytes;
    let targets = config.targets.clone();
    tokio::spawn(async move {
        for target in targets {
            let owned = SubrequestEnv {
                state: &state,
                generation: &generation,
                listener_id: &listener_id,
                authority: &authority,
                client_ip,
                scheme: &scheme,
                listener_port,
                method: &method,
                headers: &headers,
                map_results: &map_results,
                max_body_bytes,
            };
            let _ = tokio::time::timeout(Duration::from_secs(5), serve(&owned, &target, &method)).await;
        }
    });
}

/// Resolve the route a configured subrequest URI would select; used by the
/// materializer-facing diagnostics and tests.
#[cfg(test)]
pub(crate) fn route_exists(
    app: &sdkwork_webserver_core::CompiledWebServerApp,
    listener_id: &str,
    authority: &str,
    uri: &str,
) -> bool {
    app.select_route(listener_id, authority, uri, "GET").is_some()
}
