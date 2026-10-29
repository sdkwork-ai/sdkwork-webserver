//! nginx `map` evaluation on the data plane.
//!
//! The pure declaration/matching logic lives in
//! [`sdkwork_webserver_core::config::map_variables`]; this module supplies the
//! per-request facts and resolves every declared map into a result table the
//! response layers can expand.
//!
//! Source-variable semantics are differential-probed against nginx 1.29.6:
//!
//! - `$http_<name>` is the request field with `_` standing for `-`,
//!   surrounding whitespace trimmed, and repeated fields joined with `", "`;
//! - `$arg_<name>` is the **raw** query-argument value (nginx does not
//!   percent-decode `$arg_…`), the first occurrence wins;
//! - `$cookie_<name>` is the raw cookie value, the first `;`-separated pair
//!   wins;
//! - an absent header/argument/cookie and an empty one are both the empty
//!   string, which is why the authored empty key `""` matches both.

use std::net::IpAddr;

use axum::http::HeaderMap;
use sdkwork_webserver_core::{normalize_authority_host, MapVariables, WebServerAppConfig};

/// Per-request facts a `map` declaration is evaluated against.
pub(crate) struct MapRequestFacts<'a> {
    pub scheme: &'a str,
    /// Host authority as received (`Host` header or request target authority).
    pub authority: &'a str,
    pub client_ip: IpAddr,
    pub listener_port: u16,
    /// Original request target (`/path?query`), unaffected by rewrites.
    pub request_uri: &'a str,
    /// Current normalized URI path, affected by internal rewrites.
    pub path: &'a str,
    pub query: Option<&'a str>,
    pub method: &'a str,
    pub server_name: &'a str,
    pub headers: &'a HeaderMap,
}

impl MapVariables for MapRequestFacts<'_> {
    fn map_variable(&self, name: &str) -> Option<String> {
        let value = match name {
            "host" => normalize_authority_host(self.authority)
                .unwrap_or_else(|| self.authority.to_ascii_lowercase()),
            "scheme" => self.scheme.to_owned(),
            "remote_addr" => self.client_ip.to_string(),
            "server_port" => self.listener_port.to_string(),
            "request_uri" => self.request_uri.to_owned(),
            "uri" | "document_uri" => self.path.to_owned(),
            "args" => self.query.unwrap_or("").to_owned(),
            "is_args" => self
                .query
                .filter(|query| !query.is_empty())
                .map_or_else(String::new, |_| "?".to_owned()),
            "request_method" => self.method.to_owned(),
            "server_name" => self.server_name.to_owned(),
            _ => {
                if let Some(header) = name.strip_prefix("http_") {
                    let header_name = header.replace('_', "-");
                    return Some(joined_header(self.headers, &header_name));
                }
                if let Some(argument) = name.strip_prefix("arg_") {
                    return Some(raw_argument(self.query, argument));
                }
                if let Some(cookie) = name.strip_prefix("cookie_") {
                    return Some(raw_cookie(self.headers, cookie));
                }
                return None;
            }
        };
        Some(value)
    }
}

/// nginx `$http_<name>`: repeated request fields are joined with `", "` and
/// the value is trimmed of optional whitespace.
fn joined_header(headers: &HeaderMap, name: &str) -> String {
    let mut output: Option<String> = None;
    for value in headers.get_all(name) {
        let value = value.to_str().unwrap_or_default().trim();
        match &mut output {
            Some(existing) => {
                existing.push_str(", ");
                existing.push_str(value);
            }
            None => output = Some(value.to_owned()),
        }
    }
    output.unwrap_or_default()
}

/// nginx `$arg_<name>`: raw (not percent-decoded) value of the first matching
/// query argument.
fn raw_argument(query: Option<&str>, name: &str) -> String {
    let Some(query) = query else {
        return String::new();
    };
    for pair in query.split('&') {
        let (candidate, value) = match pair.split_once('=') {
            Some((candidate, value)) => (candidate, value),
            None => (pair, ""),
        };
        if candidate == name {
            return value.to_owned();
        }
    }
    String::new()
}

/// nginx `$cookie_<name>`: raw value of the first matching cookie.
fn raw_cookie(headers: &HeaderMap, name: &str) -> String {
    for header in headers.get_all("cookie") {
        let Ok(header) = header.to_str() else {
            continue;
        };
        for pair in header.split(';') {
            let pair = pair.trim();
            if let Some((candidate, value)) = pair.split_once('=') {
                if candidate.trim() == name {
                    return value.trim().to_owned();
                }
            }
        }
    }
    String::new()
}

/// Every declared map result for one request, in declaration order.
#[derive(Clone, Default)]
pub(crate) struct MapResults {
    values: Vec<(String, String)>,
}

impl MapResults {
    /// Evaluate every `map` declaration against the request facts.
    pub(crate) fn evaluate(config: &WebServerAppConfig, facts: &MapRequestFacts<'_>) -> Self {
        if config.maps.is_empty() {
            return Self::default();
        }
        let mut values = Vec::with_capacity(config.maps.len());
        for map in &config.maps {
            let value = sdkwork_webserver_core::evaluate_map(map, facts);
            if let Some(name) = map.result.strip_prefix('$') {
                values.push((name.to_owned(), value));
            }
        }
        Self { values }
    }

