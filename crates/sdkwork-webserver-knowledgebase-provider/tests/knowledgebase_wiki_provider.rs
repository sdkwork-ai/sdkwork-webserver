use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU16, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use async_trait::async_trait;
use sdkwork_knowledgebase_internal_sdk::{
    models::{
        PageInfo, ResolveWikiRouteRequest, WikiPublicPageListData, WikiPublicPageMetadata,
        WikiPublication, WikiRouteResolution,
    },
    SdkworkError,
};
use sdkwork_webserver_contract::provider::{
    OpenWebsiteContentRequest, ProviderResourceReference, ResolveWebsiteWikiRouteRequest,
    ValidateWebsiteResourceRequest, WebsiteByteRange, WebsiteProviderContentHandle,
    WebsiteProviderErrorKind, WebsiteProviderPageSize, WebsiteProviderPurpose,
    WebsiteProviderRuntimeContext, WebsiteRequestConditions, WebsiteResourceCapabilities,
    WebsiteResourceProvider, WebsiteWikiCollectionRequest, WebsiteWikiContentKind,
    WebsiteWikiProvider, WebsiteWikiRouteResolution,
};
use sdkwork_webserver_core::website_runtime::WebsiteProviderType;
use sdkwork_webserver_knowledgebase_provider::{
    FixedKnowledgebaseWikiSdkClientResolver, KnowledgebaseWikiSdkClient,
    KnowledgebaseWikiWebsiteProvider, OpenedWikiContentStream, WikiContentChunkStream,
    KNOWLEDGEBASE_WIKI_PROVIDER_CONTRACT_VERSION,
};

struct TestWikiChunkStream {
    chunks: VecDeque<Vec<u8>>,
}

#[async_trait]
impl WikiContentChunkStream for TestWikiChunkStream {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, SdkworkError> {
        Ok(self.chunks.pop_front())
    }
}

const PUBLICATION_UUID: &str = "11111111-1111-4111-8111-111111111501";
const PROJECTION_UUID: &str = "11111111-1111-4111-8111-111111111601";
const CONTENT_SHA256: &str =
    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

struct FakeKnowledgebaseWikiSdk {
    publication: Mutex<WikiPublication>,
    resolution: Mutex<WikiRouteResolution>,
    content: Mutex<Vec<u8>>,
    navigation: Mutex<WikiPublicPageListData>,
    search: Mutex<WikiPublicPageListData>,
    next_publication_status: AtomicU16,
    publication_delay_ms: AtomicU64,
    publication_calls: AtomicUsize,
    route_calls: AtomicUsize,
    content_calls: AtomicUsize,
    last_search_query: Mutex<Option<String>>,
}

impl FakeKnowledgebaseWikiSdk {
    fn new() -> Self {
        let page = public_wiki_page();
        Self {
            publication: Mutex::new(wiki_publication()),
            resolution: Mutex::new(WikiRouteResolution {
                disposition: "PAGE".to_string(),
                page: Some(page.clone()),
                content_handle: Some("opaque-content-handle".to_string()),
                requested_route: None,
                canonical_route: None,
                status: None,
                page_public_version: None,
            }),
            content: Mutex::new(b"# Wiki".to_vec()),
            navigation: Mutex::new(page_list(vec![page.clone()], Some("next-navigation"))),
            search: Mutex::new(page_list(vec![page], None)),
            next_publication_status: AtomicU16::new(0),
            publication_delay_ms: AtomicU64::new(0),
            publication_calls: AtomicUsize::new(0),
            route_calls: AtomicUsize::new(0),
            content_calls: AtomicUsize::new(0),
            last_search_query: Mutex::new(None),
        }
    }

    fn fail_next_publication_with(&self, status: u16) {
        self.next_publication_status
            .store(status, Ordering::Release);
    }
}

