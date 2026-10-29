//! nginx `server_name` forms and their host-matching semantics.
//!
//! nginx classifies every `server_name` value at configuration time and
//! searches the resulting tables in a fixed order (`ngx_hash_find_combined`,
//! `src/core/ngx_hash.c`):
//!
//! 1. exact names;
//! 2. names starting with an asterisk (`*.example.com`) — the longest
//!    matching suffix wins and the wildcard matches subdomains of any depth;
//! 3. names ending with an asterisk (`www.example.*`) — the longest matching
//!    prefix wins and the wildcard matches any suffix below that prefix;
//! 4. regular expressions (`~pattern`) in authored order;
//! 5. the listener's default server.
//!
//! The leading-dot spelling `.example.com` is nginx's shorthand for "the bare
//! domain *and* any subdomain" (`ngx_hash_add_key`: a dot-prefixed name claims
//! the exact name and is stored in the wildcard-head table), so it is expanded
//! into both an exact and a leading-wildcard claim.
//!
//! All of the above was verified against the pinned nginx 1.29.6 oracle, not
//! recalled from documentation:
//!
//! * host values are case-insensitive and the port is ignored; one trailing
//!   dot is stripped, a second one is a `400` before selection runs;
//! * `*.example.com` matches `a.b.example.com`, not `example.com`;
//! * `.example.com` matches both `example.com` and `a.b.example.com`;
//! * `www.example.*` matches `www.example.com` and `www.example.co.uk`, not
//!   `xwww.example.com` and not the bare `example.com`;
//! * with `*.example.com`, `*.sub.example.com` and `*.deep.sub.example.com`
//!   present, `x.deep.sub.example.com` selects the longest suffix;
//! * a trailing wildcard outranks a regex that also matches, and any wildcard
//!   outranks a regex; exact outranks the leading wildcard which outranks the
//!   trailing wildcard;
//! * the request host is lower-cased before a regex is evaluated, and nginx
//!   forces case-insensitive matching when the authored pattern contains an
//!   ASCII uppercase letter (`ngx_http_core_server_name`).
//!
//! Server names are carried inside [`VirtualHostConfig::server_names`] as the
//! authored strings; `~`-prefixed entries are the regex form. Keeping one list
//! preserves the authored order nginx uses for regex evaluation and keeps the
//! typed configuration surface backward compatible.
//!
//! [`VirtualHostConfig::server_names`]: crate::config::VirtualHostConfig::server_names

use std::net::IpAddr;

/// One parsed nginx `server_name` value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerNameForm {
    /// Exact DNS name or IP literal.
    Exact(String),
    /// `*.suffix`, matching subdomains of any depth.
    LeadingWildcard(String),
    /// `prefix.*`, matching any suffix below `prefix`.
    TrailingWildcard(String),
    /// `.suffix` shorthand for the exact name plus `*.suffix`.
    DotPrefix(String),
    /// `~pattern` regular expression, evaluated in authored order.
    Regex(ServerNameRegex),
}

/// A `~pattern` server name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerNameRegex {
    /// The PCRE pattern as authored (without the `~` marker).
    pub pattern: String,
    /// Case-insensitive matching. nginx forces this when the authored pattern
    /// contains an ASCII uppercase letter; an explicit `~*` prefix is also
    /// accepted as an SDKWork convenience spelling.
    pub case_insensitive: bool,
}

impl ServerNameRegex {
    /// The pattern in the form handed to the bounded regex compiler: an
    /// inline `(?i)` flag is added for case-insensitive matching because the
    /// runtime matches against the lower-cased request host exactly like
    /// nginx.
    pub fn compiled_pattern(&self) -> String {
        if self.case_insensitive {
            format!("(?i){}", self.pattern)
        } else {
            self.pattern.clone()
        }
    }
}

impl ServerNameForm {
    /// The canonical lower-case spelling of a DNS form, `None` for a regex.
    pub fn canonical(&self) -> Option<String> {
        match self {
            Self::Exact(name) => Some(name.clone()),
            Self::LeadingWildcard(suffix) => Some(format!("*.{suffix}")),
            Self::TrailingWildcard(prefix) => Some(format!("{prefix}.*")),
            Self::DotPrefix(suffix) => Some(format!(".{suffix}")),
            Self::Regex(_) => None,
        }
    }

    /// Every name table this form claims. A dot-prefixed name claims both the
    /// exact name and the leading wildcard, which is why it can conflict with
    /// either spelling.
    pub fn claims(&self) -> Vec<ServerNameClaim> {
        match self {
            Self::Exact(name) => vec![ServerNameClaim::Exact(name.clone())],
            Self::LeadingWildcard(suffix) => {
                vec![ServerNameClaim::LeadingWildcard(suffix.clone())]
            }
            Self::TrailingWildcard(prefix) => {
                vec![ServerNameClaim::TrailingWildcard(prefix.clone())]
            }
            Self::DotPrefix(suffix) => vec![
                ServerNameClaim::Exact(suffix.clone()),
                ServerNameClaim::LeadingWildcard(suffix.clone()),
            ],
            Self::Regex(_) => Vec::new(),
        }
    }
}

/// One entry in one of nginx's server-name tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerNameClaim {
    Exact(String),
    LeadingWildcard(String),
    TrailingWildcard(String),
}

/// Whether an authored `server_name` value is the regex form.
pub fn server_name_is_regex(value: &str) -> bool {
    value.trim().starts_with('~')
}

