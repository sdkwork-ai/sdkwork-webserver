//! In-process Knowledgebase transport for the website provider
//! (MODULE_SPEC.md §4.2).
//!
//! The narrowest callable sibling API for a public Wiki read is the
//! Knowledgebase module's own `KnowledgeWikiPublicProviderService` — the exact
//! service the Knowledgebase Internal API route delegates to. It is constructed
//! over the same PostgreSQL database: its persistence port is implemented by
//! `SqlxWikiPersistenceStore`, which is `sqlx::AnyPool`-typed, so the host
//! passes the compatibility pool the database framework reserves for that
//! driver (see `sdkwork_database_sqlx::create_any_pool_from_env`); the Wiki
//! source bytes come from Knowledgebase's own embedded Drive adapter on the host
//! `PgPool`. No Knowledgebase table is read directly here.
//!
//! Transport failures keep the Knowledgebase Internal API's status mapping so
//! the provider's `map_sdk_error` yields identical wire semantics.

use std::sync::Arc;

use async_trait::async_trait;
use sdkwork_intelligence_knowledgebase_repository_sqlx::SqlxWikiPersistenceStore;
use sdkwork_intelligence_knowledgebase_service::ports::knowledge_wiki_persistence::WikiPersistenceScope;
use sdkwork_intelligence_knowledgebase_service::wiki_public_provider::{
    KnowledgeWikiPublicProviderError, KnowledgeWikiPublicProviderService,
    ListWikiPublicNavigationPageRequest, ResolveWikiPublicRouteRequest,
    RetrieveWikiPublicContentRequest, RetrieveWikiPublicPublicationRequest,
    SearchWikiPublicPageRequest, WikiPublicPageList, WikiPublicRouteResolution,
};
use sdkwork_knowledgebase_drive::KnowledgebaseDriveEmbeddedWikiSourceAdapter;
use sdkwork_knowledgebase_internal_sdk::{
    models::{
        PageInfo, ResolveWikiRouteRequest, WikiPublicPageListData, WikiPublicPageMetadata,
        WikiPublication, WikiRouteResolution,
    },
    SdkworkError,
};
use sqlx::{AnyPool, PgPool};

use crate::sdk::{KnowledgebaseWikiSdkClient, OpenedWikiContentStream, WikiContentChunkStream};

const CONTENT_CHUNK_BYTES: usize = 256 * 1024;
const IN_PROCESS_OPERATOR_ID: &str = "sdkwork-webserver";
const PAGE_INFO_MODE_CURSOR: &str = "cursor";

/// Knowledgebase Internal API transport implemented by calling the
/// Knowledgebase module's public wiki provider in-process.
pub struct InProcessKnowledgebaseWikiSdkClient {
    service: KnowledgeWikiPublicProviderService,
    scope: WikiPersistenceScope,
}

impl InProcessKnowledgebaseWikiSdkClient {
    /// `pool` is the host PostgreSQL pool (shared with Drive); `compatibility_pool`
    /// is the AnyPool the database framework reserves for the compatibility
    /// driver Knowledgebase's repository requires.
    pub fn new(
        pool: PgPool,
        compatibility_pool: AnyPool,
        tenant_id: u64,
        organization_id: u64,
    ) -> Self {
        let store = Arc::new(SqlxWikiPersistenceStore::new(compatibility_pool));
        let drive_source = Arc::new(KnowledgebaseDriveEmbeddedWikiSourceAdapter::new(
            pool,
            tenant_id.to_string(),
            IN_PROCESS_OPERATOR_ID,
        ));
        Self {
            service: KnowledgeWikiPublicProviderService::new(store, drive_source),
            scope: WikiPersistenceScope {
                tenant_id,
                organization_id,
            },
        }
    }
}

