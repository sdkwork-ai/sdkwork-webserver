//! Request-time variable state for the bounded nginx rewrite/variable engine.
//!
//! The rewrite phase (`set`, `if`, `rewrite`, rewrite-phase `return`), dynamic
//! `proxy_pass` targets, `proxy_set_header` values, and the request-time
//! document root all resolve the same variable vocabulary:
//!
//! - request facts (`$host`, `$uri`, `$request_uri`, `$remote_addr`, …);
//! - request headers (`$http_<name>`), query arguments (`$arg_<name>`), and
//!   cookies (`$cookie_<name>`);
//! - declared `map` results;
//! - variables assigned by `set` (in execution order);
//! - regex captures `$1`..`$9` from the most recent location / `if` / `rewrite`
//!   match (cleared by a new match, like nginx).
//!
//! Every reference was validated at materialization; at request time an
//! unresolved variable expands to the empty string, exactly like nginx's
//! "uninitialised variable is empty" rule.

use std::collections::HashMap;

use axum::http::{HeaderMap, Method};

/// Mutable per-request variable state.
#[derive(Debug, Clone)]
pub(crate) struct RequestVariables {
    /// The `Host` authority as received (without a userinfo part).
    pub authority: String,
    /// Normalized request path (no query string).
    pub path: String,
    /// Raw request URI (`$request_uri`).
    pub request_uri: String,
    pub scheme: String,
    pub method: String,
    pub client_ip: String,
    pub listener_port: u16,
    pub server_name: String,
    pub server_protocol: String,
    pub query: Option<String>,
    headers: HeaderMap,
    /// Values assigned by `set`, in assignment order.
    assigned: HashMap<String, String>,
    /// `$1`..`$9`; index 0 is the whole match and is not exposed by nginx.
    captures: Vec<Option<String>>,
    /// True once a rewrite phase executed (`set`, captures from a location/if
    /// match), i.e. the request carries variables the legacy proxy-target
    /// expander does not know.
    rewrite_touched: bool,
}