/// Parse one authored nginx `server_name` value.
///
/// Returns `None` for values nginx itself rejects (or that this profile fails
/// closed on): wildcards without a dot border, embedded asterisks, empty
/// labels, invalid DNS labels, empty regexes, and `@`-prefixed values (an `@`
/// name is a named-location reference, never a virtual-host name).
pub fn parse_server_name(value: &str) -> Option<ServerNameForm> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(rest) = value.strip_prefix('~') {
        let (pattern, explicit_insensitive) = match rest.strip_prefix('*') {
            Some(pattern) => (pattern, true),
            None => (rest, false),
        };
        if pattern.is_empty() {
            return None;
        }
        // nginx: a pattern containing an ASCII uppercase letter is compiled
        // case-insensitively because the request host is lower-cased before
        // matching (`ngx_http_core_server_name`).
        let case_insensitive =
            explicit_insensitive || pattern.bytes().any(|byte| byte.is_ascii_uppercase());
        return Some(ServerNameForm::Regex(ServerNameRegex {
            pattern: pattern.to_owned(),
            case_insensitive,
        }));
    }
    if value.starts_with('@') {
        return None;
    }
    // nginx lower-cases every non-regex server name at configuration time.
    let lowered = value.trim_end_matches('.').to_ascii_lowercase();
    if lowered.is_empty() {
        return None;
    }
    if let Some(suffix) = lowered.strip_prefix("*.") {
        return dns_suffix(suffix).map(ServerNameForm::LeadingWildcard);
    }
    if let Some(prefix) = lowered.strip_suffix(".*") {
        return dns_suffix(prefix).map(ServerNameForm::TrailingWildcard);
    }
    if let Some(suffix) = lowered.strip_prefix('.') {
        return dns_suffix(suffix).map(ServerNameForm::DotPrefix);
    }
    if lowered.parse::<IpAddr>().is_ok() {
        return Some(ServerNameForm::Exact(lowered));
    }
    dns_suffix(&lowered).map(ServerNameForm::Exact)
}

/// Canonical lower-case spelling of a non-regex server name.
///
/// This is the permissive HTTP virtual-host normalization; certificate and
/// website host names keep using [`super::normalize_server_name`], which stays
/// restricted to exact and leading-wildcard names because that is what the
/// TLS certificate index can select.
pub fn normalize_virtual_host_server_name(value: &str) -> Option<String> {
    parse_server_name(value)?.canonical()
}

fn dns_suffix(value: &str) -> Option<String> {
    if value.is_empty() || value.len() > 253 || value.contains('*') {
        return None;
    }
    if value.parse::<IpAddr>().is_ok() {
        return None;
    }
    if value.split('.').all(valid_dns_label) {
        Some(value.to_owned())
    } else {
        None
    }
}

fn valid_dns_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_nginx_server_name_form() {
        assert_eq!(
            parse_server_name("Example.COM"),
            Some(ServerNameForm::Exact("example.com".to_owned()))
        );
        assert_eq!(
            parse_server_name("*.Example.Com"),
            Some(ServerNameForm::LeadingWildcard("example.com".to_owned()))
        );
        assert_eq!(
            parse_server_name("www.Example.*"),
            Some(ServerNameForm::TrailingWildcard("www.example".to_owned()))
        );
        assert_eq!(
            parse_server_name(".Example.Com"),
            Some(ServerNameForm::DotPrefix("example.com".to_owned()))
        );
        assert_eq!(
            parse_server_name("192.0.2.10"),
            Some(ServerNameForm::Exact("192.0.2.10".to_owned()))
        );
    }

    #[test]
    fn regex_case_folding_follows_nginx() {
        // nginx compiles `~` names case-insensitively when the authored
        // pattern contains an ASCII uppercase letter.
        let mixed = parse_server_name(r"~^WWW\.Example\.com$").expect("mixed-case regex");
        let ServerNameForm::Regex(mixed) = mixed else {
            panic!("regex");
        };
        assert!(mixed.case_insensitive);
        assert_eq!(mixed.compiled_pattern(), r"(?i)^WWW\.Example\.com$");

        let lower = parse_server_name(r"~^www\.example\.com$").expect("lowercase regex");
        let ServerNameForm::Regex(lower) = lower else {
            panic!("regex");
        };
        assert!(!lower.case_insensitive);

        // `~*` is accepted as an explicit case-insensitive spelling.
        let explicit = parse_server_name(r"~*^www\.example\.com$").expect("explicit icase");
        let ServerNameForm::Regex(explicit) = explicit else {
            panic!("regex");
        };
        assert!(explicit.case_insensitive);
        assert_eq!(explicit.pattern, r"^www\.example\.com$");
    }

    #[test]
    fn rejects_forms_nginx_rejects_or_the_profile_fails_closed_on() {
        for invalid in [
            "",
            ".",
            "..",
            "*.",
            "*.example..com",
            "*example.com",
            "www*.example.com",
            "www.example*",
            "www.*.example.com",
            "*.a_b.example",
            "@fallback",
            "~",
            "~*",
        ] {
            assert_eq!(parse_server_name(invalid), None, "{invalid:?}");
        }
    }

    #[test]
    fn dot_prefix_claims_the_exact_and_wildcard_table_entries() {
        let form = parse_server_name(".example.com").expect("dot prefix");
        assert_eq!(
            form.claims(),
            vec![
                ServerNameClaim::Exact("example.com".to_owned()),
                ServerNameClaim::LeadingWildcard("example.com".to_owned()),
            ]
        );
        assert_eq!(
            normalize_virtual_host_server_name(".Example.COM."),
            Some(".example.com".to_owned())
        );
    }
}
