use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use sdkwork_webserver_contract::provider::{
    OpenWebsiteContentRequest, OpenedWebsiteContent, ResolveWebsiteStaticPathRequest,
    ResolveWebsiteWikiRouteRequest, ResolvedWebsiteContent, ResolvedWebsiteWikiContent,
    ValidateWebsiteResourceRequest, ValidatedWebsiteResource, WebsiteByteRange,
    WebsiteContentMetadata, WebsiteContentRange, WebsiteContentResolution,
    WebsiteProviderContentHandle, WebsiteProviderContentStream, WebsiteProviderError,
    WebsiteProviderErrorKind, WebsiteProviderResult, WebsiteRequestConditions,
    WebsiteResourceProvider, WebsiteStaticContentProvider, WebsiteWikiCollectionPage,
    WebsiteWikiCollectionRequest, WebsiteWikiContentKind, WebsiteWikiProvider,
    WebsiteWikiRouteResolution,
};
use sdkwork_webserver_core::website_runtime::{
    website_runtime_descriptor_sha256, website_runtime_set_snapshot_sha256, WebsiteProviderType,
    WebsiteRuntimeDescriptor, WebsiteRuntimeEnvironment, WebsiteRuntimeRegistry,
    WebsiteRuntimeSetSnapshot,
};
use sdkwork_webserver_delivery_runtime::{
    WebsiteDeliveryContentKind, WebsiteDeliveryError, WebsiteDeliveryExecutor,
    WebsiteDeliveryExecutorConfigError, WebsiteDeliveryMethod, WebsiteDeliveryOutcome,
    WebsiteDeliveryRequest, WebsiteDeliveryRoutingContext, WebsiteDeliveryScheme,
    WebsiteProviderEventInvalidation, WebsiteProviderEventInvalidationKind,
    WebsiteProviderEventInvalidationPriority, WebsiteProviderEventScope,
    WebsiteProviderEventSource, WebsiteProviderRegistry, WebsiteProviderRegistryError,
};
use serde_json::{json, Value};

const NODE_UUID: &str = "delivery-node-0001";
const TENANT_SCOPE_HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";

#[derive(Clone, Copy)]
enum FixtureHandler {
    Spa,
    Wiki,
}

struct MemoryStream {
    chunks: VecDeque<Vec<u8>>,
}

#[async_trait]
impl WebsiteProviderContentStream for MemoryStream {
    async fn next_chunk(&mut self) -> WebsiteProviderResult<Option<Vec<u8>>> {
        Ok(self.chunks.pop_front())
    }
}

struct FakeWikiProvider {
    resolutions: Mutex<VecDeque<WebsiteProviderResult<WebsiteWikiRouteResolution>>>,
    content: Vec<Vec<u8>>,
    resolve_requests: Mutex<Vec<ResolveWebsiteWikiRouteRequest>>,
    open_requests: Mutex<Vec<OpenWebsiteContentRequest>>,
    resolve_delay: Duration,
    opened_content_length: Option<u64>,
}

impl FakeWikiProvider {
    fn new(
        resolutions: impl IntoIterator<Item = WebsiteProviderResult<WebsiteWikiRouteResolution>>,
        content: Vec<Vec<u8>>,
    ) -> Self {
        Self {
            resolutions: Mutex::new(resolutions.into_iter().collect()),
            content,
            resolve_requests: Mutex::new(Vec::new()),
            open_requests: Mutex::new(Vec::new()),
            resolve_delay: Duration::ZERO,
            opened_content_length: None,
        }
    }

    fn with_resolve_delay(mut self, resolve_delay: Duration) -> Self {
        self.resolve_delay = resolve_delay;
        self
    }

    fn with_opened_content_length(mut self, content_length: u64) -> Self {
        self.opened_content_length = Some(content_length);
        self
    }
}

#[async_trait]
impl WebsiteResourceProvider for FakeWikiProvider {
    fn maximum_content_bytes(&self) -> u64 {
        1024
    }

    async fn validate_resource(
        &self,
        request: &ValidateWebsiteResourceRequest,
    ) -> WebsiteProviderResult<ValidatedWebsiteResource> {
        Ok(ValidatedWebsiteResource {
            provider_resource_uuid: request.provider.provider_resource_uuid.clone(),
            provider_generation: "7".to_owned(),
            public_generation: "p=7;n=8;s=9".to_owned(),
            capabilities: request.required_capabilities.clone(),
        })
    }
}

#[async_trait]
impl WebsiteWikiProvider for FakeWikiProvider {
    async fn resolve_wiki_route(
        &self,
        request: &ResolveWebsiteWikiRouteRequest,
    ) -> WebsiteProviderResult<WebsiteWikiRouteResolution> {
        if !self.resolve_delay.is_zero() {
            tokio::time::sleep(self.resolve_delay).await;
        }
        self.resolve_requests.lock().unwrap().push(request.clone());
        self.resolutions
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err(provider_error(WebsiteProviderErrorKind::NotFound)))
    }

    async fn open_wiki_content(
        &self,
        request: &OpenWebsiteContentRequest,
    ) -> WebsiteProviderResult<OpenedWebsiteContent> {
        self.open_requests.lock().unwrap().push(request.clone());
        let content_length = self
            .opened_content_length
            .unwrap_or_else(|| self.content.iter().map(Vec::len).sum::<usize>() as u64);
        Ok(OpenedWebsiteContent {
            stream: Box::new(MemoryStream {
                chunks: self.content.clone().into(),
            }),
            content_length,
            content_range: None,
        })
    }

    async fn retrieve_navigation(
        &self,
        _request: &WebsiteWikiCollectionRequest,
    ) -> WebsiteProviderResult<WebsiteWikiCollectionPage> {
        unreachable!("delivery executor does not retrieve navigation on a page request")
    }

    async fn search_wiki(
        &self,
        _request: &WebsiteWikiCollectionRequest,
    ) -> WebsiteProviderResult<WebsiteWikiCollectionPage> {
        unreachable!("delivery executor does not search on a page request")
    }
}

