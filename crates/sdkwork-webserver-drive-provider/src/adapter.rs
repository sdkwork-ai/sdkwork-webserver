use std::{collections::HashSet, future::Future, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sdkwork_drive_internal_sdk::{
    models::{DriveResourceResolution, ResolveDriveResourceRequest, WebsiteRoot},
    SdkworkError,
};
use sdkwork_webserver_contract::provider::{
    OpenWebsiteContentRequest, OpenedWebsiteContent, ResolveWebsiteStaticPathRequest,
    ResolvedWebsiteContent, ValidateWebsiteResourceRequest, ValidatedWebsiteResource,
    WebsiteByteRange, WebsiteContentMetadata, WebsiteContentRange, WebsiteContentResolution,
    WebsiteProviderContentHandle, WebsiteProviderError, WebsiteProviderErrorKind,
    WebsiteProviderResult, WebsiteRequestConditions, WebsiteResourceCapabilities,
    WebsiteResourceProvider, WebsiteStaticContentProvider,
};
use sdkwork_webserver_core::website_runtime::{ProviderResourceReference, WebsiteProviderType};
use serde::{Deserialize, Serialize};

use crate::{
    cache::{cache_key, DriveContentCache},
    sdk::DriveWebsiteSdkClientResolver,
    stream::BoundedDriveContentStream,
};

pub const DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION: &str = "drive.website-root.v1";
pub const MAXIMUM_DRIVE_CONTENT_BYTES: u64 = 256 * 1024 * 1024;
const DEFAULT_PROVIDER_TIMEOUT_CAP_MS: u64 = 30_000;
const MAXIMUM_PROVIDER_PATH_BYTES: usize = 4_096;
const MAXIMUM_NODE_VERSION_ID_BYTES: usize = 64;
const DRIVE_SCOPE_TYPE: &str = "WEBSITE_ROOT";

pub struct DriveWebsiteProvider {
    clients: Arc<dyn DriveWebsiteSdkClientResolver>,
    maximum_content_bytes: u64,
    timeout_cap_ms: u64,
    cache: Option<Arc<DriveContentCache>>,
}

impl DriveWebsiteProvider {
    pub fn new(clients: Arc<dyn DriveWebsiteSdkClientResolver>) -> Self {
        Self {
            clients,
            maximum_content_bytes: MAXIMUM_DRIVE_CONTENT_BYTES,
            timeout_cap_ms: DEFAULT_PROVIDER_TIMEOUT_CAP_MS,
            cache: None,
        }
    }

    /// Provider with the local delivery cache bound (DRIVE_SPEC.md §17).
    /// `cache: None` keeps pure streaming behavior.
    pub fn with_cache(
        clients: Arc<dyn DriveWebsiteSdkClientResolver>,
        cache: Option<Arc<DriveContentCache>>,
    ) -> Self {
        Self {
            clients,
            maximum_content_bytes: MAXIMUM_DRIVE_CONTENT_BYTES,
            timeout_cap_ms: DEFAULT_PROVIDER_TIMEOUT_CAP_MS,
            cache,
        }
    }

    pub fn with_limits(
        clients: Arc<dyn DriveWebsiteSdkClientResolver>,
        maximum_content_bytes: u64,
        timeout_cap_ms: u64,
    ) -> Result<Self, String> {
        if maximum_content_bytes == 0 || maximum_content_bytes > MAXIMUM_DRIVE_CONTENT_BYTES {
            return Err(format!(
                "maximum content bytes must be between 1 and {MAXIMUM_DRIVE_CONTENT_BYTES}"
            ));
        }
        if timeout_cap_ms == 0 {
            return Err("provider timeout cap must be greater than zero".to_string());
        }
        Ok(Self {
            clients,
            maximum_content_bytes,
            timeout_cap_ms,
            cache: None,
        })
    }

    async fn call<T, F>(&self, deadline_ms: u64, operation: F) -> WebsiteProviderResult<T>
    where
        F: Future<Output = Result<T, SdkworkError>>,
    {
        if deadline_ms == 0 {
            return Err(provider_error(WebsiteProviderErrorKind::DeadlineExceeded));
        }
        tokio::time::timeout(
            Duration::from_millis(deadline_ms.min(self.timeout_cap_ms)),
            operation,
        )
        .await
        .map_err(|_| provider_error(WebsiteProviderErrorKind::DeadlineExceeded))?
        .map_err(map_sdk_error)
    }

    fn client(
        &self,
        tenant_scope_hash: &str,
    ) -> WebsiteProviderResult<Arc<dyn crate::DriveWebsiteSdkClient>> {
        self.clients.resolve(tenant_scope_hash)
    }
}

#[async_trait]
impl WebsiteResourceProvider for DriveWebsiteProvider {
    fn maximum_content_bytes(&self) -> u64 {
        self.maximum_content_bytes
    }

    async fn validate_resource(
        &self,
        request: &ValidateWebsiteResourceRequest,
    ) -> WebsiteProviderResult<ValidatedWebsiteResource> {
        validate_reference(&request.provider)?;
        let client = self.client(&request.context.tenant_scope_hash)?;
        let root = self
            .call(
                request.context.deadline_ms,
                client.retrieve_website_root(&request.provider.provider_resource_uuid),
            )
            .await?;
        let capabilities = validate_root(&request.provider, &root)?;
        require_capabilities(&request.required_capabilities, &capabilities)?;
        let provider_generation = require_positive_decimal(&root.active_generation)?.to_string();
        let root_version = require_positive_decimal(&root.version)?;
        Ok(ValidatedWebsiteResource {
            provider_resource_uuid: root.uuid,
            provider_generation: provider_generation.clone(),
            public_generation: format!(
                "generation={provider_generation};rootVersion={root_version};contentMode={}",
                root.content_mode
            ),
            capabilities,
        })
    }
}

#[async_trait]
impl WebsiteStaticContentProvider for DriveWebsiteProvider {
    async fn resolve_static_path(
        &self,
        request: &ResolveWebsiteStaticPathRequest,
    ) -> WebsiteProviderResult<WebsiteContentResolution> {
        validate_reference(&request.provider)?;
        let relative_path = drive_relative_path(&request.provider_relative_path)?;
        let client = self.client(&request.context.tenant_scope_hash)?;
        let resolution = self
            .call(
                request.context.deadline_ms,
                client.resolve_resource(&ResolveDriveResourceRequest {
                    scope_type: DRIVE_SCOPE_TYPE.to_string(),
                    scope_uuid: request.provider.provider_resource_uuid.clone(),
                    relative_path: relative_path.clone(),
                    pinned_generation: None,
                    pinned_node_version_id: None,
                }),
            )
            .await?;
        let metadata = validate_resolution(&request.provider, &relative_path, &resolution)?;
        if metadata.content_length > self.maximum_content_bytes {
            return Err(contract_mismatch());
        }
        if evaluate_conditions(&request.conditions, &metadata)? {
            return Ok(WebsiteContentResolution::NotModified);
        }
        let content_handle = create_content_handle(&resolution)?;
        Ok(WebsiteContentResolution::Found(ResolvedWebsiteContent {
            content_handle,
            metadata,
        }))
    }

    async fn open_static_content(
        &self,
        request: &OpenWebsiteContentRequest,
    ) -> WebsiteProviderResult<OpenedWebsiteContent> {
        validate_reference(&request.provider)?;
        if request.maximum_bytes == 0 {
            return Err(contract_mismatch());
        }
        let relative_path = drive_relative_path(&request.provider_relative_path)?;
        let handle = parse_content_handle(&request.content_handle)?;
        let client = self.client(&request.context.tenant_scope_hash)?;
        let resolution = self
            .call(
                request.context.deadline_ms,
                client.resolve_resource(&ResolveDriveResourceRequest {
                    scope_type: DRIVE_SCOPE_TYPE.to_string(),
                    scope_uuid: request.provider.provider_resource_uuid.clone(),
                    relative_path: relative_path.clone(),
                    pinned_generation: Some(handle.scope_generation.clone()),
                    pinned_node_version_id: Some(handle.node_version_id.clone()),
                }),
            )
            .await?;
        let metadata = validate_resolution(&request.provider, &relative_path, &resolution)?;
        if resolution.scope_generation != handle.scope_generation
            || resolution.logical_node_version_id != handle.node_version_id
        {
            return Err(contract_mismatch());
        }
        if metadata.content_length > request.maximum_bytes
            || metadata.content_length > self.maximum_content_bytes
        {
            return Err(contract_mismatch());
        }
        if evaluate_conditions(&request.conditions, &metadata)? {
            return Err(provider_error(WebsiteProviderErrorKind::NotModified));
        }
        let selected_range = select_range(request.range, &request.conditions, &metadata)?;
        // Local delivery cache (DRIVE_SPEC.md §17): upstream resolution and
        // conditional semantics above stay authoritative; only the immutable
        // byte payload is cached, keyed by the pinned node version.
        let cache_key = self.cache.as_ref().map(|_| {
            cache_key(
                &request.context.tenant_scope_hash,
                &request.provider.provider_resource_uuid,
                &resolution.logical_node_version_id,
            )
        });
        if let (Some(cache), Some(key)) = (self.cache.as_ref(), cache_key.as_ref()) {
            match selected_range {
                None => {
                    if let Some(stream) = cache.open_cached(key, metadata.content_length).await {
                        return Ok(OpenedWebsiteContent {
                            stream: Box::new(stream),
                            content_length: metadata.content_length,
                            content_range: None,
                        });
                    }
                }
                Some(range) => {
                    // Full object cached: serve ranges straight from disk.
                    if let Some(stream) = cache
                        .open_cached_range(key, range.start, range.end_inclusive)
                        .await
                    {
                        let length = range.end_inclusive - range.start + 1;
                        return Ok(OpenedWebsiteContent {
                            stream: Box::new(stream),
                            content_length: length,
                            content_range: Some(range),
                        });
                    }
                }
            }
        }
        let range_header =
            selected_range.map(|range| format!("bytes={}-{}", range.start, range.end_inclusive));
        // Streaming retrieval: the immutable object is forwarded in bounded
        // transport chunks; the expected length is enforced while the stream
        // is consumed, so memory stays O(chunk) instead of O(object size).
        let stream = self
            .call(
                request.context.deadline_ms,
                client.retrieve_content_stream(
                    &resolution.logical_node_version_id,
                    DRIVE_SCOPE_TYPE,
                    &request.provider.provider_resource_uuid,
                    &relative_path,
                    Some(&resolution.scope_generation),
                    range_header.as_deref(),
                    None,
                    None,
                    None,
                    None,
                    None,
                ),
            )
            .await?;
        let expected_length = selected_range.map_or(metadata.content_length, |range| {
            range.end_inclusive - range.start + 1
        });
        let bounded = BoundedDriveContentStream::new(stream, expected_length);
        // Only full-object fills are recorded; partial range responses never
        // enter the cache (they are derivable from a cached full object).
        if let (Some(cache), Some(key)) = (self.cache.as_ref(), cache_key) {
            if selected_range.is_none() {
                let stream = cache.fill_stream(key, bounded);
                return Ok(OpenedWebsiteContent {
                    stream: Box::new(stream),
                    content_length: expected_length,
                    content_range: None,
                });
            }
        }
        Ok(OpenedWebsiteContent {
            stream: Box::new(bounded),
            content_length: expected_length,
            content_range: selected_range,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DriveContentHandle {
    version: u8,
    scope_generation: String,
    node_version_id: String,
}

fn create_content_handle(
    resolution: &DriveResourceResolution,
) -> WebsiteProviderResult<WebsiteProviderContentHandle> {
    let handle = DriveContentHandle {
        version: 1,
        scope_generation: resolution.scope_generation.clone(),
        node_version_id: resolution.logical_node_version_id.clone(),
    };
    let serialized = serde_json::to_string(&handle).map_err(|_| contract_mismatch())?;
    WebsiteProviderContentHandle::new(serialized).map_err(|_| contract_mismatch())
}

fn parse_content_handle(
    handle: &WebsiteProviderContentHandle,
) -> WebsiteProviderResult<DriveContentHandle> {
    let parsed: DriveContentHandle =
        serde_json::from_str(handle.as_str()).map_err(|_| contract_mismatch())?;
    if parsed.version != 1 {
        return Err(contract_mismatch());
    }
    require_positive_decimal(&parsed.scope_generation)?;
    validate_node_version_id(&parsed.node_version_id)?;
    Ok(parsed)
}

fn validate_reference(reference: &ProviderResourceReference) -> WebsiteProviderResult<()> {
    if reference.provider_type != WebsiteProviderType::Drive
        || reference.provider_contract_version != DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION
        || reference.provider_resource_uuid.is_empty()
        || reference.provider_resource_uuid.len() > 128
        || reference
            .provider_resource_uuid
            .bytes()
            .any(|byte| byte.is_ascii_control())
    {
        return Err(contract_mismatch());
    }
    Ok(())
}

fn validate_root(
    reference: &ProviderResourceReference,
    root: &WebsiteRoot,
) -> WebsiteProviderResult<WebsiteResourceCapabilities> {
    if root.uuid != reference.provider_resource_uuid
        || root.space_id.is_empty()
        || root.space_id.len() > 64
        || !matches!(root.source_root_mode.as_str(), "SPACE_ROOT" | "FOLDER")
        || !matches!(
            root.content_mode.as_str(),
            "LIVE_TREE" | "ATOMIC_GENERATION"
        )
    {
        return Err(contract_mismatch());
    }
    if root.root_status != "ACTIVE" {
        return Err(provider_error(WebsiteProviderErrorKind::Revoked));
    }
    require_positive_decimal(&root.active_generation)?;
    require_positive_decimal(&root.version)?;
    parse_rfc3339(&root.updated_at)?;
    let capabilities = root
        .capabilities
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if !capabilities.contains("STATIC_CONTENT")
        || !capabilities.contains("BYTE_RANGE")
        || !capabilities.contains("CONDITIONAL_REQUESTS")
    {
        return Err(contract_mismatch());
    }
    Ok(WebsiteResourceCapabilities {
        static_content: true,
        wiki_routes: false,
        wiki_search: false,
        range_requests: true,
    })
}

fn require_capabilities(
    required: &WebsiteResourceCapabilities,
    actual: &WebsiteResourceCapabilities,
) -> WebsiteProviderResult<()> {
    if (required.static_content && !actual.static_content)
        || (required.wiki_routes && !actual.wiki_routes)
        || (required.wiki_search && !actual.wiki_search)
        || (required.range_requests && !actual.range_requests)
    {
        return Err(contract_mismatch());
    }
    Ok(())
}

fn drive_relative_path(provider_path: &str) -> WebsiteProviderResult<String> {
    if provider_path.is_empty()
        || provider_path.len() > MAXIMUM_PROVIDER_PATH_BYTES
        || !provider_path.starts_with('/')
        || provider_path.ends_with('/')
        || provider_path.contains(['\\', '%', '?', '#'])
        || provider_path.contains("//")
        || provider_path.chars().any(char::is_control)
        || provider_path
            .split('/')
            .any(|segment| segment == "." || segment == "..")
    {
        return Err(provider_error(WebsiteProviderErrorKind::InvalidPath));
    }
    let relative_path = provider_path.trim_start_matches('/');
    if relative_path.is_empty() {
        return Err(provider_error(WebsiteProviderErrorKind::InvalidPath));
    }
    Ok(relative_path.to_string())
}

fn validate_resolution(
    reference: &ProviderResourceReference,
    relative_path: &str,
    resolution: &DriveResourceResolution,
) -> WebsiteProviderResult<WebsiteContentMetadata> {
    if resolution.scope_type != DRIVE_SCOPE_TYPE
        || resolution.scope_uuid != reference.provider_resource_uuid
        || resolution.normalized_relative_path != relative_path
        || resolution.resource_type != "FILE"
        || resolution.scope_status != "ACTIVE"
        || resolution.node_status != "ACTIVE"
        || resolution.eligibility != "ELIGIBLE"
        || resolution.content_type.is_empty()
        || resolution.content_type.len() > 255
        || resolution.content_type.chars().any(char::is_control)
    {
        return Err(contract_mismatch());
    }
    require_positive_decimal(&resolution.scope_generation)?;
    require_positive_decimal(&resolution.version_no)?;
    validate_node_version_id(&resolution.logical_node_version_id)?;
    validate_sha256(&resolution.checksum_sha256_hex)?;
    if resolution.etag != format!("\"{}\"", resolution.checksum_sha256_hex) {
        return Err(contract_mismatch());
    }
    let content_length = require_nonnegative_decimal(&resolution.content_length)?;
    let last_modified = parse_rfc3339(&resolution.last_modified)?
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();
    Ok(WebsiteContentMetadata {
        content_type: resolution.content_type.clone(),
        content_length,
        etag: resolution.etag.clone(),
        last_modified,
        content_version: resolution.logical_node_version_id.clone(),
        provider_generation: resolution.scope_generation.clone(),
        range_supported: true,
    })
}

fn validate_node_version_id(value: &str) -> WebsiteProviderResult<()> {
    if value.is_empty()
        || value.len() > MAXIMUM_NODE_VERSION_ID_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(contract_mismatch());
    }
    Ok(())
}

fn validate_sha256(value: &str) -> WebsiteProviderResult<()> {
    let digest = value
        .strip_prefix("sha256:")
        .ok_or_else(contract_mismatch)?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(contract_mismatch());
    }
    Ok(())
}

fn require_nonnegative_decimal(value: &str) -> WebsiteProviderResult<u64> {
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err(contract_mismatch());
    }
    value.parse::<u64>().map_err(|_| contract_mismatch())
}

fn require_positive_decimal(value: &str) -> WebsiteProviderResult<u64> {
    let parsed = require_nonnegative_decimal(value)?;
    if parsed == 0 {
        return Err(contract_mismatch());
    }
    Ok(parsed)
}

fn parse_rfc3339(value: &str) -> WebsiteProviderResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| contract_mismatch())
}