#[async_trait]
impl KnowledgebaseWikiSdkClient for InProcessKnowledgebaseWikiSdkClient {
    async fn retrieve_publication(
        &self,
        publication_uuid: &str,
    ) -> Result<WikiPublication, SdkworkError> {
        let publication = self
            .service
            .retrieve_publication(RetrieveWikiPublicPublicationRequest {
                scope: self.scope,
                publication_uuid: publication_uuid.to_owned(),
            })
            .await
            .map_err(map_provider_error)?;
        Ok(WikiPublication {
            publication_uuid: publication.publication_uuid,
            title: publication.title,
            description: publication.description,
            homepage_source_path: publication.homepage_source_path,
            default_locale: publication.default_locale,
            supported_locales: publication.supported_locales,
            navigation_mode: publication.navigation_mode,
            theme_key: publication.theme_key,
            theme_version: publication.theme_version,
            renderer_policy_version: publication.renderer_policy_version,
            search_enabled: publication.search_enabled,
            robots_policy: publication.robots_policy,
            sitemap_enabled: publication.sitemap_enabled,
            provider_generation: publication.provider_generation.to_string(),
            navigation_generation: publication.navigation_generation.to_string(),
            search_generation: publication.search_generation.to_string(),
        })
    }

    async fn resolve_route(
        &self,
        publication_uuid: &str,
        request: &ResolveWikiRouteRequest,
    ) -> Result<WikiRouteResolution, SdkworkError> {
        let resolution = self
            .service
            .resolve_route(ResolveWikiPublicRouteRequest {
                scope: self.scope,
                publication_uuid: publication_uuid.to_owned(),
                route: request.route.clone(),
                locale: request.locale.clone(),
            })
            .await
            .map_err(map_provider_error)?;
        Ok(match resolution {
            WikiPublicRouteResolution::Page(page) => WikiRouteResolution {
                disposition: "PAGE".to_owned(),
                page: Some(map_page(&page.page)?),
                content_handle: Some(page.content_handle),
                requested_route: None,
                canonical_route: None,
                status: None,
                page_public_version: None,
            },
            WikiPublicRouteResolution::Redirect {
                requested_route,
                canonical_route,
                status,
                page_public_version,
            } => WikiRouteResolution {
                disposition: "REDIRECT".to_owned(),
                page: None,
                content_handle: None,
                requested_route: Some(requested_route),
                canonical_route: Some(canonical_route),
                status: Some(i64::from(status)),
                page_public_version: Some(page_public_version.to_string()),
            },
        })
    }

    async fn retrieve_content(
        &self,
        publication_uuid: &str,
        content_handle: &str,
    ) -> Result<Vec<u8>, SdkworkError> {
        let content = self
            .service
            .retrieve_content(RetrieveWikiPublicContentRequest {
                scope: self.scope,
                publication_uuid: publication_uuid.to_owned(),
                content_handle: content_handle.to_owned(),
            })
            .await
            .map_err(map_provider_error)?;
        Ok(content.bytes)
    }

    async fn retrieve_content_stream(
        &self,
        publication_uuid: &str,
        content_handle: &str,
    ) -> Result<OpenedWikiContentStream, SdkworkError> {
        let bytes = self.retrieve_content(publication_uuid, content_handle).await?;
        let content_length = u64::try_from(bytes.len())
            .map_err(|_| http_status(502, "Wiki content length is invalid"))?;
        Ok(OpenedWikiContentStream {
            content_length: Some(content_length),
            stream: Box::new(VecChunkStream {
                bytes,
                offset: 0,
            }),
        })
    }

    async fn list_navigation(
        &self,
        publication_uuid: &str,
        locale: Option<&str>,
        cursor: Option<&str>,
        page_size: i64,
    ) -> Result<WikiPublicPageListData, SdkworkError> {
        let page = self
            .service
            .list_navigation(ListWikiPublicNavigationPageRequest {
                scope: self.scope,
                publication_uuid: publication_uuid.to_owned(),
                locale: locale.map(str::to_owned),
                cursor: cursor.map(str::to_owned),
                page_size: Some(page_size_from_i64(page_size)?),
            })
            .await
            .map_err(map_provider_error)?;
        map_page_list(page)
    }

    async fn search_pages(
        &self,
        publication_uuid: &str,
        query: &str,
        locale: Option<&str>,
        cursor: Option<&str>,
        page_size: i64,
    ) -> Result<WikiPublicPageListData, SdkworkError> {
        let page = self
            .service
            .search_pages(SearchWikiPublicPageRequest {
                scope: self.scope,
                publication_uuid: publication_uuid.to_owned(),
                query: query.to_owned(),
                locale: locale.map(str::to_owned),
                cursor: cursor.map(str::to_owned),
                page_size: Some(page_size_from_i64(page_size)?),
            })
            .await
            .map_err(map_provider_error)?;
        map_page_list(page)
    }
}

