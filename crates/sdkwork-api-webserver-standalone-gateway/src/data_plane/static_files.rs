use std::path::Path;

use axum::{
    body::Body,
    http::{header, HeaderValue, Method, Request, Response, StatusCode},
};
use futures_util::stream;
use sdkwork_webserver_core::{
    AutoindexConfig, GzipConfig, GzipStaticMode, RouteConfig, RoutePathType,
};

use super::{
    credential_entry_injection::{inject_bootstrap_token, injection_token},
    delivery_policy::MimePolicy,
    proxy::text_response,
    static_file_response::serve_opened_file_representation,
    static_path::{open_static_path, StaticPathError, StaticPathTarget},
};

/// Resolved delivery policy for one static route: the nginx `types` table and
/// the nginx `gzip*` policy (whose `gzip_static` mode selects a precompressed
/// sidecar).
#[derive(Clone, Copy)]
pub(crate) struct StaticDeliveryPolicy<'a> {
    pub(crate) mime: MimePolicy<'a>,
    pub(crate) gzip: &'a GzipConfig,
    /// Effective nginx `max_ranges` for this route (`0` disables range
    /// processing; the app default is an effectively unbounded ceiling).
    pub(crate) max_ranges: u64,
    /// Effective nginx `autoindex` family for this route; `None` keeps nginx's
    /// default (directory listings off).
    pub(crate) autoindex: Option<&'a AutoindexConfig>,
}

pub async fn serve_static(
    root: &Path,
    route: &RouteConfig,
    strip_prefix: bool,
    spa_fallback: Option<&str>,
    normalized_request_path: &str,
    request: Request<Body>,
    delivery: StaticDeliveryPolicy<'_>,
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
        delivery
            .autoindex
            .is_some_and(|autoindex| autoindex.enabled),
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
            )
            .await
            {
                return with_adaptive_vary_if_spa(response, spa_fallback);
            }
            let response = serve_static_representation(root, file, request, route, delivery).await;
            with_adaptive_vary_if_spa(response, spa_fallback)
        }
        StaticPathTarget::RedirectToDirectory => {
            let Some(location) = append_request_path_slash(request.uri()) else {
                return text_response(StatusCode::BAD_REQUEST, "invalid request path\n");
            };
            // nginx answers a directory request without its trailing slash with
            // 301 Moved Permanently (`ngx_http_static_module`), not 307: the
            // redirect target is the canonical URI of the same resource. The
            // Location is relative because the differential fixture declares
            // `absolute_redirect off`; the runtime has no configurable absolute
            // form, and the only difference from nginx's default absolute URL is
            // the scheme/authority prefix, never the path or query.
            Response::builder()
                .status(StatusCode::MOVED_PERMANENTLY)
                .header(axum::http::header::LOCATION, location)
                .body(Body::empty())
                .unwrap_or_else(|_| {
                    text_response(StatusCode::BAD_REQUEST, "invalid request path\n")
                })
        }
        StaticPathTarget::Autoindex { mut entries } => {
            let Some(autoindex) = delivery.autoindex.filter(|autoindex| autoindex.enabled) else {
                // The listing was only produced because the caller asked for
                // one; without a policy it must stay nginx's 403.
                return text_response(StatusCode::FORBIDDEN, "forbidden\n");
            };
            let mut callback = None;
            let mut effective_autoindex = autoindex.clone();
            if autoindex.format == sdkwork_webserver_core::AutoindexFormat::Jsonp {
                let query = request.uri().query().unwrap_or("");
                let Some(raw) = query_parameter(query, "callback") else {
                    // nginx downgrades a missing callback to the JSON format
                    // (content type and wrapper both switch).
                    effective_autoindex.format = sdkwork_webserver_core::AutoindexFormat::Json;
                    return autoindex_response(
                        &mut entries,
                        &effective_autoindex,
                        normalized_request_path,
                        None,
                        request.method(),
                    );
                };
                let Some(validated) = super::autoindex::validate_jsonp_callback(raw) else {
                    return text_response(StatusCode::BAD_REQUEST, "invalid callback name\n");
                };
                callback = Some(validated.to_owned());
            }
            autoindex_response(
                &mut entries,
                autoindex,
                normalized_request_path,
                callback.as_deref(),
                request.method(),
            )
        }
    }
}

/// Raw `name=value` query lookup (nginx `ngx_http_arg` does not percent-decode
/// the value either).
fn query_parameter<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    query.split('&').find_map(|parameter| {
        let (key, value) = parameter.split_once('=')?;
        (key == name).then_some(value)
    })
}