impl RequestVariables {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        headers: &HeaderMap,
        method: &Method,
        scheme: &str,
        authority: &str,
        client_ip: std::net::IpAddr,
        listener_port: u16,
        path: &str,
        request_uri: &str,
        query: Option<&str>,
        server_name: &str,
        server_protocol: &str,
    ) -> Self {
        Self {
            authority: authority.to_owned(),
            path: path.to_owned(),
            request_uri: request_uri.to_owned(),
            scheme: scheme.to_owned(),
            method: method.as_str().to_owned(),
            client_ip: client_ip.to_string(),
            listener_port,
            server_name: server_name.to_owned(),
            server_protocol: server_protocol.to_owned(),
            query: query.map(str::to_owned),
            headers: headers.clone(),
            assigned: HashMap::new(),
            captures: Vec::new(),
            rewrite_touched: false,
        }
    }

    /// Update the request facts after a rewrite changed the URI.
    pub(crate) fn apply_rewrite(&mut self, path: &str, query: Option<&str>) {
        self.path = path.to_owned();
        self.query = query.map(str::to_owned);
        // nginx `$request_uri` keeps the original request line; `$uri` and
        // `$document_uri` follow the rewritten path.
    }

    pub(crate) fn set(&mut self, name: &str, value: String) {
        self.rewrite_touched = true;
        self.assigned.insert(name.to_owned(), value);
    }

    /// Seed a variable the request did not assign (a declared `map` result):
    /// it must be visible to lookups but must not by itself switch consumers to
    /// the rewrite-phase variable expander.
    pub(crate) fn seed(&mut self, name: &str, value: String) {
        self.assigned.insert(name.to_owned(), value);
    }

    /// Whether a rewrite phase executed and assigned/captured anything.
    pub(crate) fn uses_rewrite_variables(&self) -> bool {
        self.rewrite_touched
    }

    /// Replace `$1`..`$9` with the groups of `captures` (empty when the match
    /// failed).
    pub(crate) fn set_captures(&mut self, captures: impl IntoIterator<Item = Option<String>>) {
        let captures: Vec<Option<String>> = captures.into_iter().collect();
        if !captures.is_empty() {
            self.rewrite_touched = true;
        }
        self.captures = captures;
    }

    pub(crate) fn clear_captures(&mut self) {
        self.captures.clear();
    }

    fn capture(&self, name: &str) -> Option<String> {
        let index = name.parse::<usize>().ok()?;
        if index == 0 || index > 9 {
            return None;
        }
        Some(
            self.captures
                .get(index)
                .cloned()
                .flatten()
                .unwrap_or_default(),
        )
    }

    /// Expand one template (`$name`, `${name}`, `$1`..`$9`, `$$`).
    pub(crate) fn expand(&self, template: &str) -> String {
        sdkwork_webserver_core::expand_template(template, &mut |name| {
            Some(self.lookup(name).unwrap_or_default())
        })
    }

    /// Resolve a single variable reference (without `$`).
    pub(crate) fn lookup(&self, name: &str) -> Option<String> {
        if name.bytes().all(|byte| byte.is_ascii_digit()) && !name.is_empty() {
            return self.capture(name);
        }
        if let Some(value) = self.assigned.get(name) {
            return Some(value.clone());
        }
        if let Some(suffix) = name.strip_prefix("http_") {
            if !suffix.is_empty() {
                let header = suffix.replace('_', "-");
                return Some(
                    self.headers
                        .get(&header)
                        .and_then(|value| value.to_str().ok())
                        .map(|value| value.trim().to_owned())
                        .unwrap_or_default(),
                );
            }
        }
        if let Some(suffix) = name.strip_prefix("arg_") {
            if !suffix.is_empty() {
                return Some(
                    self.query
                        .as_deref()
                        .map(|query| query_argument(query, suffix))
                        .unwrap_or_default(),
                );
            }
        }
        if let Some(suffix) = name.strip_prefix("cookie_") {
            if !suffix.is_empty() {
                return Some(
                    self.headers
                        .get("cookie")
                        .and_then(|value| value.to_str().ok())
                        .map(|cookies| cookie_value(cookies, suffix))
                        .unwrap_or_default(),
                );
            }
        }
        let host_without_port = self
            .authority
            .rsplit_once(':')
            .filter(|(_, port)| port.parse::<u16>().is_ok())
            .map(|(host, _)| host)
            .unwrap_or(self.authority.as_str());
        Some(match name {
            "host" => host_without_port.to_owned(),
            "scheme" => self.scheme.clone(),
            "https" => {
                if self.scheme == "https" {
                    "on".to_owned()
                } else {
                    String::new()
                }
            }
            "remote_addr" => self.client_ip.clone(),
            "server_port" => self.listener_port.to_string(),
            "request_method" => self.method.clone(),
            "server_name" => self.server_name.clone(),
            "server_protocol" => self.server_protocol.clone(),
            "request_uri" => self.request_uri.clone(),
            "uri" | "document_uri" => self.path.clone(),
            "args" | "query_string" => self.query.clone().unwrap_or_default(),
            "is_args" => {
                if self.query.as_deref().is_some_and(|query| !query.is_empty()) {
                    "?".to_owned()
                } else {
                    String::new()
                }
            }
            "content_type" => self
                .headers
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_owned(),
            "content_length" => self
                .headers
                .get("content-length")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_owned(),
            "request_filename" => String::new(),
            "fastcgi_script_name" => self.path.clone(),
            "fastcgi_path_info" => String::new(),
            "document_root" => String::new(),
            _ => return None,
        })
    }

    /// Resolve a request-time `root $variable` / `alias $variable` template.
    ///
    /// The evaluated path must be absolute, free of `.`/`..` components, and
    /// inside one of the operator-configured allowlist prefixes; anything else
    /// is a runtime error, never a silent fallback.
    pub(crate) fn resolve_document_root(
        &self,
        template: &str,
        allowlist: &[String],
    ) -> Result<String, String> {
        let expanded = self.expand(template);
        if expanded.is_empty() || expanded.len() > 4096 {
            return Err("evaluated document root is empty or too long".to_owned());
        }
        if expanded.contains('\0') || expanded.bytes().any(|byte| byte < 0x20) {
            return Err("evaluated document root contains control characters".to_owned());
        }
        if !expanded.starts_with('/') {
            return Err(format!(
                "evaluated document root `{expanded}` is not absolute"
            ));
        }
        if std::path::Path::new(&expanded)
            .components()
            .any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
        {
            return Err(format!(
                "evaluated document root `{expanded}` contains `.` or `..` components"
            ));
        }
        let confined = allowlist.iter().any(|prefix| {
            expanded == *prefix
                || expanded
                    .strip_prefix(prefix.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        });
        if !confined {
            return Err(format!(
                "evaluated document root `{expanded}` is outside the documentRootAllowlist"
            ));
        }
        Ok(expanded)
    }
}

