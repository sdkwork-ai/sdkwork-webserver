use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::WebApiError;
use sdkwork_utils_rust::SdkWorkResultCode;

const MAXIMUM_PAGE_SIZE: i64 = 200;
const MAXIMUM_CURSOR_BYTES: usize = 512;

/// Path patterns whose list operations declare cursor (keyset) pagination in
/// their OpenAPI contract. `cursor` on any other endpoint fails closed.
const CURSOR_PAGINATED_PATH_PATTERNS: [&str; 7] = [
    "/backend/v3/api/audit_logs",
    "/backend/v3/api/applications/{applicationId}/deployments",
    "/backend/v3/api/applications/{applicationId}/source_versions",
    "/backend/v3/api/servers",
    "/app/v3/api/sites/{siteId}/deployments",
    "/app/v3/api/sites/{siteId}/source_versions",
    "/app/v3/api/audit_logs",
];

/// Reject malformed or non-canonical pagination query parameters before handlers run.
pub async fn validate_pagination_query(request: Request, next: Next) -> Response {
    if let Err(detail) = validate_query(request.uri().query(), request.uri().path()) {
        return WebApiError::new(SdkWorkResultCode::ValidationError, detail).into_response();
    }
    next.run(request).await
}

fn validate_query(query: Option<&str>, path: &str) -> Result<(), String> {
    let Some(query) = query else {
        return Ok(());
    };
    let mut page: Option<String> = None;
    let mut page_size: Option<String> = None;
    let mut cursor = false;
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        match key.as_ref() {
            "page" => {
                if page.replace(value.into_owned()).is_some() {
                    return Err("page must be specified at most once".to_string());
                }
                let parsed = page
                    .as_deref()
                    .unwrap_or_default()
                    .parse::<i64>()
                    .map_err(|_| {
                        "page must be an integer greater than or equal to 1".to_string()
                    })?;
                if parsed < 1 {
                    return Err("page must be greater than or equal to 1".to_string());
                }
            }
            "page_size" => {
                if page_size.replace(value.into_owned()).is_some() {
                    return Err("page_size must be specified at most once".to_string());
                }
                let parsed = page_size
                    .as_deref()
                    .unwrap_or_default()
                    .parse::<i64>()
                    .map_err(|_| "page_size must be an integer between 1 and 200".to_string())?;
                if !(1..=MAXIMUM_PAGE_SIZE).contains(&parsed) {
                    return Err("page_size must be between 1 and 200".to_string());
                }
            }
            "cursor" => {
                if cursor {
                    return Err("cursor must be specified at most once".to_string());
                }
                cursor = true;
                let value = value.into_owned();
                if value.is_empty() || value.len() > MAXIMUM_CURSOR_BYTES {
                    return Err("cursor must contain 1..512 bytes".to_string());
                }
            }
            "pageSize" | "limit" | "page_no" | "pageNo" | "per_page" | "size" => {
                return Err(format!(
                    "{key} is not a supported pagination parameter; use page_size"
                ));
            }
            _ => {}
        }
    }
    if cursor && page.is_some() {
        return Err("page and cursor cannot be combined".to_string());
    }
    if page.is_some() && path_matches_cursor_patterns(path) {
        return Err("page is not supported by this endpoint; use cursor pagination".to_string());
    }
    if cursor && !path_matches_cursor_patterns(path) {
        return Err("cursor pagination is not supported by this endpoint".to_string());
    }
    Ok(())
}

/// Matches the request path against the cursor-paginated operation patterns.
fn path_matches_cursor_patterns(path: &str) -> bool {
    CURSOR_PAGINATED_PATH_PATTERNS.iter().any(|pattern| {
        let segments = pattern.split('/').collect::<Vec<_>>();
        let path_segments = path.split('/').collect::<Vec<_>>();
        segments.len() == path_segments.len()
            && segments
                .iter()
                .zip(path_segments.iter())
                .all(|(pattern, actual)| {
                    pattern.starts_with('{') && pattern.ends_with('}') || pattern == actual
                })
    })
}

#[cfg(test)]
mod tests {
    use super::validate_query;

    #[test]
    fn accepts_canonical_values_and_rejects_aliases() {
        assert!(validate_query(Some("page=2&page_size=20"), "/backend/v3/api/audit_logs").is_err());
        assert!(validate_query(Some("pageSize=20"), "/backend/v3/api/audit_logs").is_err());
        assert!(validate_query(Some("%70ageSize=20"), "/backend/v3/api/audit_logs").is_err());
        assert!(validate_query(Some("page_size=201"), "/backend/v3/api/audit_logs").is_err());
        assert!(validate_query(Some("page=0"), "/backend/v3/api/audit_logs").is_err());
        assert!(validate_query(Some("page=1&page=2"), "/backend/v3/api/audit_logs").is_err());
        assert!(validate_query(
            Some("cursor=opaque-token&page=1"),
            "/backend/v3/api/audit_logs"
        )
        .is_err());
        assert!(validate_query(Some("cursor=opaque-token"), "/backend/v3/api/audit_logs").is_ok());
        assert!(validate_query(Some("page_size=20"), "/backend/v3/api/audit_logs").is_ok());
        assert!(validate_query(Some("cursor="), "/backend/v3/api/audit_logs").is_err());
        assert!(validate_query(
            Some("cursor=opaque-token"),
            "/backend/v3/api/applications/app-1/deployments"
        )
        .is_ok());
        // Cursor-paginated growing collections (nodes, revisions) accept
        // cursor after the keyset upgrade; other lists still fail closed.
        assert!(validate_query(Some("cursor=opaque-token"), "/backend/v3/api/servers").is_ok());
        assert!(validate_query(
            Some("cursor=opaque-token"),
            "/app/v3/api/sites/site-1/source_versions"
        )
        .is_ok());
        assert!(validate_query(
            Some("cursor=opaque-token"),
            "/backend/v3/api/applications/app-1/source_versions"
        )
        .is_ok());
        assert!(validate_query(Some("cursor=opaque-token"), "/backend/v3/api/sites").is_err());
    }
}
