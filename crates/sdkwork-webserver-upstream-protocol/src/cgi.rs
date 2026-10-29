//! CGI variable construction shared by the FastCGI, SCGI and uWSGI encoders.
//!
//! nginx's `fastcgi_params` / `uwsgi_params` / `scgi_params` files and the
//! automatic `HTTP_*` request-header mapping are the reference here: the
//! dedicated CGI variables are emitted first in a stable order, then every
//! remaining request header becomes `HTTP_<UPPER_NAME>` with `-` replaced by
//! `_`. nginx skips the headers that already have a dedicated variable
//! (`Host`, `Content-Type`, `Content-Length`) and the hop-by-hop fields it owns,
//! and joins repeated fields with `", "` (`"; "` for `Cookie`).
//!
//! The output is an ordered `Vec<(String, String)>`, so a caller can hand it to
//! any of the three protocol encoders without re-deriving the variable set.

use crate::{
    error::{Error, Result},
    header_block::is_token,
    limits::Limits,
};

/// `GATEWAY_INTERFACE` reported to the application, matching the CGI/1.1 value
/// nginx's default parameter files use.
pub const GATEWAY_INTERFACE: &str = "CGI/1.1";
/// `SERVER_SOFTWARE` reported to the application.
pub const SERVER_SOFTWARE: &str = "sdkwork-webserver";

/// Request facts an upstream protocol encoder turns into CGI variables.
///
/// All fields are borrowed so the edge can build the facts from its own request
/// representation without copying. `headers` must already exclude fields the
/// edge does not forward (for example hop-by-hop fields named by `Connection`);
/// the encoder additionally drops the headers that have dedicated CGI variables.
#[derive(Debug, Clone, Copy)]
pub struct UpstreamRequestFacts<'a> {
    /// Request method (`GET`, `POST`, …).
    pub method: &'a str,
    /// Request scheme (`http` or `https`).
    pub scheme: &'a str,
    /// Request URI path, beginning with `/` (the query string is separate).
    pub uri: &'a str,
    /// Query string without the leading `?`.
    pub query_string: Option<&'a str>,
    /// Request protocol version (`HTTP/1.1`, `HTTP/2`, …).
    pub server_protocol: &'a str,
    /// Host name without the port.
    pub server_name: &'a str,
    /// Listener port the request arrived on.
    pub server_port: u16,
    /// Client address as text (IPv4 or IPv6).
    pub remote_addr: &'a str,
    /// Static document root for the resource (may be empty when the upstream
    /// derives it).
    pub document_root: &'a str,
    /// Script filename handed to the application (the static file for plain
    /// FastCGI/SCGI, or the `.php` script for PHP-FPM).
    pub script_filename: &'a str,
    /// Request content type, when the request declares one.
    pub content_type: Option<&'a str>,
    /// Request content length, when the request declares one.
    pub content_length: Option<u64>,
    /// Request header fields, in arrival order; repeated names are allowed.
    pub headers: &'a [(String, String)],
}

/// Header names that already have a dedicated CGI variable and are therefore
/// not repeated as `HTTP_*`, plus hop-by-hop fields the edge never forwards.
const REDIRECTED_HEADERS: &[&str] = &[
    "host",
    "content-type",
    "content-length",
    "connection",
    "keep-alive",
    "transfer-encoding",
    "te",
    "trailer",
    "upgrade",
    "proxy-connection",
    "expect",
];