fn query_argument(query: &str, name: &str) -> String {
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key == name {
            return percent_decode(value);
        }
    }
    String::new()
}

fn cookie_value(cookies: &str, name: &str) -> String {
    for cookie in cookies.split(';') {
        let Some((key, value)) = cookie.trim().split_once('=') else {
            continue;
        };
        if key.trim() == name {
            return value.trim().to_owned();
        }
    }
    String::new()
}

/// `$arg_*` values are percent-decoded (nginx `ngx_unescape_uri`).
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &value[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                output.push(byte);
                index += 3;
                continue;
            }
        }
        if bytes[index] == b'+' {
            output.push(b' ');
        } else {
            output.push(bytes[index]);
        }
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn variables() -> RequestVariables {
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("api.example.com:8443"));
        headers.insert("user-agent", HeaderValue::from_static("curl/8.0"));
        headers.insert("cookie", HeaderValue::from_static("session=abc; env=dev"));
        RequestVariables::new(
            &headers,
            &Method::GET,
            "https",
            "api.example.com:8443",
            "127.0.0.1".parse().expect("ip"),
            443,
            "/v1/items",
            "/v1/items?q=hello%20world&x=1",
            Some("q=hello%20world&x=1"),
            "api.example.com",
            "HTTP/1.1",
        )
    }

    #[test]
    fn expands_request_facts_headers_arguments_and_cookies() {
        let mut variables = variables();
        variables.set("env", "prod".to_owned());
        assert_eq!(
            variables.expand("$host|${scheme}|$remote_addr"),
            "api.example.com|https|127.0.0.1"
        );
        assert_eq!(variables.expand("$http_user_agent"), "curl/8.0");
        assert_eq!(variables.expand("$arg_q|$arg_missing"), "hello world|");
        assert_eq!(variables.expand("$cookie_env|$cookie_missing"), "dev|");
        assert_eq!(variables.expand("$env"), "prod");
        assert_eq!(
            variables.expand("$uri?$is_args$args"),
            "/v1/items??q=hello%20world&x=1"
        );
        assert_eq!(variables.expand("$$env"), "$env");
        assert_eq!(variables.expand("$unknown"), "");
    }

    #[test]
    fn captures_are_scoped_to_the_last_match() {
        let mut variables = variables();
        variables.set_captures(vec![
            Some("whole".to_owned()),
            Some("file".to_owned()),
            None,
        ]);
        assert_eq!(variables.expand("$1.txt-${2}"), "file.txt-");
        variables.clear_captures();
        assert_eq!(variables.expand("$1"), "");
        assert!(variables.lookup("0").is_none());
    }

    #[test]
    fn dynamic_document_root_is_confined_to_the_allowlist() {
        let mut variables = variables();
        variables.set("root_path", "/srv/www".to_owned());
        let allowlist = vec!["/srv/www".to_owned(), "/srv/static".to_owned()];
        assert_eq!(
            variables
                .resolve_document_root("$root_path", &allowlist)
                .expect("allowlisted root"),
            "/srv/www"
        );
        assert!(variables
            .resolve_document_root("$root_path/../secret", &allowlist)
            .is_err());
        variables.set("escape", "/etc".to_owned());
        assert!(variables
            .resolve_document_root("$escape", &allowlist)
            .is_err());
        variables.set("relative", "srv/www".to_owned());
        assert!(variables
            .resolve_document_root("$relative", &allowlist)
            .is_err());
        assert!(variables.resolve_document_root("$root_path", &[]).is_err());
    }

    #[test]
    fn rewrite_updates_uri_but_not_request_uri() {
        let mut variables = variables();
        variables.apply_rewrite("/v2/items", None);
        assert_eq!(variables.lookup("uri").as_deref(), Some("/v2/items"));
        assert_eq!(
            variables.lookup("request_uri").as_deref(),
            Some("/v1/items?q=hello%20world&x=1")
        );
        assert_eq!(variables.lookup("args").as_deref(), Some(""));
    }
}
