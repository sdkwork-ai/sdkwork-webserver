//! nginx proxy response-header policy (`proxy_hide_header`, `proxy_pass_header`,
//! `proxy_redirect`, `proxy_cookie_path`, `proxy_cookie_domain`).
//!
//! Every semantic here was differential-probed against nginx 1.29.6 (see
//! `tests/nginx/differential-http`):
//!
//! - the default-hidden upstream response header set is
//!   `ngx_http_proxy_hide_headers`: `Date`, `Server`, `X-Pad`,
//!   `X-Accel-Expires`, `X-Accel-Redirect`, `X-Accel-Limit-Rate`,
//!   `X-Accel-Buffering`, `X-Accel-Charset`;
//! - `proxy_pass_header` removes a name from the hidden set (pass wins when a
//!   name is both hidden and passed);
//! - `proxy_redirect` rewrites the `Location` header and the URL part of a
//!   `Refresh` header (`url=` is found case-insensitively); a rule matches when
//!   its literal `<from>` is a case-insensitive prefix of the target and the
//!   replacement is emitted verbatim. `proxy_redirect off` leaves both
//!   untouched; the materializer expands nginx's `default` rule to a literal
//!   `<from> <to>` pair;
//! - `proxy_cookie_path` rewrites a `Path` attribute whose value starts with
//!   the literal `<from>`; `proxy_cookie_domain` compares a `Domain` attribute
//!   case-insensitively after ignoring one leading dot and re-applies the
//!   matched value's leading dot. Both only rewrite the attribute they name
//!   and preserve every other attribute byte-for-byte (nginx re-serializes the
//!   cookie with `; ` separators when at least one attribute changed).
//!
//! The runtime applies the rewrite to the *final* upstream headers, so a policy
//! is observable exactly where nginx's upstream header filter would run.
//!
//! Bounded divergence: nginx marks only a `Location` that a `proxy_redirect`
//! rule rewrote (or an absolute one) as the redirect target of its header
//! filter, so a *relative* upstream `Location` that no rule matched stays
//! relative even under `absolute_redirect on`. The runtime's generic redirect
//! finalizer makes every relative `Location` absolute under
//! `absolute_redirect on`, including a proxied one that no rule matched. The
//! differential fixture pins `absolute_redirect off` for the proxy-policy hosts
//! and exercises the absolute form only on rewritten targets, so this
//! divergence is not part of the compared contract.

use axum::http::{
    header::{HeaderName, HeaderValue, LOCATION, REFRESH, SET_COOKIE},
    HeaderMap,
};
use sdkwork_webserver_core::{
    CookieRewritePolicyConfig, ProxyRedirectPolicyConfig, ProxyResponsePolicyConfig,
    ProxyRewriteRuleConfig,
};

/// nginx `ngx_http_proxy_hide_headers`: response headers the proxy module never
/// forwards from the upstream.
pub(crate) const NGINX_DEFAULT_HIDDEN_HEADERS: &[&str] = &[
    "date",
    "server",
    "x-pad",
    "x-accel-expires",
    "x-accel-redirect",
    "x-accel-limit-rate",
    "x-accel-buffering",
    "x-accel-charset",
];

/// Everything needed to apply one proxy resource's response-header policy.
pub(crate) struct ProxyResponsePolicyContext<'a> {
    pub(crate) policy: &'a ProxyResponsePolicyConfig,
    /// nginx's `proxy_redirect default` pattern for this resource, expanded at
    /// materialization time (raw `proxy_pass` argument, plus `/` when it has no
    /// URI part). Only used for a typed-plane `Default` declaration, where no
    /// materializer expanded it.
    pub(crate) default_redirect_from: Option<&'a str>,
    /// Replacement for the expanded `default` rule (the location name, or `/`).
    pub(crate) default_redirect_to: &'a str,
}

/// Apply hide/pass, `proxy_redirect`, and cookie-attribute rewriting to the
/// upstream response headers.
pub(crate) fn apply_proxy_response_policy(
    headers: &mut HeaderMap,
    context: &ProxyResponsePolicyContext<'_>,
) {
    apply_hidden_headers(headers, context.policy);
    apply_redirect_policy(headers, context);
    apply_cookie_policy(headers, context.policy);
}