/// Build the ordered CGI variable set for `facts`.
///
/// The dedicated variables always appear in the same order; `HTTP_*` variables
/// follow in first-arrival header order. Repeated request headers are joined
/// with `", "` (`"; "` for `Cookie`), matching nginx.
///
/// # Errors
///
/// - [`Error::Protocol`] when a fact contains a control character or the
///   request facts are structurally invalid (empty method, relative URI,
///   unknown scheme, invalid header name).
/// - [`Error::LimitExceeded`] when the encoded variable count or byte size
///   exceeds the configured bounds.
pub fn cgi_variables(
    facts: &UpstreamRequestFacts<'_>,
    limits: &Limits,
) -> Result<Vec<(String, String)>> {
    validate_facts(facts)?;
    let limits = limits.sanitized();
    let uri_path = facts.uri;
    let request_uri = match facts.query_string {
        Some(query) if !query.is_empty() => format!("{uri_path}?{query}"),
        _ => uri_path.to_owned(),
    };

    let mut variables: Vec<(String, String)> = Vec::new();
    // Order mirrors nginx's default parameter files closely enough that an
    // application cannot observe a difference in variable presence.
    push(&mut variables, "SCRIPT_FILENAME", facts.script_filename);
    push(&mut variables, "SCRIPT_NAME", uri_path);
    push(&mut variables, "REQUEST_URI", &request_uri);
    push(&mut variables, "DOCUMENT_URI", uri_path);
    push(
        &mut variables,
        "QUERY_STRING",
        facts.query_string.unwrap_or(""),
    );
    push(&mut variables, "REQUEST_METHOD", facts.method);
    push(
        &mut variables,
        "CONTENT_TYPE",
        facts.content_type.unwrap_or(""),
    );
    push(
        &mut variables,
        "CONTENT_LENGTH",
        &facts
            .content_length
            .map_or_else(String::new, |length| length.to_string()),
    );
    push(&mut variables, "DOCUMENT_ROOT", facts.document_root);
    push(&mut variables, "SERVER_PROTOCOL", facts.server_protocol);
    push(&mut variables, "REQUEST_SCHEME", facts.scheme);
    if facts.scheme.eq_ignore_ascii_case("https") {
        push(&mut variables, "HTTPS", "on");
    }
    push(&mut variables, "GATEWAY_INTERFACE", GATEWAY_INTERFACE);
    push(&mut variables, "SERVER_SOFTWARE", SERVER_SOFTWARE);
    push(&mut variables, "SERVER_NAME", facts.server_name);
    push(
        &mut variables,
        "SERVER_PORT",
        &facts.server_port.to_string(),
    );
    push(&mut variables, "REMOTE_ADDR", facts.remote_addr);

    for (name, value) in merged_http_headers(facts.headers)? {
        variables.push((name, value));
    }

    enforce_bounds(&variables, &limits)?;
    Ok(variables)
}

fn validate_facts(facts: &UpstreamRequestFacts<'_>) -> Result<()> {
    const PROTOCOL: &str = "cgi";
    if facts.method.is_empty() || !is_token(facts.method) {
        return Err(Error::protocol(
            PROTOCOL,
            "request method must be a non-empty HTTP token",
        ));
    }
    if !facts.uri.starts_with('/') {
        return Err(Error::protocol(
            PROTOCOL,
            "request URI path must begin with `/`",
        ));
    }
    if !facts.scheme.eq_ignore_ascii_case("http") && !facts.scheme.eq_ignore_ascii_case("https") {
        return Err(Error::protocol(
            PROTOCOL,
            "request scheme must be `http` or `https`",
        ));
    }
    for (label, value) in [
        ("request URI", facts.uri),
        ("query string", facts.query_string.unwrap_or("")),
        ("server protocol", facts.server_protocol),
        ("server name", facts.server_name),
        ("remote address", facts.remote_addr),
        ("document root", facts.document_root),
        ("script filename", facts.script_filename),
        ("content type", facts.content_type.unwrap_or("")),
    ] {
        if value
            .bytes()
            .any(|byte| byte == 0 || byte < 0x20 || byte == 0x7f)
        {
            return Err(Error::protocol(
                PROTOCOL,
                format!("{label} contains a control character"),
            ));
        }
    }
    if facts.server_protocol.is_empty() {
        return Err(Error::protocol(
            PROTOCOL,
            "server protocol must not be empty",
        ));
    }
    if facts.server_name.is_empty() {
        return Err(Error::protocol(PROTOCOL, "server name must not be empty"));
    }
    Ok(())
}

