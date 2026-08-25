use std::path::Path;

use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
};
use sdkwork_webserver_core::{RouteConfig, RoutePathType};

use super::{
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
            serve_opened_file(file, request.method(), request.headers()).await
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
