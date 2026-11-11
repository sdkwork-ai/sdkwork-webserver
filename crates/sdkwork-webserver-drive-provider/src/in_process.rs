//! In-process Drive transport for the website provider (MODULE_SPEC.md §4.2).
//!
//! The SQLWork Drive module owns the resolution rules, the tenant scoping and
//! the object-store lookup this file needs, so it calls Drive's own services on
//! the host PostgreSQL pool instead of the Drive Internal HTTP API:
//!
//! * `SqlDriveWebsiteRootService::get_root` replaces `website_roots_retrieve`.
//! * `SqlDriveResourceResolutionService::resolve` replaces
//!   `drive_resources_resolve` (same tenant scope, generation pinning,
//!   eligibility predicate and normalized relative path).
//! * `DriveObjectStoreRuntime` + `DriveObjectStore::read_object_range` replace
//!   `drive_resource_content_retrieve`, including the route's byte-range,
//!   conditional-request and integrity rules.
//!
//! Failures are translated to the same HTTP status the Drive Internal API
//! returned for the same condition, so the provider's existing error mapping
//! (and therefore every public HTTP status code) is unchanged.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, SecondsFormat, Utc};
use sdkwork_drive_internal_sdk::{
    models::{DriveResourceResolution, ResolveDriveResourceRequest, WebsiteRoot},
    SdkworkError,
};
use sdkwork_drive_object_runtime::DriveObjectStoreRuntime;
use sdkwork_drive_storage_contract::{
    DriveByteRange, DriveObjectChunkStream, DriveObjectLocator, DriveObjectStoreError,
    DriveObjectStoreErrorKind, HeadObjectRequest, ReadObjectRangeRequest,
};
use sdkwork_drive_workspace_service::{
    application::resource_resolution_service::{
        ResolveDriveResourceCommand, SqlDriveResourceResolutionService,
    },
    application::website_root_service::{GetWebsiteRootCommand, SqlDriveWebsiteRootService},
    domain::resource_resolution::{DriveResourceScopeKind, ResolvedDriveResource},
    infrastructure::sql::resource_resolution_store::SqlResourceResolutionStore,
    infrastructure::sql::website_root_store::SqlWebsiteRootStore,
    DriveServiceError,
};
use sqlx::PgPool;

use crate::sdk::{DriveContentChunkStream, DriveWebsiteSdkClient};

const WEBSITE_ROOT_CAPABILITIES: [&str; 3] =
    ["STATIC_CONTENT", "BYTE_RANGE", "CONDITIONAL_REQUESTS"];

/// Drive Internal API transport implemented by calling Drive in-process on the
/// shared host pool. `tenant_id` comes from the deployment identity bound to the
/// Web Node's tenant scope, so every read keeps the exact tenant scope the
/// internal ingress key carried.
pub struct InProcessDriveWebsiteSdkClient {
    pool: PgPool,
    tenant_id: String,
    object_runtime: DriveObjectStoreRuntime,
}

impl InProcessDriveWebsiteSdkClient {
    pub fn new(pool: PgPool, tenant_id: impl Into<String>) -> Self {
        Self {
            object_runtime: DriveObjectStoreRuntime::new(pool.clone()),
            pool,
            tenant_id: tenant_id.into(),
        }
    }

    async fn resolve(
        &self,
        scope_type: &str,
        scope_uuid: &str,
        relative_path: &str,
        pinned_generation: Option<&str>,
        pinned_node_version_id: Option<&str>,
    ) -> Result<ResolvedDriveResource, SdkworkError> {
        let scope_kind = parse_scope_kind(scope_type)?;
        let pinned_generation = pinned_generation.map(parse_positive_i64).transpose()?;
        SqlDriveResourceResolutionService::new(SqlResourceResolutionStore::new(self.pool.clone()))
            .resolve(ResolveDriveResourceCommand {
                tenant_id: self.tenant_id.clone(),
                scope_kind,
                scope_uuid: scope_uuid.to_owned(),
                relative_path: relative_path.to_owned(),
                pinned_generation,
                pinned_node_version_id: pinned_node_version_id.map(str::to_owned),
            })
            .await
            .map_err(map_drive_service_error)
    }