struct FakeStaticProvider {
    files: HashMap<String, Vec<u8>>,
    resolve_paths: Mutex<Vec<String>>,
    open_requests: Mutex<Vec<OpenWebsiteContentRequest>>,
}

#[async_trait]
impl WebsiteResourceProvider for FakeStaticProvider {
    fn maximum_content_bytes(&self) -> u64 {
        1024
    }

    async fn validate_resource(
        &self,
        request: &ValidateWebsiteResourceRequest,
    ) -> WebsiteProviderResult<ValidatedWebsiteResource> {
        Ok(ValidatedWebsiteResource {
            provider_resource_uuid: request.provider.provider_resource_uuid.clone(),
            provider_generation: "3".to_owned(),
            public_generation: "3".to_owned(),
            capabilities: request.required_capabilities.clone(),
        })
    }
}

#[async_trait]
impl WebsiteStaticContentProvider for FakeStaticProvider {
    async fn resolve_static_path(
        &self,
        request: &ResolveWebsiteStaticPathRequest,
    ) -> WebsiteProviderResult<WebsiteContentResolution> {
        self.resolve_paths
            .lock()
            .unwrap()
            .push(request.provider_relative_path.clone());
        let Some(content) = self.files.get(&request.provider_relative_path) else {
            return Err(provider_error(WebsiteProviderErrorKind::NotFound));
        };
        Ok(WebsiteContentResolution::Found(ResolvedWebsiteContent {
            content_handle: WebsiteProviderContentHandle::new(
                request.provider_relative_path.clone(),
            )
            .unwrap(),
            metadata: content_metadata(content.len() as u64, true),
        }))
    }

    async fn open_static_content(
        &self,
        request: &OpenWebsiteContentRequest,
    ) -> WebsiteProviderResult<OpenedWebsiteContent> {
        self.open_requests.lock().unwrap().push(request.clone());
        let content = self
            .files
            .get(request.content_handle.as_str())
            .cloned()
            .ok_or_else(|| provider_error(WebsiteProviderErrorKind::NotFound))?;
        let complete_length = content.len() as u64;
        let effective_range = if request.conditions.if_range.as_deref() == Some("\"mismatch\"") {
            None
        } else {
            request.range
        };
        let (content, content_range) = match effective_range {
            Some(range) if range.start < complete_length => {
                let end_inclusive = range
                    .end_inclusive
                    .unwrap_or(complete_length - 1)
                    .min(complete_length - 1);
                if end_inclusive < range.start {
                    return Err(provider_error(WebsiteProviderErrorKind::InvalidPath));
                }
                let start = usize::try_from(range.start)
                    .map_err(|_| provider_error(WebsiteProviderErrorKind::InvalidPath))?;
                let end = usize::try_from(end_inclusive)
                    .map_err(|_| provider_error(WebsiteProviderErrorKind::InvalidPath))?;
                (
                    content[start..=end].to_vec(),
                    Some(WebsiteContentRange {
                        start: range.start,
                        end_inclusive,
                        complete_length,
                    }),
                )
            }
            Some(_) => return Err(provider_error(WebsiteProviderErrorKind::InvalidPath)),
            None => (content, None),
        };
        let content_length = content.len() as u64;
        Ok(OpenedWebsiteContent {
            stream: Box::new(MemoryStream {
                chunks: VecDeque::from([content]),
            }),
            content_length,
            content_range,
        })
    }
}

#[tokio::test]
async fn routes_wiki_through_the_registered_provider_with_compiled_scope() {
    let runtime = active_runtime(FixtureHandler::Wiki);
    let wiki = Arc::new(FakeWikiProvider::new(
        [Ok(wiki_content_resolution(4, false))],
        vec![b"wiki".to_vec()],
    ));
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_wiki(WebsiteProviderType::Knowledgebase, wiki.clone())
        .unwrap();
    let executor = WebsiteDeliveryExecutor::new(runtime, Arc::new(providers));

    let outcome = executor
        .execute(delivery_request("/guide", WebsiteDeliveryMethod::Get))
        .await
        .unwrap();
    let WebsiteDeliveryOutcome::Content(mut content) = outcome else {
        panic!("expected Wiki content")
    };
    assert_eq!(
        content.kind,
        WebsiteDeliveryContentKind::Wiki(WebsiteWikiContentKind::Asset)
    );
    assert_eq!(content.route.runtime_set_generation, 1);
    assert_eq!(content.route.tenant_scope_hash, TENANT_SCOPE_HASH);
    assert_eq!(content.route.app_uuid, "site-wiki");
    assert_eq!(content.route.binding_uuid, "binding-wiki");
    assert_eq!(content.route.variant_uuid, "variant-default");
    assert_eq!(content.route.mount_uuid, "mount-wiki");
    assert_eq!(content.route.resource_uuid, "resource-wiki");
    assert_eq!(content.route.provider_relative_path, "/guide");
    assert_eq!(content.canonical_route.as_deref(), Some("/guide"));
    let request = wiki.resolve_requests.lock().unwrap()[0].clone();
    assert_eq!(request.context.tenant_scope_hash, TENANT_SCOPE_HASH);
    assert_eq!(request.context.request_id, "request-0001");
    assert_eq!(request.context.trace_id, "trace-0001");
    assert!((1..=2_500).contains(&request.context.deadline_ms));
    assert_eq!(request.provider.provider_resource_uuid, "publication-0001");
    let body = content.body.as_mut().expect("GET must open content");
    assert_eq!(body.next_chunk().await.unwrap(), Some(b"wiki".to_vec()));
    assert_eq!(body.next_chunk().await.unwrap(), None);
    assert_eq!(wiki.open_requests.lock().unwrap().len(), 1);
    assert_eq!(
        wiki.open_requests.lock().unwrap()[0].provider_relative_path,
        "/guide"
    );
}

