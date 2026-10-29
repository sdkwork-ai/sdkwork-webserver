//! nginx-compatible gzip eligibility for tower-http CompressionLayer.

use std::sync::Arc;

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use axum::{body::Body, extract::Request};
use http::{header, HeaderValue, Response, StatusCode};
use http_body::Body as HttpBody;
use sdkwork_webserver_core::config::GzipConfig;
use tower::{Layer, Service};
use tower_http::compression::predicate::{Predicate, SizeAbove};

use super::runtime::DataPlaneRuntime;

/// Per-virtual-host compression policy resolved from server-level nginx
/// `gzip` directives and attached to the response by the request handler;
/// `None` inherits the app-level policy.
#[derive(Clone, Default)]
pub(crate) struct CompressionOverride(pub Option<GzipConfig>);

/// Compress when `[http] gzip = true` and the response MIME matches
/// `gzipTypes` (plus always `text/html`), with `gzipMinLength`.
#[derive(Clone)]
pub(crate) struct NginxGzipPredicate {
    runtime: Arc<DataPlaneRuntime>,
}

impl NginxGzipPredicate {
    pub(crate) fn new(runtime: Arc<DataPlaneRuntime>) -> Self {
        Self { runtime }
    }
}

impl Predicate for NginxGzipPredicate {
    fn should_compress<B>(&self, response: &http::Response<B>) -> bool
    where
        B: HttpBody,
    {
        let generation = self.runtime.current();
        // Server-level nginx `gzip` overrides the app-wide policy when the
        // matched virtual host declares one.
        let override_compression = response
            .extensions()
            .get::<CompressionOverride>()
            .and_then(|override_value| override_value.0.clone());
        let gzip = match &override_compression {
            Some(resolved) => resolved,
            None => &generation.app.config().gzip,
        };
        response_is_compressible(response, gzip)
    }
}

/// The eligibility matrix shared by the compression predicate and by the
/// validator-weakening step: gzip enabled, a compressible status, no existing
/// `Content-Encoding`, a MIME the policy selects, and a body above the minimum.
fn response_is_compressible<B>(response: &http::Response<B>, gzip: &GzipConfig) -> bool
where
    B: HttpBody,
{
    if !gzip.enabled {
        return false;
    }
    if response.status().is_informational()
        || response.status() == StatusCode::NO_CONTENT
        || response.status() == StatusCode::NOT_MODIFIED
        || response.status() == StatusCode::PARTIAL_CONTENT
        || response.status() == StatusCode::SWITCHING_PROTOCOLS
    {
        return false;
    }
    if response.headers().contains_key(header::CONTENT_ENCODING) {
        return false;
    }
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if !content_type_matches(content_type, &gzip.types) {
        return false;
    }
    SizeAbove::new(gzip.min_length).should_compress(response)
}

/// nginx marks a dynamically gzip-compressed representation with a *weak*
/// entity tag: the tag no longer validates the bytes on the wire, only the
/// decoded representation, and a strong tag would let a `Range`/`If-Range`
/// client treat the compressed bytes as the tagged representation. The
/// compression layer (correctly) never rewrites validators, so the handler
/// applies the same weakening up front when the response is about to be
/// negotiated to gzip.
pub(crate) fn weaken_etag_for_negotiated_gzip(
    mut response: Response<Body>,
    app_gzip: &GzipConfig,
    override_gzip: Option<&GzipConfig>,
    accept_encoding: Option<&HeaderValue>,
) -> Response<Body> {
    let gzip = override_gzip.unwrap_or(app_gzip);
    if !response_is_compressible(&response, gzip) || !accepts_gzip(accept_encoding) {
        return response;
    }
    let Some(etag) = response.headers().get(header::ETAG).cloned() else {
        return response;
    };
    let Ok(etag) = etag.to_str() else {
        return response;
    };
    if etag.starts_with("W/") {
        return response;
    }
    if let Ok(weak) = HeaderValue::from_str(&format!("W/{etag}")) {
        response.headers_mut().insert(header::ETAG, weak);
    }
    response
}

/// Whether the request's `Accept-Encoding` selects gzip. Mirrors the
/// tower-http negotiation this layer runs: an absent header means identity, an
/// explicit `q=0` rejects the coding, and `*` covers gzip when no explicit
/// token does.
pub(crate) fn accepts_gzip(header: Option<&HeaderValue>) -> bool {
    let Some(header) = header else {
        return false;
    };
    let Ok(value) = header.to_str() else {
        return false;
    };
    let mut wildcard = None;
    for entry in value.split(',') {
        let mut pieces = entry.trim().split(';');
        let coding = pieces.next().unwrap_or("").trim();
        let mut quality = 1.0_f32;
        for parameter in pieces {
            if let Some(value) = parameter.trim().strip_prefix("q=") {
                quality = value.trim().parse::<f32>().unwrap_or(0.0);
            }
        }
        if coding.eq_ignore_ascii_case("gzip") {
            return quality > 0.0;
        }
        if coding == "*" {
            wildcard = Some(quality);
        }
    }
    wildcard.is_some_and(|quality| quality > 0.0)
}

fn content_type_matches(content_type: &str, configured: &[String]) -> bool {
    let media = content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim()
        .to_ascii_lowercase();
    if media.is_empty() {
        return false;
    }
    // nginx always compresses text/html when gzip is on.
    if media == "text/html" {
        return true;
    }
    configured.iter().any(|entry| {
        let expected = entry
            .split(';')
            .next()
            .unwrap_or(entry)
            .trim()
            .to_ascii_lowercase();
        !expected.is_empty() && media == expected
    })
}

