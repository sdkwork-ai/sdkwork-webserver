//! nginx `sub_filter` response body substitution (pure function).
//!
//! Rules apply in declaration order. With `once` (nginx `sub_filter_once`,
//! default on) each rule replaces only its first occurrence, mirroring
//! nginx's per-directive semantics. The substitution operates on bytes, so
//! arbitrary (non-UTF-8) bodies pass through unchanged when no rule matches;
//! both the pattern and replacement are valid UTF-8 strings, and byte-level
//! search of a UTF-8 pattern in a UTF-8 body cannot split characters.

use super::model::SubFilterConfig;

/// Maximum response body the data plane buffers for substitution. Larger
/// bodies are served unchanged (nginx would fail on its own buffer limits;
/// availability wins).
pub const MAX_SUB_FILTER_BODY_BYTES: usize = 16 * 1024 * 1024;

/// Apply the configured substitution rules to a response body.
pub fn apply_sub_filters(body: &[u8], config: &SubFilterConfig) -> Vec<u8> {
    let mut output = body.to_vec();
    for rule in &config.rules {
        if rule.from.is_empty() {
            continue;
        }
        let needle = rule.from.as_bytes();
        let replacement = rule.to.as_bytes();
        output = if config.once {
            replace_first(&output, needle, replacement)
        } else {
            replace_all(&output, needle, replacement)
        };
    }
    output
}

/// Whether a response `Content-Type` value is eligible for substitution.
/// The type token (before `;`) is compared case-insensitively against the
/// configured types (nginx `sub_filter_types`).
pub fn sub_filter_content_type_matches(content_type: &str, types: &[String]) -> bool {
    let type_token = content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim();
    types
        .iter()
        .any(|candidate| candidate.trim().eq_ignore_ascii_case(type_token))
}

fn replace_first(haystack: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    let Some(index) = find_subslice(haystack, needle) else {
        return haystack.to_vec();
    };
    let mut output = Vec::with_capacity(haystack.len() + replacement.len().saturating_sub(needle.len()));
    output.extend_from_slice(&haystack[..index]);
    output.extend_from_slice(replacement);
    output.extend_from_slice(&haystack[index + needle.len()..]);
    output
}

fn replace_all(haystack: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(haystack.len());
    let mut search_from = 0;
    while let Some(relative) = find_subslice(&haystack[search_from..], needle) {
        let index = search_from + relative;
        output.extend_from_slice(&haystack[search_from..index]);
        output.extend_from_slice(replacement);
        search_from = index + needle.len();
    }
    output.extend_from_slice(&haystack[search_from..]);
    output
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SubFilterRule;

    fn config(rules: Vec<(&str, &str)>, once: bool) -> SubFilterConfig {
        SubFilterConfig {
            rules: rules
                .into_iter()
                .map(|(from, to)| SubFilterRule {
                    from: from.to_owned(),
                    to: to.to_owned(),
                })
                .collect(),
            once,
            types: default_sub_filter_types(),
            last_modified: false,
        }
    }

    fn default_sub_filter_types() -> Vec<String> {
        vec!["text/html".to_owned()]
    }

    #[test]
    fn replaces_first_occurrence_when_once() {
        let config = config(vec![("foo", "bar")], true);
        let result = apply_sub_filters(b"foo and foo", &config);
        assert_eq!(result, b"bar and foo");
    }

    #[test]
    fn replaces_all_occurrences_when_not_once() {
        let config = config(vec![("foo", "bar")], false);
        let result = apply_sub_filters(b"foo and foo", &config);
        assert_eq!(result, b"bar and bar");
    }

    #[test]
    fn multiple_rules_apply_in_declaration_order() {
        let config = config(vec![("a", "b"), ("b", "c")], false);
        // First rule turns a→b, second rule then rewrites the produced b→c.
        let result = apply_sub_filters(b"a", &config);
        assert_eq!(result, b"c");
    }

    #[test]
    fn non_matching_body_passes_through() {
        let config = config(vec![("absent", "x")], true);
        let result = apply_sub_filters(b"hello", &config);
        assert_eq!(result, b"hello");
    }

    #[test]
    fn multibyte_patterns_do_not_split_characters() {
        let config = config(vec![("中文", "cn")], false);
        let result = apply_sub_filters("中文abc中文".as_bytes(), &config);
        assert_eq!(result, "cnabccn".as_bytes());
    }

    #[test]
    fn content_type_matching_ignores_parameters_and_case() {
        let types = vec!["text/html".to_owned()];
        assert!(sub_filter_content_type_matches("text/html", &types));
        assert!(sub_filter_content_type_matches("text/html; charset=utf-8", &types));
        assert!(sub_filter_content_type_matches("TEXT/HTML", &types));
        assert!(!sub_filter_content_type_matches("application/json", &types));
    }
}
