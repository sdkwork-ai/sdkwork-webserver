//! nginx-compatible gzip eligibility for tower-http CompressionLayer.

use std::sync::Arc;

use http::{header, StatusCode};
use http_body::Body;
use sdkwork_webserver_core::config::GzipConfig;
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
        B: Body,
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

#[cfg(test)]
mod tests {
    use super::content_type_matches;

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
}