/// Marks a response whose effective nginx gzip policy has `gzip_vary off`, so
/// the tower-http compression layer's automatic `Vary: Accept-Encoding` must be
/// removed. nginx only declares the negotiation field when `gzip_vary on`; the
/// compression layer has no per-route configuration and always declares it when
/// it selects a representation, so the handler states the exception.
#[derive(Clone, Copy, Default)]
pub(crate) struct SuppressGzipVary;

/// Outermost response layer: honour [`SuppressGzipVary`] by removing the
/// `Accept-Encoding` token from every `Vary` field (other selectors such as the
/// adaptive-surface `User-Agent` are preserved).
///
/// A tower layer (rather than `axum::middleware::from_fn`) is required because
/// it sits *outside* `CompressionLayer`, whose response body type is
/// `CompressionBody<Body>`; the layer is therefore generic over the body type.
#[derive(Clone, Copy, Default)]
pub(crate) struct SuppressGzipVaryLayer;

impl<S> Layer<S> for SuppressGzipVaryLayer {
    type Service = SuppressGzipVaryService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SuppressGzipVaryService { inner }
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct SuppressGzipVaryService<S> {
    inner: S,
}

impl<S, ResponseBody> Service<Request> for SuppressGzipVaryService<S>
where
    S: Service<Request, Response = Response<ResponseBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    ResponseBody: HttpBody<Data = bytes::Bytes> + Send + 'static,
    ResponseBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(context)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let response = inner.call(request).await?;
            // Wrap the response body back into `axum::body::Body` so the body
            // type stays compatible with the router and every inner layer.
            let (parts, body) = response.into_parts();
            let mut response = Response::from_parts(parts, Body::new(body));
            if response.extensions().get::<SuppressGzipVary>().is_some() {
                strip_accept_encoding_vary(&mut response);
            }
            Ok(response)
        })
    }
}

fn strip_accept_encoding_vary<ResponseBody>(response: &mut Response<ResponseBody>) {
    let headers = response.headers_mut();
    let values = headers
        .get_all(header::VARY)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .filter(|token| !token.eq_ignore_ascii_case("accept-encoding"))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    headers.remove(header::VARY);
    if !values.is_empty() {
        if let Ok(value) = HeaderValue::from_str(&values.join(", ")) {
            headers.insert(header::VARY, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use http::{header, HeaderValue, Response, StatusCode};
    use sdkwork_webserver_core::config::GzipConfig;

    use super::{accepts_gzip, content_type_matches, weaken_etag_for_negotiated_gzip};

    #[test]
    fn text_html_always_matches() {
        assert!(content_type_matches(
            "text/html; charset=utf-8",
            &["text/css".to_owned()]
        ));
    }

    #[test]
    fn configured_types_match_exact_media() {
        assert!(content_type_matches(
            "application/json; charset=utf-8",
            &["application/json".to_owned()]
        ));
        assert!(!content_type_matches(
            "image/png",
            &["image/svg+xml".to_owned()]
        ));
    }

    #[test]
    fn accept_encoding_negotiates_gzip_like_nginx() {
        assert!(!accepts_gzip(None));
        assert!(accepts_gzip(Some(&HeaderValue::from_static("gzip"))));
        assert!(accepts_gzip(Some(&HeaderValue::from_static(
            "identity, gzip"
        ))));
        assert!(accepts_gzip(Some(&HeaderValue::from_static("gzip;q=0.5"))));
        assert!(!accepts_gzip(Some(&HeaderValue::from_static("gzip;q=0"))));
        assert!(!accepts_gzip(Some(&HeaderValue::from_static("identity"))));
        assert!(accepts_gzip(Some(&HeaderValue::from_static("*"))));
        assert!(!accepts_gzip(Some(&HeaderValue::from_static("*;q=0"))));
    }

    fn gzip_config() -> GzipConfig {
        GzipConfig {
            enabled: true,
            types: vec!["text/plain".to_owned()],
            min_length: 0,
            vary: true,
            static_files: sdkwork_webserver_core::GzipStaticMode::Off,
        }
    }

    fn response_with_etag() -> Response<Body> {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/plain")
            .header(header::CONTENT_LENGTH, "18")
            .header(header::ETAG, "\"5e0be100-12\"")
            .body(Body::from("hello static body\n"))
            .unwrap()
    }

    #[test]
    fn negotiated_gzip_weakens_the_entity_tag_like_nginx() {
        let compressed = weaken_etag_for_negotiated_gzip(
            response_with_etag(),
            &gzip_config(),
            None,
            Some(&HeaderValue::from_static("gzip")),
        );
        assert_eq!(compressed.headers()[header::ETAG], "W/\"5e0be100-12\"");
    }

    #[test]
    fn identity_negotiation_keeps_the_strong_entity_tag() {
        for accept_encoding in [
            None,
            Some(HeaderValue::from_static("identity")),
            Some(HeaderValue::from_static("gzip;q=0")),
        ] {
            let response = weaken_etag_for_negotiated_gzip(
                response_with_etag(),
                &gzip_config(),
                None,
                accept_encoding.as_ref(),
            );
            assert_eq!(response.headers()[header::ETAG], "\"5e0be100-12\"");
        }
    }

    #[test]
    fn virtual_host_gzip_policy_decides_the_validator() {
        let disabled = GzipConfig {
            enabled: false,
            types: Vec::new(),
            min_length: 0,
            vary: true,
            static_files: sdkwork_webserver_core::GzipStaticMode::Off,
        };
        let response = weaken_etag_for_negotiated_gzip(
            response_with_etag(),
            &gzip_config(),
            Some(&disabled),
            Some(&HeaderValue::from_static("gzip")),
        );
        assert_eq!(response.headers()[header::ETAG], "\"5e0be100-12\"");
    }
}
