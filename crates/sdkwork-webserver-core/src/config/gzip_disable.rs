//! nginx `gzip_disable` user-agent matching.
//!
//! nginx compiles every `gzip_disable` argument as a case-insensitive PCRE and
//! disables gzip when any pattern matches the request `User-Agent`. The
//! predefined argument `msie6` is not a regex: it sets a separate flag whose
//! test is nginx's `ngx_http_process_user_agent` MSIE detection (case-sensitive
//! `MSIE ` marker, versions 4.x/5.x, or 6.x without `SV1`; any `Opera` token
//! clears it). Both tests live here so the nginx materializer (validation) and
//! the data plane (negotiation) cannot drift apart.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// Bounded cache of compiled `gzip_disable` patterns.
static PATTERN_CACHE: LazyLock<Mutex<HashMap<String, Option<regex::Regex>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Upper bound on cached patterns; a configuration with more distinct patterns
/// than this still works, the cache simply restarts (the list is tiny in
/// practice and validation already bounds accepted patterns to compilable ones).
const PATTERN_CACHE_MAXIMUM: usize = 64;

/// Whether any configured pattern matches `user_agent` (case-insensitively).
#[must_use]
pub fn user_agent_matches_disable_patterns(patterns: &[String], user_agent: &str) -> bool {
    patterns.iter().any(|pattern| {
        let mut cache = PATTERN_CACHE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cache.len() >= PATTERN_CACHE_MAXIMUM && !cache.contains_key(pattern) {
            cache.clear();
        }
        let regex = cache.entry(pattern.clone()).or_insert_with(|| {
            regex::RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .ok()
        });
        regex
            .as_ref()
            .is_some_and(|regex| regex.is_match(user_agent))
    })
}

/// nginx's predefined `gzip_disable "msie6"` test
/// (`ngx_http_process_user_agent`).
#[must_use]
pub fn user_agent_is_msie6(user_agent: &str) -> bool {
    if user_agent.contains("Opera") {
        return false;
    }
    let Some(index) = user_agent.find("MSIE ") else {
        return false;
    };
    let bytes = user_agent.as_bytes();
    let version = index + 5;
    if version + 1 >= bytes.len() {
        return false;
    }
    if bytes[version + 1] != b'.' {
        return false;
    }
    match bytes[version] {
        b'4' | b'5' => true,
        b'6' => !user_agent[version + 2..].contains("SV1"),
        _ => false,
    }
}

/// Whether nginx's `gzip_disable` policy disables gzip for this user agent.
#[must_use]
pub fn gzip_disabled_for_user_agent(
    disable_msie6: bool,
    patterns: &[String],
    user_agent: Option<&str>,
) -> bool {
    let Some(user_agent) = user_agent else {
        return false;
    };
    if disable_msie6 && user_agent_is_msie6(user_agent) {
        return true;
    }
    user_agent_matches_disable_patterns(patterns, user_agent)
}

#[cfg(test)]
mod tests {
    use super::{
        gzip_disabled_for_user_agent, user_agent_is_msie6, user_agent_matches_disable_patterns,
    };

    #[test]
    fn msie6_matches_internet_explorer_4_5_6_like_nginx() {
        assert!(user_agent_is_msie6(
            "Mozilla/4.0 (compatible; MSIE 6.0; Windows NT 5.1)"
        ));
        assert!(user_agent_is_msie6(
            "Mozilla/4.0 (compatible; MSIE 5.5; Windows 98)"
        ));
        assert!(user_agent_is_msie6(
            "Mozilla/4.0 (compatible; MSIE 4.01; Windows 95)"
        ));
        // IE 6 with the SV1 token is a different engine build.
        assert!(!user_agent_is_msie6(
            "Mozilla/4.0 (compatible; MSIE 6.0; Windows NT 5.1; SV1)"
        ));
        // IE 7+ and non-IE browsers do not match.
        assert!(!user_agent_is_msie6(
            "Mozilla/4.0 (compatible; MSIE 7.0; Windows NT 6.0)"
        ));
        assert!(!user_agent_is_msie6("Mozilla/5.0 (X11; Linux x86_64)"));
        // Opera clears the MSIE marker, exactly like nginx.
        assert!(!user_agent_is_msie6(
            "Mozilla/4.0 (compatible; MSIE 6.0; Windows NT 5.1; Opera 9.0)"
        ));
    }

    #[test]
    fn patterns_match_case_insensitively() {
        let patterns = vec!["Googlebot".to_owned()];
        assert!(user_agent_matches_disable_patterns(
            &patterns,
            "Mozilla/5.0 (compatible; googlebot/2.1)"
        ));
        assert!(!user_agent_matches_disable_patterns(
            &patterns,
            "Mozilla/5.0 (X11; Linux x86_64)"
        ));
    }

    #[test]
    fn policy_combines_msie6_and_patterns() {
        let patterns = vec!["curl".to_owned()];
        assert!(gzip_disabled_for_user_agent(true, &[], Some("MSIE 6.0")));
        assert!(gzip_disabled_for_user_agent(
            false,
            &patterns,
            Some("curl/8.0")
        ));
        assert!(!gzip_disabled_for_user_agent(false, &patterns, Some("curl/8.0")).eq(&false));
        assert!(!gzip_disabled_for_user_agent(
            false,
            &patterns,
            Some("Mozilla/5.0")
        ));
        // No User-Agent header never matches.
        assert!(!gzip_disabled_for_user_agent(true, &patterns, None));
    }
}
