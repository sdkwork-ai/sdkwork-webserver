use super::{EnginePool, EngineRow};
use sdkwork_database_id::{uuid_v4, uuid_v4_with_prefix, SnowflakeIdGenerator};
use sdkwork_utils_rust::crypto::sha256_hash;
use sdkwork_webserver_contract::{WebServiceError, WebServiceResult};
use sqlx::{Error as SqlxError, Row};

pub(crate) fn now_rfc3339() -> String {
    sdkwork_utils_rust::datetime::format_datetime(sdkwork_utils_rust::datetime::now(), None)
}

/// Builds the replacement argument of a `... LIKE $N ESCAPE '\'` contains
/// filter: the caller's keyword becomes a literal substring pattern, with
/// backslash, percent, and underscore escaped so a keyword of `"%"` filters on
/// a literal percent sign instead of matching every row.
pub(crate) fn like_contains_pattern(keyword: &str) -> String {    let escaped = keyword
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

pub(crate) fn store_error(context: &str, error: SqlxError) -> WebServiceError {
    tracing::error!(context, error = ?error, "database operation failed");
    match error {
        SqlxError::Database(db) => match db.code().as_deref() {
            Some("23505") => WebServiceError::conflict("resource already exists"),
            Some("23503" | "23514" | "22P02") => {
                WebServiceError::validation("database constraint rejected the request")
            }
            Some("40001" | "40P01" | "55P03" | "57014") => WebServiceError::DatabaseUnavailable,
            _ => WebServiceError::Internal("database operation failed".to_string()),
        },
        SqlxError::RowNotFound => WebServiceError::not_found("resource not found"),
        SqlxError::PoolTimedOut | SqlxError::PoolClosed | SqlxError::Io(_) | SqlxError::Tls(_) => {
            WebServiceError::DatabaseUnavailable
        }
        _ => WebServiceError::Internal("database operation failed".to_string()),
    }
}

pub(crate) fn is_unique_violation(error: &SqlxError) -> bool {
    matches!(error, SqlxError::Database(database) if database.code().as_deref() == Some("23505"))
}

/// Retry a transaction body whose failure is worth re-running: PostgreSQL
/// serialization failures (`40001`) and deadlock victims (`40P01`) are
/// transient by definition, and lock/statement timeouts (`55P03`/`57014`) can
/// succeed once the competing transaction finishes. `store_error` maps all of
/// these to `DatabaseUnavailable`, which is also what a pool timeout maps to;
/// retrying that is harmless because the attempt is capped.
///
/// Only wrap an operation whose *whole* body is one atomic transaction and that
/// is safe to re-run from the start: the failed attempt rolled back, and any id
/// generated inside it is regenerated. The caller's idempotency key (where the
/// operation has one) makes a replay return the committed result instead of
/// inserting twice.
pub(crate) async fn retry_transient_transaction<T, F, Fut>(
    mut operation: F,
) -> WebServiceResult<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = WebServiceResult<T>>,
{
    const MAX_ATTEMPTS: u32 = 3;
    let mut attempt = 1_u32;
    loop {
        match operation().await {
            Err(WebServiceError::DatabaseUnavailable) if attempt < MAX_ATTEMPTS => {
                // Small exponential backoff plus sub-millisecond jitter so two
                // contending writers do not retry in lockstep.
                let jitter = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| u64::from(elapsed.subsec_nanos() % 7))
                    .unwrap_or(0);
                let backoff_ms = 20 * u64::from(attempt) + jitter;
                tracing::warn!(
                    attempt,
                    backoff_ms,
                    "transient database transaction failure; retrying"
                );
                tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                attempt += 1;
            }
            other => return other,
        }
    }
}

pub(crate) fn pagination(page: i32, page_size: i32) -> Result<(i32, i32, i64), WebServiceError> {
    if page < 1 {
        return Err(WebServiceError::validation(
            "page must be greater than or equal to 1",
        ));
    }
    if !(1..=200).contains(&page_size) {
        return Err(WebServiceError::validation(
            "page_size must be between 1 and 200",
        ));
    }
    let offset = (i64::from(page) - 1)
        .checked_mul(i64::from(page_size))
        .ok_or_else(|| WebServiceError::validation("pagination offset is too large"))?;
    Ok((page, page_size, offset))
}

