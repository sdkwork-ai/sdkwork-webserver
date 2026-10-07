use std::{future::Future, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use sdkwork_knowledgebase_internal_sdk::{
    models::{ResolveWikiRouteRequest, WikiPublicPageMetadata, WikiPublication},
    SdkworkError,
};
use sdkwork_webserver_contract::provider::{
    OpenWebsiteContentRequest, OpenedWebsiteContent, ResolveWebsiteWikiRouteRequest,
    ResolvedWebsiteWikiContent, ValidateWebsiteResourceRequest, ValidatedWebsiteResource,
    WebsiteContentMetadata, WebsiteProviderContentHandle, WebsiteProviderError,
    WebsiteProviderErrorKind, WebsiteProviderResult, WebsiteResourceCapabilities,
    WebsiteResourceProvider, WebsiteWikiCollectionItem, WebsiteWikiCollectionPage,
    WebsiteWikiCollectionRequest, WebsiteWikiContentKind, WebsiteWikiProvider, WebsiteWikiRedirect,
    WebsiteWikiRouteResolution,
};
use sdkwork_webserver_core::website_runtime::{ProviderResourceReference, WebsiteProviderType};

use crate::{sdk::KnowledgebaseWikiSdkClientResolver, stream::BoundedWikiContentStream};

pub const KNOWLEDGEBASE_WIKI_PROVIDER_CONTRACT_VERSION: &str = "knowledgebase.wiki-publication.v1";
pub const MAXIMUM_WIKI_CONTENT_BYTES: u64 = 16 * 1024 * 1024;
const DEFAULT_PROVIDER_TIMEOUT_CAP_MS: u64 = 30_000;
const MAXIMUM_ROUTE_BYTES: usize = 2_048;
const MAXIMUM_QUERY_BYTES: usize = 256;
const MAXIMUM_LOCALE_BYTES: usize = 35;
const MAXIMUM_CURSOR_BYTES: usize = 4_096;

pub struct KnowledgebaseWikiWebsiteProvider {
    clients: Arc<dyn KnowledgebaseWikiSdkClientResolver>,
    maximum_content_bytes: u64,
    timeout_cap_ms: u64,
}

impl KnowledgebaseWikiWebsiteProvider {
    pub fn new(clients: Arc<dyn KnowledgebaseWikiSdkClientResolver>) -> Self {
        Self {
            clients,
            maximum_content_bytes: MAXIMUM_WIKI_CONTENT_BYTES,
            timeout_cap_ms: DEFAULT_PROVIDER_TIMEOUT_CAP_MS,
        }
    }

    pub fn with_limits(
        clients: Arc<dyn KnowledgebaseWikiSdkClientResolver>,
        maximum_content_bytes: u64,
        timeout_cap_ms: u64,
    ) -> Result<Self, String> {
        if maximum_content_bytes == 0 || maximum_content_bytes > MAXIMUM_WIKI_CONTENT_BYTES {
            return Err(format!(
                "maximum content bytes must be between 1 and {MAXIMUM_WIKI_CONTENT_BYTES}"
            ));
        }
        if timeout_cap_ms == 0 {
            return Err("provider timeout cap must be greater than zero".to_string());
        }
        Ok(Self {
            clients,
            maximum_content_bytes,
            timeout_cap_ms,
        })
    }

    async fn call<T, F>(&self, deadline_ms: u64, operation: F) -> WebsiteProviderResult<T>
    where
        F: Future<Output = Result<T, SdkworkError>>,
    {
        if deadline_ms == 0 {
            return Err(provider_error(WebsiteProviderErrorKind::DeadlineExceeded));
        }
        let timeout = Duration::from_millis(deadline_ms.min(self.timeout_cap_ms));
        tokio::time::timeout(timeout, operation)
            .await
            .map_err(|_| provider_error(WebsiteProviderErrorKind::DeadlineExceeded))?
            .map_err(map_sdk_error)
    }

    fn client(
        &self,
        tenant_scope_hash: &str,
    ) -> WebsiteProviderResult<Arc<dyn crate::KnowledgebaseWikiSdkClient>> {
        self.clients.resolve(tenant_scope_hash)
    }
}

#[async_trait]
impl WebsiteResourceProvider for KnowledgebaseWikiWebsiteProvider {
    fn maximum_content_bytes(&self) -> u64 {
        self.maximum_content_bytes
    }

    async fn validate_resource(
        &self,
        request: &ValidateWebsiteResourceRequest,
    ) -> WebsiteProviderResult<ValidatedWebsiteResource> {
        validate_reference(&request.provider)?;
        let client = self.client(&request.context.tenant_scope_hash)?;
        let publication = self
            .call(
                request.context.deadline_ms,
                client.retrieve_publication(&request.provider.provider_resource_uuid),
            )
            .await?;
        validate_publication_identity(&request.provider, &publication)?;
        let capabilities = publication_capabilities(&publication);
        require_capabilities(&request.required_capabilities, &capabilities)?;
        let provider_generation =
            require_positive_decimal("providerGeneration", &publication.provider_generation)?
                .to_string();
        let public_generation = public_generation(&publication)?;
        Ok(ValidatedWebsiteResource {
            provider_resource_uuid: publication.publication_uuid,
            provider_generation,
            public_generation,
            capabilities,
        })
    }
}

#[async_trait]
impl WebsiteWikiProvider for KnowledgebaseWikiWebsiteProvider {
    async fn resolve_wiki_route(
        &self,
        request: &ResolveWebsiteWikiRouteRequest,
    ) -> WebsiteProviderResult<WebsiteWikiRouteResolution> {
        validate_reference(&request.provider)?;
        validate_route(&request.route)?;
        validate_locale(request.locale.as_deref())?;
        let client = self.client(&request.context.tenant_scope_hash)?;
        let route_request = ResolveWikiRouteRequest {
            route: request.route.clone(),
            locale: request.locale.clone(),
        };
        let publication_uuid = request.provider.provider_resource_uuid.as_str();
        let (publication, resolution) = self
            .call(request.context.deadline_ms, async {
                let publication = client.retrieve_publication(publication_uuid).await?;
                let resolution = client
                    .resolve_route(publication_uuid, &route_request)
                    .await?;
                Ok((publication, resolution))
            })
            .await?;
        validate_publication_identity(&request.provider, &publication)?;

        match resolution.disposition.as_str() {
            "PAGE" => {
                let page = resolution.page.ok_or_else(contract_mismatch)?;
                let content_handle = resolution
                    .content_handle
                    .ok_or_else(contract_mismatch)
                    .and_then(|value| {
                        WebsiteProviderContentHandle::new(value).map_err(|_| contract_mismatch())
                    })?;
                let resolved = resolved_page(content_handle, page, &publication)?;
                if evaluate_conditions(&request.conditions, &resolved.metadata)? {
                    return Ok(WebsiteWikiRouteResolution::NotModified);
                }
                Ok(WebsiteWikiRouteResolution::Content(Box::new(resolved)))
            }
            "REDIRECT" => {
                let status_code = resolution
                    .status
                    .and_then(|value| u16::try_from(value).ok())
                    .filter(|value| matches!(value, 301 | 302 | 307 | 308))
                    .ok_or_else(contract_mismatch)?;
                let canonical_route = resolution.canonical_route.ok_or_else(contract_mismatch)?;
                validate_route(&canonical_route)?;
                let page_public_version = resolution
                    .page_public_version
                    .ok_or_else(contract_mismatch)?;
                require_positive_decimal("pagePublicVersion", &page_public_version)?;
                Ok(WebsiteWikiRouteResolution::Redirect(WebsiteWikiRedirect {
                    status_code,
                    canonical_route,
                }))
            }
            _ => Err(contract_mismatch()),
        }
    }

    async fn open_wiki_content(
        &self,
        request: &OpenWebsiteContentRequest,
    ) -> WebsiteProviderResult<OpenedWebsiteContent> {
        validate_reference(&request.provider)?;
        if request.range.is_some() {
            return Err(contract_mismatch());
        }
        if request.maximum_bytes == 0 {
            return Err(contract_mismatch());
        }
        let client = self.client(&request.context.tenant_scope_hash)?;
        // Streaming retrieval: the wiki body is forwarded in bounded
        // transport chunks; the byte ceiling is enforced while the stream is
        // consumed, so memory stays O(chunk) instead of O(page size).
        let opened = self
            .call(
                request.context.deadline_ms,
                client.retrieve_content_stream(
                    &request.provider.provider_resource_uuid,
                    request.content_handle.as_str(),
                ),
            )
            .await?;
        let maximum_bytes = request.maximum_bytes.min(self.maximum_content_bytes);
        if opened
            .content_length
            .is_some_and(|length| length > maximum_bytes)
        {
            return Err(contract_mismatch());
        }
        let mut stream = BoundedWikiContentStream::new(opened.stream, maximum_bytes);
        if let Some(expected) = opened.content_length.filter(|length| *length > 0) {
            stream = stream.with_expected_length(expected);
        }
        Ok(OpenedWebsiteContent {
            stream: Box::new(stream),
            content_length: opened.content_length.unwrap_or(0),
            content_range: None,
        })
    }

    async fn retrieve_navigation(
        &self,
        request: &WebsiteWikiCollectionRequest,
    ) -> WebsiteProviderResult<WebsiteWikiCollectionPage> {
        validate_reference(&request.provider)?;
        validate_locale(request.locale.as_deref())?;
        validate_cursor(request.cursor.as_deref())?;
        if request.query.is_some() {
            return Err(provider_error(WebsiteProviderErrorKind::InvalidPath));
        }
        let client = self.client(&request.context.tenant_scope_hash)?;
        let publication_uuid = request.provider.provider_resource_uuid.as_str();
        let page_size = i64::from(request.page_size.get());
        let (publication, page) = self
            .call(request.context.deadline_ms, async {
                let publication = client.retrieve_publication(publication_uuid).await?;
                let page = client
                    .list_navigation(
                        publication_uuid,
                        request.locale.as_deref(),
                        request.cursor.as_deref(),
                        page_size,
                    )
                    .await?;
                Ok((publication, page))
            })
            .await?;
        validate_publication_identity(&request.provider, &publication)?;
        collection_page(
            page.items,
            page.page_info.next_cursor,
            &publication.navigation_generation,
        )
    }

    async fn search_wiki(
        &self,
        request: &WebsiteWikiCollectionRequest,
    ) -> WebsiteProviderResult<WebsiteWikiCollectionPage> {
        validate_reference(&request.provider)?;
        validate_locale(request.locale.as_deref())?;
        validate_cursor(request.cursor.as_deref())?;
        let query = normalize_query(request.query.as_deref())?;
        let client = self.client(&request.context.tenant_scope_hash)?;
        let publication_uuid = request.provider.provider_resource_uuid.as_str();
        let page_size = i64::from(request.page_size.get());
        let (publication, page) = self
            .call(request.context.deadline_ms, async {
                let publication = client.retrieve_publication(publication_uuid).await?;
                let page = client
                    .search_pages(
                        publication_uuid,
                        &query,
                        request.locale.as_deref(),
                        request.cursor.as_deref(),
                        page_size,
                    )
                    .await?;
                Ok((publication, page))
            })
            .await?;
        validate_publication_identity(&request.provider, &publication)?;
        if !publication.search_enabled {
            return Err(provider_error(WebsiteProviderErrorKind::NotFound));
        }
        collection_page(
            page.items,
            page.page_info.next_cursor,
            &publication.search_generation,
        )
    }
}

fn validate_reference(reference: &ProviderResourceReference) -> WebsiteProviderResult<()> {
    if reference.provider_type != WebsiteProviderType::Knowledgebase
        || reference.provider_contract_version != KNOWLEDGEBASE_WIKI_PROVIDER_CONTRACT_VERSION
    {
        return Err(contract_mismatch());
    }
    if reference.provider_resource_uuid.is_empty()
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

fn validate_publication_identity(
    reference: &ProviderResourceReference,
    publication: &WikiPublication,
) -> WebsiteProviderResult<()> {
    if publication.publication_uuid != reference.provider_resource_uuid {
        return Err(contract_mismatch());
    }
    require_positive_decimal("providerGeneration", &publication.provider_generation)?;
    require_positive_decimal("navigationGeneration", &publication.navigation_generation)?;
    require_positive_decimal("searchGeneration", &publication.search_generation)?;
    Ok(())
}

fn publication_capabilities(publication: &WikiPublication) -> WebsiteResourceCapabilities {
    WebsiteResourceCapabilities {
        static_content: true,
        wiki_routes: true,
        wiki_search: publication.search_enabled,
        range_requests: false,
    }
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

fn public_generation(publication: &WikiPublication) -> WebsiteProviderResult<String> {
    require_positive_decimal("providerGeneration", &publication.provider_generation)?;
    require_positive_decimal("navigationGeneration", &publication.navigation_generation)?;
    require_positive_decimal("searchGeneration", &publication.search_generation)?;
    Ok(format!(
        "p={};n={};s={};renderer={};theme={}",
        publication.provider_generation,
        publication.navigation_generation,
        publication.search_generation,
        publication.renderer_policy_version,
        publication.theme_version
    ))
}

fn resolved_page(
    content_handle: WebsiteProviderContentHandle,
    page: WikiPublicPageMetadata,
    publication: &WikiPublication,
) -> WebsiteProviderResult<ResolvedWebsiteWikiContent> {
    validate_route(&page.canonical_route)?;
    let content_length = require_nonnegative_decimal("sizeBytes", &page.size_bytes)?;
    let public_page_version =
        require_positive_decimal("pagePublicVersion", &page.page_public_version)?.to_string();
    validate_sha256(&page.content_sha256)?;
    let (last_modified, _) = parse_public_updated_at(&page.public_updated_at)?;
    let etag = format!("\"{}-v{}\"", page.content_sha256, public_page_version);
    let kind = if page.media_type.eq_ignore_ascii_case("text/html")
        || page
            .media_type
            .to_ascii_lowercase()
            .starts_with("text/html;")
    {
        WebsiteWikiContentKind::Html
    } else {
        WebsiteWikiContentKind::Asset
    };
    Ok(ResolvedWebsiteWikiContent {
        content_handle,
        kind,
        canonical_route: page.canonical_route,
        page_uuid: Some(page.projection_uuid),
        public_page_version: public_page_version.clone(),
        renderer_version: publication.renderer_policy_version.clone(),
        navigation_generation: publication.navigation_generation.clone(),
        search_generation: publication.search_generation.clone(),
        metadata: WebsiteContentMetadata {
            content_type: page.media_type,
            content_length,
            etag,
            last_modified,
            content_version: public_page_version,
            provider_generation: publication.provider_generation.clone(),
            range_supported: false,
        },
    })
}

fn collection_page(
    pages: Vec<WikiPublicPageMetadata>,
    next_cursor: Option<String>,
    generation: &str,
) -> WebsiteProviderResult<WebsiteWikiCollectionPage> {
    require_positive_decimal("collectionGeneration", generation)?;
    validate_cursor(next_cursor.as_deref())?;
    let items = pages
        .into_iter()
        .map(|page| {
            validate_route(&page.canonical_route)?;
            let public_page_version =
                require_positive_decimal("pagePublicVersion", &page.page_public_version)?
                    .to_string();
            let title = page
                .title
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| page.canonical_route.clone());
            Ok(WebsiteWikiCollectionItem {
                page_uuid: page.projection_uuid,
                canonical_route: page.canonical_route,
                title,
                summary: page.description,
                public_page_version,
            })
        })
        .collect::<WebsiteProviderResult<Vec<_>>>()?;
    Ok(WebsiteWikiCollectionPage {
        items,
        next_cursor,
        generation: generation.to_string(),
    })
}

fn validate_route(route: &str) -> WebsiteProviderResult<()> {
    if route.is_empty()
        || route.len() > MAXIMUM_ROUTE_BYTES
        || !route.starts_with('/')
        || route.contains(['\\', '%', '?', '#'])
        || route.contains("//")
        || route.chars().any(char::is_control)
        || route
            .split('/')
            .any(|segment| segment == "." || segment == "..")
    {
        return Err(provider_error(WebsiteProviderErrorKind::InvalidPath));
    }
    Ok(())
}

fn validate_locale(locale: Option<&str>) -> WebsiteProviderResult<()> {
    if locale.is_some_and(|value| {
        value.is_empty()
            || value.len() > MAXIMUM_LOCALE_BYTES
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    }) {
        return Err(provider_error(WebsiteProviderErrorKind::InvalidPath));
    }
    Ok(())
}

fn validate_cursor(cursor: Option<&str>) -> WebsiteProviderResult<()> {
    if cursor.is_some_and(|value| {
        value.is_empty()
            || value.len() > MAXIMUM_CURSOR_BYTES
            || value.chars().any(char::is_control)
    }) {
        return Err(provider_error(WebsiteProviderErrorKind::InvalidPath));
    }
    Ok(())
}

fn normalize_query(query: Option<&str>) -> WebsiteProviderResult<String> {
    let query = query.map(str::trim).unwrap_or_default();
    if query.is_empty() || query.len() > MAXIMUM_QUERY_BYTES || query.chars().any(char::is_control)
    {
        return Err(provider_error(WebsiteProviderErrorKind::InvalidPath));
    }
    Ok(query.to_string())
}

fn require_nonnegative_decimal(_field: &str, value: &str) -> WebsiteProviderResult<u64> {
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err(contract_mismatch());
    }
    value.parse::<u64>().map_err(|_| contract_mismatch())
}

fn require_positive_decimal(field: &str, value: &str) -> WebsiteProviderResult<u64> {
    let parsed = require_nonnegative_decimal(field, value)?;
    if parsed == 0 {
        return Err(contract_mismatch());
    }
    Ok(parsed)
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

fn parse_public_updated_at(value: &str) -> WebsiteProviderResult<(String, DateTime<Utc>)> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .or_else(|_| DateTime::<FixedOffset>::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f%:z"))
        .map_err(|_| contract_mismatch())?
        .with_timezone(&Utc);
    Ok((
        parsed.format("%a, %d %b %Y %H:%M:%S GMT").to_string(),
        parsed,
    ))
}

fn evaluate_conditions(
    conditions: &sdkwork_webserver_contract::provider::WebsiteRequestConditions,
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
            candidate.strip_prefix("W/").unwrap_or(candidate) == expected
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
        SdkworkError::HttpStatus {
            status: 408 | 504, ..
        } => WebsiteProviderErrorKind::DeadlineExceeded,
        SdkworkError::HttpStatus { status: 410, .. } => WebsiteProviderErrorKind::Revoked,
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
