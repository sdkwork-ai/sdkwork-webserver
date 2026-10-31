//! nginx `charset` family header filter (`charset`, `charset_types`,
//! `source_charset`, `override_charset`).
//!
//! Every header-level semantic here was differential-probed against nginx
//! 1.29.6 (see `tests/nginx/differential-http`):
//!
//! - `charset <name>` appends `; charset=<name>` to `Content-Type` when the
//!   response media type is in the effective `charset_types` list (default:
//!   `text/html`, `text/xml`, `text/plain`, `text/vnd.wap.wml`,
//!   `application/javascript`, `application/rss+xml`);
//! - a declared `charset_types` list is seeded with `text/html` first (nginx's
//!   `ngx_http_types_slot` starts a declared array from the first default
//!   type), so `charset_types text/css` applies to `text/html` AND `text/css`;
//!   `*` matches every media type;
//! - a response that already carries a `charset=...` parameter keeps it unless
//!   `override_charset on`; with `on`, the configured charset replaces it only
//!   when the response charset and the configured one are the same
//!   (case-insensitively, which also normalizes the spelling). When they
//!   differ nginx needs a `charset_map` to transcode; byte transcoding is out
//!   of scope, so the filter is accepted-unenforced for that pair and leaves
//!   the header untouched rather than claiming an encoding the bytes do not
//!   have;
//! - nginx never sets the field on a 301/302 response
//!   (`ngx_http_set_charset`), so redirects keep their `Content-Type`;
//! - a charset only applies when a `Content-Type` header is present.
//!
//! The media type matched against `charset_types` is nginx's
//! `content_type_len` prefix: for a static/`return` representation it is the
//! whole `Content-Type`, for an upstream response it is the portion before the
//! `; charset=` parameter (which keeps any preceding parameters, exactly like
//! `ngx_http_upstream_copy_content_type`).

use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, HeaderValue, Response, StatusCode},
};
use sdkwork_webserver_core::CharsetConfig;

/// nginx `ngx_http_charset_default_types`.
pub(crate) const NGINX_DEFAULT_CHARSET_TYPES: &[&str] = &[
    "text/html",
    "text/xml",
    "text/plain",
    "text/vnd.wap.wml",
    "application/javascript",
    "application/rss+xml",
];

/// The parsed shape of a `Content-Type` value for the charset filter.
struct ParsedContentType<'a> {
    /// nginx `content_type_len` prefix: the media type (plus any parameters
    /// that precede the `charset=` parameter).
    base: &'a str,
    /// The charset parameter value, when the response already carries one.
    existing_charset: Option<&'a str>,
}

/// Split a `Content-Type` value the way nginx does for the charset filter.
///
/// `base` is the `content_type_len` prefix: everything before the `;` that
/// starts the `charset=` parameter (so parameters declared before it are kept),
/// or the whole value when there is no charset parameter.
fn parse_content_type(value: &str) -> ParsedContentType<'_> {
    let bytes = value.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b';' {
            index += 1;
            continue;
        }
        let semicolon = index;
        let mut parameter_start = index + 1;
        while parameter_start < bytes.len() && bytes[parameter_start] == b' ' {
            parameter_start += 1;
        }
        if parameter_start >= bytes.len() {
            break;
        }
        if bytes.len() >= parameter_start + 8
            && bytes[parameter_start..parameter_start + 8].eq_ignore_ascii_case(b"charset=")
        {
            let mut raw = &value[parameter_start + 8..];
            if let Some(stripped) = raw.strip_prefix('"') {
                raw = stripped;
            }
            if let Some(stripped) = raw.strip_suffix('"') {
                raw = stripped;
            }
            return ParsedContentType {
                base: &value[..semicolon],
                existing_charset: Some(raw),
            };
        }
        index = parameter_start;
        while index < bytes.len() && bytes[index] != b';' {
            index += 1;
        }
    }
    ParsedContentType {
        base: value,
        existing_charset: None,
    }
}