fn apply_hidden_headers(headers: &mut HeaderMap, policy: &ProxyResponsePolicyConfig) {
    let mut hidden: Vec<String> = NGINX_DEFAULT_HIDDEN_HEADERS
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    for name in &policy.hide_headers {
        if !hidden
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(name))
        {
            hidden.push(name.to_ascii_lowercase());
        }
    }
    // nginx's pass list is applied after the hide list, so it wins on
    // conflicts (differential-confirmed: `proxy_hide_header X` +
    // `proxy_pass_header X` forwards X).
    hidden.retain(|name| {
        !policy
            .pass_headers
            .iter()
            .any(|passed| passed.eq_ignore_ascii_case(name))
    });
    let remove: Vec<HeaderName> = headers
        .keys()
        .filter(|name| {
            hidden
                .iter()
                .any(|hidden| name.as_str().eq_ignore_ascii_case(hidden))
        })
        .cloned()
        .collect();
    for name in remove {
        headers.remove(name);
    }
}

fn apply_redirect_policy(headers: &mut HeaderMap, context: &ProxyResponsePolicyContext<'_>) {
    let Some(policy) = &context.policy.redirect else {
        return;
    };
    let rules: Vec<ProxyRewriteRuleConfig> = match policy {
        // nginx `proxy_redirect off`: pass Location/Refresh through.
        ProxyRedirectPolicyConfig::Off => return,
        ProxyRedirectPolicyConfig::Default => {
            let Some(from) = context.default_redirect_from else {
                return;
            };
            vec![ProxyRewriteRuleConfig {
                from: from.to_owned(),
                to: context.default_redirect_to.to_owned(),
            }]
        }
        ProxyRedirectPolicyConfig::Rules(rules) => rules.clone(),
    };

    if let Some(value) = headers.get(LOCATION) {
        if let Ok(location) = value.to_str() {
            if let Some(rewritten) = rewrite_target(location, &rules) {
                if let Ok(value) = HeaderValue::from_str(&rewritten) {
                    headers.insert(LOCATION, value);
                }
            }
        }
    }

    // nginx `proxy_redirect` also rewrites the URL of a `Refresh` header: the
    // first case-insensitive `url=` marks the start of the rewritten value.
    let refresh_values: Vec<HeaderValue> = headers.get_all(REFRESH).iter().cloned().collect();
    if refresh_values.is_empty() {
        return;
    }
    let mut rewritten_values = Vec::with_capacity(refresh_values.len());
    let mut changed = false;
    for value in refresh_values {
        let Ok(text) = value.to_str() else {
            rewritten_values.push(value);
            continue;
        };
        let Some(index) = find_ascii_case_insensitive(text, "url=") else {
            rewritten_values.push(value);
            continue;
        };
        let url_start = index + 4;
        match rewrite_target(&text[url_start..], &rules) {
            Some(rewritten) => {
                let new_value = format!("{}{rewritten}", &text[..url_start]);
                match HeaderValue::from_str(&new_value) {
                    Ok(new_value) => {
                        rewritten_values.push(new_value);
                        changed = true;
                    }
                    Err(_) => rewritten_values.push(value),
                }
            }
            None => rewritten_values.push(value),
        }
    }
    if changed {
        headers.remove(REFRESH);
        for value in rewritten_values {
            headers.append(REFRESH, value);
        }
    }
}

fn apply_cookie_policy(headers: &mut HeaderMap, policy: &ProxyResponsePolicyConfig) {
    if policy.cookie_path.is_none() && policy.cookie_domain.is_none() {
        return;
    }
    let values: Vec<HeaderValue> = headers.get_all(SET_COOKIE).iter().cloned().collect();
    if values.is_empty() {
        return;
    }
    let mut rewritten = Vec::with_capacity(values.len());
    let mut changed = false;
    for value in values {
        let Ok(text) = value.to_str() else {
            rewritten.push(value);
            continue;
        };
        match rewrite_set_cookie(text, policy) {
            Some(new_value) => match HeaderValue::from_str(&new_value) {
                Ok(new_value) => {
                    rewritten.push(new_value);
                    changed = true;
                }
                Err(_) => rewritten.push(value),
            },
            None => rewritten.push(value),
        }
    }
    if changed {
        headers.remove(SET_COOKIE);
        for value in rewritten {
            headers.append(SET_COOKIE, value);
        }
    }
}