/// HMAC secret for keyset cursor integrity. A cursor issued by one gateway
/// node must validate on every sibling node, so the secret is
/// deployment-stable: `SDKWORK_WEBSERVER_CURSOR_HMAC_KEY` when the operator
/// provides one, otherwise the authoritative database URL, which every node
/// of a deployment shares by definition. Production-like environments must
/// set the dedicated key (enforced by [`require_cursor_hmac_key_in_production`]
/// at bootstrap), and the fallback chain never produces an empty secret.
fn cursor_hmac_secret() -> &'static str {
    use std::sync::OnceLock;
    static SECRET: OnceLock<String> = OnceLock::new();
    SECRET
        .get_or_init(|| match std::env::var("SDKWORK_WEBSERVER_CURSOR_HMAC_KEY") {
            Ok(seed) if !seed.trim().is_empty() => seed.trim().to_owned(),
            _ => {
                // Deployment-stable fallback so sibling nodes still agree,
                // but the operator must know the key material now overlaps
                // with the database URL: any leak of the URL would also
                // enable cursor forgery. Blocked at bootstrap in production.
                tracing::warn!(
                    "SDKWORK_WEBSERVER_CURSOR_HMAC_KEY is not set; keyset cursors are signed with the database URL. Set a dedicated secret before exposing list APIs to untrusted clients."
                );
                match std::env::var("SDKWORK_DATABASE_URL") {
                    Ok(url) if !url.is_empty() => url,
                    // Never an empty secret: a per-process unpredictable
                    // value beats a forgeable constant when nothing is
                    // configured (cursors then reset across restarts).
                    // `RandomState` seeds from OS randomness per process.
                    _ => {
                        use std::hash::{BuildHasher, Hasher};
                        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
                        hasher.write_u128(
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_nanos())
                                .unwrap_or_default(),
                        );
                        format!("cursor-secret-{}", hasher.finish())
                    }
                }
            }
        })
        .as_str()
}

/// Encodes a keyset cursor bound to the operation and tenant that issued it.
///
/// The scope is part of the signed payload, so a cursor minted for one list
/// (or one tenant) cannot be replayed against another: the decode below returns
/// `None` when the caller's scope differs, and the route layer turns that into
/// a validation error rather than silently continuing another collection.
pub(crate) fn encode_keyset_cursor(scope: &str, created_at: &str, id: i64) -> String {
    use base64::Engine as _;
    let payload = format!("v1|{scope}|{created_at}|{id}");
    let signature = sdkwork_utils_rust::crypto::hmac_sha256(
        payload.as_bytes(),
        cursor_hmac_secret().as_bytes(),
    );
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(format!("{payload}|{signature}").as_bytes())
}

/// Decodes a keyset cursor produced by [`encode_keyset_cursor`] for the given
/// scope; returns `None` for malformed, oversized, unsigned, out-of-range, or
/// scope-mismatched tokens so callers fail closed.
pub(crate) fn decode_keyset_cursor(scope: &str, token: &str) -> Option<(String, i64)> {
    use base64::Engine as _;
    if token.is_empty() || token.len() > 512 || !valid_cursor_scope(scope) {
        return None;
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(token.as_bytes())
        .ok()?;
    let payload = std::str::from_utf8(&decoded).ok()?;
    let mut parts = payload.splitn(5, '|');
    let version = parts.next()?;
    let payload_scope = parts.next()?;
    let created_at = parts.next()?;
    let id = parts.next()?;
    let signature = parts.next()?;
    if version != "v1"
        || payload_scope != scope
        || created_at.is_empty()
        || created_at.len() > 64
    {
        return None;
    }
    // Integrity first: an unsigned or tampered tuple never reaches the
    // query layer (constant-time compare via the shared utils).
    let expected = sdkwork_utils_rust::crypto::hmac_sha256(
        format!("v1|{payload_scope}|{created_at}|{id}").as_bytes(),
        cursor_hmac_secret().as_bytes(),
    );
    if !sdkwork_utils_rust::crypto::secure_compare(signature, &expected) {
        return None;
    }
    let id = id.parse::<i64>().ok()?;
    if id <= 0 {
        return None;
    }
    if chrono::DateTime::parse_from_rfc3339(created_at).is_err() {
        return None;
    }
    Some((created_at.to_string(), id))
}

/// A scope is a bounded ASCII label such as `certificates:410001`; the `|`
/// separator and control bytes are rejected so the signed payload stays
/// unambiguous.
fn valid_cursor_scope(scope: &str) -> bool {
    !scope.is_empty()
        && scope.len() <= 128
        && scope
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b'|')
}

pub(crate) fn next_id(generator: &SnowflakeIdGenerator) -> Result<i64, WebServiceError> {
    generator
        .generate()
        .map_err(|error| WebServiceError::Internal(error.to_string()))
}