fn content_type_matches(media_type: &str, types: Option<&[String]>) -> bool {
    let Some(types) = types else {
        return NGINX_DEFAULT_CHARSET_TYPES
            .iter()
            .any(|candidate| media_type.eq_ignore_ascii_case(candidate));
    };
    if types.iter().any(|candidate| candidate == "*") {
        return true;
    }
    types
        .iter()
        .any(|candidate| media_type.eq_ignore_ascii_case(candidate.as_str()))
}

/// Apply the effective charset policy to a response.
///
/// nginx's charset header filter runs after every content handler, so this is
/// applied at the response header choke point, exactly like `expires` /
/// `add_header`.
pub(crate) fn apply_charset_policy(
    mut response: Response<Body>,
    config: &CharsetConfig,
) -> Response<Body> {
    let Some(charset) = config.charset.as_deref() else {
        return response;
    };
    // nginx `ngx_http_set_charset`: never touch a 301/302 redirect (NN 4.x
    // would apply the target page's charset to the redirect itself).
    if matches!(
        response.status(),
        StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND
    ) {
        return response;
    }
    let Some(content_type) = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
    else {
        return response;
    };
    let parsed = parse_content_type(&content_type);
    let base = parsed.base.to_owned();
    let existing_charset = parsed.existing_charset.map(str::to_owned);

    if existing_charset.is_some() {
        if !config.override_charset {
            return response;
        }
    } else if !content_type_matches(&base, config.types.as_deref()) {
        return response;
    }

    let source = existing_charset
        .as_deref()
        .or(config.source_charset.as_deref());
    match source {
        None => {}
        Some(source) if source.eq_ignore_ascii_case(charset) => {}
        Some(_) => {
            // A differing source charset needs `charset_map` transcoding, which
            // is out of scope: leave the response untouched (accepted-
            // unenforced, documented on `CharsetConfig`).
            return response;
        }
    }

    let mut value = String::with_capacity(base.len() + charset.len() + 10);
    value.push_str(&base);
    value.push_str("; charset=");
    value.push_str(charset);
    if let Ok(value) = HeaderValue::from_str(&value) {
        response.headers_mut().insert(CONTENT_TYPE, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{header::CONTENT_TYPE, HeaderValue, Response, StatusCode};
    use sdkwork_webserver_core::CharsetConfig;
    use serde_json::json;

    use super::apply_charset_policy;

    fn config(value: serde_json::Value) -> CharsetConfig {
        serde_json::from_value(value).expect("charset config")
    }

    fn response(status: u16, content_type: Option<&str>) -> Response<Body> {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::from_u16(status).expect("status");
        if let Some(content_type) = content_type {
            response.headers_mut().insert(
                CONTENT_TYPE,
                HeaderValue::from_str(content_type).expect("value"),
            );
        }
        response
    }

    fn applied(response: Response<Body>, config: &CharsetConfig) -> Option<String> {
        apply_charset_policy(response, config)
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    }

    #[test]
    fn appends_the_configured_charset_to_default_types() {
        let config = config(json!({ "charset": "utf-8" }));
        assert_eq!(
            applied(response(200, Some("text/html")), &config).as_deref(),
            Some("text/html; charset=utf-8")
        );
        assert_eq!(
            applied(response(200, Some("application/javascript")), &config).as_deref(),
            Some("application/javascript; charset=utf-8")
        );
        // Media types outside the default list are untouched.
        assert_eq!(
            applied(response(200, Some("application/json")), &config).as_deref(),
            Some("application/json")
        );
        assert_eq!(
            applied(response(200, Some("image/png")), &config).as_deref(),
            Some("image/png")
        );
    }

    #[test]
    fn off_and_absent_charsets_do_nothing() {
        let off = config(json!({ "charset": null }));
        assert_eq!(
            applied(response(200, Some("text/html")), &off).as_deref(),
            Some("text/html")
        );
        // A response without Content-Type is untouched even when configured.
        let config = config(json!({ "charset": "utf-8" }));
        assert!(applied(response(200, None), &config).is_none());
    }

    #[test]
    fn declared_type_list_is_seeded_with_text_html() {
        // nginx seeds a declared `charset_types` list with `text/html`, so
        // `charset_types text/css` applies to both media types
        // (differential-confirmed against nginx 1.29.6).
        let config = config(json!({ "charset": "utf-8", "types": ["text/html", "text/css"] }));
        assert_eq!(
            applied(response(200, Some("text/html")), &config).as_deref(),
            Some("text/html; charset=utf-8")
        );
        assert_eq!(
            applied(response(200, Some("text/css")), &config).as_deref(),
            Some("text/css; charset=utf-8")
        );
        assert_eq!(
            applied(response(200, Some("text/plain")), &config).as_deref(),
            Some("text/plain")
        );
    }

    #[test]
    fn wildcard_type_list_matches_everything() {
        let config = config(json!({ "charset": "utf-8", "types": ["*"] }));
        assert_eq!(
            applied(response(200, Some("image/png")), &config).as_deref(),
            Some("image/png; charset=utf-8")
        );
    }

    #[test]
    fn redirects_never_get_a_charset() {
        let config = config(json!({ "charset": "utf-8" }));
        for status in [301, 302] {
            assert_eq!(
                applied(response(status, Some("text/html")), &config).as_deref(),
                Some("text/html")
            );
        }
        // Other redirect statuses are not skipped by nginx.
        assert_eq!(
            applied(response(307, Some("text/html")), &config).as_deref(),
            Some("text/html; charset=utf-8")
        );
    }

    #[test]
    fn upstream_charset_is_kept_unless_overridden() {
        let keep = config(json!({ "charset": "utf-8" }));
        assert_eq!(
            applied(response(200, Some("text/html; charset=iso-8859-1")), &keep).as_deref(),
            Some("text/html; charset=iso-8859-1")
        );
        let override_on = config(json!({ "charset": "utf-8", "overrideCharset": true }));
        // Same charset (case-insensitively): nginx normalizes the spelling.
        assert_eq!(
            applied(
                response(200, Some("text/html; charset=UTF-8")),
                &override_on
            )
            .as_deref(),
            Some("text/html; charset=utf-8")
        );
        // Differing charset needs a charset_map; accepted-unenforced.
        assert_eq!(
            applied(
                response(200, Some("text/html; charset=iso-8859-1")),
                &override_on
            )
            .as_deref(),
            Some("text/html; charset=iso-8859-1")
        );
        // Parameters before the charset parameter survive the normalization.
        assert_eq!(
            applied(
                response(200, Some("text/html; boundary=xyz; charset=UTF-8")),
                &override_on
            )
            .as_deref(),
            Some("text/html; boundary=xyz; charset=utf-8")
        );
    }

    #[test]
    fn configured_source_charset_that_matches_is_applied() {
        let source_config = config(json!({ "charset": "utf-8", "sourceCharset": "utf-8" }));
        assert_eq!(
            applied(response(200, Some("text/html")), &source_config).as_deref(),
            Some("text/html; charset=utf-8")
        );
        // A differing configured source charset needs transcoding; leave it.
        let transcoding = config(json!({ "charset": "utf-8", "sourceCharset": "windows-1251" }));
        assert_eq!(
            applied(response(200, Some("text/html")), &transcoding).as_deref(),
            Some("text/html")
        );
    }

    #[test]
    fn content_type_with_parameters_is_not_matched_like_nginx() {
        // nginx's media-type hash uses `content_type_len`, which for a static
        // response is the full header; a parameterized `Content-Type` therefore
        // does not match the type list.
        let config = config(json!({ "charset": "utf-8" }));
        assert_eq!(
            applied(response(200, Some("text/html; level=1")), &config).as_deref(),
            Some("text/html; level=1")
        );
    }
}