/// Merge repeated request header fields into `HTTP_*` variables.
///
/// The first occurrence keeps its position; later occurrences are appended to
/// the same variable with `", "` (nginx uses `"; "` for `Cookie`). Header names
/// that have a dedicated CGI variable are dropped, as are hop-by-hop fields.
fn merged_http_headers(headers: &[(String, String)]) -> Result<Vec<(String, String)>> {
    const PROTOCOL: &str = "cgi";
    let mut merged: Vec<(String, String, bool)> = Vec::new(); // name, value, is_cookie
    for (name, value) in headers {
        if !is_token(name) {
            return Err(Error::protocol(
                PROTOCOL,
                "request header name is not a valid token",
            ));
        }
        if value
            .bytes()
            .any(|byte| byte == 0 || byte == b'\r' || byte == b'\n')
        {
            return Err(Error::protocol(
                PROTOCOL,
                "request header value contains a control character",
            ));
        }
        if REDIRECTED_HEADERS
            .iter()
            .any(|candidate| name.eq_ignore_ascii_case(candidate))
        {
            continue;
        }
        let variable = format!("HTTP_{}", name.to_ascii_uppercase().replace('-', "_"));
        let is_cookie = name.eq_ignore_ascii_case("cookie");
        if let Some(existing) = merged.iter_mut().find(|(key, _, _)| key == &variable) {
            existing.1.push_str(if is_cookie { "; " } else { ", " });
            existing.1.push_str(value);
        } else {
            merged.push((variable, value.clone(), is_cookie));
        }
    }
    Ok(merged
        .into_iter()
        .map(|(name, value, _)| (name, value))
        .collect())
}

fn push(variables: &mut Vec<(String, String)>, name: &str, value: &str) {
    variables.push((name.to_owned(), value.to_owned()));
}