/// Build the nginx `autoindex` response: 200 with the rendered body and its
/// media type, bounded by `Content-Length`; HEAD keeps the headers and drops
/// the body like nginx's generated listing.
fn autoindex_response(
    entries: &mut [super::static_path::AutoindexEntry],
    autoindex: &AutoindexConfig,
    request_uri: &str,
    callback: Option<&str>,
    method: &axum::http::Method,
) -> Response<Body> {
    let rendered = super::autoindex::render(autoindex, request_uri, entries, callback);
    let content_type = match axum::http::HeaderValue::from_str(rendered.content_type) {
        Ok(value) => value,
        Err(_) => return text_response(StatusCode::INTERNAL_SERVER_ERROR, "invalid listing\n"),
    };
    // nginx's autoindex handler sends the response header before it builds the
    // listing, so the body is streamed (chunked on HTTP/1.1) with no
    // `Content-Length`; HEAD keeps the headers and drops the body. The body has
    // no exact size hint here for the same reason.
    let body = if method == axum::http::Method::HEAD {
        // A size-less empty stream keeps hyper from inferring `Content-Length:
        // 0`: nginx's HEAD listing carries only the generated headers.
        Body::from_stream(stream::empty::<Result<bytes::Bytes, std::io::Error>>())
    } else {
        let bytes = bytes::Bytes::from(rendered.body);
        Body::from_stream(stream::once(async move { Ok::<_, std::io::Error>(bytes) }))
    };
    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(axum::http::header::CONTENT_TYPE, content_type);
    response
}

/// Serve one opened static file, preferring a precompressed `<path>.gz` sidecar
/// when nginx `gzip_static` selects it (differential-probed against nginx
/// 1.29.6):
///
/// - `on`: the sidecar is served when the client accepts gzip; when the client
///   does not, the identity file is served instead and `gzip_vary on` still
///   declares `Vary: Accept-Encoding`;
/// - `always`: the sidecar is served regardless of `Accept-Encoding`;
/// - `off`: the identity file is served.
///
/// A sidecar representation carries `Content-Encoding: gzip` and its
/// `Content-Length`/`ETag`/`Last-Modified` are the **sidecar file's** (nginx
/// opens the `.gz` file and publishes its metadata), while `Content-Type` and
/// the freshness policy belong to the original path. Range requests range over
/// the stored (gzip) bytes, exactly like nginx. The sidecar is opened through
/// the same no-symlink, no-`..` path walk as the identity file, so it can never
/// escape the static root.
async fn serve_static_representation(
    root: &Path,
    file: super::static_path::OpenedStaticFile,
    request: Request<Body>,
    route: &RouteConfig,
    delivery: StaticDeliveryPolicy<'_>,
) -> Response<Body> {
    let content_type = delivery.mime.content_type(&file.path_hint);
    let cache_control_path = file.path_hint.clone();
    let identity_hints =
        |vary_accept_encoding: bool| super::static_file_response::RepresentationHints {
            content_type: content_type.clone(),
            content_encoding: None,
            vary_accept_encoding,
            cache_control_path: &cache_control_path,
        };
    let mode = delivery.gzip.static_files;
    if !matches!(mode, GzipStaticMode::Off) {
        if let Some(sidecar) = open_gzip_sidecar(root, &file.path_hint).await {
            let accepted = super::gzip_predicate::accepts_gzip(
                request.headers().get(axum::http::header::ACCEPT_ENCODING),
            );
            if matches!(mode, GzipStaticMode::Always) || accepted {
                let hints = super::static_file_response::RepresentationHints {
                    content_type: content_type.clone(),
                    content_encoding: Some("gzip"),
                    // nginx 1.29.6 only sets the vary flag in `on` mode: the
                    // `always` path never consults `Accept-Encoding` and does
                    // not advertise the negotiation (differential-confirmed).
                    vary_accept_encoding: matches!(mode, GzipStaticMode::On) && delivery.gzip.vary,
                    cache_control_path: &cache_control_path,
                };
                return serve_opened_file_representation(
                    sidecar,
                    request.method(),
                    request.headers(),
                    route.cache_policy.as_ref(),
                    &hints,
                    delivery.max_ranges,
                )
                .await;
            }
            // The sidecar exists but this client will not decode it; serve the
            // identity representation and still vary when the policy says so.
            return serve_opened_file_representation(
                file,
                request.method(),
                request.headers(),
                route.cache_policy.as_ref(),
                &identity_hints(matches!(mode, GzipStaticMode::On) && delivery.gzip.vary),
                delivery.max_ranges,
            )
            .await;
        }
    }
    serve_opened_file_representation(
        file,
        request.method(),
        request.headers(),
        route.cache_policy.as_ref(),
        &identity_hints(false),
        delivery.max_ranges,
    )
    .await
}

