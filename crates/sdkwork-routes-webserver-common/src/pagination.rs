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
/// their OpenAPI contract. `cursor` on any other endpoint fails closed. The
/// patterns MUST stay in lockstep with the OpenAPI authorities
/// (`apis/*openapi.yaml` `x-sdkwork-pagination-mode: cursor`); the unit tests
/// below pin the exact route constants from the backend-api path modules to
/// fail fast on route renames. The retired `/app/v3/api` surface is owned by
/// sdkwork-deployments and never reaches this middleware, so it has no
/// entries here.
const CURSOR_PAGINATED_PATH_PATTERNS: [&str; 10] = [
    "/backend/v3/api/audit_logs",
    "/backend/v3/api/applications/{applicationId}/deployments",
    "/backend/v3/api/applications/{applicationId}/source_versions",
    "/backend/v3/api/certificates",
    "/backend/v3/api/servers",
    "/backend/v3/api/clusters/hosts",
    "/backend/v3/api/clusters/instances",
    "/backend/v3/api/clusters/events",
    "/backend/v3/api/clusters/instances/{instanceId}/heartbeats",
    "/backend/v3/api/clusters/instances/{instanceId}/metrics/history",
];

/// Reject malformed or non-canonical pagination query parameters before handlers run.
pub async fn validate_pagination_query(request: Request, next: Next) -> Response {
    if let Err(detail) = validate_query(request.uri().query(), request.uri().path()) {
        // `API_SPEC.md` §14.1 and `PAGINATION_SPEC.md` §10.1 mandate
        // `40003 INVALID_PARAMETER` for page_size overflow, forbidden
        // aliases, and cursor/page combination rejections.
        return WebApiError::new(SdkWorkResultCode::InvalidParameter, detail).into_response();
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
                // Handler query structs bind `page` as i32; a value beyond the
                // i32 range would pass this middleware and then fail serde
                // binding as a plain-text extractor rejection instead of a
                // normalized problem. Reject it here with the same wording.
                if parsed > i32::MAX as i64 {
                    return Err("page must be an integer greater than or equal to 1".to_string());
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
    use super::{validate_query, CURSOR_PAGINATED_PATH_PATTERNS};

    #[test]
    fn cursor_patterns_match_the_openapi_authority() {
        // The documented invariant above is enforced here against the source
        // of truth itself: every operation the backend authority declares
        // with `x-sdkwork-pagination-mode: cursor` must have its path in the
        // middleware allowlist, and the allowlist must contain nothing the
        // authority does not declare. A new keyset list cannot ship without
        // its cursor becoming reachable on the wire (the missing-certificates
        // regression class).
        let authority = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apis/backend-api/web/openapi.yaml");
        let source = std::fs::read_to_string(&authority)
            .unwrap_or_else(|error| panic!("read {}: {error}", authority.display()));
        let mut declared: Vec<String> = Vec::new();
        let mut current_path: Option<String> = None;
        for line in source.lines() {
            if let Some(rest) = line.strip_prefix("  /") {
                if let Some(path) = rest.strip_suffix(':') {
                    current_path = Some(format!("/{path}"));
                }
            } else if line.trim() == "x-sdkwork-pagination-mode: cursor" {
                let path = current_path
                    .clone()
                    .unwrap_or_else(|| "pagination mode declared outside any path".to_string());
                declared.push(path);
            }
        }
        assert!(
            !declared.is_empty(),
            "the OpenAPI authority stopped declaring cursor pagination; \
             the middleware allowlist and this test need revisiting"
        );
        let mut expected: Vec<String> = CURSOR_PAGINATED_PATH_PATTERNS
            .iter()
            .map(|pattern| pattern.to_string())
            .collect();
        declared.sort();
        expected.sort();
        assert_eq!(
            declared, expected,
            "cursor allowlist and the OpenAPI authority drifted apart"
        );
    }

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
        // The per-issuance-growing certificate ledger is keyset-paginated:
        // `cursor` is the only continuation and `page` beyond one is refused
        // here exactly like the repository refuses deep offsets.
        assert!(
            validate_query(Some("cursor=opaque-token"), "/backend/v3/api/certificates").is_ok()
        );
        assert!(validate_query(Some("page=2"), "/backend/v3/api/certificates").is_err());
        // Even the first offset page is requested without `page` on a
        // keyset endpoint: the parameter is refused outright.
        assert!(validate_query(Some("page=1"), "/backend/v3/api/certificates").is_err());
        // The retired `/app/v3/api` surface is owned by sdkwork-deployments
        // and never reaches this middleware; cursor on such a path fails
        // closed here.
        assert!(validate_query(
            Some("cursor=opaque-token"),
            "/app/v3/api/applications/app-1/source_versions"
        )
        .is_err());
        assert!(validate_query(
            Some("cursor=opaque-token"),
            "/app/v3/api/applications/app-1/deployments"
        )
        .is_err());
        assert!(validate_query(
            Some("cursor=opaque-token"),
            "/backend/v3/api/applications/app-1/source_versions"
        )
        .is_ok());
        assert!(validate_query(Some("cursor=opaque-token"), "/backend/v3/api/sites").is_err());
        // The per-instance metric history is a cursor-paginated growing
        // time-series: `limit` stays a forbidden alias, `page_size`/`cursor`
        // are the only accepted shape, exactly like the heartbeat list.
        assert!(validate_query(
            Some("limit=100"),
            "/backend/v3/api/clusters/instances/i-1/metrics/history"
        )
        .is_err());
        assert!(validate_query(
            Some("page_size=20&cursor=opaque-token"),
            "/backend/v3/api/clusters/instances/i-1/metrics/history"
        )
        .is_ok());
    }
}
