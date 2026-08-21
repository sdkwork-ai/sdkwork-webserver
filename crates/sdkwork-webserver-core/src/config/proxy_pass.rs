//! Variable `proxy_pass` target templates (pure validation and expansion).
//!
//! nginx allows `proxy_pass http://$host;` style targets whose host is
//! resolved per request. The runtime supports a bounded variable subset:
//! `$host`, `$server_port`, `$uri`, `$request_uri`, and `$http_<name>` for
//! any request header name. Anything else fails closed at materialization.

use std::collections::HashMap;

/// Validate a dynamic `proxy_pass` template at materialization.
pub fn validate_proxy_pass_template(template: &str) -> Result<(), String> {
    let mut remainder = template;
    while let Some(index) = remainder.find('$') {
        let rest = &remainder[index..];
        if rest.starts_with("$$") {
            remainder = &rest[2..];
            continue;
        }
        let Some(end) = rest[1..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .map(|offset| offset + 1)
            .or_else(|| {
                if rest[1..].is_empty() {
                    None
                } else {
                    Some(rest.len())
                }
            })
        else {
            return Err(format!("proxy_pass template `{template}` ends with a bare `$`"));
        };
        let variable = &rest[..end];
        if variable == "$host" || variable == "$server_port" || variable == "$uri" || variable == "$request_uri" {
            remainder = &rest[end..];
            continue;
        }
        if let Some(name) = variable.strip_prefix("$http_") {
            if name.is_empty() {
                return Err(format!("proxy_pass variable `{variable}` has an empty header name"));
            }
            remainder = &rest[end..];
            continue;
        }
        return Err(format!(
            "proxy_pass variable `{variable}` is not supported; use $host/$server_port/$uri/$request_uri/$http_<name> or a literal"
        ));
    }
    Ok(())
}

/// Expand a dynamic `proxy_pass` template into a full URL.
pub fn expand_proxy_pass_template(
    template: &str,
    host: &str,
    server_port: u16,
    uri_path: &str,
    request_uri: &str,
    headers: &HashMap<String, String>,
) -> Result<String, ()> {
    let mut output = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            output.push(bytes[index] as char);
            index += 1;
            continue;
        }
        index += 1;
        if index < bytes.len() && bytes[index] == b'$' {
            output.push('$');
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_') {
            index += 1;
        }
        let variable = std::str::from_utf8(&bytes[start..index]).map_err(|_| ())?;
        match variable {
            "host" => output.push_str(host),
            "server_port" => output.push_str(&server_port.to_string()),
            "uri" => output.push_str(uri_path),
            "request_uri" => output.push_str(request_uri),
            _ => {
                let header = variable.strip_prefix("http_").ok_or(())?;
                // nginx `$http_<name>`: `_` stands for `-` in header names.
                let header_name = header.replace('_', "-");
                output.push_str(
                    headers
                        .get(&header_name)
                        .map(String::as_str)
                        .unwrap_or_default(),
                );
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers() -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("x-forwarded-host".to_owned(), "edge.example.com".to_owned());
        map
    }

    #[test]
    fn validates_supported_templates() {
        assert!(validate_proxy_pass_template("http://$host").is_ok());
        assert!(validate_proxy_pass_template("http://$host:$server_port").is_ok());
        assert!(validate_proxy_pass_template("http://$http_x_forwarded_host$request_uri").is_ok());
        assert!(validate_proxy_pass_template("https://fixed.example.com$uri").is_ok());
    }

    #[test]
    fn rejects_unsupported_variables() {
        assert!(validate_proxy_pass_template("http://$request_uri").is_err() == false
            || validate_proxy_pass_template("http://$request_uri").is_ok());
        assert!(validate_proxy_pass_template("http://$remote_addr").is_err());
        assert!(validate_proxy_pass_template("http://$scheme").is_err());
        assert!(validate_proxy_pass_template("http://$").is_err());
    }

    #[test]
    fn expands_host_port_uri_and_headers() {
        let template = "http://$host:$server_port$request_uri";
        let expanded = expand_proxy_pass_template(
            template,
            "api.internal",
            8080,
            "/path",
            "/path?q=1",
            &headers(),
        )
        .expect("expand");
        assert_eq!(expanded, "http://api.internal:8080/path?q=1");

        let template = "http://$http_x_forwarded_host$uri";
        let expanded = expand_proxy_pass_template(template, "ignored", 80, "/p", "/p", &headers())
            .expect("expand");
        assert_eq!(expanded, "http://edge.example.com/p");
    }

    #[test]
    fn missing_header_expands_to_empty() {
        let template = "http://$http_missing$uri";
        let expanded = expand_proxy_pass_template(template, "h", 80, "/p", "/p", &headers())
            .expect("expand");
        assert_eq!(expanded, "http:///p");
    }
}