#[async_trait]
impl KnowledgebaseWikiSdkClient for FakeKnowledgebaseWikiSdk {
    async fn retrieve_publication(
        &self,
        _publication_uuid: &str,
    ) -> Result<WikiPublication, SdkworkError> {
        self.publication_calls.fetch_add(1, Ordering::AcqRel);
        let delay_ms = self.publication_delay_ms.load(Ordering::Acquire);
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
        let status = self.next_publication_status.swap(0, Ordering::AcqRel);
        if status > 0 {
            return Err(SdkworkError::HttpStatus {
                status,
                body: "{}".to_string(),
            });
        }
        Ok(self.publication.lock().expect("publication lock").clone())
    }

    async fn resolve_route(
        &self,
        _publication_uuid: &str,
        _request: &ResolveWikiRouteRequest,
    ) -> Result<WikiRouteResolution, SdkworkError> {
        self.route_calls.fetch_add(1, Ordering::AcqRel);
        Ok(self.resolution.lock().expect("resolution lock").clone())
    }

    async fn retrieve_content(
        &self,
        _publication_uuid: &str,
        _content_handle: &str,
    ) -> Result<Vec<u8>, SdkworkError> {
        self.content_calls.fetch_add(1, Ordering::AcqRel);
        Ok(self.content.lock().expect("content lock").clone())
    }

    async fn retrieve_content_stream(
        &self,
        _publication_uuid: &str,
        _content_handle: &str,
    ) -> Result<OpenedWikiContentStream, SdkworkError> {
        self.content_calls.fetch_add(1, Ordering::AcqRel);
        let content = self.content.lock().expect("content lock").clone();
        let content_length = u64::try_from(content.len()).ok();
        let chunks = content
            .chunks(4)
            .map(<[u8]>::to_vec)
            .collect::<VecDeque<_>>();
        Ok(OpenedWikiContentStream {
            content_length,
            stream: Box::new(TestWikiChunkStream { chunks }),
        })
    }

    async fn list_navigation(
        &self,
        _publication_uuid: &str,
        _locale: Option<&str>,
        _cursor: Option<&str>,
        _page_size: i64,
    ) -> Result<WikiPublicPageListData, SdkworkError> {
        Ok(self.navigation.lock().expect("navigation lock").clone())
    }

    async fn search_pages(
        &self,
        _publication_uuid: &str,
        query: &str,
        _locale: Option<&str>,
        _cursor: Option<&str>,
        _page_size: i64,
    ) -> Result<WikiPublicPageListData, SdkworkError> {
        *self.last_search_query.lock().expect("search query lock") = Some(query.to_string());
        Ok(self.search.lock().expect("search lock").clone())
    }
}

#[tokio::test]
async fn validates_active_publication_capabilities_and_tenant_bound_client() {
    let (provider, sdk) = test_provider();
    let validated = provider
        .validate_resource(&validate_request(false))
        .await
        .expect("active publication");
    assert_eq!(validated.provider_resource_uuid, PUBLICATION_UUID);
    assert_eq!(validated.provider_generation, "3");
    assert_eq!(
        validated.public_generation,
        "p=3;n=4;s=5;renderer=renderer-v2;theme=theme-v1"
    );
    assert_eq!(
        validated.capabilities,
        WebsiteResourceCapabilities {
            static_content: true,
            wiki_routes: true,
            wiki_search: true,
            range_requests: false,
        }
    );

    let range_error = provider
        .validate_resource(&validate_request(true))
        .await
        .expect_err("range capability must not be advertised");
    assert_eq!(range_error.kind, WebsiteProviderErrorKind::ContractMismatch);

    let mut wrong_scope = validate_request(false);
    wrong_scope.context.tenant_scope_hash = "another-tenant".to_string();
    let scope_error = provider
        .validate_resource(&wrong_scope)
        .await
        .expect_err("tenant-bound client must fail closed");
    assert_eq!(scope_error.kind, WebsiteProviderErrorKind::NotFound);
    assert_eq!(sdk.publication_calls.load(Ordering::Acquire), 2);
}

