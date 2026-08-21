use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use sdkwork_webserver_contract::provider::{
    OpenWebsiteContentRequest, OpenedWebsiteContent, ResolveWebsiteStaticPathRequest,
    ResolveWebsiteWikiRouteRequest, ResolvedWebsiteWikiContent, ValidateWebsiteResourceRequest,
    ValidatedWebsiteResource, WebsiteByteRange, WebsiteContentMetadata, WebsiteContentRange,
    WebsiteContentResolution, WebsiteProviderContentHandle, WebsiteProviderContentStream,
    WebsiteProviderError, WebsiteProviderErrorKind, WebsiteProviderResult,
    WebsiteRequestConditions, WebsiteResourceProvider, WebsiteStaticContentProvider,
    WebsiteWikiContentKind, WebsiteWikiProvider, WebsiteWikiRouteResolution,
};
use sdkwork_webserver_core::{
    config::ProviderCachePolicy,
    website_runtime::{ProviderResourceReference, WebsiteProviderType},
};
use sdkwork_webserver_delivery_runtime::{
    AppConfigProviderPolicy, AppConfigProviderRequest, AppConfigResourceExecutor,
    AppConfigResourceHandler, AppConfigResourceRoute, WebsiteDeliveryMethod,
    WebsiteDeliveryOutcome, WebsiteProviderRegistry,
};

const TENANT_SCOPE_HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";

fn content_handle(value: &str) -> WebsiteProviderContentHandle {
    WebsiteProviderContentHandle::new(format!("h:{value}")).expect("valid content handle")
}

struct FakeFile {
    content_type: String,
    bytes: Vec<u8>,
}

struct FakeStaticProvider {
    files: Mutex<HashMap<String, FakeFile>>,
    resolved: std::sync::atomic::AtomicUsize,
}

