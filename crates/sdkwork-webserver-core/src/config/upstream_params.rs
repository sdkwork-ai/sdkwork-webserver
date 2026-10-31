//! Validation for upstream-protocol parameter templates (`fastcgi_param`,
//! `uwsgi_param`, `scgi_param`).
//!
//! The nginx import plane and the typed configuration plane share one
//! vocabulary so a parameter value that can be materialized can also be
//! executed: dedicated CGI variables, the per-request header/argument/cookie
//! families, the FastCGI path variables, and declared nginx `map` results.
//! Anything else fails closed at configuration load with a located diagnostic
//! instead of reaching the upstream as a literal `$name`.

/// Variables the FastCGI/SCGI/uWSGI parameter expander resolves. The
/// `$http_<name>`, `$arg_<name>` and `$cookie_<name>` families are accepted by
/// prefix.
pub const UPSTREAM_PARAM_VARIABLES: &[&str] = &[
    "host",
    "scheme",
    "remote_addr",
    "server_port",
    "request_method",
    "server_name",
    "server_protocol",
    "request_uri",
    "uri",
    "document_uri",
    "args",
    "query_string",
    "is_args",
    "content_type",
    "content_length",
    "document_root",
    "request_filename",
    "fastcgi_script_name",
    "fastcgi_path_info",
    "https",
];

/// Variable families accepted by prefix.
pub const UPSTREAM_PARAM_PREFIXES: &[&str] = &["http_", "arg_", "cookie_"];

/// Maximum parameter name length.
pub const MAX_PARAM_NAME_BYTES: usize = 128;
/// Maximum parameter value template length.
pub const MAX_PARAM_VALUE_BYTES: usize = 8192;

/// Validate one FastCGI/SCGI/uWSGI parameter name.
///
/// nginx parameter names are conventional uppercase CGI names; the bounded
/// subset only allows ASCII alphanumerics and `_` because those are the names
/// the wire encoders accept.
pub fn validate_param_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("upstream parameter name must not be empty".to_owned());
    }
    if name.len() > MAX_PARAM_NAME_BYTES {
        return Err(format!(
            "upstream parameter name `{name}` exceeds the {MAX_PARAM_NAME_BYTES}-byte bound"
        ));
    }
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(format!(
            "upstream parameter name `{name}` may contain only ASCII alphanumerics and `_`"
        ));
    }
    Ok(())
}

/// Validate one parameter value template.
///
/// `map_variables` are the declared nginx `map` result names (without `$`).
/// `$$` is a literal `$`.
pub fn validate_param_value(value: &str, map_variables: &[&str]) -> Result<(), String> {
    if value.len() > MAX_PARAM_VALUE_BYTES {
        return Err(format!(
            "upstream parameter value exceeds the {MAX_PARAM_VALUE_BYTES}-byte bound"
        ));
    }
    if value
        .bytes()
        .any(|byte| byte == 0 || byte == b'\r' || byte == b'\n' || byte == 0x7f)
    {
        return Err("upstream parameter value must not contain control characters".to_owned());
    }
    let mut remainder = value;
    while let Some(index) = remainder.find('$') {
        let rest = &remainder[index + 1..];
        if let Some(escaped) = rest.strip_prefix('$') {
            remainder = escaped;
            continue;
        }
        let end = rest
            .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .unwrap_or(rest.len());
        if end == 0 {
            return Err("upstream parameter value contains a bare `$`".to_owned());
        }
        let name = &rest[..end];
        if UPSTREAM_PARAM_VARIABLES.contains(&name) || map_variables.contains(&name) {
            remainder = &rest[end..];
            continue;
        }
        if UPSTREAM_PARAM_PREFIXES.iter().any(|prefix| {
            name.strip_prefix(prefix)
                .is_some_and(|rest| !rest.is_empty())
        }) {
            remainder = &rest[end..];
            continue;
        }
        return Err(format!(
            "upstream parameter variable `${name}` is not executable; supported: ${} and $http_<name>/$arg_<name>/$cookie_<name> or a declared map result",
            UPSTREAM_PARAM_VARIABLES.join(" $")
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_supported_vocabulary() {
        for value in [
            "$document_root$fastcgi_script_name",
            "$request_uri",
            "$http_cookie",
            "$arg_page",
            "$cookie_session",
            "literal",
            "a$$b",
        ] {
            validate_param_value(value, &[]).expect(value);
        }
        validate_param_value("$mapped_result", &["mapped_result"]).expect("map result");
    }

    #[test]
    fn rejects_unknown_variables_and_bare_control_characters() {
        assert!(validate_param_value("$upstream_addr", &[]).is_err());
        assert!(validate_param_value("a$", &[]).is_err());
        assert!(validate_param_value("a\rb", &[]).is_err());
        assert!(validate_param_name("BAD-NAME").is_err());
        assert!(validate_param_name("").is_err());
        assert!(validate_param_name("GOOD_NAME_1").is_ok());
    }
}