#[tokio::test]
async fn resolves_pages_redirects_and_http_conditions_with_versioned_metadata() {
    let (provider, sdk) = test_provider();
    let resolved = provider
        .resolve_wiki_route(&resolve_request(WebsiteRequestConditions::default()))
        .await
        .expect("resolve page");
    let WebsiteWikiRouteResolution::Content(content) = resolved else {
        panic!("expected Wiki content");
    };
    assert_eq!(content.kind, WebsiteWikiContentKind::Asset);
    assert_eq!(content.canonical_route, "/guide/");
    assert_eq!(content.page_uuid.as_deref(), Some(PROJECTION_UUID));
    assert_eq!(content.public_page_version, "7");
    assert_eq!(content.renderer_version, "renderer-v2");
    assert_eq!(content.metadata.content_length, 6);
    assert_eq!(content.metadata.content_type, "text/markdown");
    assert_eq!(content.metadata.etag, format!("\"{CONTENT_SHA256}-v7\""));
    assert_eq!(
        content.metadata.last_modified,
        "Tue, 21 Jul 2026 00:00:00 GMT"
    );
    assert!(!content.metadata.range_supported);

    let not_modified = provider
        .resolve_wiki_route(&resolve_request(WebsiteRequestConditions {
            if_none_match: Some(content.metadata.etag.clone()),
            ..WebsiteRequestConditions::default()
        }))
        .await
        .expect("conditional route");
    assert_eq!(not_modified, WebsiteWikiRouteResolution::NotModified);

    let precondition = provider
        .resolve_wiki_route(&resolve_request(WebsiteRequestConditions {
            if_match: Some("\"different\"".to_string()),
            ..WebsiteRequestConditions::default()
        }))
        .await
        .expect_err("If-Match mismatch");
    assert_eq!(
        precondition.kind,
        WebsiteProviderErrorKind::PreconditionFailed
    );

    *sdk.resolution.lock().expect("resolution lock") = WikiRouteResolution {
        disposition: "REDIRECT".to_string(),
        page: None,
        content_handle: None,
        requested_route: Some("/old-guide/".to_string()),
        canonical_route: Some("/guide/".to_string()),
        status: Some(308),
        page_public_version: Some("7".to_string()),
    };
    let redirect = provider
        .resolve_wiki_route(&resolve_request(WebsiteRequestConditions::default()))
        .await
        .expect("resolve redirect");
    assert_eq!(
        redirect,
        WebsiteWikiRouteResolution::Redirect(
            sdkwork_webserver_contract::provider::WebsiteWikiRedirect {
                status_code: 308,
                canonical_route: "/guide/".to_string(),
            }
        )
    );
}

#[tokio::test]
async fn opens_revalidated_bounded_content_and_rejects_range_or_oversize() {
    let (provider, sdk) = test_provider();
    let mut opened = provider
        .open_wiki_content(&open_request(None, 1024))
        .await
        .expect("open content");
    assert_eq!(opened.content_length, 6);
    assert_eq!(opened.content_range, None);
    let mut collected = Vec::new();
    while let Some(chunk) = opened.stream.next_chunk().await.expect("content chunk") {
        collected.extend_from_slice(&chunk);
    }
    assert_eq!(collected, b"# Wiki".to_vec());
    assert_eq!(opened.stream.next_chunk().await.expect("end stream"), None);

    let range = provider
        .open_wiki_content(&open_request(
            Some(WebsiteByteRange {
                start: 0,
                end_inclusive: Some(2),
                suffix_bytes: None,
            }),
            1024,
        ))
        .await
        .err()
        .expect("range must fail");
    assert_eq!(range.kind, WebsiteProviderErrorKind::ContractMismatch);

    let oversized = provider
        .open_wiki_content(&open_request(None, 3))
        .await
        .err()
        .expect("bounded open must fail");
    assert_eq!(oversized.kind, WebsiteProviderErrorKind::ContractMismatch);
    assert_eq!(sdk.content_calls.load(Ordering::Acquire), 2);
}