    /// Mirrors `sdkwork-routes-drive-internal-api/src/content.rs`: resolve the
    /// pinned node version, evaluate the conditional headers, select the byte
    /// range and open the immutable object through the provider runtime.
    async fn open_content(
        &self,
        node_version_id: &str,
        scope_type: &str,
        scope_uuid: &str,
        relative_path: &str,
        pinned_generation: Option<&str>,
        range: Option<&str>,
        if_match: Option<&str>,
        if_none_match: Option<&str>,
        if_range: Option<&str>,
        if_modified_since: Option<&str>,
        if_unmodified_since: Option<&str>,
    ) -> Result<OpenedDriveContent, SdkworkError> {
        let resource = self
            .resolve(
                scope_type,
                scope_uuid,
                relative_path,
                pinned_generation,
                Some(node_version_id),
            )
            .await?;
        if resource.node_version_id != node_version_id {
            return Err(http_status(404, "root-qualified Drive node version was not found"));
        }
        let content_length = u64::try_from(resource.content_length)
            .map_err(|_| http_status(500, "resolved Drive resource content length is invalid"))?;
        let etag = format!("\"{}\"", resource.checksum_sha256_hex);
        let last_modified = parse_timestamp(&resource.last_modified)
            .ok_or_else(|| http_status(500, "resolved Drive resource timestamp is invalid"))?;

        evaluate_preconditions(if_match, if_unmodified_since, &etag, last_modified)?;
        if is_not_modified(
            if_none_match,
            if_modified_since,
            &etag,
            last_modified,
        ) {
            return Err(SdkworkError::HttpStatus {
                status: 304,
                body: String::new(),
            });
        }

        let range_allowed =
            range.is_some() && if_range_allows(if_range, &etag, last_modified);
        let selected_range = if range_allowed {
            match parse_single_range(range.unwrap_or_default(), content_length) {
                Some(range) => Some(range),
                None => {
                    return Err(http_status(
                        416,
                        format!("requested byte range is not satisfiable for {content_length} bytes"),
                    ))
                }
            }
        } else {
            None
        };
        let (start, end) = match selected_range {
            Some(range) => (range.start_inclusive, range.end_inclusive),
            None if content_length > 0 => (0, content_length - 1),
            None => (0, 0),
        };
        let response_length = if content_length == 0 {
            0
        } else {
            end - start + 1
        };
        let object_store = self
            .object_runtime
            .resolve_locator(
                &resource.content_locator.storage_provider_id,
                &resource.content_locator.bucket,
            )
            .await
            .map_err(map_object_store_error)?;
        let locator = DriveObjectLocator {
            bucket: resource.content_locator.bucket,
            object_key: resource.content_locator.object_key,
        };
        let stream: Box<dyn DriveObjectChunkStream> = if content_length == 0 {
            let head = object_store
                .head_object(HeadObjectRequest {
                    locator: locator.clone(),
                })
                .await
                .map_err(map_object_store_error)?;
            if head.content_length != 0 {
                return Err(integrity_failed(
                    "empty Drive resource metadata does not match storage",
                ));
            }
            Box::new(EmptyChunkStream)
        } else {
            let (read, chunks) = object_store
                .read_object_range(ReadObjectRangeRequest {
                    locator,
                    range: DriveByteRange {
                        start_inclusive: start,
                        end_inclusive: end,
                    },
                })
                .await
                .map_err(map_object_store_error)?;
            if read.content_length != response_length {
                return Err(integrity_failed(
                    "Drive content range length does not match committed metadata",
                ));
            }
            chunks
        };
        Ok(OpenedDriveContent {
            stream,
            content_length: response_length,
        })
    }
}

struct OpenedDriveContent {
    stream: Box<dyn DriveObjectChunkStream>,
    content_length: u64,
}