pub(crate) fn new_uuid() -> String {
    uuid_v4()
}

pub(crate) fn new_agent_token() -> String {
    uuid_v4_with_prefix("wagent_")
}

pub(crate) fn sha256_hex(content: &str) -> String {
    sha256_hash(content.as_bytes())
}

pub(crate) fn bool_from_row(row: &EngineRow, column: &str) -> Result<bool, SqlxError> {
    if let Ok(value) = row.try_get::<bool, _>(column) {
        return Ok(value);
    }
    let value: i64 = row.try_get(column)?;
    Ok(value != 0)
}

pub(crate) fn json_from_row(
    row: &EngineRow,
    column: &str,
) -> Result<Option<serde_json::Value>, SqlxError> {
    let raw: Option<String> = row.try_get(column)?;
    raw.map(|text| serde_json::from_str(&text).map_err(|error| SqlxError::Decode(Box::new(error))))
        .transpose()
}

/// Reads an optional JSON text column as a JSON object: SQL NULL becomes an
/// empty object, while a non-null value that fails to parse is a decode
/// error. Corrupted stored payloads must surface to the caller instead of
/// silently vanishing into `{}`.
pub(crate) fn optional_json_object_from_row(
    row: &EngineRow,
    column: &str,
) -> Result<serde_json::Value, SqlxError> {
    Ok(json_from_row(row, column)?
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new())))
}

/// PostgreSQL-only JSONB write expression. The repository is instantiated
/// exclusively with the PostgreSQL engine (PRD: PostgreSQL is the only
/// authoritative server database), so no engine branching exists.
pub(crate) fn json_write_expression(placeholder: &str) -> String {
    format!("CAST({placeholder} AS JSONB)")
}

pub(crate) fn instant_write_expression(placeholder: &str) -> String {
    format!("CAST({placeholder} AS TIMESTAMPTZ)")
}

pub(crate) fn instant_from_row(row: &EngineRow, column: &str) -> Result<String, SqlxError> {
    let value: String = row.try_get(column)?;
    normalize_database_instant(&value).ok_or_else(|| invalid_instant_error(column, &value))
}

pub(crate) fn optional_instant_from_row(
    row: &EngineRow,
    column: &str,
) -> Result<Option<String>, SqlxError> {
    let value: Option<String> = row.try_get(column)?;
    value
        .map(|value| {
            normalize_database_instant(&value).ok_or_else(|| invalid_instant_error(column, &value))
        })
        .transpose()
}

/// Reads a keyset page-boundary instant with full database precision.
///
/// The API payload form ([`normalize_database_instant`]) collapses to the
/// canonical millisecond wire format, which is lossy as a page boundary: two
/// rows inside the same millisecond make `(instant, id) < (boundary, id)` skip
/// whichever row has the larger sub-millisecond fraction. PostgreSQL stores
/// microseconds, so the cursor keeps every one of them while staying RFC 3339
/// for [`decode_keyset_cursor`].
pub(crate) fn cursor_instant_from_row(row: &EngineRow, column: &str) -> Result<String, SqlxError> {
    let value: String = row.try_get(column)?;
    parse_database_instant(&value)
        .map(|parsed| {
            parsed
                .with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
        })
        .ok_or_else(|| invalid_instant_error(column, &value))
}

fn normalize_database_instant(value: &str) -> Option<String> {
    parse_database_instant(value).map(|value| {
        sdkwork_utils_rust::datetime::format_datetime(value.with_timezone(&chrono::Utc), None)
    })
}

/// Parses either an RFC 3339 instant or PostgreSQL's `timestamp with time zone`
/// text form (`2026-09-16 10:47:00.123456+00`).
fn parse_database_instant(value: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .or_else(|_| chrono::DateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f%#z"))
        .ok()
}

fn invalid_instant_error(column: &str, value: &str) -> SqlxError {
    SqlxError::ColumnDecode {
        index: column.to_string(),
        source: Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid database instant: {value}"),
        )),
    }
}

pub(crate) async fn resolve_site_internal_id(
    pool: &EnginePool,
    tenant_id: i64,
    site_uuid: &str,
) -> Result<i64, WebServiceError> {
    let row = sqlx::query(
        "SELECT id FROM webserver_site
         WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(site_uuid)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("resolve webserver_site id", error))?;

    let row = row.ok_or_else(|| WebServiceError::not_found("site not found"))?;
    row.try_get("id")
        .map_err(|error| store_error("map webserver_site id", error))
}