    /// Build a result table directly (tests only).
    #[cfg(test)]
    pub(crate) fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        Self {
            values: pairs
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        }
    }

    /// Resolve one map result by name (without `$`).
    pub(crate) fn lookup(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, value)| value.as_str())
    }

    /// Substitute declared map result references (`$name`) in a template,
    /// leaving every other `$…` token untouched for the caller's own
    /// expander. `$$` is a literal `$`.
    ///
    /// Used where the surrounding code already performs its own variable
    /// expansion (redirect URLs, `rewrite` replacements).
    pub(crate) fn expand_tokens(&self, template: &str) -> String {
        self.expand_tokens_inner(template, false)
    }

    /// Like [`Self::expand_tokens`], but a `$` that survives from a map value
    /// is written as `$$` so a *later* nginx-variable expansion pass renders
    /// it literally instead of re-expanding a value the map already resolved.
    pub(crate) fn expand_tokens_escaped(&self, template: &str) -> String {
        self.expand_tokens_inner(template, true)
    }

    fn expand_tokens_inner(&self, template: &str, escape_dollars: bool) -> String {
        if self.values.is_empty() || !template.contains('$') {
            return template.to_owned();
        }
        let mut output = String::with_capacity(template.len() + 16);
        let bytes = template.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != b'$' {
                let character = template[index..]
                    .chars()
                    .next()
                    .expect("index is on a character boundary");
                output.push(character);
                index += character.len_utf8();
                continue;
            }
            index += 1;
            if index < bytes.len() && bytes[index] == b'$' {
                output.push('$');
                index += 1;
                continue;
            }
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            if start == index {
                output.push('$');
                continue;
            }
            let name = &template[start..index];
            match self.lookup(name) {
                Some(value) => {
                    if escape_dollars {
                        for character in value.chars() {
                            if character == '$' {
                                output.push_str("$$");
                            } else {
                                output.push(character);
                            }
                        }
                    } else {
                        output.push_str(value);
                    }
                }
                None => {
                    output.push('$');
                    output.push_str(name);
                }
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn facts<'a>(headers: &'a HeaderMap, query: Option<&'a str>) -> MapRequestFacts<'a> {
        MapRequestFacts {
            scheme: "https",
            authority: "Web.Example.com:8443",
            client_ip: "127.0.0.1".parse().expect("client ip"),
            listener_port: 443,
            request_uri: "/probe/path?q=one&raw=a%20b",
            path: "/probe/path",
            query,
            method: "GET",
            server_name: "web.example.com",
            headers,
        }
    }

    #[test]
    fn fixed_request_variables_match_nginx() {
        let headers = HeaderMap::new();
        let facts = facts(&headers, Some("q=one&raw=a%20b"));
        assert_eq!(
            facts.map_variable("host"),
            Some("web.example.com".to_owned())
        );
        assert_eq!(facts.map_variable("scheme"), Some("https".to_owned()));
        assert_eq!(facts.map_variable("server_port"), Some("443".to_owned()));
        assert_eq!(facts.map_variable("uri"), Some("/probe/path".to_owned()));
        assert_eq!(
            facts.map_variable("args"),
            Some("q=one&raw=a%20b".to_owned())
        );
        assert_eq!(facts.map_variable("is_args"), Some("?".to_owned()));
        assert_eq!(
            facts.map_variable("request_uri"),
            Some("/probe/path?q=one&raw=a%20b".to_owned())
        );
        assert_eq!(
            facts.map_variable("server_name"),
            Some("web.example.com".to_owned())
        );
        assert_eq!(facts.map_variable("request_method"), Some("GET".to_owned()));
        assert_eq!(
            facts.map_variable("remote_addr"),
            Some("127.0.0.1".to_owned())
        );
    }

    #[test]
    fn request_variable_families_follow_nginx() {
        let mut headers = HeaderMap::new();
        headers.append("x-key", HeaderValue::from_static("  first  "));
        headers.append("x-key", HeaderValue::from_static("second"));
        headers.insert("cookie", HeaderValue::from_static("a=1; c=raw value"));
        let facts = facts(&headers, Some("q=one&raw=a%20b"));
        // Repeated request fields are joined with `", "`, whitespace trimmed.
        assert_eq!(
            facts.map_variable("http_x_key"),
            Some("first, second".to_owned())
        );
        // `$arg_` values are raw (not percent-decoded) and first occurrence wins.
        assert_eq!(facts.map_variable("arg_raw"), Some("a%20b".to_owned()));
        assert_eq!(facts.map_variable("arg_q"), Some("one".to_owned()));
        assert_eq!(facts.map_variable("arg_missing"), Some(String::new()));
        assert_eq!(facts.map_variable("cookie_c"), Some("raw value".to_owned()));
        assert_eq!(facts.map_variable("cookie_missing"), Some(String::new()));
        assert_eq!(facts.map_variable("upstream_addr"), None);
    }

    #[test]
    fn map_results_expand_tokens_and_escape_resolved_dollars() {
        let results = MapResults {
            values: vec![
                ("tenant".to_owned(), "acme".to_owned()),
                ("literal".to_owned(), "$host".to_owned()),
            ],
        };
        assert_eq!(
            results.expand_tokens("zone=$tenant;other=$host"),
            "zone=acme;other=$host"
        );
        assert_eq!(results.expand_tokens_escaped("$literal"), "$$host");
        assert_eq!(results.expand_tokens("$$tenant"), "$tenant");
        assert_eq!(results.expand_tokens("$unknown"), "$unknown");
    }
}