#[async_trait]
impl DriveWebsiteSdkClient for InProcessDriveWebsiteSdkClient {
    async fn retrieve_website_root(
        &self,
        website_root_uuid: &str,
    ) -> Result<WebsiteRoot, SdkworkError> {
        let root = SqlDriveWebsiteRootService::new(SqlWebsiteRootStore::new(self.pool.clone()))
            .get_root(GetWebsiteRootCommand {
                tenant_id: self.tenant_id.clone(),
                root_uuid: website_root_uuid.to_owned(),
            })
            .await
            .map_err(map_drive_service_error)?;
        Ok(WebsiteRoot {
            uuid: root.uuid,
            space_id: root.space_id,
            source_root_mode: root.source_root_mode.as_str().to_ascii_uppercase(),
            content_mode: root.content_mode.as_str().to_ascii_uppercase(),
            active_generation: root.active_generation.to_string(),
            root_status: root.root_status.to_ascii_uppercase(),
            capabilities: WEBSITE_ROOT_CAPABILITIES
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            version: root.version.to_string(),
            updated_at: normalize_timestamp(&root.updated_at),
        })
    }

    async fn resolve_resource(
        &self,
        request: &ResolveDriveResourceRequest,
    ) -> Result<DriveResourceResolution, SdkworkError> {
        let resolved = self
            .resolve(
                &request.scope_type,
                &request.scope_uuid,
                &request.relative_path,
                request.pinned_generation.as_deref(),
                request.pinned_node_version_id.as_deref(),
            )
            .await?;
        map_resolution(&resolved)
    }

