//! Shared nginx `proxy_set_header` validation (component boundary for
//! TOML materialize and nginx.conf materialize).

/// Variables the Rust data plane expands when applying `proxy_set_header`.
pub const SUPPORTED_PROXY_HEADER_VARS: &[&str] = &[
    "host",
    "scheme",
    "remote_addr",
    "proxy_add_x_forwarded_for",
    "http_upgrade",
    "server_port",
];

/// Validate one `"Name value"` entry. Unknown `$…` variables fail closed.
pub fn validate_proxy_set_header_entry(entry: &str) -> Result<(), String> {
    let trimmed = entry.trim();
    let (name, value) = trimmed
        .split_once(char::is_whitespace)
        .map(|(n, v)| (n.trim(), v.trim()))
        .unwrap_or((trimmed, ""));
    if name.is_empty() {
        return Err("proxy_set_header entry is missing a header name".to_owned());
    }
    if name
        .bytes()
        .any(|b| !(b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
    {
        return Err(format!(
            "proxy_set_header name `{name}` is not a valid HTTP header token"
        ));
    }
    let mut rest = value;
    while let Some(start) = rest.find('$') {
        let after = &rest[start + 1..];
        if after.starts_with('$') {
            rest = &after[1..];
            continue;
        }
        let end = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        let var = &after[..end];
        if var.is_empty() {
            return Err("proxy_set_header contains a bare `$`".to_owned());
        }
        if !SUPPORTED_PROXY_HEADER_VARS.contains(&var) {
            return Err(format!(
                "proxy_set_header variable `${var}` is not supported by the runtime; use $host/$scheme/$remote_addr/$proxy_add_x_forwarded_for/$http_upgrade/$server_port or a literal"
            ));
        }
        rest = &after[end..];
    }
    Ok(())
}

/// Format a nginx directive's args into a single `"Name value"` string.
pub fn format_proxy_set_header_entry(args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("proxy_set_header requires a header name".to_owned());
    }
    let name = args[0].trim();
    if name.is_empty() {
        return Err("proxy_set_header requires a header name".to_owned());
    }
    let value = args[1..].join(" ");
    let entry = if value.is_empty() {
        name.to_owned()
    } else {
        format!("{name} {value}")
    };
    validate_proxy_set_header_entry(&entry)?;
    Ok(entry)
}

/// Merge server-level then location-level `proxy_set_header` entries.
/// Later entries with the same header name replace earlier ones (nginx).
pub fn merge_proxy_set_headers(server: &[String], location: &[String]) -> Vec<String> {
    let mut merged: Vec<(String, String)> = Vec::new();
    for entry in server.iter().chain(location.iter()) {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (name, value) = trimmed
            .split_once(char::is_whitespace)
            .map(|(n, v)| (n.trim().to_owned(), v.trim().to_owned()))
            .unwrap_or((trimmed.to_owned(), String::new()));
        let lower = name.to_ascii_lowercase();
        if let Some(existing) = merged
            .iter_mut()
            .find(|(existing_name, _)| existing_name.eq_ignore_ascii_case(&lower))
        {
            *existing = (name, value);
        } else {
            merged.push((name, value));
        }
    }
    merged
        .into_iter()
        .map(|(name, value)| {
            if value.is_empty() {
                name
            } else {
                format!("{name} {value}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_supported_variables() {
        validate_proxy_set_header_entry("Host $host").expect("host");
        validate_proxy_set_header_entry("X-Forwarded-For $proxy_add_x_forwarded_for").expect("xff");
        validate_proxy_set_header_entry("Host $host:$server_port").expect("server_port");
    }

    #[test]
    fn rejects_unknown_variables() {
        assert!(validate_proxy_set_header_entry("X-Uri $request_uri").is_err());
    }

    #[test]
    fn merges_by_header_name() {
        let merged = merge_proxy_set_headers(
            &["Host $host".to_owned(), "X-Real-IP $remote_addr".to_owned()],
            &["Host example.com".to_owned()],
        );
        assert_eq!(
            merged,
            vec![
                "Host example.com".to_owned(),
                "X-Real-IP $remote_addr".to_owned()
            ]
        );
    }
}