/// Rewrite the first matching rule's literal prefix on `target`.
///
/// nginx matches with `ngx_rstrncmp` on the byte sequence, i.e. ASCII
/// case-insensitively; a non-ASCII pattern never matches (nginx configuration
/// literals are bytes and the runtime's header values are ASCII here).
fn rewrite_target(target: &str, rules: &[ProxyRewriteRuleConfig]) -> Option<String> {
    for rule in rules {
        if rule.from.is_empty() {
            continue;
        }
        if !ascii_prefix_matches(target, &rule.from) {
            continue;
        }
        let mut rewritten = String::with_capacity(rule.to.len() + target.len());
        rewritten.push_str(&rule.to);
        rewritten.push_str(target.get(rule.from.len()..)?);
        return Some(rewritten);
    }
    None
}

fn ascii_prefix_matches(target: &str, prefix: &str) -> bool {
    let target = target.as_bytes();
    let prefix = prefix.as_bytes();
    if prefix.len() > target.len() {
        return false;
    }
    target[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .find(|&index| haystack[index..index + needle.len()].eq_ignore_ascii_case(needle))
}

/// One parsed `Set-Cookie` attribute (nginx `ngx_http_proxy_parse_cookie`).
#[derive(Debug, PartialEq, Eq)]
struct CookieAttribute {
    key: String,
    value: Option<String>,
}

fn parse_cookie_attributes(value: &str) -> Vec<CookieAttribute> {
    let mut attributes = Vec::new();
    let mut start = 0usize;
    let bytes = value.as_bytes();
    loop {
        let end = value[start..]
            .find(';')
            .map(|offset| start + offset)
            .unwrap_or(value.len());
        let mut piece_start = start;
        while piece_start < end && bytes[piece_start] == b' ' {
            piece_start += 1;
        }
        let equality = value[piece_start..end].find('=');
        let (key_start, key_end, attr_value) = match equality {
            Some(offset) => {
                let equality_index = piece_start + offset;
                let mut value_start = equality_index + 1;
                while value_start < end && bytes[value_start] == b' ' {
                    value_start += 1;
                }
                let mut value_end = end;
                while value_end > value_start && bytes[value_end - 1] == b' ' {
                    value_end -= 1;
                }
                (
                    piece_start,
                    equality_index,
                    Some(value[value_start..value_end].to_owned()),
                )
            }
            None => (piece_start, end, None),
        };
        let mut key_end = key_end;
        while key_end > key_start && bytes[key_end - 1] == b' ' {
            key_end -= 1;
        }
        attributes.push(CookieAttribute {
            key: value[key_start..key_end].to_owned(),
            value: attr_value,
        });
        if end == value.len() {
            break;
        }
        start = end + 1;
    }
    attributes
}

fn rewrite_set_cookie(value: &str, policy: &ProxyResponsePolicyConfig) -> Option<String> {
    let mut attributes = parse_cookie_attributes(value);
    // nginx declines when the cookie pair itself has no `=` (attr[0] without a
    // value): there is nothing to rewrite in that case.
    if attributes.first().is_none_or(|attr| attr.value.is_none()) {
        return None;
    }
    let mut changed = false;
    for attribute in attributes.iter_mut().skip(1) {
        let Some(current) = attribute.value.as_deref() else {
            continue;
        };
        if attribute.key.eq_ignore_ascii_case("domain") {
            if let Some(rewritten) = policy
                .cookie_domain
                .as_ref()
                .and_then(|policy| rewrite_cookie_domain(current, policy))
            {
                attribute.value = Some(rewritten);
                changed = true;
            }
        } else if attribute.key.eq_ignore_ascii_case("path") {
            if let Some(rewritten) = policy
                .cookie_path
                .as_ref()
                .and_then(|policy| rewrite_cookie_path(current, policy))
            {
                attribute.value = Some(rewritten);
                changed = true;
            }
        }
    }
    if !changed {
        return None;
    }
    Some(
        attributes
            .iter()
            .map(|attribute| match &attribute.value {
                Some(value) => format!("{}={value}", attribute.key),
                None => attribute.key.clone(),
            })
            .collect::<Vec<_>>()
            .join("; "),
    )
}

fn rewrite_cookie_path(value: &str, policy: &CookieRewritePolicyConfig) -> Option<String> {
    if let CookieRewritePolicyConfig::RegexRules(rules) = policy {
        for rule in rules {
            let pattern = if rule.case_insensitive {
                format!("(?i){}", rule.pattern)
            } else {
                rule.pattern.clone()
            };
            let Ok(regex) = regex::Regex::new(&pattern) else {
                continue;
            };
            if regex.is_match(value) {
                // nginx replaces the matching portion with the configured
                // replacement (capture references `$1..$9` expand).
                return Some(regex.replace(value, rule.replacement.as_str()).into_owned());
            }
        }
        return None;
    }
    let CookieRewritePolicyConfig::Rules(rules) = policy else {
        return None;
    };
    for rule in rules {
        if rule.from.is_empty() {
            continue;
        }
        if let Some(remainder) = value.strip_prefix(rule.from.as_str()) {
            return Some(format!("{}{remainder}", rule.to));
        }
    }
    None
}

fn rewrite_cookie_domain(value: &str, policy: &CookieRewritePolicyConfig) -> Option<String> {
    let CookieRewritePolicyConfig::Rules(rules) = policy else {
        return None;
    };
    let (leading_dot, bare) = match value.strip_prefix('.') {
        Some(bare) => (true, bare),
        None => (false, value),
    };
    for rule in rules {
        if rule.from.is_empty() {
            continue;
        }
        if bare.eq_ignore_ascii_case(&rule.from) {
            let mut rewritten = String::with_capacity(rule.to.len() + 1);
            if leading_dot {
                rewritten.push('.');
            }
            rewritten.push_str(&rule.to);
            return Some(rewritten);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(json: serde_json::Value) -> ProxyResponsePolicyConfig {
        serde_json::from_value(json).expect("policy")
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.append(
                HeaderName::from_bytes(name.as_bytes()).expect("name"),
                HeaderValue::from_str(value).expect("value"),
            );
        }
        headers
    }

    fn apply(headers: &mut HeaderMap, policy: &ProxyResponsePolicyConfig) {
        apply_proxy_response_policy(
            headers,
            &ProxyResponsePolicyContext {
                policy,
                default_redirect_from: None,
                default_redirect_to: "/",
            },
        );
    }

    #[test]
    fn default_hidden_headers_are_removed() {
        let mut headers = headers(&[
            ("Server", "up/1.0"),
            ("Date", "Thu, 01 Jan 1970 00:00:00 GMT"),
            ("X-Pad", "pad"),
            ("X-Accel-Expires", "60"),
            ("X-Accel-Redirect", "/internal"),
            ("X-Accel-Limit-Rate", "100"),
            ("X-Accel-Buffering", "yes"),
            ("X-Accel-Charset", "utf-8"),
            ("X-Upstream", "yes"),
            ("Vary", "Accept-Encoding"),
        ]);
        apply(&mut headers, &policy(serde_json::json!({})));
        for name in NGINX_DEFAULT_HIDDEN_HEADERS {
            assert!(headers.get(*name).is_none(), "{name} must be hidden");
        }
        assert_eq!(headers.get("x-upstream").unwrap(), "yes");
        // nginx does not hide Vary/Cache-Control/Set-Cookie by default.
        assert_eq!(headers.get("vary").unwrap(), "Accept-Encoding");
    }

    #[test]
    fn hide_and_pass_headers_interact_like_nginx() {
        let mut headers = headers(&[("X-Secret", "s"), ("X-Pad", "pad"), ("X-Other", "o")]);
        let policy = policy(serde_json::json!({
            "hideHeaders": ["x-secret"],
            "passHeaders": ["X-Pad"],
        }));
        apply(&mut headers, &policy);
        assert!(headers.get("x-secret").is_none());
        assert_eq!(headers.get("x-pad").unwrap(), "pad");
        assert_eq!(headers.get("x-other").unwrap(), "o");
    }

    #[test]
    fn proxy_redirect_rewrites_location_and_refresh() {
        let mut headers = headers(&[
            ("Location", "http://upstream.internal:8080/echo/target?x=1"),
            (
                "Refresh",
                "5; url=http://UPSTREAM.internal:8080/echo/target",
            ),
        ]);
        let policy = policy(serde_json::json!({
            "redirect": {"rules": [{"from": "http://upstream.internal:8080/", "to": "/client/"}]}
        }));
        apply(&mut headers, &policy);
        assert_eq!(headers.get("location").unwrap(), "/client/echo/target?x=1");
        // The `url=` marker is matched case-insensitively and only the URL part
        // is rewritten; the delay part is preserved.
        assert_eq!(
            headers.get("refresh").unwrap(),
            "5; url=/client/echo/target"
        );
    }

    #[test]
    fn proxy_redirect_off_leaves_targets_untouched() {
        let mut headers = headers(&[
            ("Location", "http://upstream/echo"),
            ("Refresh", "1; url=/x"),
        ]);
        apply(
            &mut headers,
            &policy(serde_json::json!({"redirect": "off"})),
        );
        assert_eq!(headers.get("location").unwrap(), "http://upstream/echo");
        assert_eq!(headers.get("refresh").unwrap(), "1; url=/x");
    }

    #[test]
    fn proxy_cookie_path_and_domain_are_rewritten() {
        let mut headers = HeaderMap::new();
        headers.append(
            SET_COOKIE,
            HeaderValue::from_static(
                "sid=abc; Path=/upstream; Domain=.upstream.example.com; HttpOnly",
            ),
        );
        headers.append(SET_COOKIE, HeaderValue::from_static("other=1; Path=/"));
        headers.append(SET_COOKIE, HeaderValue::from_static("bare=2"));
        let policy = policy(serde_json::json!({
            "cookiePath": {"rules": [{"from": "/upstream", "to": "/client"}]},
            "cookieDomain": {"rules": [{"from": "upstream.example.com", "to": "edge.example.com"}]}
        }));
        apply(&mut headers, &policy);
        let values: Vec<&[u8]> = headers
            .get_all(SET_COOKIE)
            .iter()
            .map(HeaderValue::as_bytes)
            .collect();
        assert_eq!(
            values,
            vec![
                b"sid=abc; Path=/client; Domain=.edge.example.com; HttpOnly".as_slice(),
                b"other=1; Path=/".as_slice(),
                b"bare=2".as_slice(),
            ]
        );
    }

    #[test]
    fn cookie_attribute_names_are_case_insensitive_and_preserved() {
        let mut headers = HeaderMap::new();
        headers.append(
            SET_COOKIE,
            HeaderValue::from_static("c=1; path=/upstream; domain=upstream.example.com"),
        );
        let policy = policy(serde_json::json!({
            "cookiePath": {"rules": [{"from": "/upstream", "to": "/client"}]},
            "cookieDomain": {"rules": [{"from": "upstream.example.com", "to": "edge.example.com"}]}
        }));
        apply(&mut headers, &policy);
        assert_eq!(
            headers.get(SET_COOKIE).unwrap(),
            "c=1; path=/client; domain=edge.example.com"
        );
    }

    #[test]
    fn cookie_off_disables_rewriting() {
        let mut headers = HeaderMap::new();
        headers.append(
            SET_COOKIE,
            HeaderValue::from_static("a=1; Path=/upstream; Domain=upstream.example.com"),
        );
        let policy = policy(serde_json::json!({
            "cookiePath": "off",
            "cookieDomain": "off"
        }));
        apply(&mut headers, &policy);
        assert_eq!(
            headers.get(SET_COOKIE).unwrap(),
            "a=1; Path=/upstream; Domain=upstream.example.com"
        );
    }

    #[test]
    fn cookie_without_cookie_pair_value_is_not_rewritten() {
        let mut headers = HeaderMap::new();
        headers.append(
            SET_COOKIE,
            HeaderValue::from_static("no-pair; Path=/upstream"),
        );
        let policy = policy(serde_json::json!({
            "cookiePath": {"rules": [{"from": "/upstream", "to": "/client"}]}
        }));
        apply(&mut headers, &policy);
        assert_eq!(headers.get(SET_COOKIE).unwrap(), "no-pair; Path=/upstream");
    }

    #[test]
    fn typed_plane_default_redirect_uses_expanded_rule() {
        let mut headers = headers(&[("Location", "http://up/echo/target")]);
        let policy = policy(serde_json::json!({"redirect": "default"}));
        apply_proxy_response_policy(
            &mut headers,
            &ProxyResponsePolicyContext {
                policy: &policy,
                default_redirect_from: Some("http://up/"),
                default_redirect_to: "/client/",
            },
        );
        assert_eq!(headers.get("location").unwrap(), "/client/echo/target");
    }
}