    async fn retrieve_content(
        &self,
        node_version_id: &str,
        scope_type: &str,
        scope_uuid: &str,
        relative_path: &str,
        pinned_generation: Option<&str>,
        range: Option<&str>,
        if_match: Option<&str>,
        if_none_match: Option<&str>,
        if_range: Option<&str>,
        if_modified_since: Option<&str>,
        if_unmodified_since: Option<&str>,
    ) -> Result<Vec<u8>, SdkworkError> {
        let mut opened = self
            .open_content(
                node_version_id,
                scope_type,
                scope_uuid,
                relative_path,
                pinned_generation,
                range,
                if_match,
                if_none_match,
                if_range,
                if_modified_since,
                if_unmodified_since,
            )
            .await?;
        let mut bytes = Vec::new();
        while let Some(chunk) = opened.stream.next_chunk().await.map_err(map_object_store_error)? {
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    async fn retrieve_content_stream(
        &self,
        node_version_id: &str,
        scope_type: &str,
        scope_uuid: &str,
        relative_path: &str,
        pinned_generation: Option<&str>,
        range: Option<&str>,
        if_match: Option<&str>,
        if_none_match: Option<&str>,
        if_range: Option<&str>,
        if_modified_since: Option<&str>,
        if_unmodified_since: Option<&str>,
    ) -> Result<Box<dyn DriveContentChunkStream>, SdkworkError> {
        let opened = self
            .open_content(
                node_version_id,
                scope_type,
                scope_uuid,
                relative_path,
                pinned_generation,
                range,
                if_match,
                if_none_match,
                if_range,
                if_modified_since,
                if_unmodified_since,
            )
            .await?;
        Ok(Box::new(BoundedObjectChunkStream {
            source: opened.stream,
            remaining: opened.content_length,
        }))
    }
}

/// Splits an already-materialized `DriveObjectChunkStream` result into transport
/// chunks while enforcing the committed range length, mirroring the route's
/// `stream_body` unfold.
struct BoundedObjectChunkStream {
    source: Box<dyn DriveObjectChunkStream>,
    remaining: u64,
}

#[async_trait]
impl DriveContentChunkStream for BoundedObjectChunkStream {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, SdkworkError> {
        if self.remaining == 0 {
            return Ok(None);
        }
        match self.source.next_chunk().await.map_err(map_object_store_error)? {
            Some(chunk) => {
                let length = chunk.len() as u64;
                if length > self.remaining {
                    return Err(integrity_failed(
                        "Drive content stream exceeded committed range length",
                    ));
                }
                self.remaining -= length;
                Ok(Some(chunk))
            }
            None => Err(http_status(
                500,
                "Drive content stream ended before committed range length",
            )),
        }
    }
}

struct EmptyChunkStream;

#[async_trait]
impl DriveObjectChunkStream for EmptyChunkStream {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, DriveObjectStoreError> {
        Ok(None)
    }
}

fn map_resolution(resolved: &ResolvedDriveResource) -> Result<DriveResourceResolution, SdkworkError> {
    if !resolved.checksum_sha256_hex.starts_with("sha256:") {
        return Err(http_status(500, "resolved Drive resource checksum is invalid"));
    }
    Ok(DriveResourceResolution {
        scope_type: resolved.scope_kind.as_str().to_owned(),
        scope_uuid: resolved.scope_uuid.clone(),
        scope_generation: resolved.scope_generation.to_string(),
        normalized_relative_path: resolved.relative_path.clone(),
        resource_type: resolved.resource_type.clone(),
        node_id: resolved.node_id.clone(),
        logical_node_version_id: resolved.node_version_id.clone(),
        version_no: resolved.version_no.to_string(),
        checksum_sha256_hex: resolved.checksum_sha256_hex.clone(),
        etag: format!("\"{}\"", resolved.checksum_sha256_hex),
        content_type: resolved.content_type.clone(),
        content_length: resolved.content_length.to_string(),
        last_modified: normalize_timestamp(&resolved.last_modified),
        scope_status: resolved.scope_status.clone(),
        node_status: resolved.node_status.clone(),
        eligibility: resolved.eligibility.clone(),
    })
}

fn parse_scope_kind(value: &str) -> Result<DriveResourceScopeKind, SdkworkError> {
    match value {
        "WEBSITE_ROOT" => Ok(DriveResourceScopeKind::WebsiteRoot),
        "ROOT_SCOPE_SUBSCRIPTION" => Ok(DriveResourceScopeKind::RootScopeSubscription),
        _ => Err(http_status(
            400,
            "scopeType must be WEBSITE_ROOT or ROOT_SCOPE_SUBSCRIPTION",
        )),
    }
}

fn parse_positive_i64(value: &str) -> Result<i64, SdkworkError> {
    let parsed = value.parse::<i64>().map_err(|_| {
        http_status(400, "pinnedGeneration must be a positive int64 string")
    })?;
    if parsed < 1 || parsed.to_string() != value {
        return Err(http_status(
            400,
            "pinnedGeneration must be a canonical positive int64 string",
        ));
    }
    Ok(parsed)
}

fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f")
                .ok()
                .map(|value| value.and_utc())
        })
}

fn normalize_timestamp(value: &str) -> String {
    parse_timestamp(value)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true))
        .unwrap_or_else(|| value.to_owned())
}

fn evaluate_preconditions(
    if_match: Option<&str>,
    if_unmodified_since: Option<&str>,
    etag: &str,
    last_modified: DateTime<Utc>,
) -> Result<(), SdkworkError> {
    if let Some(value) = if_match {
        if !etag_list_matches(value, etag, false) {
            return Err(http_status(412, "If-Match validator did not match"));
        }
    } else if let Some(value) = if_unmodified_since {
        if let Ok(date) = httpdate::parse_http_date(value) {
            if last_modified.timestamp() > epoch_seconds(date) {
                return Err(http_status(412, "resource changed after If-Unmodified-Since"));
            }
        }
    }
    Ok(())
}

fn is_not_modified(
    if_none_match: Option<&str>,
    if_modified_since: Option<&str>,
    etag: &str,
    last_modified: DateTime<Utc>,
) -> bool {
    if let Some(value) = if_none_match {
        return etag_list_matches(value, etag, true);
    }
    if_modified_since
        .and_then(|value| httpdate::parse_http_date(value).ok())
        .is_some_and(|date| last_modified.timestamp() <= epoch_seconds(date))
}