fn evaluate_conditions(
    conditions: &WebsiteRequestConditions,
    metadata: &WebsiteContentMetadata,
) -> WebsiteProviderResult<bool> {
    let updated_at = parse_http_date(&metadata.last_modified).ok_or_else(contract_mismatch)?;
    if conditions
        .if_match
        .as_deref()
        .is_some_and(|value| !etag_matches(value, &metadata.etag, false))
    {
        return Err(provider_error(WebsiteProviderErrorKind::PreconditionFailed));
    }
    if conditions.if_match.is_none()
        && conditions
            .if_unmodified_since
            .as_deref()
            .and_then(parse_http_date)
            .is_some_and(|value| updated_at > value)
    {
        return Err(provider_error(WebsiteProviderErrorKind::PreconditionFailed));
    }
    if conditions
        .if_none_match
        .as_deref()
        .is_some_and(|value| etag_matches(value, &metadata.etag, true))
    {
        return Ok(true);
    }
    if conditions.if_none_match.is_none()
        && conditions
            .if_modified_since
            .as_deref()
            .and_then(parse_http_date)
            .is_some_and(|value| updated_at <= value)
    {
        return Ok(true);
    }
    Ok(false)
}

fn select_range(
    requested: Option<WebsiteByteRange>,
    conditions: &WebsiteRequestConditions,
    metadata: &WebsiteContentMetadata,
) -> WebsiteProviderResult<Option<WebsiteContentRange>> {
    let Some(requested) = requested else {
        return Ok(None);
    };
    if conditions
        .if_range
        .as_deref()
        .is_some_and(|value| !if_range_matches(value, metadata))
    {
        return Ok(None);
    }
    if metadata.content_length == 0 || requested.start >= metadata.content_length {
        return Err(provider_error(
            WebsiteProviderErrorKind::RangeNotSatisfiable,
        ));
    }
    let end_inclusive = requested
        .end_inclusive
        .unwrap_or(metadata.content_length - 1)
        .min(metadata.content_length - 1);
    if end_inclusive < requested.start {
        return Err(provider_error(
            WebsiteProviderErrorKind::RangeNotSatisfiable,
        ));
    }
    Ok(Some(WebsiteContentRange {
        start: requested.start,
        end_inclusive,
        complete_length: metadata.content_length,
    }))
}

