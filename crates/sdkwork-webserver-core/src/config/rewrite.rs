//! Parse and apply nginx `rewrite` directive strings (http-core-v1 subset).

use regex::Regex;

use super::model::{RewriteFlag, RewriteRuleConfig};

/// Maximum internal `last` redirects per request (nginx default is 10).
pub const MAX_REWRITE_INTERNAL_REDIRECTS: u32 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewriteParseError {
    Empty,
    MissingReplacement,
    InvalidFlag(String),
    InvalidPattern(String),
}

impl std::fmt::Display for RewriteParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty rewrite directive"),
            Self::MissingReplacement => {
                write!(f, "rewrite requires pattern and replacement")
            }
            Self::InvalidFlag(flag) => write!(
                f,
                "unsupported rewrite flag `{flag}`; use last, break, redirect, or permanent"
            ),
            Self::InvalidPattern(error) => write!(f, "invalid rewrite pattern: {error}"),
        }
    }
}

/// Parse one `rewrite` entry: `"^/old/(.*)$ /new/$1 last"`.
pub fn parse_rewrite(entry: &str) -> Result<RewriteRuleConfig, RewriteParseError> {
    let trimmed = entry.trim();
    if trimmed.is_empty() {
        return Err(RewriteParseError::Empty);
    }
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    for ch in trimmed.chars() {
        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            ' ' | '\t' if !in_single && !in_double => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    if parts.len() < 2 {
        return Err(RewriteParseError::MissingReplacement);
    }
    let pattern = parts[0].clone();
    let replacement = parts[1].clone();
    let flag = match parts.get(2).map(String::as_str) {
        None | Some("last") => RewriteFlag::Last,
        Some("break") => RewriteFlag::Break,
        Some("redirect") => RewriteFlag::Redirect,
        Some("permanent") => RewriteFlag::Permanent,
        Some(other) => return Err(RewriteParseError::InvalidFlag(other.to_owned())),
    };
    if parts.len() > 3 {
        return Err(RewriteParseError::InvalidFlag(parts[3].clone()));
    }
    Regex::new(&pattern).map_err(|error| RewriteParseError::InvalidPattern(error.to_string()))?;
    Ok(RewriteRuleConfig {
        pattern,
        replacement,
        flag,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewriteOutcome {
    /// Continue serving the current location with this URI path (+ optional query).
    Continue { path: String, query: Option<String> },
    /// Re-select locations with this URI path.
    Reselect { path: String, query: Option<String> },
    /// External redirect.
    Redirect { status: u16, location: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewriteApplyError {
    InvalidPattern(String),
    TooManyRedirects,
}

impl std::fmt::Display for RewriteApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPattern(error) => write!(f, "rewrite pattern failed: {error}"),
            Self::TooManyRedirects => write!(
                f,
                "rewrite exceeded {MAX_REWRITE_INTERNAL_REDIRECTS} internal redirects"
            ),
        }
    }
}

/// Apply ordered rewrite rules to a request path (and optional query string).
pub fn apply_rewrites(
    path: &str,
    query: Option<&str>,
    rules: &[RewriteRuleConfig],
) -> Result<RewriteOutcome, RewriteApplyError> {
    let uri = match query {
        Some(q) if !q.is_empty() => format!("{path}?{q}"),
        _ => path.to_owned(),
    };
    for rule in rules {
        let regex = Regex::new(&rule.pattern)
            .map_err(|error| RewriteApplyError::InvalidPattern(error.to_string()))?;
        let Some(captures) = regex.captures(&uri) else {
            continue;
        };
        let mut replacement = rule.replacement.clone();
        for index in 1..captures.len() {
            let value = captures.get(index).map(|m| m.as_str()).unwrap_or("");
            replacement = replacement.replace(&format!("${index}"), value);
            replacement = replacement.replace(&format!("\\{index}"), value);
        }
        match rule.flag {
            RewriteFlag::Redirect => {
                return Ok(RewriteOutcome::Redirect {
                    status: 302,
                    location: replacement,
                });
            }
            RewriteFlag::Permanent => {
                return Ok(RewriteOutcome::Redirect {
                    status: 301,
                    location: replacement,
                });
            }
            RewriteFlag::Break => {
                let (next_path, next_query) = split_uri(&replacement);
                return Ok(RewriteOutcome::Continue {
                    path: next_path,
                    query: next_query,
                });
            }
            RewriteFlag::Last => {
                let (next_path, next_query) = split_uri(&replacement);
                return Ok(RewriteOutcome::Reselect {
                    path: next_path,
                    query: next_query,
                });
            }
        }
    }
    let (next_path, next_query) = split_uri(&uri);
    Ok(RewriteOutcome::Continue {
        path: next_path,
        query: next_query,
    })
}

fn split_uri(uri: &str) -> (String, Option<String>) {
    match uri.split_once('?') {
        Some((path, query)) => (path.to_owned(), Some(query.to_owned())),
        None => (uri.to_owned(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rewrite_with_flag() {
        let rule = parse_rewrite(r"^/old/(.*)$ /new/$1 last").unwrap();
        assert_eq!(rule.pattern, r"^/old/(.*)$");
        assert_eq!(rule.replacement, "/new/$1");
        assert_eq!(rule.flag, RewriteFlag::Last);
    }

    #[test]
    fn applies_capture_replacement_and_break() {
        let rule = parse_rewrite(r"^/api/(.*)$ /v2/$1 break").unwrap();
        let outcome = apply_rewrites("/api/users", None, &[rule]).unwrap();
        assert_eq!(
            outcome,
            RewriteOutcome::Continue {
                path: "/v2/users".to_owned(),
                query: None,
            }
        );
    }

    #[test]
    fn redirect_flag_returns_302() {
        let rule = parse_rewrite(r"^/gone$ https://example.com/ permanent").unwrap();
        let outcome = apply_rewrites("/gone", None, &[rule]).unwrap();
        assert_eq!(
            outcome,
            RewriteOutcome::Redirect {
                status: 301,
                location: "https://example.com/".to_owned(),
            }
        );
    }
}
