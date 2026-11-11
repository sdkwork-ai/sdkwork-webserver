//! Split-deployment HTTP transport for the Knowledgebase website provider.
//!
//! MODULE_SPEC.md §4.2.3 allows a deliberate split deployment (the website data
//! plane running as its own container, without the Knowledgebase module
//! reachable in-process) to keep calling the Knowledgebase Internal API. That is
//! the only transport this module implements, it is never selected by default,
//! and the gateway selects it only through the explicit
//! `SDKWORK_WEBSERVER_KNOWLEDGEBASE_PROVIDER_MODE=http` opt-in whose endpoints
//! are validated at startup.

use async_trait::async_trait;
use sdkwork_knowledgebase_internal_sdk::{
    api::KnowledgebaseInternalWikiApi,
    http::BinaryResponseStream,
    models::{
        ResolveWikiRouteRequest, WikiPublicPageListData, WikiPublication, WikiRouteResolution,
    },
    SdkworkError,
};

use crate::sdk::{
    KnowledgebaseWikiSdkClient, OpenedWikiContentStream, WikiContentChunkStream,
};

/// Adapter that forwards the generated SDK's `BinaryResponseStream` chunk by
/// chunk, preserving its byte budget enforcement.
pub(crate) struct SdkWikiBinaryChunkStream {
    source: Option<BinaryResponseStream>,
}

impl SdkWikiBinaryChunkStream {
    pub(crate) fn new(source: BinaryResponseStream) -> Self {
        Self {
            source: Some(source),
        }
    }
}

#[async_trait]
impl WikiContentChunkStream for SdkWikiBinaryChunkStream {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, SdkworkError> {
        let Some(source) = self.source.as_mut() else {
            return Ok(None);
        };
        let chunk = source.next_chunk().await?;
        if chunk.is_none() {
            self.source = None;
        }
        Ok(chunk)
    }
}

#[async_trait]
impl KnowledgebaseWikiSdkClient for KnowledgebaseInternalWikiApi {
    async fn retrieve_publication(
        &self,
        publication_uuid: &str,
    ) -> Result<WikiPublication, SdkworkError> {
        self.wiki_publications_retrieve(publication_uuid).await
    }

    async fn resolve_route(
        &self,
        publication_uuid: &str,
        request: &ResolveWikiRouteRequest,
    ) -> Result<WikiRouteResolution, SdkworkError> {
        self.wiki_publications_routes_resolve(publication_uuid, request)
            .await
    }

    async fn retrieve_content(
        &self,
        publication_uuid: &str,
        content_handle: &str,
    ) -> Result<Vec<u8>, SdkworkError> {
        self.wiki_publications_contents_retrieve(publication_uuid, content_handle)
            .await
    }

    async fn retrieve_content_stream(
        &self,
        publication_uuid: &str,
        content_handle: &str,
    ) -> Result<OpenedWikiContentStream, SdkworkError> {
        let stream = self
            .wiki_publications_contents_retrieve_stream(publication_uuid, content_handle)
            .await?;
        let content_length = stream.content_length();
        Ok(OpenedWikiContentStream {
            content_length,
            stream: Box::new(SdkWikiBinaryChunkStream::new(stream)),
        })
    }

    async fn list_navigation(
        &self,
        publication_uuid: &str,
        locale: Option<&str>,
        cursor: Option<&str>,
        page_size: i64,
    ) -> Result<WikiPublicPageListData, SdkworkError> {
        self.wiki_publications_navigation_list(publication_uuid, locale, cursor, Some(page_size))
            .await
    }

    async fn search_pages(
        &self,
        publication_uuid: &str,
        query: &str,
        locale: Option<&str>,
        cursor: Option<&str>,
        page_size: i64,
    ) -> Result<WikiPublicPageListData, SdkworkError> {
        self.wiki_publications_pages_search(
            publication_uuid,
            query,
            locale,
            cursor,
            Some(page_size),
        )
        .await
    }
}