fn if_range_matches(value: &str, metadata: &WebsiteContentMetadata) -> bool {
    let value = value.trim();
    if value.starts_with('"') || value.starts_with("W/") {
        return !value.starts_with("W/") && value == metadata.etag;
    }
    let Some(condition_date) = parse_http_date(value) else {
        return false;
    };
    parse_http_date(&metadata.last_modified)
        .is_some_and(|last_modified| last_modified <= condition_date)
}

fn parse_http_date(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc2822(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn etag_matches(header: &str, expected: &str, allow_weak: bool) -> bool {
    header.split(',').map(str::trim).any(|candidate| {
        if candidate == "*" {
            return true;
        }
        if allow_weak {
            candidate.strip_prefix("W/").unwrap_or(candidate)
                == expected.strip_prefix("W/").unwrap_or(expected)
        } else {
            !candidate.starts_with("W/") && candidate == expected
        }
    })
}

fn map_sdk_error(error: SdkworkError) -> WebsiteProviderError {
    let kind = match error {
        SdkworkError::Http(error) if error.is_timeout() => {
            WebsiteProviderErrorKind::DeadlineExceeded
        }
        SdkworkError::Http(_) => WebsiteProviderErrorKind::Unavailable,
        SdkworkError::HttpStatus { status: 404, .. } => WebsiteProviderErrorKind::NotFound,
        SdkworkError::HttpStatus { status: 304, .. } => WebsiteProviderErrorKind::NotModified,
        SdkworkError::HttpStatus { status: 410, .. } => WebsiteProviderErrorKind::Revoked,
        SdkworkError::HttpStatus { status: 412, .. } => {
            WebsiteProviderErrorKind::PreconditionFailed
        }
        SdkworkError::HttpStatus { status: 416, .. } => {
            WebsiteProviderErrorKind::RangeNotSatisfiable
        }
        SdkworkError::HttpStatus {
            status: 408 | 504, ..
        } => WebsiteProviderErrorKind::DeadlineExceeded,
        SdkworkError::HttpStatus { status: 429, .. } => WebsiteProviderErrorKind::RateLimited,
        SdkworkError::HttpStatus { status, .. } if status >= 500 => {
            WebsiteProviderErrorKind::Unavailable
        }
        SdkworkError::HttpStatus { .. }
        | SdkworkError::Serialization(_)
        | SdkworkError::InvalidHeaderName(_)
        | SdkworkError::InvalidHeaderValue(_)
        | SdkworkError::InvalidHttpMethod(_)
        | SdkworkError::ResponseBodyTooLarge { .. }
        | SdkworkError::ApiStatus { .. }
        | SdkworkError::MissingAccessToken => WebsiteProviderErrorKind::ContractMismatch,
    };
    provider_error(kind)
}

fn contract_mismatch() -> WebsiteProviderError {
    provider_error(WebsiteProviderErrorKind::ContractMismatch)
}

fn provider_error(kind: WebsiteProviderErrorKind) -> WebsiteProviderError {
    WebsiteProviderError::new(kind)
}