#[tokio::test]
async fn maps_navigation_and_search_with_owner_generations_and_bounded_query() {
    let (provider, sdk) = test_provider();
    let navigation = provider
        .retrieve_navigation(&collection_request(None))
        .await
        .expect("navigation");
    assert_eq!(navigation.generation, "4");
    assert_eq!(navigation.next_cursor.as_deref(), Some("next-navigation"));
    assert_eq!(navigation.items[0].page_uuid, PROJECTION_UUID);

    let search = provider
        .search_wiki(&collection_request(Some("  Guide  ")))
        .await
        .expect("search");
    assert_eq!(search.generation, "5");
    assert_eq!(search.items[0].title, "Guide");
    assert_eq!(
        sdk.last_search_query
            .lock()
            .expect("search query lock")
            .as_deref(),
        Some("Guide")
    );

    let invalid = provider
        .search_wiki(&collection_request(Some("   ")))
        .await
        .expect_err("empty query");
    assert_eq!(invalid.kind, WebsiteProviderErrorKind::InvalidPath);
}

#[tokio::test]
async fn maps_generated_sdk_statuses_and_adapter_deadlines_to_provider_errors() {
    let (provider, sdk) = test_provider();
    for (status, expected) in [
        (404, WebsiteProviderErrorKind::NotFound),
        (429, WebsiteProviderErrorKind::RateLimited),
        (503, WebsiteProviderErrorKind::Unavailable),
        (401, WebsiteProviderErrorKind::ContractMismatch),
    ] {
        sdk.fail_next_publication_with(status);
        let error = provider
            .validate_resource(&validate_request(false))
            .await
            .expect_err("mapped SDK status");
        assert_eq!(error.kind, expected);
    }

    sdk.publication_delay_ms.store(25, Ordering::Release);
    let mut request = validate_request(false);
    request.context.deadline_ms = 1;
    let timeout = provider
        .validate_resource(&request)
        .await
        .expect_err("provider deadline");
    assert_eq!(timeout.kind, WebsiteProviderErrorKind::DeadlineExceeded);
}

#[tokio::test]
async fn rejects_noncanonical_routes_before_calling_the_sdk() {
    let (provider, sdk) = test_provider();
    let mut request = resolve_request(WebsiteRequestConditions::default());
    request.route = "/docs/%2e%2e/private".to_string();
    let error = provider
        .resolve_wiki_route(&request)
        .await
        .expect_err("invalid route");
    assert_eq!(error.kind, WebsiteProviderErrorKind::InvalidPath);
    assert_eq!(sdk.publication_calls.load(Ordering::Acquire), 0);
    assert_eq!(sdk.route_calls.load(Ordering::Acquire), 0);
}

fn test_provider() -> (
    KnowledgebaseWikiWebsiteProvider,
    Arc<FakeKnowledgebaseWikiSdk>,
) {
    let sdk = Arc::new(FakeKnowledgebaseWikiSdk::new());
    let client: Arc<dyn KnowledgebaseWikiSdkClient> = sdk.clone();
    let resolver = FixedKnowledgebaseWikiSdkClientResolver::new("tenant-scope", client)
        .expect("fixed resolver");
    (
        KnowledgebaseWikiWebsiteProvider::new(Arc::new(resolver)),
        sdk,
    )
}

fn context() -> WebsiteProviderRuntimeContext {
    WebsiteProviderRuntimeContext {
        tenant_scope_hash: "tenant-scope".to_string(),
        app_uuid: "site-1".to_string(),
        binding_uuid: "binding-1".to_string(),
        variant_uuid: "variant-1".to_string(),
        mount_uuid: "mount-1".to_string(),
        resource_uuid: "resource-1".to_string(),
        request_id: "request-1".to_string(),
        trace_id: "trace-1".to_string(),
        deadline_ms: 1_000,
        purpose: WebsiteProviderPurpose::Request,
    }
}