impl FakeStaticProvider {
    fn new(files: Vec<(&str, &str, &[u8])>) -> Self {
        let files = files
            .into_iter()
            .map(|(path, content_type, bytes)| {
                (
                    path.to_owned(),
                    FakeFile {
                        content_type: content_type.to_owned(),
                        bytes: bytes.to_vec(),
                    },
                )
            })
            .collect();
        Self {
            files: Mutex::new(files),
            resolved: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

struct ChunkStream {
    chunks: VecDeque<Vec<u8>>,
}

#[async_trait]
impl WebsiteProviderContentStream for ChunkStream {
    async fn next_chunk(&mut self) -> WebsiteProviderResult<Option<Vec<u8>>> {
        Ok(self.chunks.pop_front())
    }
}

#[async_trait]
impl WebsiteResourceProvider for FakeStaticProvider {
    fn maximum_content_bytes(&self) -> u64 {
        1024 * 1024
    }

    async fn validate_resource(
        &self,
        _request: &ValidateWebsiteResourceRequest,
    ) -> WebsiteProviderResult<ValidatedWebsiteResource> {
        Err(WebsiteProviderError::new(
            WebsiteProviderErrorKind::Unavailable,
        ))
    }
}

#[async_trait]
impl WebsiteStaticContentProvider for FakeStaticProvider {
    async fn resolve_static_path(
        &self,
        request: &ResolveWebsiteStaticPathRequest,
    ) -> WebsiteProviderResult<WebsiteContentResolution> {
        self.resolved
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let files = self.files.lock().unwrap();
        let Some(file) = files.get(&request.provider_relative_path) else {
            return Err(WebsiteProviderError::new(
                WebsiteProviderErrorKind::NotFound,
            ));
        };
        let etag = format!(
            "sha256:{}-{}",
            request.provider_relative_path.len(),
            file.bytes.len()
        );
        Ok(WebsiteContentResolution::Found(
            sdkwork_webserver_contract::provider::ResolvedWebsiteContent {
                content_handle: content_handle(&request.provider_relative_path),
                metadata: WebsiteContentMetadata {
                    content_type: file.content_type.clone(),
                    content_length: file.bytes.len() as u64,
                    etag,
                    last_modified: "Wed, 21 Oct 2015 07:28:00 GMT".to_owned(),
                    content_version: "v1".to_owned(),
                    provider_generation: "1".to_owned(),
                    range_supported: true,
                },
            },
        ))
    }

    async fn open_static_content(
        &self,
        request: &OpenWebsiteContentRequest,
    ) -> WebsiteProviderResult<OpenedWebsiteContent> {
        let files = self.files.lock().unwrap();
        let Some(file) = files.get(&request.provider_relative_path) else {
            return Err(WebsiteProviderError::new(
                WebsiteProviderErrorKind::NotFound,
            ));
        };
        let (start, end) = match request.range {
            Some(WebsiteByteRange {
                start,
                end_inclusive,
                ..
            }) => {
                let end = end_inclusive.unwrap_or(file.bytes.len() as u64 - 1);
                (start, end)
            }
            None => (0, file.bytes.len() as u64 - 1),
        };
        let bytes = file.bytes[start as usize..=end as usize].to_vec();
        Ok(OpenedWebsiteContent {
            stream: Box::new(ChunkStream {
                chunks: VecDeque::from([bytes]),
            }),
            content_length: (end - start + 1),
            content_range: request.range.map(|_| WebsiteContentRange {
                start,
                end_inclusive: end,
                complete_length: file.bytes.len() as u64,
            }),
        })
    }
}

struct FakeWikiProvider {
    pages: Mutex<HashMap<String, Vec<u8>>>,
    redirects: Mutex<HashMap<String, (u16, String)>>,
}

impl FakeWikiProvider {
    fn new(pages: Vec<(&str, &[u8])>, redirects: Vec<(&str, u16, &str)>) -> Self {
        Self {
            pages: Mutex::new(
                pages
                    .into_iter()
                    .map(|(route, bytes)| (route.to_owned(), bytes.to_vec()))
                    .collect(),
            ),
            redirects: Mutex::new(
                redirects
                    .into_iter()
                    .map(|(route, status, canonical)| {
                        (route.to_owned(), (status, canonical.to_owned()))
                    })
                    .collect(),
            ),
        }
    }
}

#[async_trait]
impl WebsiteResourceProvider for FakeWikiProvider {
    fn maximum_content_bytes(&self) -> u64 {
        1024 * 1024
    }

    async fn validate_resource(
        &self,
        _request: &ValidateWebsiteResourceRequest,
    ) -> WebsiteProviderResult<ValidatedWebsiteResource> {
        Err(WebsiteProviderError::new(
            WebsiteProviderErrorKind::Unavailable,
        ))
    }
}

#[async_trait]
impl WebsiteWikiProvider for FakeWikiProvider {
    async fn resolve_wiki_route(
        &self,
        request: &ResolveWebsiteWikiRouteRequest,
    ) -> WebsiteProviderResult<WebsiteWikiRouteResolution> {
        if let Some((status, canonical)) =
            self.redirects.lock().unwrap().get(&request.route).cloned()
        {
            return Ok(WebsiteWikiRouteResolution::Redirect(
                sdkwork_webserver_contract::provider::WebsiteWikiRedirect {
                    status_code: status,
                    canonical_route: canonical,
                },
            ));
        }
        let pages = self.pages.lock().unwrap();
        let Some(bytes) = pages.get(&request.route) else {
            return Err(WebsiteProviderError::new(
                WebsiteProviderErrorKind::NotFound,
            ));
        };
        Ok(WebsiteWikiRouteResolution::Content(Box::new(
            ResolvedWebsiteWikiContent {
                content_handle: content_handle(&request.route),
                kind: WebsiteWikiContentKind::Html,
                canonical_route: request.route.clone(),
                page_uuid: Some("page-1".to_owned()),
                public_page_version: "3".to_owned(),
                renderer_version: "2".to_owned(),
                navigation_generation: "1".to_owned(),
                search_generation: "1".to_owned(),
                metadata: WebsiteContentMetadata {
                    content_type: "text/html; charset=utf-8".to_owned(),
                    content_length: bytes.len() as u64,
                    etag: format!("sha256:{}", bytes.len()),
                    last_modified: "Wed, 21 Oct 2015 07:28:00 GMT".to_owned(),
                    content_version: "v3".to_owned(),
                    provider_generation: "1".to_owned(),
                    range_supported: false,
                },
            },
        )))
    }

    async fn open_wiki_content(
        &self,
        request: &OpenWebsiteContentRequest,
    ) -> WebsiteProviderResult<OpenedWebsiteContent> {
        let pages = self.pages.lock().unwrap();
        let Some(bytes) = pages.get(&request.provider_relative_path) else {
            return Err(WebsiteProviderError::new(
                WebsiteProviderErrorKind::NotFound,
            ));
        };
        Ok(OpenedWebsiteContent {
            stream: Box::new(ChunkStream {
                chunks: VecDeque::from([bytes.clone()]),
            }),
            content_length: bytes.len() as u64,
            content_range: None,
        })
    }

    async fn retrieve_navigation(
        &self,
        _request: &sdkwork_webserver_contract::provider::WebsiteWikiCollectionRequest,
    ) -> WebsiteProviderResult<sdkwork_webserver_contract::provider::WebsiteWikiCollectionPage>
    {
        Err(WebsiteProviderError::new(
            WebsiteProviderErrorKind::NotFound,
        ))
    }

    async fn search_wiki(
        &self,
        _request: &sdkwork_webserver_contract::provider::WebsiteWikiCollectionRequest,
    ) -> WebsiteProviderResult<sdkwork_webserver_contract::provider::WebsiteWikiCollectionPage>
    {
        Err(WebsiteProviderError::new(
            WebsiteProviderErrorKind::NotFound,
        ))
    }
}

fn static_route(path: &str) -> AppConfigResourceRoute {
    AppConfigResourceRoute {
        virtual_host_id: "vh".to_owned(),
        route_id: "route".to_owned(),
        resource_id: "docs-drive".to_owned(),
        provider: ProviderResourceReference {
            provider_type: WebsiteProviderType::Drive,
            provider_resource_uuid: "root-1234".to_owned(),
            provider_contract_version: "drive.website-root.v1".to_owned(),
        },
        handler: AppConfigResourceHandler::Static,
        provider_relative_path: path.to_owned(),
        index_files: vec!["index.html".to_owned()],
        spa_fallback: Some("index.html".to_owned()),
        directory_request: path.ends_with('/'),
        locale: None,
        cache: ProviderCachePolicy::default(),
    }
}

fn wiki_route(path: &str) -> AppConfigResourceRoute {
    AppConfigResourceRoute {
        virtual_host_id: "vh".to_owned(),
        route_id: "route".to_owned(),
        resource_id: "kb-docs".to_owned(),
        provider: ProviderResourceReference {
            provider_type: WebsiteProviderType::Knowledgebase,
            provider_resource_uuid: "wiki-pub-1234".to_owned(),
            provider_contract_version: "knowledgebase.wiki-publication.v1".to_owned(),
        },
        handler: AppConfigResourceHandler::Wiki,
        provider_relative_path: path.to_owned(),
        index_files: Vec::new(),
        spa_fallback: None,
        directory_request: false,
        locale: None,
        cache: ProviderCachePolicy::default(),
    }
}

fn request(method: WebsiteDeliveryMethod) -> AppConfigProviderRequest {
    AppConfigProviderRequest {
        method,
        request_id: "req-1".to_owned(),
        trace_id: "trace-1".to_owned(),
        conditions: WebsiteRequestConditions::default(),
        range: None,
        locale: None,
        spa_fallback_eligible: false,
    }
}

fn policy() -> AppConfigProviderPolicy {
    AppConfigProviderPolicy {
        provider_timeout_ms: 5_000,
        maximum_object_bytes: 1024 * 1024,
    }
}

#[tokio::test]
async fn serve_static_returns_found_content() {
    let provider = Arc::new(FakeStaticProvider::new(vec![(
        "/index.html",
        "text/html",
        b"<html>hello</html>",
    )]));
    let mut registry = WebsiteProviderRegistry::new();
    registry
        .register_static(WebsiteProviderType::Drive, provider)
        .unwrap();
    let executor = AppConfigResourceExecutor::new(
        Arc::new(registry),
        "sdkwork-test-web".to_owned(),
        TENANT_SCOPE_HASH.to_owned(),
    )
    .unwrap();

    let outcome = executor
        .serve_static(
            &static_route("/index.html"),
            &policy(),
            &request(WebsiteDeliveryMethod::Get),
        )
        .await
        .unwrap();
    let sdkwork_webserver_delivery_runtime::WebsiteDeliveryOutcome::Content(content) = outcome
    else {
        panic!("expected Content outcome");
    };
    assert_eq!(content.metadata.content_type, "text/html");
    assert_eq!(content.response_content_length, 18);
    assert!(content.content_range.is_none());
}

#[tokio::test]
async fn serve_static_head_suppresses_body() {
    let provider = Arc::new(FakeStaticProvider::new(vec![(
        "/index.html",
        "text/html",
        b"<html>hello</html>",
    )]));
    let mut registry = WebsiteProviderRegistry::new();
    registry
        .register_static(WebsiteProviderType::Drive, provider)
        .unwrap();
    let executor = AppConfigResourceExecutor::new(
        Arc::new(registry),
        "sdkwork-test-web".to_owned(),
        TENANT_SCOPE_HASH.to_owned(),
    )
    .unwrap();

    let outcome = executor
        .serve_static(
            &static_route("/index.html"),
            &policy(),
            &request(WebsiteDeliveryMethod::Head),
        )
        .await
        .unwrap();
    let WebsiteDeliveryOutcome::Content(content) = outcome else {
        panic!("expected Content outcome");
    };
    assert!(content.body.is_none());
    assert_eq!(content.response_content_length, 18);
}

#[tokio::test]
async fn serve_static_missing_path_falls_back_to_spa() {
    let provider = Arc::new(FakeStaticProvider::new(vec![(
        "/index.html",
        "text/html",
        b"<html>app</html>",
    )]));
    let mut registry = WebsiteProviderRegistry::new();
    registry
        .register_static(WebsiteProviderType::Drive, provider)
        .unwrap();
    let executor = AppConfigResourceExecutor::new(
        Arc::new(registry),
        "sdkwork-test-web".to_owned(),
        TENANT_SCOPE_HASH.to_owned(),
    )
    .unwrap();
    let mut req = request(WebsiteDeliveryMethod::Get);
    req.spa_fallback_eligible = true;

    let outcome = executor
        .serve_static(&static_route("/app/route"), &policy(), &req)
        .await
        .unwrap();
    let WebsiteDeliveryOutcome::Content(content) = outcome else {
        panic!("expected Content outcome");
    };
    assert_eq!(content.route.provider_relative_path, "/index.html");
    assert_eq!(content.response_content_length, 16);
}

#[tokio::test]
async fn serve_static_all_candidates_missing_is_not_found() {
    let provider = Arc::new(FakeStaticProvider::new(Vec::new()));
    let mut registry = WebsiteProviderRegistry::new();
    registry
        .register_static(WebsiteProviderType::Drive, provider)
        .unwrap();
    let executor = AppConfigResourceExecutor::new(
        Arc::new(registry),
        "sdkwork-test-web".to_owned(),
        TENANT_SCOPE_HASH.to_owned(),
    )
    .unwrap();

    let outcome = executor
        .serve_static(
            &static_route("/missing"),
            &policy(),
            &request(WebsiteDeliveryMethod::Get),
        )
        .await
        .unwrap();
    assert!(matches!(outcome, WebsiteDeliveryOutcome::NotFound));
}

#[tokio::test]
async fn serve_static_range_returns_partial_content() {
    let provider = Arc::new(FakeStaticProvider::new(vec![(
        "/big.bin",
        "application/octet-stream",
        b"0123456789abcdef",
    )]));
    let mut registry = WebsiteProviderRegistry::new();
    registry
        .register_static(WebsiteProviderType::Drive, provider)
        .unwrap();
    let executor = AppConfigResourceExecutor::new(
        Arc::new(registry),
        "sdkwork-test-web".to_owned(),
        TENANT_SCOPE_HASH.to_owned(),
    )
    .unwrap();
    let mut req = request(WebsiteDeliveryMethod::Get);
    req.range = Some(WebsiteByteRange {
        start: 2,
        end_inclusive: Some(5),
        suffix_bytes: None,
    });

    let outcome = executor
        .serve_static(&static_route("/big.bin"), &policy(), &req)
        .await
        .unwrap();
    let WebsiteDeliveryOutcome::Content(content) = outcome else {
        panic!("expected Content outcome");
    };
    assert_eq!(content.response_content_length, 4);
    assert_eq!(
        content.content_range,
        Some(WebsiteContentRange {
            start: 2,
            end_inclusive: 5,
            complete_length: 16,
        })
    );
}

#[tokio::test]
async fn serve_wiki_returns_page_content() {
    let provider = Arc::new(FakeWikiProvider::new(
        vec![("/getting-started", b"<h1>Start</h1>")],
        Vec::new(),
    ));
    let mut registry = WebsiteProviderRegistry::new();
    registry
        .register_wiki(WebsiteProviderType::Knowledgebase, provider)
        .unwrap();
    let executor = AppConfigResourceExecutor::new(
        Arc::new(registry),
        "sdkwork-test-web".to_owned(),
        TENANT_SCOPE_HASH.to_owned(),
    )
    .unwrap();

    let outcome = executor
        .serve_wiki(
            &wiki_route("/getting-started"),
            &policy(),
            &request(WebsiteDeliveryMethod::Get),
        )
        .await
        .unwrap();
    let WebsiteDeliveryOutcome::Content(content) = outcome else {
        panic!("expected Content outcome");
    };
    assert_eq!(content.metadata.content_type, "text/html; charset=utf-8");
    assert_eq!(content.canonical_route.as_deref(), Some("/getting-started"));
    assert_eq!(content.page_uuid.as_deref(), Some("page-1"));
}

#[tokio::test]
async fn serve_wiki_returns_redirect_outcome() {
    let provider = Arc::new(FakeWikiProvider::new(
        Vec::new(),
        vec![("/old-page", 301, "/new-page")],
    ));
    let mut registry = WebsiteProviderRegistry::new();
    registry
        .register_wiki(WebsiteProviderType::Knowledgebase, provider)
        .unwrap();
    let executor = AppConfigResourceExecutor::new(
        Arc::new(registry),
        "sdkwork-test-web".to_owned(),
        TENANT_SCOPE_HASH.to_owned(),
    )
    .unwrap();

    let outcome = executor
        .serve_wiki(
            &wiki_route("/old-page"),
            &policy(),
            &request(WebsiteDeliveryMethod::Get),
        )
        .await
        .unwrap();
    let WebsiteDeliveryOutcome::Redirect(redirect) = outcome else {
        panic!("expected Redirect outcome");
    };
    match redirect {
        sdkwork_webserver_delivery_runtime::WebsiteDeliveryRedirect::Wiki {
            status_code,
            canonical_route,
            ..
        } => {
            assert_eq!(status_code, 301);
            assert_eq!(canonical_route, "/new-page");
        }
        other => panic!("expected Wiki redirect, got {other:?}"),
    }
}

#[tokio::test]
async fn serve_wiki_missing_route_is_not_found() {
    let provider = Arc::new(FakeWikiProvider::new(Vec::new(), Vec::new()));
    let mut registry = WebsiteProviderRegistry::new();
    registry
        .register_wiki(WebsiteProviderType::Knowledgebase, provider)
        .unwrap();
    let executor = AppConfigResourceExecutor::new(
        Arc::new(registry),
        "sdkwork-test-web".to_owned(),
        TENANT_SCOPE_HASH.to_owned(),
    )
    .unwrap();

    let outcome = executor
        .serve_wiki(
            &wiki_route("/nope"),
            &policy(),
            &request(WebsiteDeliveryMethod::Get),
        )
        .await
        .unwrap();
    assert!(matches!(outcome, WebsiteDeliveryOutcome::NotFound));
}

#[tokio::test]
async fn serve_without_registered_provider_is_provider_error() {
    let registry = WebsiteProviderRegistry::new();
    let executor = AppConfigResourceExecutor::new(
        Arc::new(registry),
        "sdkwork-test-web".to_owned(),
        TENANT_SCOPE_HASH.to_owned(),
    )
    .unwrap();

    let result = executor
        .serve_static(
            &static_route("/index.html"),
            &policy(),
            &request(WebsiteDeliveryMethod::Get),
        )
        .await;
    match result {
        Err(sdkwork_webserver_delivery_runtime::WebsiteDeliveryError::ProviderNotRegistered {
            provider_type: WebsiteProviderType::Drive,
            ..
        }) => {}
        _ => panic!("expected ProviderNotRegistered for Drive"),
    }
}
#[tokio::test]
async fn can_serve_config_matches_registered_providers() {
    let provider = Arc::new(FakeStaticProvider::new(Vec::new()));
    let mut registry = WebsiteProviderRegistry::new();
    registry
        .register_static(WebsiteProviderType::Drive, provider)
        .unwrap();
    let executor = AppConfigResourceExecutor::new(
        Arc::new(registry),
        "sdkwork-test-web".to_owned(),
        TENANT_SCOPE_HASH.to_owned(),
    )
    .unwrap();

    assert!(executor.can_serve_config(&[WebsiteProviderType::Drive]));
    assert!(!executor.can_serve_config(&[WebsiteProviderType::Knowledgebase]));
    assert!(executor.can_serve_config(&[]));
}