pub(crate) async fn resolve_site_owner_id(
    pool: &EnginePool,
    tenant_id: i64,
    site_id: i64,
) -> Result<Option<i64>, WebServiceError> {
    sqlx::query_scalar(
        "SELECT user_id FROM webserver_site
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(site_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("resolve webserver_site owner", error))?
    .ok_or_else(|| WebServiceError::not_found("site not found"))
}

#[cfg(test)]
mod tests {
    use super::{like_contains_pattern, normalize_database_instant, pagination};

    #[test]
    fn escapes_wildcards_and_backslash_in_contains_patterns() {
        assert_eq!(like_contains_pattern("plain"), "%plain%");
        // A bare percent must filter on a literal percent sign, not match
        // every row.
        assert_eq!(like_contains_pattern("%"), "%\\%%");
        assert_eq!(like_contains_pattern("a_b"), "%a\\_b%");
        assert_eq!(like_contains_pattern("a\\b"), "%a\\\\b%");
    }

    #[test]
    fn pagination_rejects_invalid_inputs_and_computes_offset_without_i32_overflow() {
        assert!(pagination(-10, 20).is_err());
        assert!(pagination(1, 0).is_err());
        assert!(pagination(1, 201).is_err());
        assert_eq!(
            pagination(i32::MAX, 200).unwrap(),
            (i32::MAX, 200, 429_496_729_200)
        );
    }

    #[test]
    fn database_instants_are_normalized_to_rfc3339_utc() {
        let expected = "2027-01-01T00:00:00.123Z";
        assert_eq!(
            normalize_database_instant(expected).as_deref(),
            Some(expected)
        );
        assert_eq!(
            normalize_database_instant("2027-01-01 08:00:00.123+08").as_deref(),
            Some(expected)
        );
        assert!(normalize_database_instant("not-an-instant").is_none());
    }

    #[test]
    fn keyset_cursors_round_trip_and_reject_tampering() {
        let scope = "certificates:410001";
        let token = super::encode_keyset_cursor(scope, "2027-01-01T00:00:00Z", 42);
        assert_eq!(
            super::decode_keyset_cursor(scope, &token),
            Some(("2027-01-01T00:00:00Z".to_owned(), 42))
        );
        // A cursor minted for one operation/tenant must not continue another.
        assert!(super::decode_keyset_cursor("deployments:410001", &token).is_none());
        assert!(super::decode_keyset_cursor("certificates:999999", &token).is_none());

        // A legacy unsigned tuple must fail closed.
        let unsigned = {
            use base64::Engine as _;
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(format!("v1|{scope}|2027-01-01T00:00:00Z|42").as_bytes())
        };
        assert!(super::decode_keyset_cursor(scope, &unsigned).is_none());

        // Tampering with any tuple field breaks the HMAC.
        let forged = {
            use base64::Engine as _;
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
                format!("v1|{scope}|2027-01-01T00:00:00Z|43|deadbeef").as_bytes(),
            )
        };
        assert!(super::decode_keyset_cursor(scope, &forged).is_none());
        assert!(super::decode_keyset_cursor(scope, "").is_none());
    }
}

#[cfg(test)]
mod transaction_retry_tests {
    use super::retry_transient_transaction;
    use sdkwork_webserver_contract::{WebServiceError, WebServiceResult};
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };

    #[tokio::test]
    async fn retries_transient_failures_and_returns_the_committed_result() {
        let attempts = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&attempts);
        let result: WebServiceResult<u32> = retry_transient_transaction(move || {
            let counter = Arc::clone(&counter);
            async move {
                let attempt = counter.fetch_add(1, Ordering::SeqCst) + 1;
                if attempt < 3 {
                    Err(WebServiceError::DatabaseUnavailable)
                } else {
                    Ok(attempt)
                }
            }
        })
        .await;
        assert_eq!(result.expect("third attempt commits"), 3);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn non_transient_failures_are_not_retried() {
        let attempts = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&attempts);
        let result: WebServiceResult<()> = retry_transient_transaction(move || {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err(WebServiceError::conflict("slug already exists"))
            }
        })
        .await;
        assert!(matches!(result, Err(WebServiceError::Conflict(_))));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retries_are_bounded() {
        let attempts = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&attempts);
        let result: WebServiceResult<()> = retry_transient_transaction(move || {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err(WebServiceError::DatabaseUnavailable)
            }
        })
        .await;
        assert!(matches!(
            result,
            Err(WebServiceError::DatabaseUnavailable)
        ));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }
}