fn if_range_allows(value: Option<&str>, etag: &str, last_modified: DateTime<Utc>) -> bool {
    let Some(value) = value else {
        return true;
    };
    let value = value.trim();
    if value.starts_with('"') || value.starts_with("W/") {
        return !value.starts_with("W/") && value == etag;
    }
    httpdate::parse_http_date(value)
        .ok()
        .is_some_and(|date| last_modified.timestamp() <= epoch_seconds(date))
}

fn etag_list_matches(value: &str, etag: &str, weak_match: bool) -> bool {
    value.split(',').any(|candidate| {
        let candidate = candidate.trim();
        if candidate == "*" {
            return true;
        }
        if weak_match {
            candidate.strip_prefix("W/").unwrap_or(candidate)
                == etag.strip_prefix("W/").unwrap_or(etag)
        } else {
            !candidate.starts_with("W/") && candidate == etag
        }
    })
}

fn parse_single_range(value: &str, content_length: u64) -> Option<DriveByteRange> {
    if content_length == 0 || value.contains(',') {
        return None;
    }
    let raw = value.strip_prefix("bytes=")?;
    let (start_raw, end_raw) = raw.split_once('-')?;
    if start_raw.is_empty() {
        let suffix_length = end_raw.parse::<u64>().ok()?;
        if suffix_length == 0 {
            return None;
        }
        let selected_length = suffix_length.min(content_length);
        return Some(DriveByteRange {
            start_inclusive: content_length - selected_length,
            end_inclusive: content_length - 1,
        });
    }
    let start = start_raw.parse::<u64>().ok()?;
    if start >= content_length {
        return None;
    }
    let end = if end_raw.is_empty() {
        content_length - 1
    } else {
        end_raw.parse::<u64>().ok()?.min(content_length - 1)
    };
    if end < start {
        return None;
    }
    Some(DriveByteRange {
        start_inclusive: start,
        end_inclusive: end,
    })
}

fn epoch_seconds(value: std::time::SystemTime) -> i64 {
    value
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(i64::MIN)
}

/// Status mapping of the Drive Internal API route (`error.rs`), so the
/// provider's `map_sdk_error` produces the same `WebsiteProviderErrorKind` the
/// HTTP transport produced.
fn map_drive_service_error(error: DriveServiceError) -> SdkworkError {
    match error {
        DriveServiceError::Validation(detail) => http_status(400, detail),
        DriveServiceError::Conflict(detail) => http_status(409, detail),
        DriveServiceError::NotFound(detail) => http_status(404, detail),
        DriveServiceError::PermissionDenied(detail) => http_status(403, detail),
        DriveServiceError::Internal(_) => {
            http_status(500, "internal Drive service operation failed")
        }
    }
}

fn map_object_store_error(error: DriveObjectStoreError) -> SdkworkError {
    let status = match error.kind {
        DriveObjectStoreErrorKind::NotFound => 404,
        DriveObjectStoreErrorKind::InvalidRequest => 400,
        DriveObjectStoreErrorKind::Conflict | DriveObjectStoreErrorKind::IntegrityFailed => 409,
        DriveObjectStoreErrorKind::PermissionDenied => 403,
        DriveObjectStoreErrorKind::RateLimited => 429,
        DriveObjectStoreErrorKind::Timeout => 504,
        DriveObjectStoreErrorKind::Unavailable => 503,
        DriveObjectStoreErrorKind::UpstreamError => 502,
        DriveObjectStoreErrorKind::NotSupported | DriveObjectStoreErrorKind::Internal => 500,
    };
    http_status(status, error.message)
}

fn integrity_failed(detail: &str) -> SdkworkError {
    http_status(409, detail)
}

fn http_status(status: u16, body: impl Into<String>) -> SdkworkError {
    SdkworkError::HttpStatus {
        status,
        body: body.into(),
    }
}

/// Re-exported so the gateway can build the resolver without importing the
/// generated SDK itself.
pub fn in_process_drive_client(
    pool: PgPool,
    tenant_id: impl Into<String>,
) -> Arc<dyn DriveWebsiteSdkClient> {
    Arc::new(InProcessDriveWebsiteSdkClient::new(pool, tenant_id))
}
