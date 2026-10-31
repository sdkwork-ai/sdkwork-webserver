pub const API_PREFIX: &str = "/internal/v3/api";

pub fn custom_path(path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }

    let normalized_prefix = normalize_prefix(API_PREFIX);
    let normalized_path = normalize_path(path);

    if normalized_prefix.is_empty() {
        return normalized_path;
    }
    if normalized_path == normalized_prefix
        || normalized_path.starts_with(&(normalized_prefix.clone() + "/"))
    {
        return normalized_path;
    }

    format!("{}{}", normalized_prefix, normalized_path)
}

pub fn append_query_string(path: String, raw_query_string: &str) -> String {
    let query = raw_query_string.trim_start_matches('?');
    if query.is_empty() {
        return path;
    }
    if path.contains('?') {
        format!("{}&{}", path, query)
    } else {
        format!("{}?{}", path, query)
    }
}

fn normalize_prefix(prefix: &str) -> String {
    let trimmed = prefix.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return String::new();
    }
    format!("/{}", trimmed.trim_matches('/'))
}

fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return "/".to_string();
    }
    if trimmed.starts_with('/') {
        return trimmed.to_string();
    }
    format!("/{}", trimmed)
}