fn reference() -> ProviderResourceReference {
    ProviderResourceReference {
        provider_type: WebsiteProviderType::Knowledgebase,
        provider_resource_uuid: PUBLICATION_UUID.to_string(),
        provider_contract_version: KNOWLEDGEBASE_WIKI_PROVIDER_CONTRACT_VERSION.to_string(),
    }
}

fn validate_request(range_requests: bool) -> ValidateWebsiteResourceRequest {
    ValidateWebsiteResourceRequest {
        context: context(),
        provider: reference(),
        required_capabilities: WebsiteResourceCapabilities {
            static_content: true,
            wiki_routes: true,
            wiki_search: true,
            range_requests,
        },
    }
}

fn resolve_request(conditions: WebsiteRequestConditions) -> ResolveWebsiteWikiRouteRequest {
    ResolveWebsiteWikiRouteRequest {
        context: context(),
        provider: reference(),
        route: "/guide/".to_string(),
        locale: Some("zh-CN".to_string()),
        conditions,
    }
}

fn open_request(range: Option<WebsiteByteRange>, maximum_bytes: u64) -> OpenWebsiteContentRequest {
    OpenWebsiteContentRequest {
        context: context(),
        provider: reference(),
        provider_relative_path: "/guide/".to_string(),
        content_handle: WebsiteProviderContentHandle::new("opaque-content-handle")
            .expect("content handle"),
        range,
        conditions: WebsiteRequestConditions::default(),
        maximum_bytes,
    }
}

fn collection_request(query: Option<&str>) -> WebsiteWikiCollectionRequest {
    WebsiteWikiCollectionRequest {
        context: context(),
        provider: reference(),
        query: query.map(str::to_string),
        locale: Some("zh-CN".to_string()),
        cursor: None,
        page_size: WebsiteProviderPageSize::try_from(20).expect("page size"),
    }
}

fn wiki_publication() -> WikiPublication {
    WikiPublication {
        publication_uuid: PUBLICATION_UUID.to_string(),
        title: "SDKWork Wiki".to_string(),
        description: None,
        homepage_source_path: "README.md".to_string(),
        default_locale: "zh-CN".to_string(),
        supported_locales: vec!["zh-CN".to_string()],
        navigation_mode: "DIRECTORY".to_string(),
        theme_key: "sdkwork-wiki".to_string(),
        theme_version: "theme-v1".to_string(),
        renderer_policy_version: "renderer-v2".to_string(),
        search_enabled: true,
        robots_policy: "INDEX_FOLLOW".to_string(),
        sitemap_enabled: true,
        provider_generation: "3".to_string(),
        navigation_generation: "4".to_string(),
        search_generation: "5".to_string(),
    }
}

fn public_wiki_page() -> WikiPublicPageMetadata {
    WikiPublicPageMetadata {
        projection_uuid: PROJECTION_UUID.to_string(),
        canonical_route: "/guide/".to_string(),
        file_kind: "PAGE".to_string(),
        media_type: "text/markdown".to_string(),
        size_bytes: "6".to_string(),
        content_sha256: CONTENT_SHA256.to_string(),
        title: Some("Guide".to_string()),
        description: Some("Wiki guide".to_string()),
        locale: Some("zh-CN".to_string()),
        nav_order: Some(1),
        page_public_version: "7".to_string(),
        public_updated_at: "2026-07-21T00:00:00Z".to_string(),
    }
}

fn page_list(
    items: Vec<WikiPublicPageMetadata>,
    next_cursor: Option<&str>,
) -> WikiPublicPageListData {
    WikiPublicPageListData {
        items,
        page_info: PageInfo {
            mode: "cursor".to_string(),
            page_size: 20,
            next_cursor: next_cursor.map(str::to_string),
            has_more: next_cursor.is_some(),
        },
    }
}
