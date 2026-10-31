//! Resolved delivery policy for static content: the MIME table and the gzip
//! policy effective for one selected route.
//!
//! nginx resolves `types` / `default_type` / `gzip*` by declaration level:
//! each declared value replaces the inherited one at the level that declares
//! it, and a directive that is not declared at a level keeps inheriting. The
//! materializer stores only what a level declared (`Option` / absent fields),
//! so the data plane resolves the effective policy here:
//!
//! - `types`: route → host → app; `None` keeps the runtime's built-in table.
//! - `default_type`: route → host → app; `None` keeps the runtime's safe
//!   `application/octet-stream` fallback (typed-plane behaviour).
//! - `gzip*`: route → host → app; absent everywhere is the disabled default.
//!
//! The MIME lookup lowercases the request extension (nginx `ngx_strlow` on
//! the extension before its hash lookup). A declared table is consulted in
//! authored order, so a later row that re-declares an extension wins (nginx
//! logs a duplicate-extension warning and keeps the last declaration).

use std::path::Path;

use axum::http::HeaderValue;
use sdkwork_webserver_core::{
    GzipConfig, MimeTypeConfig, RouteConfig, VirtualHostConfig, WebServerAppConfig,
};

/// Built-in MIME fallback used when the resolved table has no entry for the
/// extension. nginx's own `default_type` default is `text/plain`; the runtime
/// keeps `application/octet-stream` as its safe fallback unless a
/// `default_type` is configured, which is the documented typed-plane contract.
const FALLBACK_CONTENT_TYPE: &str = "application/octet-stream";

/// Resolved `types` / `default_type` policy for one selected route.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MimePolicy<'a> {
    pub(crate) types: Option<&'a [MimeTypeConfig]>,
    pub(crate) default_type: Option<&'a str>,
}

impl<'a> MimePolicy<'a> {
    /// Resolve the effective policy: route declaration, else host, else app.
    pub(crate) fn resolve(
        app: &'a WebServerAppConfig,
        host: &'a VirtualHostConfig,
        route: &'a RouteConfig,
    ) -> Self {
        let types = route
            .mime_types
            .as_deref()
            .or(host.mime_types.as_deref())
            .or(app.mime_types.as_deref());
        let default_type = route
            .default_type
            .as_deref()
            .or(host.default_type.as_deref())
            .or(app.default_type.as_deref());
        Self {
            types,
            default_type,
        }
    }

    /// Media type for a filesystem path's extension.
    pub(crate) fn content_type(&self, path: &Path) -> HeaderValue {
        let extension = path
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        self.content_type_for_extension(&extension)
    }

    /// Media type for a request URI path's extension (nginx derives
    /// `return`-body content types from the request URI, not from a file).
    pub(crate) fn content_type_for_uri(&self, uri_path: &str) -> HeaderValue {
        let extension = Path::new(uri_path)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        self.content_type_for_extension(&extension)
    }

    fn content_type_for_extension(&self, extension: &str) -> HeaderValue {
        if let Some(types) = self.types {
            let mut resolved = None;
            if !extension.is_empty() {
                for entry in types {
                    // Authored order: a later duplicate row wins (nginx).
                    if entry
                        .extensions
                        .iter()
                        .any(|candidate| candidate.eq_ignore_ascii_case(extension))
                    {
                        resolved = Some(entry.mime.clone());
                    }
                }
            }
            return header_value(resolved.as_deref().or(self.default_type));
        }
        if !extension.is_empty() {
            if extension == "js" {
                // nginx `mime.types` maps `js` to `application/javascript`;
                // `mime_guess` answers `text/javascript` today.
                return header_value(Some("application/javascript"));
            }
            if let Some(guessed) = mime_guess::from_ext(extension).first_raw() {
                return header_value(Some(guessed));
            }
        }
        header_value(self.default_type)
    }
}

fn header_value(mime: Option<&str>) -> HeaderValue {
    let value = mime.unwrap_or(FALLBACK_CONTENT_TYPE);
    HeaderValue::from_str(value).unwrap_or_else(|_| HeaderValue::from_static(FALLBACK_CONTENT_TYPE))
}

/// Resolved gzip policy for one selected route: route, else host, else app.
pub(crate) fn effective_gzip<'a>(
    app: &'a WebServerAppConfig,
    host: &'a VirtualHostConfig,
    route: &'a RouteConfig,
) -> &'a GzipConfig {
    route
        .compression
        .as_ref()
        .or(host.compression.as_ref())
        .unwrap_or(&app.gzip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn types(rows: &[(&str, &[&str])]) -> Vec<MimeTypeConfig> {
        rows.iter()
            .map(|(mime, extensions)| MimeTypeConfig {
                mime: (*mime).to_owned(),
                extensions: extensions.iter().map(|ext| (*ext).to_owned()).collect(),
            })
            .collect()
    }

    fn media(value: HeaderValue) -> String {
        value.to_str().expect("ascii header").to_owned()
    }

    #[test]
    fn declared_table_replaces_the_builtin_table() {
        let table = types(&[("application/x-custom", &["custom"])]);
        let policy = MimePolicy {
            types: Some(&table),
            default_type: Some("application/x-default"),
        };
        assert_eq!(
            media(policy.content_type(Path::new("a.custom"))),
            "application/x-custom"
        );
        // A built-in extension is unknown to the declared table and falls to
        // `default_type`.
        assert_eq!(
            media(policy.content_type(Path::new("a.html"))),
            "application/x-default"
        );
        assert_eq!(
            media(policy.content_type(Path::new("a.unknown"))),
            "application/x-default"
        );
    }

    #[test]
    fn undeclared_table_keeps_the_builtin_and_safe_fallback() {
        let policy = MimePolicy::default();
        assert_eq!(media(policy.content_type(Path::new("a.html"))), "text/html");
        assert_eq!(
            media(policy.content_type(Path::new("app.js"))),
            "application/javascript"
        );
        assert_eq!(
            media(policy.content_type(Path::new("blob.zzz"))),
            "application/octet-stream"
        );
        let with_default = MimePolicy {
            types: None,
            default_type: Some("text/plain"),
        };
        assert_eq!(
            media(with_default.content_type(Path::new("blob.zzz"))),
            "text/plain"
        );
    }

    #[test]
    fn later_rows_win_and_extensions_are_case_insensitive() {
        let table = types(&[
            ("application/x-first", &["ext"]),
            ("application/x-second", &["EXT"]),
        ]);
        let policy = MimePolicy {
            types: Some(&table),
            default_type: None,
        };
        assert_eq!(
            media(policy.content_type(Path::new("file.ext"))),
            "application/x-second"
        );
        assert_eq!(
            media(policy.content_type(Path::new("FILE.EXT"))),
            "application/x-second"
        );
    }

    #[test]
    fn return_bodies_derive_the_type_from_the_request_uri() {
        let table = types(&[("application/x-txt", &["txt"])]);
        let policy = MimePolicy {
            types: Some(&table),
            default_type: Some("application/x-default"),
        };
        assert_eq!(
            media(policy.content_type_for_uri("/ret.txt")),
            "application/x-txt"
        );
        assert_eq!(
            media(policy.content_type_for_uri("/ret")),
            "application/x-default"
        );
    }
}