/// Open `<path_hint>.gz` through the static-root path walk. `path_hint` is
/// already root-relative and symlink-free, so appending the suffix cannot
/// escape the root; the component walk then refuses a symlinked sidecar.
async fn open_gzip_sidecar(
    root: &Path,
    path_hint: &Path,
) -> Option<super::static_path::OpenedStaticFile> {
    let mut sidecar = path_hint.as_os_str().to_os_string();
    sidecar.push(".gz");
    let sidecar = sidecar.to_str()?;
    match open_static_path(root, sidecar, false, None, false).await {
        Ok(StaticPathTarget::File(file)) => Some(file),
        _ => None,
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

/// Upper bound for any `index.html` the process reads for token injection:
/// the SPA data-plane fallback read here, and the management-gated shell
/// preflight in `app_shell`, which imports this constant. Lives on the
/// always-compiled data plane so the gateway lib builds without the
/// `management` feature.
pub(crate) const MAX_BOOTSTRAP_FILE_BYTES: u64 = 4 * 1024 * 1024;

fn with_adaptive_vary_if_spa(
    mut response: Response<Body>,
    spa_fallback: Option<&str>,
) -> Response<Body> {
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
async fn serve_spa_index_with_credential_entry_bootstrap(
    root: &Path,
    file: &super::static_path::OpenedStaticFile,
    spa_fallback: Option<&str>,
    method: &Method,
) -> Option<Response<Body>> {
    let token = injection_token()?;
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
    let path = root.join(anchored);
    // The read runs on the blocking pool: an index.html is operator-deployed
    // and small, but a slow disk must never stall the runtime worker. The cap
    // mirrors the shell's `read_bounded_index`: an oversized mis-deploy would
    // otherwise be materialized in full — twice, once more for the token
    // injection copy — on every fallback request.
    let html = tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut handle = std::fs::File::open(path)?;
        let mut buffer = Vec::new();
        handle
            .by_ref()
            .take(MAX_BOOTSTRAP_FILE_BYTES + 1)
            .read_to_end(&mut buffer)?;
        if buffer.len() as u64 > MAX_BOOTSTRAP_FILE_BYTES {
            return Err(std::io::Error::other(
                "SPA fallback index exceeds the bootstrap file cap",
            ));
        }
        Ok(buffer)
    })
    .await
    .ok()?
    .ok()?;
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// The sidecar lookup walks the static root exactly like the identity file:
    /// a sidecar symlink that points outside the root must never be served.
    #[tokio::test]
    async fn gzip_sidecar_never_escapes_the_static_root_through_a_symlink() {
        let directory = tempfile::tempdir().expect("temp dir");
        let root = directory.path().join("public");
        std::fs::create_dir_all(&root).expect("root");
        std::fs::write(root.join("page.txt"), b"identity body\n").expect("identity");
        let outside = directory.path().join("outside.txt.gz");
        std::fs::write(&outside, b"outside secret\n").expect("outside");
        symlink(&outside, root.join("page.txt.gz")).expect("symlink sidecar");

        let sidecar = open_gzip_sidecar(&root, Path::new("page.txt")).await;
        assert!(
            sidecar.is_none(),
            "a symlinked .gz sidecar must not be opened"
        );
    }

    /// The sidecar path is derived by appending `.gz` to the already
    /// root-relative, symlink-free resolved path, so a directory index
    /// (`sub/index.html`) looks for `sub/index.html.gz`.
    #[tokio::test]
    async fn gzip_sidecar_is_derived_from_the_resolved_root_relative_path() {
        let directory = tempfile::tempdir().expect("temp dir");
        let root = directory.path().join("public");
        std::fs::create_dir_all(root.join("sub")).expect("root");
        std::fs::write(root.join("sub/index.html"), b"<html></html>").expect("index");
        std::fs::write(root.join("sub/index.html.gz"), b"gzip bytes").expect("sidecar");

        let sidecar = open_gzip_sidecar(&root, Path::new("sub/index.html"))
            .await
            .expect("sidecar opened");
        assert!(sidecar.path_hint.ends_with("sub/index.html.gz"));
    }
}