/// Splits an already-materialized Wiki body into bounded transport chunks; the
/// Knowledgebase provider port materializes rendered representations itself, so
/// there is nothing left to stream from storage.
struct VecChunkStream {
    bytes: Vec<u8>,
    offset: usize,
}

#[async_trait]
impl WikiContentChunkStream for VecChunkStream {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, SdkworkError> {
        if self.offset >= self.bytes.len() {
            return Ok(None);
        }
        let end = (self.offset + CONTENT_CHUNK_BYTES).min(self.bytes.len());
        let chunk = self.bytes[self.offset..end].to_vec();
        self.offset = end;
        Ok(Some(chunk))
    }
}

fn page_size_from_i64(page_size: i64) -> Result<u32, SdkworkError> {
    u32::try_from(page_size).map_err(|_| http_status(400, "pageSize is invalid"))
}

fn map_page_list(page: WikiPublicPageList) -> Result<WikiPublicPageListData, SdkworkError> {
    let items = page
        .items
        .into_iter()
        .map(|item| map_page(&item.page))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(WikiPublicPageListData {
        items,
        page_info: PageInfo {
            mode: PAGE_INFO_MODE_CURSOR.to_owned(),
            page_size: i64::from(page.page_size),
            has_more: page.next_cursor.is_some(),
            next_cursor: page.next_cursor,
        },
    })
}

fn map_page(
    page: &sdkwork_intelligence_knowledgebase_service::wiki_public_provider::WikiPublicPageMetadata,
) -> Result<WikiPublicPageMetadata, SdkworkError> {
    Ok(WikiPublicPageMetadata {
        projection_uuid: page.projection_uuid.clone(),
        canonical_route: page.canonical_route.clone(),
        file_kind: page.file_kind.as_str().to_owned(),
        media_type: page.media_type.clone(),
        size_bytes: page.size_bytes.to_string(),
        content_sha256: page.content_sha256.clone(),
        title: page.title.clone(),
        description: page.description.clone(),
        locale: page.locale.clone(),
        nav_order: page.nav_order.map(i64::from),
        page_public_version: page.page_public_version.to_string(),
        public_updated_at: page.public_updated_at.clone(),
    })
}

/// Status mapping of the Knowledgebase Internal API route
/// (`error.rs::From<KnowledgeWikiPublicProviderError>`).
fn map_provider_error(error: KnowledgeWikiPublicProviderError) -> SdkworkError {
    match error {
        KnowledgeWikiPublicProviderError::InvalidRequest(detail) => http_status(400, detail),
        KnowledgeWikiPublicProviderError::NotFoundOrNotPublic => {
            http_status(404, "The requested Wiki resource was not found.")
        }
        KnowledgeWikiPublicProviderError::ContentUnavailable => {
            http_status(503, "The requested Wiki representation is temporarily unavailable.")
        }
        KnowledgeWikiPublicProviderError::IntegrityFailed => {
            http_status(502, "The Wiki content provider failed integrity validation.")
        }
        KnowledgeWikiPublicProviderError::TemporarilyUnavailable => {
            http_status(503, "The Wiki provider is temporarily unavailable.")
        }
    }
}

fn http_status(status: u16, body: impl Into<String>) -> SdkworkError {
    SdkworkError::HttpStatus {
        status,
        body: body.into(),
    }
}

/// Builds the in-process transport for the gateway without exposing the
/// generated SDK or the Knowledgebase pool types.
pub fn in_process_knowledgebase_wiki_client(
    pool: PgPool,
    compatibility_pool: AnyPool,
    tenant_id: u64,
    organization_id: u64,
) -> Arc<dyn KnowledgebaseWikiSdkClient> {
    Arc::new(InProcessKnowledgebaseWikiSdkClient::new(
        pool,
        compatibility_pool,
        tenant_id,
        organization_id,
    ))
}
