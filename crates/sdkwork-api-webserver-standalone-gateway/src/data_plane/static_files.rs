use std::path::Path;

use axum::{
    body::Body,
    http::{header, HeaderValue, Method, Request, Response, StatusCode},
};
use sdkwork_webserver_core::{RouteConfig, RoutePathType};

use super::{
    credential_entry_injection::{inject_bootstrap_token, injection_token},
    proxy::text_response,
    static_file_response::serve_opened_file,
    static_path::{open_static_path, StaticPathError, StaticPathTarget},
};

pub async fn serve_static(
    root: &Path,
    route: &RouteConfig,
    strip_prefix: bool,
    spa_fallback: Option<&str>,
    normalized_request_path: &str,
    request: Request<Body>,
) -> Response<Body> {
    if !matches!(request.method().as_str(), "GET" | "HEAD") {
        return text_response(StatusCode::METHOD_NOT_ALLOWED, "method is not allowed\n");
    }

    let relative = relative_request_path(route, strip_prefix, normalized_request_path);
    let target = match open_static_path(
        root,
        relative,
        normalized_request_path.ends_with('/'),
        spa_fallback,
    )
    .await
    {
        Ok(target) => target,
        Err(error) => return static_path_error_response(error),
    };

    match target {
        StaticPathTarget::File(file) => {
            if let Some(response) = serve_spa_index_with_credential_entry_bootstrap(
                root,
                &file,
                spa_fallback,
                request.method(),
            ) {
                return with_adaptive_vary_if_spa(response, spa_fallback);
            }
            let response = serve_opened_file(file, request.method(), request.headers()).await;
            with_adaptive_vary_if_spa(response, spa_fallback)
        }
        StaticPathTarget::RedirectToDirectory => {
            let Some(location) = append_request_path_slash(request.uri()) else {
                return text_response(StatusCode::BAD_REQUEST, "invalid request path\n");
            };
            Response::builder()
                .status(StatusCode::TEMPORARY_REDIRECT)
                .header(axum::http::header::LOCATION, location)
                .body(Body::empty())
                .unwrap_or_else(|_| {
                    text_response(StatusCode::BAD_REQUEST, "invalid request path\n")
                })
        }
    }
}

/// Adaptive Web dispatch resolves the same request path against either the PC
/// or the H5 surface root depending on `User-Agent` / `Sec-CH-UA-Mobile`
/// (SDKWORK_DEPLOY_SPEC.md §8.1; APP_RUNTIME_TOPOLOGY_SPEC adaptive ingress
/// requires `Vary: user-agent`, and `Sec-CH-UA-Mobile` participates in the
/// same device-class detection). Every SPA-surface response therefore declares
/// both request headers as representation selectors so shared caches and CDNs
/// never hand one renderer's representation to the other device class.
const ADAPTIVE_VARY_VALUE: &str = "User-Agent, Sec-CH-UA-Mobile";

fn with_adaptive_vary_if_spa(mut response: Response<Body>, spa_fallback: Option<&str>) -> Response<Body> {
    if spa_fallback.is_some() && !response.headers().contains_key(header::VARY) {
        if let Ok(value) = HeaderValue::from_str(ADAPTIVE_VARY_VALUE) {
            response.headers_mut().insert(header::VARY, value);
        }
    }
    response
}

/// nginx path semantics for a static resource: `root` appends the full
/// request path (`strip_prefix = false`), `alias` replaces the route's
/// matched prefix with the alias value (`strip_prefix = true`).
pub(crate) fn relative_request_path<'a>(
    route: &RouteConfig,
    strip_prefix: bool,
    request_path: &'a str,
) -> &'a str {
    if !strip_prefix {
        return request_path;
    }
    match route.route_match.path_type {
        RoutePathType::Exact | RoutePathType::Prefix | RoutePathType::PrefixExclusive => {
            request_path
                .strip_prefix(&route.route_match.path)
                .unwrap_or(request_path)
        }
        // Regex locations keep the full request path for proxy/static mapping;
        // capture-based rewrite is a separate capability.
        RoutePathType::Regex | RoutePathType::RegexIgnoreCase => request_path,
    }
}

fn append_request_path_slash(uri: &axum::http::Uri) -> Option<axum::http::HeaderValue> {
    let path_and_query = uri.path_and_query()?;
    let value = match path_and_query.query() {
        Some(query) => format!("{}/?{query}", path_and_query.path()),
        None => format!("{}/", path_and_query.path()),
    };
    axum::http::HeaderValue::from_str(&value).ok()
}

fn static_path_error_response(error: StaticPathError) -> Response<Body> {
    let status = match error {
        StaticPathError::Invalid => StatusCode::BAD_REQUEST,
        StaticPathError::Forbidden => StatusCode::FORBIDDEN,
        StaticPathError::NotFound => StatusCode::NOT_FOUND,
        StaticPathError::Io => StatusCode::INTERNAL_SERVER_ERROR,
    };
    text_response(status, "static path is not available\n")
}

/// Serves the SPA `index.html` with the development credential-entry bootstrap
/// Access-Token injected (`IAM_CREDENTIAL_ENTRY_SPEC.md` §4/§5), mirroring
/// `app_shell::serve_index_with_bootstrap_token` for module-import surfaces.
///
/// Returns `None` whenever the request should fall through to the ordinary
/// static file service: no development bootstrap token is configured, the
/// route has no SPA fallback, or the served file is not the fallback index.
/// HEAD mirrors the GET headers including the injected `Content-Length`.
fn serve_spa_index_with_credential_entry_bootstrap(
    root: &Path,
    file: &super::static_path::OpenedStaticFile,
    spa_fallback: Option<&str>,
    method: &Method,
) -> Option<Response<Body>> {
    let Some(token) = injection_token() else {
        return None;
    };
    // Only SPA routes carry the credential-entry login renderer; ordinary
    // static surfaces never receive the token.
    let spa_fallback = spa_fallback?;
    if file
        .path_hint
        .file_name()
        .is_none_or(|name| name != "index.html")
    {
        return None;
    }
    if !is_spa_fallback_index(spa_fallback) {
        return None;
    }

    // Data-plane path hints keep the request shape (a `/` request yields an
    // absolute `/index.html` hint); re-anchor the hint to the surface root
    // before reading, mirroring how `open_fallback` re-opens relative hints.
    let anchored = file
        .path_hint
        .strip_prefix(std::path::Path::new(std::path::MAIN_SEPARATOR_STR))
        .unwrap_or(&file.path_hint);
    let html = std::fs::read(root.join(anchored)).ok()?;
    let html = inject_bootstrap_token(&html, token);
    let builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_LENGTH, html.len())
        .header(header::CACHE_CONTROL, "public, no-cache")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    if method == Method::HEAD {
        return builder.body(Body::empty()).ok();
    }
    builder.body(Body::from(html)).ok()
}

/// The SPA fallback must itself name the index document (`try_files ...
/// /index.html`); this keeps the token scoped to SPA login renderers instead
/// of any coincidentally named file.
fn is_spa_fallback_index(spa_fallback: &str) -> bool {
    spa_fallback
        .rsplit('/')
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case("index.html"))
}
