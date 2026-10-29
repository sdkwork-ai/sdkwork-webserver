//! nginx `ngx_http_referer_module`: `valid_referers` evaluation for the
//! `$invalid_referer` variable.
//!
//! Semantics mirror the C module:
//! - no `valid_referers` declared: the variable is empty (valid);
//! - a missing `Referer` is valid only with `none`;
//! - a `Referer` that does not start with `http://`/`https://` (case
//!   insensitive, minimum nginx length) is valid only with `blocked`;
//! - otherwise the host part (lowercased, cut at the first `/` or `:`) is
//!   matched against the configured hosts/wildcards, `server_names`, then the
//!   `~regex` list (regexes match the scheme-stripped value case-insensitively
//!   per `ngx_http_add_regex_referer`);
//! - a configured host with a `/path` suffix additionally requires the
//!   referer path to start with that prefix.

use std::sync::{Mutex, OnceLock};

use regex::Regex;

use super::model::{RefererConfig, RefererHostEntry};

/// nginx `sizeof("http://i.ru") - 1`: a shorter value cannot carry a scheme.
const MINIMUM_SCHEMED_REFERER: usize = 11;
const REGEX_CACHE_MAXIMUM: usize = 64;

type RegexCache = Mutex<Vec<(String, Option<Regex>)>>;

fn regex_cache() -> &'static RegexCache {
    static CACHE: OnceLock<RegexCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Vec::new()))
}

fn compile_regex(pattern: &str) -> Option<Regex> {
    let mut cache = regex_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if cache.len() >= REGEX_CACHE_MAXIMUM && !cache.iter().any(|(key, _)| key == pattern) {
        cache.clear();
    }
    cache
        .iter()
        .find(|(key, _)| key == pattern)
        .map(|(_, regex)| regex.clone())
        .unwrap_or_else(|| {
            let regex = regex::RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .ok();
            cache.push((pattern.to_owned(), regex.clone()));
            regex
        })
}

/// Compile a configured `~regex` the same way the data plane does; used by
/// configuration validation to fail closed on patterns the regex engine
/// cannot compile.
#[must_use]
pub fn compile_referer_regex(pattern: &str) -> Option<Regex> {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .ok()
}

fn host_matches(configured: &str, host: &str) -> bool {
    if let Some(suffix) = configured.strip_prefix('*') {
        // nginx wildcard head (`*.example.com`): a subdomain of the suffix,
        // not the apex itself.
        host.len() > suffix.len() && host.ends_with(suffix)
    } else if let Some(apex) = configured.strip_prefix('.') {
        // nginx wildcard tail (`.example.com`): the apex or any subdomain.
        host == apex || (host.len() > configured.len() && host.ends_with(configured))
    } else {
        host == configured
    }
}

fn server_name_matches(name: &str, host: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    if let Some(pattern) = lowered.strip_prefix('~') {
        return compile_regex(pattern).is_some_and(|regex| regex.is_match(host));
    }
    host_matches(&lowered, host)
}

