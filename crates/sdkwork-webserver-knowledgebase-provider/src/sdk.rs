use std::sync::Arc;

use async_trait::async_trait;
use sdkwork_knowledgebase_internal_sdk::{
    models::{
        ResolveWikiRouteRequest, WikiPublicPageListData, WikiPublication, WikiRouteResolution,
    },
    SdkworkError,
};
use sdkwork_webserver_contract::provider::{
    WebsiteProviderError, WebsiteProviderErrorKind, WebsiteProviderResult,
};

/// Bounded chunk stream over immutable Wiki content. Implementations must
/// yield one transport chunk at a time and never materialize the whole body
/// in memory.
#[async_trait]
pub trait WikiContentChunkStream: Send {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, SdkworkError>;
}

#[async_trait]
pub trait KnowledgebaseWikiSdkClient: Send + Sync {
    async fn retrieve_publication(
        &self,
        publication_uuid: &str,
    ) -> Result<WikiPublication, SdkworkError>;

    async fn resolve_route(
        &self,
        publication_uuid: &str,
        request: &ResolveWikiRouteRequest,
    ) -> Result<WikiRouteResolution, SdkworkError>;

    async fn retrieve_content(
        &self,
        publication_uuid: &str,
        content_handle: &str,
    ) -> Result<Vec<u8>, SdkworkError>;

    /// Streaming variant of [`KnowledgebaseWikiSdkClient::retrieve_content`]:
    /// yields the wiki body in bounded transport chunks without
    /// materializing the whole payload in memory.
    async fn retrieve_content_stream(
        &self,
        publication_uuid: &str,
        content_handle: &str,
    ) -> Result<OpenedWikiContentStream, SdkworkError>;

    async fn list_navigation(
        &self,
        publication_uuid: &str,
        locale: Option<&str>,
        cursor: Option<&str>,
        page_size: i64,
    ) -> Result<WikiPublicPageListData, SdkworkError>;

    async fn search_pages(
        &self,
        publication_uuid: &str,
        query: &str,
        locale: Option<&str>,
        cursor: Option<&str>,
        page_size: i64,
    ) -> Result<WikiPublicPageListData, SdkworkError>;
}

/// Streaming wiki body with the server-declared content length (when sent).
pub struct OpenedWikiContentStream {
    pub content_length: Option<u64>,
    pub stream: Box<dyn WikiContentChunkStream>,
}

pub trait KnowledgebaseWikiSdkClientResolver: Send + Sync {
    fn resolve(
        &self,
        tenant_scope_hash: &str,
    ) -> WebsiteProviderResult<Arc<dyn KnowledgebaseWikiSdkClient>>;
}

pub struct FixedKnowledgebaseWikiSdkClientResolver {
    tenant_scope_hash: String,
    client: Arc<dyn KnowledgebaseWikiSdkClient>,
}

impl FixedKnowledgebaseWikiSdkClientResolver {
    pub fn new(
        tenant_scope_hash: impl Into<String>,
        client: Arc<dyn KnowledgebaseWikiSdkClient>,
    ) -> Result<Self, String> {
        let tenant_scope_hash = tenant_scope_hash.into();
        if tenant_scope_hash.is_empty()
            || tenant_scope_hash.len() > 256
            || tenant_scope_hash
                .bytes()
                .any(|byte| byte.is_ascii_control())
        {
            return Err(
                "tenant scope hash must be non-empty, bounded, and control-free".to_string(),
            );
        }
        Ok(Self {
            tenant_scope_hash,
            client,
        })
    }
}

impl KnowledgebaseWikiSdkClientResolver for FixedKnowledgebaseWikiSdkClientResolver {
    fn resolve(
        &self,
        tenant_scope_hash: &str,
    ) -> WebsiteProviderResult<Arc<dyn KnowledgebaseWikiSdkClient>> {
        if tenant_scope_hash != self.tenant_scope_hash {
            return Err(WebsiteProviderError::new(
                WebsiteProviderErrorKind::NotFound,
            ));
        }
        Ok(Arc::clone(&self.client))
    }
}