/// Enforce the caller's count/byte bounds over an encoded variable set.
pub(crate) fn enforce_bounds(variables: &[(String, String)], limits: &Limits) -> Result<()> {
    const PROTOCOL: &str = "cgi";
    if variables.len() > limits.max_params {
        return Err(Error::limit(
            PROTOCOL,
            "CGI variable count",
            variables.len(),
            limits.max_params,
        ));
    }
    let mut bytes = 0usize;
    for (name, value) in variables {
        bytes = bytes
            .checked_add(name.len())
            .and_then(|total| total.checked_add(value.len()))
            .and_then(|total| total.checked_add(8))
            .ok_or_else(|| {
                Error::limit(
                    PROTOCOL,
                    "CGI variable bytes",
                    usize::MAX,
                    limits.max_params_bytes,
                )
            })?;
        if bytes > limits.max_params_bytes {
            return Err(Error::limit(
                PROTOCOL,
                "CGI variable bytes",
                bytes,
                limits.max_params_bytes,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts<'a>(headers: &'a [(String, String)]) -> UpstreamRequestFacts<'a> {
        UpstreamRequestFacts {
            method: "POST",
            scheme: "https",
            uri: "/index.php",
            query_string: Some("a=1&b=2"),
            server_protocol: "HTTP/1.1",
            server_name: "example.com",
            server_port: 443,
            remote_addr: "203.0.113.7",
            document_root: "/srv/www",
            script_filename: "/srv/www/index.php",
            content_type: Some("application/x-www-form-urlencoded"),
            content_length: Some(42),
            headers,
        }
    }

    fn header(name: &str, value: &str) -> (String, String) {
        (name.to_owned(), value.to_owned())
    }

    #[test]
    fn dedicated_variables_are_emitted_in_stable_order() {
        let headers = [header("Accept", "*/*")];
        let variables = cgi_variables(&facts(&headers), &Limits::default()).expect("variables");
        let names: Vec<&str> = variables.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            &names[..4],
            [
                "SCRIPT_FILENAME",
                "SCRIPT_NAME",
                "REQUEST_URI",
                "DOCUMENT_URI"
            ]
        );
        let lookup = |name: &str| {
            variables
                .iter()
                .find(|(candidate, _)| candidate == name)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(lookup("REQUEST_METHOD"), Some("POST"));
        assert_eq!(lookup("REQUEST_URI"), Some("/index.php?a=1&b=2"));
        assert_eq!(lookup("QUERY_STRING"), Some("a=1&b=2"));
        assert_eq!(lookup("CONTENT_LENGTH"), Some("42"));
        assert_eq!(lookup("HTTPS"), Some("on"));
        assert_eq!(lookup("SERVER_PORT"), Some("443"));
        assert_eq!(lookup("GATEWAY_INTERFACE"), Some("CGI/1.1"));
        assert_eq!(lookup("HTTP_ACCEPT"), Some("*/*"));
    }

    #[test]
    fn dedicated_header_names_are_not_repeated_as_http_variables() {
        let headers = [
            header("Host", "example.com"),
            header("Content-Type", "text/plain"),
            header("Content-Length", "42"),
            header("Connection", "keep-alive"),
            header("X-Forwarded-For", "203.0.113.1"),
        ];
        let variables = cgi_variables(&facts(&headers), &Limits::default()).expect("variables");
        assert!(variables.iter().all(|(name, _)| name != "HTTP_HOST"));
        assert!(variables
            .iter()
            .all(|(name, _)| name != "HTTP_CONTENT_TYPE"));
        assert!(variables.iter().all(|(name, _)| name != "HTTP_CONNECTION"));
        assert!(variables
            .iter()
            .any(|(name, value)| name == "HTTP_X_FORWARDED_FOR" && value == "203.0.113.1"));
    }

    #[test]
    fn repeated_headers_are_joined_like_nginx() {
        let headers = [
            header("X-Tag", "one"),
            header("Cookie", "a=1"),
            header("X-Tag", "two"),
            header("Cookie", "b=2"),
        ];
        let variables = cgi_variables(&facts(&headers), &Limits::default()).expect("variables");
        let lookup = |name: &str| {
            variables
                .iter()
                .find(|(candidate, _)| candidate == name)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(lookup("HTTP_X_TAG"), Some("one, two"));
        assert_eq!(lookup("HTTP_COOKIE"), Some("a=1; b=2"));
    }

    #[test]
    fn plain_http_omits_https_and_empty_content_variables() {
        let headers: [(String, String); 0] = [];
        let facts = UpstreamRequestFacts {
            method: "GET",
            scheme: "http",
            content_type: None,
            content_length: None,
            ..facts(&headers)
        };
        let variables = cgi_variables(&facts, &Limits::default()).expect("variables");
        assert!(variables.iter().all(|(name, _)| name != "HTTPS"));
        assert_eq!(
            variables
                .iter()
                .find(|(name, _)| name == "CONTENT_LENGTH")
                .map(|(_, value)| value.as_str()),
            Some("")
        );
    }

    #[test]
    fn malformed_facts_fail_closed() {
        let headers = [header("Bad Name", "x")];
        let mut broken = facts(&headers);
        assert!(cgi_variables(&broken, &Limits::default()).is_err());
        broken = facts(&[]);
        broken.uri = "relative";
        assert!(cgi_variables(&broken, &Limits::default()).is_err());
        broken = facts(&[]);
        broken.scheme = "ftp";
        assert!(cgi_variables(&broken, &Limits::default()).is_err());
        broken = facts(&[]);
        broken.method = "";
        assert!(cgi_variables(&broken, &Limits::default()).is_err());
        broken = facts(&[]);
        broken.remote_addr = "1.2.3.4\r\nX-Injected: 1";
        assert!(cgi_variables(&broken, &Limits::default()).is_err());
    }

    #[test]
    fn variable_bounds_are_enforced() {
        let headers = [
            header("X-A", &"a".repeat(64)),
            header("X-B", &"b".repeat(64)),
        ];
        let limits = Limits {
            max_params_bytes: 200,
            ..Limits::default()
        };
        let error = cgi_variables(&facts(&headers), &limits).expect_err("too many bytes");
        assert!(matches!(error, Error::LimitExceeded { .. }));

        let limits = Limits {
            max_params: 8,
            ..Limits::default()
        };
        let error = cgi_variables(&facts(&[]), &limits).expect_err("too many variables");
        assert!(matches!(error, Error::LimitExceeded { .. }));
    }
}