/// Evaluate nginx `$invalid_referer` for one request.
///
/// `referer` is the raw `Referer` header value. Returns `true` when nginx would
/// set `$invalid_referer` to `"1"`.
#[must_use]
pub fn invalid_referer(
    config: Option<&RefererConfig>,
    server_names: &[String],
    referer: Option<&str>,
) -> bool {
    let Some(config) = config else {
        return false;
    };
    let Some(referer) = referer else {
        return !config.none;
    };
    let bytes = referer.as_bytes();
    if bytes.len() < MINIMUM_SCHEMED_REFERER {
        return !config.blocked;
    }
    let scheme_len = if referer
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
    {
        7
    } else if referer
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
    {
        8
    } else {
        return !config.blocked;
    };
    let after_scheme = &referer[scheme_len..];
    let host_end = after_scheme.find(['/', ':']).unwrap_or(after_scheme.len());
    let host = after_scheme[..host_end].to_ascii_lowercase();
    // nginx resumes at the first `/` after the host or the port separator.
    let path_start = after_scheme.find('/');
    let path = path_start.map(|index| &after_scheme[index..]);

    if config.server_names
        && server_names
            .iter()
            .any(|name| server_name_matches(name, &host))
    {
        return false;
    }
    for RefererHostEntry {
        host: configured,
        path: configured_path,
    } in &config.hosts
    {
        if host_matches(configured, &host) {
            let path_ok = match configured_path {
                None => true,
                Some(prefix) => path.is_some_and(|value| value.starts_with(prefix)),
            };
            if path_ok {
                return false;
            }
        }
    }
    if config
        .regexes
        .iter()
        .any(|pattern| compile_regex(pattern).is_some_and(|regex| regex.is_match(after_scheme)))
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{invalid_referer, RefererConfig, RefererHostEntry};

    fn config(entries: &[&str], none: bool, blocked: bool, server_names: bool) -> RefererConfig {
        let mut hosts = Vec::new();
        let mut regexes = Vec::new();
        for entry in entries {
            if let Some(pattern) = entry.strip_prefix('~') {
                regexes.push(pattern.to_owned());
            } else if let Some((host, path)) = entry.split_once('/') {
                hosts.push(RefererHostEntry {
                    host: host.to_ascii_lowercase(),
                    path: Some(format!("/{path}")),
                });
            } else {
                hosts.push(RefererHostEntry {
                    host: entry.to_ascii_lowercase(),
                    path: None,
                });
            }
        }
        RefererConfig {
            none,
            blocked,
            server_names,
            hosts,
            regexes,
        }
    }

    fn valid(policy: &RefererConfig, referer: Option<&str>) -> bool {
        !invalid_referer(Some(policy), &[], referer)
    }

    #[test]
    fn missing_referer_is_valid_only_with_none() {
        let without = config(&["example.com"], false, false, false);
        assert!(!valid(&without, None));
        let with_none = config(&["example.com"], true, false, false);
        assert!(valid(&with_none, None));
    }

    #[test]
    fn blocked_referer_is_valid_only_with_blocked() {
        let without = config(&["example.com"], false, false, false);
        assert!(!valid(&without, Some("relative/path")));
        let with_blocked = config(&["example.com"], false, true, false);
        assert!(valid(&with_blocked, Some("relative/path")));
        // A scheme-less value shorter than the nginx minimum is blocked too.
        assert!(valid(&with_blocked, Some("x")));
    }

    #[test]
    fn scheme_and_host_matching_is_case_insensitive() {
        let policy = config(&["example.com"], false, false, false);
        assert!(valid(&policy, Some("HTTP://EXAMPLE.COM/page")));
        assert!(valid(&policy, Some("https://example.com:8443/page")));
        assert!(!valid(&policy, Some("http://other.test/page")));
    }

    #[test]
    fn wildcards_paths_and_regexes() {
        let policy = config(
            &[
                "*.example.com",
                "example.org/galleries",
                "~^www\\.google\\.",
            ],
            false,
            false,
            false,
        );
        assert!(valid(&policy, Some("http://a.example.com/x")));
        assert!(valid(&policy, Some("http://a.b.example.com/x")));
        assert!(!valid(&policy, Some("http://example.com/x")));
        assert!(valid(&policy, Some("http://EXAMPLE.ORG/galleries/pic")));
        assert!(!valid(&policy, Some("http://example.org/other")));
        assert!(valid(&policy, Some("http://www.google.com/search")));
        assert!(!valid(&policy, Some("http://evil.test/")));
    }

    #[test]
    fn server_names_matches_declared_names_and_their_wildcards() {
        let policy = config(&[], false, false, true);
        let names = vec!["referer.example.com".to_owned(), "*.wild.test".to_owned()];
        assert!(!invalid_referer(
            Some(&policy),
            &names,
            Some("http://referer.example.com/x")
        ));
        assert!(!invalid_referer(
            Some(&policy),
            &names,
            Some("http://a.wild.test/x")
        ));
        assert!(invalid_referer(
            Some(&policy),
            &names,
            Some("http://other.test/x")
        ));
    }

    #[test]
    fn no_policy_means_valid() {
        assert!(!invalid_referer(None, &[], None));
        assert!(!invalid_referer(None, &[], Some("http://anything/")));
    }
}