#[tokio::test]
async fn head_redirect_and_non_public_outcomes_do_not_open_content() {
    let runtime = active_runtime(FixtureHandler::Wiki);
    let wiki = Arc::new(FakeWikiProvider::new(
        [
            Ok(wiki_content_resolution(4, false)),
            Ok(WebsiteWikiRouteResolution::Redirect(
                sdkwork_webserver_contract::provider::WebsiteWikiRedirect {
                    status_code: 308,
                    canonical_route: "/canonical".to_owned(),
                },
            )),
            Err(provider_error(WebsiteProviderErrorKind::NotPublic)),
        ],
        vec![b"wiki".to_vec()],
    ));
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_wiki(WebsiteProviderType::Knowledgebase, wiki.clone())
        .unwrap();
    let executor = WebsiteDeliveryExecutor::new(runtime, Arc::new(providers));

    let head = executor
        .execute(delivery_request("/guide", WebsiteDeliveryMethod::Head))
        .await
        .unwrap();
    let WebsiteDeliveryOutcome::Content(content) = head else {
        panic!("expected HEAD metadata")
    };
    assert!(content.body.is_none());

    let redirect = executor
        .execute(delivery_request("/old", WebsiteDeliveryMethod::Get))
        .await
        .unwrap();
    assert!(matches!(redirect, WebsiteDeliveryOutcome::Redirect(_)));

    let hidden = executor
        .execute(delivery_request("/private", WebsiteDeliveryMethod::Get))
        .await
        .unwrap();
    assert!(matches!(hidden, WebsiteDeliveryOutcome::NotFound));
    assert!(wiki.open_requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn force_https_redirects_before_calling_the_provider() {
    let runtime = active_runtime(FixtureHandler::Wiki);
    let wiki = Arc::new(FakeWikiProvider::new([], Vec::new()));
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_wiki(WebsiteProviderType::Knowledgebase, wiki.clone())
        .unwrap();
    let executor = WebsiteDeliveryExecutor::new(runtime, Arc::new(providers));
    let mut request = delivery_request("/guide", WebsiteDeliveryMethod::Get);
    request.scheme = WebsiteDeliveryScheme::Http;

    let outcome = executor.execute(request).await.unwrap();
    assert!(matches!(
        outcome,
        WebsiteDeliveryOutcome::Redirect(
            sdkwork_webserver_delivery_runtime::WebsiteDeliveryRedirect::Binding {
                status_code: 308,
                scheme: sdkwork_webserver_core::website_runtime::WebsiteRedirectScheme::Https,
                ref hostname,
                ref path,
                preserve_query: true,
            }
        ) if hostname == "example.com" && path == "/guide"
    ));
    assert!(wiki.resolve_requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn static_index_and_spa_fallback_are_provider_relative_and_explicit() {
    let runtime = active_runtime(FixtureHandler::Spa);
    let static_provider = Arc::new(FakeStaticProvider {
        files: HashMap::from([("/web/index.html".to_owned(), b"shell".to_vec())]),
        resolve_paths: Mutex::new(Vec::new()),
        open_requests: Mutex::new(Vec::new()),
    });
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_static(WebsiteProviderType::Drive, static_provider.clone())
        .unwrap();
    let executor = WebsiteDeliveryExecutor::new(runtime, Arc::new(providers));

    let root = executor
        .execute(delivery_request("/", WebsiteDeliveryMethod::Get))
        .await
        .unwrap();
    let WebsiteDeliveryOutcome::Content(root) = root else {
        panic!("expected index content")
    };
    assert_eq!(root.route.provider_relative_path, "/web/index.html");

    let not_navigation = executor
        .execute(delivery_request("/missing", WebsiteDeliveryMethod::Get))
        .await
        .unwrap();
    assert!(matches!(not_navigation, WebsiteDeliveryOutcome::NotFound));

    let mut navigation_request = delivery_request("/missing", WebsiteDeliveryMethod::Get);
    navigation_request.spa_fallback_eligible = true;
    let navigation = executor.execute(navigation_request).await.unwrap();
    let WebsiteDeliveryOutcome::Content(navigation) = navigation else {
        panic!("expected explicit SPA fallback")
    };
    assert_eq!(navigation.route.provider_relative_path, "/web/index.html");
    assert_eq!(
        static_provider.resolve_paths.lock().unwrap().as_slice(),
        ["/web/index.html", "/web/missing"]
    );
}

#[tokio::test]
async fn buffered_content_budget_rejects_without_queueing_and_recovers_on_drop() {
    let mut descriptor = descriptor_fixture(FixtureHandler::Spa);
    descriptor["deliveryPolicy"]["maximumObjectBytes"] = json!(5);
    let runtime = active_runtime_from_descriptor(descriptor, 2_500);
    let static_provider = Arc::new(FakeStaticProvider {
        files: HashMap::from([("/web/index.html".to_owned(), b"shell".to_vec())]),
        resolve_paths: Mutex::new(Vec::new()),
        open_requests: Mutex::new(Vec::new()),
    });
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_static(WebsiteProviderType::Drive, static_provider.clone())
        .unwrap();
    let executor =
        WebsiteDeliveryExecutor::with_buffered_content_budget(runtime, Arc::new(providers), 5)
            .unwrap();

    let first = executor
        .execute(delivery_request("/", WebsiteDeliveryMethod::Get))
        .await
        .unwrap();
    let WebsiteDeliveryOutcome::Content(first) = first else {
        panic!("expected first content")
    };

    let saturated = match executor
        .execute(delivery_request("/", WebsiteDeliveryMethod::Get))
        .await
    {
        Ok(_) => panic!("held response must own the entire buffered-content budget"),
        Err(error) => error,
    };
    assert!(matches!(
        saturated,
        WebsiteDeliveryError::Provider(WebsiteProviderError {
            kind: WebsiteProviderErrorKind::Unavailable,
            retry_after_ms: Some(100),
        })
    ));
    assert_eq!(static_provider.open_requests.lock().unwrap().len(), 1);

    drop(first);
    let recovered = executor
        .execute(delivery_request("/", WebsiteDeliveryMethod::Get))
        .await
        .expect("dropping a response releases its buffered-content permit");
    assert!(matches!(recovered, WebsiteDeliveryOutcome::Content(_)));
    assert_eq!(static_provider.open_requests.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn buffered_content_budget_reserves_the_compiled_object_ceiling() {
    let runtime = active_runtime(FixtureHandler::Spa);
    let static_provider = Arc::new(FakeStaticProvider {
        files: HashMap::from([("/web/index.html".to_owned(), b"shell".to_vec())]),
        resolve_paths: Mutex::new(Vec::new()),
        open_requests: Mutex::new(Vec::new()),
    });
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_static(WebsiteProviderType::Drive, static_provider.clone())
        .unwrap();
    let executor =
        WebsiteDeliveryExecutor::with_buffered_content_budget(runtime, Arc::new(providers), 5)
            .unwrap();

    let rejected = match executor
        .execute(delivery_request("/", WebsiteDeliveryMethod::Get))
        .await
    {
        Ok(_) => panic!("object ceiling above the process budget must be rejected"),
        Err(error) => error,
    };
    assert!(matches!(
        rejected,
        WebsiteDeliveryError::Provider(WebsiteProviderError {
            kind: WebsiteProviderErrorKind::Unavailable,
            retry_after_ms: Some(100),
        })
    ));
    assert_eq!(static_provider.open_requests.lock().unwrap().len(), 0);
}

#[test]
fn buffered_content_budget_rejects_zero_capacity() {
    let error = WebsiteDeliveryExecutor::with_buffered_content_budget(
        Arc::new(WebsiteRuntimeRegistry::new(
            NODE_UUID,
            WebsiteRuntimeEnvironment::Production,
        )),
        Arc::new(WebsiteProviderRegistry::new()),
        0,
    )
    .err()
    .expect("zero capacity must fail");
    assert_eq!(
        error,
        WebsiteDeliveryExecutorConfigError::InvalidBufferedContentBudget {
            configured_bytes: 0,
            maximum_bytes: u32::MAX as usize,
        }
    );
}

#[tokio::test]
async fn rejects_unsupported_ranges_and_streams_that_violate_declared_length() {
    let runtime = active_runtime(FixtureHandler::Wiki);
    let wiki = Arc::new(
        FakeWikiProvider::new(
            [
                Ok(wiki_content_resolution(4, false)),
                Ok(wiki_content_resolution(4, false)),
            ],
            vec![b"12345".to_vec()],
        )
        .with_opened_content_length(4),
    );
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_wiki(WebsiteProviderType::Knowledgebase, wiki)
        .unwrap();
    let executor = WebsiteDeliveryExecutor::new(runtime, Arc::new(providers));

    let mut range_request = delivery_request("/guide", WebsiteDeliveryMethod::Get);
    range_request.range = Some(WebsiteByteRange {
        start: 0,
        end_inclusive: Some(1),
        suffix_bytes: None,
    });
    assert!(matches!(
        executor.execute(range_request).await,
        Err(WebsiteDeliveryError::RangeNotSupported)
    ));

    let outcome = executor
        .execute(delivery_request("/guide", WebsiteDeliveryMethod::Get))
        .await
        .unwrap();
    let WebsiteDeliveryOutcome::Content(mut content) = outcome else {
        panic!("expected content")
    };
    let error = content
        .body
        .as_mut()
        .unwrap()
        .next_chunk()
        .await
        .unwrap_err();
    assert_eq!(error.kind, WebsiteProviderErrorKind::ContractMismatch);
}

#[tokio::test]
async fn preserves_valid_static_range_evidence_and_bounded_bytes() {
    let runtime = active_runtime(FixtureHandler::Spa);
    let static_provider = Arc::new(FakeStaticProvider {
        files: HashMap::from([("/web/range.bin".to_owned(), b"0123456789".to_vec())]),
        resolve_paths: Mutex::new(Vec::new()),
        open_requests: Mutex::new(Vec::new()),
    });
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_static(WebsiteProviderType::Drive, static_provider)
        .unwrap();
    let executor = WebsiteDeliveryExecutor::new(runtime, Arc::new(providers));
    let mut request = delivery_request("/range.bin", WebsiteDeliveryMethod::Get);
    request.range = Some(WebsiteByteRange {
        start: 2,
        end_inclusive: Some(5),
        suffix_bytes: None,
    });

    let outcome = executor.execute(request).await.unwrap();
    let WebsiteDeliveryOutcome::Content(mut content) = outcome else {
        panic!("expected ranged static content")
    };
    assert_eq!(content.response_content_length, 4);
    assert_eq!(
        content.content_range,
        Some(WebsiteContentRange {
            start: 2,
            end_inclusive: 5,
            complete_length: 10,
        })
    );
    let body = content.body.as_mut().expect("GET opens the range stream");
    assert_eq!(body.next_chunk().await.unwrap(), Some(b"2345".to_vec()));
    assert_eq!(body.next_chunk().await.unwrap(), None);
}

#[tokio::test]
async fn accepts_full_static_content_when_if_range_does_not_match() {
    let runtime = active_runtime(FixtureHandler::Spa);
    let static_provider = Arc::new(FakeStaticProvider {
        files: HashMap::from([("/web/range.bin".to_owned(), b"0123456789".to_vec())]),
        resolve_paths: Mutex::new(Vec::new()),
        open_requests: Mutex::new(Vec::new()),
    });
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_static(WebsiteProviderType::Drive, static_provider)
        .unwrap();
    let executor = WebsiteDeliveryExecutor::new(runtime, Arc::new(providers));
    let mut request = delivery_request("/range.bin", WebsiteDeliveryMethod::Get);
    request.range = Some(WebsiteByteRange {
        start: 2,
        end_inclusive: Some(5),
        suffix_bytes: None,
    });
    request.conditions.if_range = Some("\"mismatch\"".to_owned());

    let outcome = executor.execute(request).await.unwrap();
    let WebsiteDeliveryOutcome::Content(mut content) = outcome else {
        panic!("expected full static content")
    };
    assert_eq!(content.response_content_length, 10);
    assert_eq!(content.content_range, None);
    let body = content.body.as_mut().expect("GET opens full content");
    assert_eq!(
        body.next_chunk().await.unwrap(),
        Some(b"0123456789".to_vec())
    );
    assert_eq!(body.next_chunk().await.unwrap(), None);
}

#[tokio::test]
async fn fails_closed_for_missing_runtime_provider_and_duplicate_registration() {
    let empty_runtime = Arc::new(WebsiteRuntimeRegistry::new(
        NODE_UUID,
        WebsiteRuntimeEnvironment::Production,
    ));
    let executor =
        WebsiteDeliveryExecutor::new(empty_runtime, Arc::new(WebsiteProviderRegistry::new()));
    assert!(matches!(
        executor
            .execute(delivery_request("/guide", WebsiteDeliveryMethod::Get))
            .await,
        Err(WebsiteDeliveryError::RuntimeUnavailable)
    ));

    let executor = WebsiteDeliveryExecutor::new(
        active_runtime(FixtureHandler::Wiki),
        Arc::new(WebsiteProviderRegistry::new()),
    );
    assert!(matches!(
        executor
            .execute(delivery_request("/guide", WebsiteDeliveryMethod::Get))
            .await,
        Err(WebsiteDeliveryError::ProviderNotRegistered {
            provider_type: WebsiteProviderType::Knowledgebase,
            capability: "wiki"
        })
    ));

    let provider = Arc::new(FakeWikiProvider::new([], Vec::new()));
    let mut registry = WebsiteProviderRegistry::new();
    registry
        .register_wiki(WebsiteProviderType::Knowledgebase, provider.clone())
        .unwrap();
    assert_eq!(
        registry.register_wiki(WebsiteProviderType::Knowledgebase, provider),
        Err(WebsiteProviderRegistryError::DuplicateProvider {
            provider_type: WebsiteProviderType::Knowledgebase,
            capability: "wiki"
        })
    );
}

#[tokio::test]
async fn enforces_the_compiled_provider_deadline_outside_provider_implementations() {
    let runtime = active_runtime_with_timeout(FixtureHandler::Wiki, 5);
    let wiki = Arc::new(
        FakeWikiProvider::new([Ok(wiki_content_resolution(4, false))], Vec::new())
            .with_resolve_delay(Duration::from_millis(50)),
    );
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_wiki(WebsiteProviderType::Knowledgebase, wiki)
        .unwrap();
    let executor = WebsiteDeliveryExecutor::new(runtime, Arc::new(providers));

    let result = executor
        .execute(delivery_request("/guide", WebsiteDeliveryMethod::Get))
        .await;
    let Err(error) = result else {
        panic!("slow provider must exceed the compiled deadline")
    };
    assert!(matches!(
        error,
        WebsiteDeliveryError::Provider(WebsiteProviderError {
            kind: WebsiteProviderErrorKind::DeadlineExceeded,
            ..
        })
    ));
}

#[tokio::test]
async fn maps_provider_canonical_routes_back_through_binding_and_alias_mounts() {
    let mut descriptor = descriptor_fixture(FixtureHandler::Wiki);
    descriptor["bindings"][0]["pathPrefix"] = json!("/docs");
    descriptor["mounts"][0]["pathPrefix"] = json!("/wiki");
    descriptor["mounts"][0]["translation"] = json!({
        "mode": "ALIAS",
        "resourceSubpath": "/published"
    });
    let runtime = active_runtime_from_descriptor(descriptor, 2_500);
    let wiki = Arc::new(FakeWikiProvider::new(
        [
            Ok(wiki_content_resolution_at("/published/guide", 4, false)),
            Ok(WebsiteWikiRouteResolution::Redirect(
                sdkwork_webserver_contract::provider::WebsiteWikiRedirect {
                    status_code: 308,
                    canonical_route: "/published/new".to_owned(),
                },
            )),
            Ok(WebsiteWikiRouteResolution::Redirect(
                sdkwork_webserver_contract::provider::WebsiteWikiRedirect {
                    status_code: 308,
                    canonical_route: "/outside".to_owned(),
                },
            )),
        ],
        vec![b"wiki".to_vec()],
    ));
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_wiki(WebsiteProviderType::Knowledgebase, wiki.clone())
        .unwrap();
    let executor = WebsiteDeliveryExecutor::new(runtime, Arc::new(providers));

    let content = executor
        .execute(delivery_request(
            "/docs/wiki/legacy",
            WebsiteDeliveryMethod::Head,
        ))
        .await
        .unwrap();
    let WebsiteDeliveryOutcome::Content(content) = content else {
        panic!("expected Wiki content")
    };
    assert_eq!(content.canonical_route.as_deref(), Some("/docs/wiki/guide"));
    assert_eq!(
        wiki.resolve_requests.lock().unwrap()[0].route,
        "/published/legacy"
    );

    let redirect = executor
        .execute(delivery_request(
            "/docs/wiki/old",
            WebsiteDeliveryMethod::Get,
        ))
        .await
        .unwrap();
    let WebsiteDeliveryOutcome::Redirect(
        sdkwork_webserver_delivery_runtime::WebsiteDeliveryRedirect::Wiki {
            canonical_route, ..
        },
    ) = redirect
    else {
        panic!("expected Wiki redirect")
    };
    assert_eq!(canonical_route, "/docs/wiki/new");

    let result = executor
        .execute(delivery_request(
            "/docs/wiki/unsafe",
            WebsiteDeliveryMethod::Get,
        ))
        .await;
    let Err(error) = result else {
        panic!("provider route outside the compiled resource subpath must fail")
    };
    assert!(matches!(
        error,
        WebsiteDeliveryError::Provider(WebsiteProviderError {
            kind: WebsiteProviderErrorKind::ContractMismatch,
            ..
        })
    ));
}

#[tokio::test]
async fn caches_positive_and_negative_resolutions_until_exact_event_invalidation() {
    let runtime = active_runtime(FixtureHandler::Wiki);
    let wiki = Arc::new(FakeWikiProvider::new(
        [
            Ok(wiki_content_resolution(4, false)),
            Err(provider_error(WebsiteProviderErrorKind::NotPublic)),
            Ok(wiki_content_resolution_at("/private", 4, false)),
        ],
        vec![b"wiki".to_vec()],
    ));
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_wiki(WebsiteProviderType::Knowledgebase, wiki.clone())
        .unwrap();
    let executor = WebsiteDeliveryExecutor::new(runtime, Arc::new(providers));

    for _ in 0..2 {
        let outcome = executor
            .execute(delivery_request("/guide", WebsiteDeliveryMethod::Head))
            .await
            .unwrap();
        assert!(matches!(outcome, WebsiteDeliveryOutcome::Content(_)));
    }
    for _ in 0..2 {
        let outcome = executor
            .execute(delivery_request("/private", WebsiteDeliveryMethod::Head))
            .await
            .unwrap();
        assert!(matches!(outcome, WebsiteDeliveryOutcome::NotFound));
    }
    assert_eq!(wiki.resolve_requests.lock().unwrap().len(), 2);
    let snapshot = executor.provider_resolution_cache_snapshot().await;
    assert_eq!(snapshot.misses, 2);
    assert_eq!(snapshot.hits, 1);
    assert_eq!(snapshot.negative_hits, 1);

    executor
        .provider_event_invalidator()
        .invalidate(&[WebsiteProviderEventInvalidation {
            provider_type: WebsiteProviderType::Knowledgebase,
            provider_resource_uuid: "publication-0001".to_owned(),
            kind: WebsiteProviderEventInvalidationKind::Route {
                path: "/private".to_owned(),
            },
            priority: WebsiteProviderEventInvalidationPriority::Revocation,
            provider_generation: Some("8".to_owned()),
            public_generation: Some("12".to_owned()),
        }])
        .await
        .unwrap();

    let refreshed = executor
        .execute(delivery_request("/private", WebsiteDeliveryMethod::Head))
        .await
        .unwrap();
    assert!(matches!(refreshed, WebsiteDeliveryOutcome::Content(_)));
    assert_eq!(wiki.resolve_requests.lock().unwrap().len(), 3);

    let still_cached = executor
        .execute(delivery_request("/guide", WebsiteDeliveryMethod::Head))
        .await
        .unwrap();
    assert!(matches!(still_cached, WebsiteDeliveryOutcome::Content(_)));
    assert_eq!(wiki.resolve_requests.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn coalesces_concurrent_resolution_misses_without_an_origin_waiter_queue() {
    let runtime = active_runtime(FixtureHandler::Wiki);
    let wiki = Arc::new(
        FakeWikiProvider::new(
            [Ok(wiki_content_resolution(4, false))],
            vec![b"wiki".to_vec()],
        )
        .with_resolve_delay(Duration::from_millis(25)),
    );
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_wiki(WebsiteProviderType::Knowledgebase, wiki.clone())
        .unwrap();
    let executor = Arc::new(WebsiteDeliveryExecutor::new(runtime, Arc::new(providers)));

    let requests = (0..16).map(|index| {
        let executor = Arc::clone(&executor);
        async move {
            let mut request = delivery_request("/guide", WebsiteDeliveryMethod::Head);
            request.request_id = format!("request-{index:04}");
            request.trace_id = format!("trace-{index:04}");
            executor.execute(request).await
        }
    });
    let outcomes = futures_util::future::join_all(requests).await;

    assert!(outcomes
        .into_iter()
        .all(|outcome| matches!(outcome, Ok(WebsiteDeliveryOutcome::Content(_)))));
    assert_eq!(wiki.resolve_requests.lock().unwrap().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn serves_bounded_stale_metadata_once_while_revalidating_in_the_background() {
    let mut descriptor = descriptor_fixture(FixtureHandler::Wiki);
    descriptor["deliveryPolicy"]["metadataCacheTtlSeconds"] = json!(1);
    descriptor["deliveryPolicy"]["negativeCacheTtlSeconds"] = json!(1);
    descriptor["deliveryPolicy"]["staleWhileRevalidateSeconds"] = json!(30);
    let runtime = active_runtime_from_descriptor(descriptor, 2_500);
    let wiki = Arc::new(FakeWikiProvider::new(
        [
            Ok(wiki_content_resolution(4, false)),
            Ok(wiki_content_resolution(5, false)),
        ],
        Vec::new(),
    ));
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_wiki(WebsiteProviderType::Knowledgebase, wiki.clone())
        .unwrap();
    let executor = WebsiteDeliveryExecutor::new(runtime, Arc::new(providers));

    let first = executor
        .execute(delivery_request("/guide", WebsiteDeliveryMethod::Head))
        .await
        .unwrap();
    let WebsiteDeliveryOutcome::Content(first) = first else {
        panic!("expected initial Wiki content")
    };
    assert_eq!(first.metadata.content_length, 4);

    tokio::time::advance(Duration::from_secs(2)).await;
    let stale = executor
        .execute(delivery_request("/guide", WebsiteDeliveryMethod::Head))
        .await
        .unwrap();
    let WebsiteDeliveryOutcome::Content(stale) = stale else {
        panic!("expected bounded stale Wiki content")
    };
    assert_eq!(stale.metadata.content_length, 4);

    for _ in 0..10 {
        if wiki.resolve_requests.lock().unwrap().len() == 2 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(wiki.resolve_requests.lock().unwrap().len(), 2);
    let refreshed = executor
        .execute(delivery_request("/guide", WebsiteDeliveryMethod::Head))
        .await
        .unwrap();
    let WebsiteDeliveryOutcome::Content(refreshed) = refreshed else {
        panic!("expected revalidated Wiki content")
    };
    assert_eq!(refreshed.metadata.content_length, 5);
    let snapshot = executor.provider_resolution_cache_snapshot().await;
    assert_eq!(snapshot.stale_hits, 1);
    assert_eq!(snapshot.revalidations, 1);
}

#[tokio::test]
async fn bounds_resolution_cache_entries_and_evicts_the_least_recently_used_entry() {
    let runtime = active_runtime(FixtureHandler::Wiki);
    let wiki = Arc::new(FakeWikiProvider::new(
        [
            Ok(wiki_content_resolution_at("/guide", 4, false)),
            Ok(wiki_content_resolution_at("/other", 5, false)),
            Ok(wiki_content_resolution_at("/guide", 6, false)),
        ],
        Vec::new(),
    ));
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_wiki(WebsiteProviderType::Knowledgebase, wiki.clone())
        .unwrap();
    let executor = WebsiteDeliveryExecutor::with_provider_runtime_limits(
        runtime,
        Arc::new(providers),
        1024,
        1,
    )
    .unwrap();

    for path in ["/guide", "/other", "/guide"] {
        let outcome = executor
            .execute(delivery_request(path, WebsiteDeliveryMethod::Head))
            .await
            .unwrap();
        assert!(matches!(outcome, WebsiteDeliveryOutcome::Content(_)));
    }
    let snapshot = executor.provider_resolution_cache_snapshot().await;
    assert_eq!(snapshot.entries, 1);
    assert_eq!(snapshot.evictions, 2);
    assert_eq!(wiki.resolve_requests.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn an_uncertain_event_stream_clears_all_cached_entries_for_the_provider_type() {
    let runtime = active_runtime(FixtureHandler::Wiki);
    let wiki = Arc::new(FakeWikiProvider::new(
        [
            Ok(wiki_content_resolution(4, false)),
            Ok(wiki_content_resolution(5, false)),
        ],
        Vec::new(),
    ));
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_wiki(WebsiteProviderType::Knowledgebase, wiki.clone())
        .unwrap();
    let executor = WebsiteDeliveryExecutor::new(runtime, Arc::new(providers));

    executor
        .execute(delivery_request("/guide", WebsiteDeliveryMethod::Head))
        .await
        .unwrap();
    executor
        .provider_event_invalidator()
        .mark_uncertain(&WebsiteProviderEventScope {
            source: WebsiteProviderEventSource::Knowledgebase,
            tenant_id: "tenant-1".to_owned(),
            organization_id: Some("organization-1".to_owned()),
            stream_id: "knowledgebase:tenant-1:organization-1".to_owned(),
        })
        .await
        .unwrap();
    let refreshed = executor
        .execute(delivery_request("/guide", WebsiteDeliveryMethod::Head))
        .await
        .unwrap();
    let WebsiteDeliveryOutcome::Content(refreshed) = refreshed else {
        panic!("expected refreshed Wiki content")
    };
    assert_eq!(refreshed.metadata.content_length, 5);
    assert_eq!(wiki.resolve_requests.lock().unwrap().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn event_invalidation_fences_an_in_flight_stale_origin_result() {
    let runtime = active_runtime(FixtureHandler::Wiki);
    let wiki = Arc::new(
        FakeWikiProvider::new([Ok(wiki_content_resolution(4, false))], Vec::new())
            .with_resolve_delay(Duration::from_secs(1)),
    );
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_wiki(WebsiteProviderType::Knowledgebase, wiki.clone())
        .unwrap();
    let executor = Arc::new(WebsiteDeliveryExecutor::new(runtime, Arc::new(providers)));
    let request_executor = Arc::clone(&executor);
    let request = tokio::spawn(async move {
        request_executor
            .execute(delivery_request("/guide", WebsiteDeliveryMethod::Head))
            .await
    });
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(500)).await;

    executor
        .provider_event_invalidator()
        .invalidate(&[WebsiteProviderEventInvalidation {
            provider_type: WebsiteProviderType::Knowledgebase,
            provider_resource_uuid: "publication-0001".to_owned(),
            kind: WebsiteProviderEventInvalidationKind::Route {
                path: "/guide".to_owned(),
            },
            priority: WebsiteProviderEventInvalidationPriority::Revocation,
            provider_generation: Some("8".to_owned()),
            public_generation: Some("12".to_owned()),
        }])
        .await
        .unwrap();
    tokio::time::advance(Duration::from_secs(1)).await;
    let result = request.await.unwrap();

    assert!(matches!(
        result,
        Err(WebsiteDeliveryError::Provider(WebsiteProviderError {
            kind: WebsiteProviderErrorKind::Unavailable,
            ..
        }))
    ));
    let snapshot = executor.provider_resolution_cache_snapshot().await;
    assert_eq!(snapshot.entries, 0);
    assert_eq!(snapshot.in_flight, 0);
    assert_eq!(wiki.resolve_requests.lock().unwrap().len(), 1);
}

fn active_runtime(handler: FixtureHandler) -> Arc<WebsiteRuntimeRegistry> {
    active_runtime_with_timeout(handler, 2_500)
}

fn active_runtime_with_timeout(
    handler: FixtureHandler,
    provider_timeout_ms: u64,
) -> Arc<WebsiteRuntimeRegistry> {
    active_runtime_from_descriptor(descriptor_fixture(handler), provider_timeout_ms)
}

fn active_runtime_from_descriptor(
    descriptor: Value,
    provider_timeout_ms: u64,
) -> Arc<WebsiteRuntimeRegistry> {
    let registry = Arc::new(WebsiteRuntimeRegistry::new(
        NODE_UUID,
        WebsiteRuntimeEnvironment::Production,
    ));
    registry
        .compile_and_activate(&signed_runtime_set_from_descriptor(
            descriptor,
            provider_timeout_ms,
        ))
        .unwrap();
    registry
}

fn signed_runtime_set_from_descriptor(mut descriptor: Value, provider_timeout_ms: u64) -> Vec<u8> {
    descriptor["deliveryPolicy"]["providerTimeoutMs"] = json!(provider_timeout_ms);
    let descriptor = signed_descriptor(descriptor);
    let mut value = json!({
        "schemaVersion": "sdkwork.website-runtime-set.v1",
        "kind": "sdkwork.website-runtime-set.snapshot",
        "snapshotUuid": "snapshot-0001",
        "nodeUuid": NODE_UUID,
        "environment": "production",
        "generation": 1,
        "generatedAt": "2026-07-21T00:00:00Z",
        "compilerVersion": "deploy-runtime-set-compiler/1",
        "snapshotSha256": "0".repeat(64),
        "maximumSites": 8,
        "descriptors": [descriptor]
    });
    let snapshot: WebsiteRuntimeSetSnapshot = serde_json::from_value(value.clone()).unwrap();
    value["snapshotSha256"] =
        Value::String(website_runtime_set_snapshot_sha256(&snapshot).unwrap());
    serde_json::to_vec(&value).unwrap()
}

fn signed_descriptor(mut value: Value) -> Value {
    let descriptor: WebsiteRuntimeDescriptor = serde_json::from_value(value.clone()).unwrap();
    value["descriptorSha256"] =
        Value::String(website_runtime_descriptor_sha256(&descriptor).unwrap());
    value
}

fn descriptor_fixture(handler: FixtureHandler) -> Value {
    let (
        site,
        provider_type,
        provider_uuid,
        contract,
        handler_name,
        subpath,
        capabilities,
        fallback,
    ) = match handler {
        FixtureHandler::Spa => (
            "spa",
            "DRIVE",
            "website-root-0001",
            "drive.website-root.v1",
            "SPA",
            "/web",
            (true, false, false, true),
            Value::String("/web/index.html".to_owned()),
        ),
        FixtureHandler::Wiki => (
            "wiki",
            "KNOWLEDGEBASE",
            "publication-0001",
            "knowledgebase.wiki-publication.v1",
            "WIKI",
            "/",
            (true, true, true, false),
            Value::Null,
        ),
    };
    let mut mount = json!({
        "mountUuid": format!("mount-{site}"),
        "variantUuid": "variant-default",
        "pathPrefix": "/",
        "resourceUuid": format!("resource-{site}"),
        "handler": handler_name,
        "translation": {
            "mode": "ROOT",
            "resourceSubpath": subpath
        },
        "indexFiles": if matches!(handler, FixtureHandler::Wiki) { vec![] } else { vec!["index.html"] }
    });
    if !fallback.is_null() {
        mount["spaFallback"] = fallback;
    }
    json!({
        "schemaVersion": "sdkwork.website-runtime.v1",
        "kind": "sdkwork.website-runtime.descriptor",
        "revisionUuid": format!("revision-{site}"),
        "appUuid": format!("site-{site}"),
        "tenantScopeHash": TENANT_SCOPE_HASH,
        "environment": "production",
        "generatedAt": "2026-07-21T00:00:00Z",
        "compilerVersion": "deploy-descriptor-compiler/1",
        "descriptorSha256": "0".repeat(64),
        "appDefaultVariantUuid": "variant-default",
        "bindings": [{
            "bindingUuid": format!("binding-{site}"),
            "hostname": "example.com",
            "pathPrefix": "/",
            "action": { "type": "SERVE" }
        }],
        "variants": [{
            "variantUuid": "variant-default",
            "label": "Default"
        }],
        "variantRules": [],
        "resources": [{
            "resourceUuid": format!("resource-{site}"),
            "provider": {
                "providerType": provider_type,
                "providerResourceUuid": provider_uuid,
                "providerContractVersion": contract
            },
            "capabilities": {
                "staticContent": capabilities.0,
                "wikiRoutes": capabilities.1,
                "wikiSearch": capabilities.2,
                "rangeRequests": capabilities.3
            }
        }],
        "mounts": [mount],
        "deliveryPolicy": {
            "providerTimeoutMs": 2500,
            "metadataCacheTtlSeconds": 60,
            "negativeCacheTtlSeconds": 5,
            "staleWhileRevalidateSeconds": 30,
            "maximumObjectBytes": 1024
        },
        "securityPolicy": {
            "forceHttps": true,
            "denyDotFiles": true,
            "deniedPathPrefixes": []
        },
        "limits": {
            "maximumBindings": 8,
            "maximumVariants": 8,
            "maximumVariantRules": 8,
            "maximumResources": 8,
            "maximumMounts": 8,
            "maximumIndexFilesPerMount": 8,
            "maximumPathBytes": 2048,
            "maximumPathSegments": 64
        },
        "observabilityPolicy": {
            "accessLogEnabled": true,
            "usageMeteringEnabled": true,
            "traceSampleRatePerMille": 10
        }
    })
}

fn delivery_request(path: &str, method: WebsiteDeliveryMethod) -> WebsiteDeliveryRequest {
    WebsiteDeliveryRequest {
        authority: "example.com".to_owned(),
        path: path.to_owned(),
        scheme: WebsiteDeliveryScheme::Https,
        method,
        request_id: "request-0001".to_owned(),
        trace_id: "trace-0001".to_owned(),
        routing: WebsiteDeliveryRoutingContext::default(),
        conditions: WebsiteRequestConditions::default(),
        range: None,
        locale: Some("en-US".to_owned()),
        spa_fallback_eligible: false,
    }
}

fn wiki_content_resolution(
    content_length: u64,
    range_supported: bool,
) -> WebsiteWikiRouteResolution {
    wiki_content_resolution_at("/guide", content_length, range_supported)
}

fn wiki_content_resolution_at(
    canonical_route: &str,
    content_length: u64,
    range_supported: bool,
) -> WebsiteWikiRouteResolution {
    WebsiteWikiRouteResolution::Content(Box::new(ResolvedWebsiteWikiContent {
        content_handle: WebsiteProviderContentHandle::new("opaque-content").unwrap(),
        kind: WebsiteWikiContentKind::Asset,
        canonical_route: canonical_route.to_owned(),
        page_uuid: Some("page-0001".to_owned()),
        public_page_version: "11".to_owned(),
        renderer_version: "renderer-v1".to_owned(),
        navigation_generation: "8".to_owned(),
        search_generation: "9".to_owned(),
        metadata: content_metadata(content_length, range_supported),
    }))
}

fn content_metadata(content_length: u64, range_supported: bool) -> WebsiteContentMetadata {
    WebsiteContentMetadata {
        content_type: "text/markdown".to_owned(),
        content_length,
        etag: "\"sha256:content:v11\"".to_owned(),
        last_modified: "Tue, 21 Jul 2026 00:00:00 GMT".to_owned(),
        content_version: "11".to_owned(),
        provider_generation: "7".to_owned(),
        range_supported,
    }
}

fn provider_error(kind: WebsiteProviderErrorKind) -> WebsiteProviderError {
    WebsiteProviderError::new(kind)
}
