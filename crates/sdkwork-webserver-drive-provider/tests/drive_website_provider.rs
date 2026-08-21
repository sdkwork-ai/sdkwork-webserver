use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU16, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use async_trait::async_trait;
use sdkwork_drive_internal_sdk::{
    models::{DriveResourceResolution, ResolveDriveResourceRequest, WebsiteRoot},
    SdkworkError,
};
use sdkwork_webserver_contract::provider::{
    OpenWebsiteContentRequest, ProviderResourceReference, ResolveWebsiteStaticPathRequest,
    ValidateWebsiteResourceRequest, WebsiteByteRange, WebsiteContentResolution,
    WebsiteProviderContentHandle, WebsiteProviderErrorKind, WebsiteProviderPurpose,
    WebsiteProviderRuntimeContext, WebsiteRequestConditions, WebsiteResourceCapabilities,
    WebsiteResourceProvider, WebsiteStaticContentProvider,
};
use sdkwork_webserver_core::website_runtime::WebsiteProviderType;
use sdkwork_webserver_drive_provider::{
    DriveContentChunkStream, DriveWebsiteProvider, DriveWebsiteSdkClient,
    FixedDriveWebsiteSdkClientResolver, DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION,
};

const WEBSITE_ROOT_UUID: &str = "11111111-1111-4111-8111-111111111701";
const CHECKSUM: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[derive(Clone, Debug, PartialEq, Eq)]
struct ContentCall {
    node_version_id: String,
    scope_uuid: String,
    relative_path: String,
    pinned_generation: Option<String>,
    range: Option<String>,
}

struct FakeDriveWebsiteSdk {
    root: Mutex<WebsiteRoot>,
    resolution: Mutex<DriveResourceResolution>,
    content: Mutex<Vec<u8>>,
    resolve_requests: Mutex<Vec<ResolveDriveResourceRequest>>,
    content_calls: Mutex<Vec<ContentCall>>,
    next_root_status: AtomicU16,
    next_content_status: AtomicU16,
    root_delay_ms: AtomicU64,
    root_calls: AtomicUsize,
}

impl FakeDriveWebsiteSdk {
    fn new() -> Self {
        Self {
            root: Mutex::new(website_root()),
            resolution: Mutex::new(resource_resolution()),
            content: Mutex::new(b"0123456789".to_vec()),
            resolve_requests: Mutex::new(Vec::new()),
            content_calls: Mutex::new(Vec::new()),
            next_root_status: AtomicU16::new(0),
            next_content_status: AtomicU16::new(0),
            root_delay_ms: AtomicU64::new(0),
            root_calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl DriveWebsiteSdkClient for FakeDriveWebsiteSdk {
    async fn retrieve_website_root(
        &self,
        _website_root_uuid: &str,
    ) -> Result<WebsiteRoot, SdkworkError> {
        self.root_calls.fetch_add(1, Ordering::AcqRel);
        let delay_ms = self.root_delay_ms.load(Ordering::Acquire);
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
        let status = self.next_root_status.swap(0, Ordering::AcqRel);
        if status > 0 {
            return Err(SdkworkError::HttpStatus {
                status,
                body: "{}".to_string(),
            });
        }
        Ok(self.root.lock().expect("root lock").clone())
    }

    async fn resolve_resource(
        &self,
        request: &ResolveDriveResourceRequest,
    ) -> Result<DriveResourceResolution, SdkworkError> {
        self.resolve_requests
            .lock()
            .expect("resolve requests lock")
            .push(request.clone());
        Ok(self.resolution.lock().expect("resolution lock").clone())
    }

    async fn retrieve_content(
        &self,
        node_version_id: &str,
        _scope_type: &str,
        scope_uuid: &str,
        relative_path: &str,
        pinned_generation: Option<&str>,
        range: Option<&str>,
        _if_match: Option<&str>,
        _if_none_match: Option<&str>,
        _if_range: Option<&str>,
        _if_modified_since: Option<&str>,
        _if_unmodified_since: Option<&str>,
    ) -> Result<Vec<u8>, SdkworkError> {
        self.content_calls
            .lock()
            .expect("content calls lock")
            .push(ContentCall {
                node_version_id: node_version_id.to_string(),
                scope_uuid: scope_uuid.to_string(),
                relative_path: relative_path.to_string(),
                pinned_generation: pinned_generation.map(str::to_string),
                range: range.map(str::to_string),
            });
        let status = self.next_content_status.swap(0, Ordering::AcqRel);
        if status > 0 {
            return Err(SdkworkError::HttpStatus {
                status,
                body: "{}".to_string(),
            });
        }
        let content = self.content.lock().expect("content lock").clone();
        match range {
            Some("bytes=2-5") => Ok(content[2..=5].to_vec()),
            Some(other) => panic!("unexpected test range {other}"),
            None => Ok(content),
        }
    }

    async fn retrieve_content_stream(
        &self,
        node_version_id: &str,
        _scope_type: &str,
        scope_uuid: &str,
        relative_path: &str,
        pinned_generation: Option<&str>,
        range: Option<&str>,
        _if_match: Option<&str>,
        _if_none_match: Option<&str>,
        _if_range: Option<&str>,
        _if_modified_since: Option<&str>,
        _if_unmodified_since: Option<&str>,
    ) -> Result<Box<dyn DriveContentChunkStream>, SdkworkError> {
        self.content_calls
            .lock()
            .expect("content calls lock")
            .push(ContentCall {
                node_version_id: node_version_id.to_string(),
                scope_uuid: scope_uuid.to_string(),
                relative_path: relative_path.to_string(),
                pinned_generation: pinned_generation.map(str::to_string),
                range: range.map(str::to_string),
            });
        let status = self.next_content_status.swap(0, Ordering::AcqRel);
        if status > 0 {
            return Err(SdkworkError::HttpStatus {
                status,
                body: "{}".to_string(),
            });
        }
        let content = self.content.lock().expect("content lock").clone();
        let selected = match range {
            Some("bytes=2-5") => content[2..=5].to_vec(),
            Some(other) => panic!("unexpected test range {other}"),
            None => content,
        };
        let chunks = selected
            .chunks(4)
            .map(<[u8]>::to_vec)
            .collect::<VecDeque<_>>();
        Ok(Box::new(TestMemoryChunkStream { chunks }))
    }
}

struct TestMemoryChunkStream {
    chunks: VecDeque<Vec<u8>>,
}

#[async_trait]
impl DriveContentChunkStream for TestMemoryChunkStream {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, SdkworkError> {
        Ok(self.chunks.pop_front())
    }
}

#[tokio::test]
async fn validates_active_space_or_folder_roots_capabilities_and_tenant_scope() {
    let (provider, sdk) = test_provider();
    let validated = provider
        .validate_resource(&validate_request(false))
        .await
        .expect("active WebsiteRoot");
    assert_eq!(validated.provider_resource_uuid, WEBSITE_ROOT_UUID);
    assert_eq!(validated.provider_generation, "3");
    assert_eq!(
        validated.public_generation,
        "generation=3;rootVersion=7;contentMode=LIVE_TREE"
    );
    assert_eq!(
        validated.capabilities,
        WebsiteResourceCapabilities {
            static_content: true,
            wiki_routes: false,
            wiki_search: false,
            range_requests: true,
        }
    );

    sdk.root.lock().expect("root lock").source_root_mode = "FOLDER".to_string();
    provider
        .validate_resource(&validate_request(true))
        .await
        .expect("folder WebsiteRoot uses the same provider contract");

    let mut wiki_request = validate_request(false);
    wiki_request.required_capabilities.wiki_routes = true;
    let unsupported = provider
        .validate_resource(&wiki_request)
        .await
        .expect_err("Drive does not provide Wiki routing");
    assert_eq!(unsupported.kind, WebsiteProviderErrorKind::ContractMismatch);

    let mut wrong_tenant = validate_request(false);
    wrong_tenant.context.tenant_scope_hash = "tenant-other".to_string();
    let hidden = provider
        .validate_resource(&wrong_tenant)
        .await
        .expect_err("tenant-bound resolver must hide the resource");
    assert_eq!(hidden.kind, WebsiteProviderErrorKind::NotFound);
}

#[tokio::test]
async fn resolves_canonical_paths_metadata_and_http_conditions() {
    let (provider, sdk) = test_provider();
    let resolved = provider
        .resolve_static_path(&resolve_request(WebsiteRequestConditions::default()))
        .await
        .expect("resolve static content");
    let WebsiteContentResolution::Found(content) = resolved else {
        panic!("expected resolved content")
    };
    assert_eq!(content.metadata.content_type, "application/javascript");
    assert_eq!(content.metadata.content_length, 10);
    assert_eq!(content.metadata.etag, format!("\"{CHECKSUM}\""));
    assert_eq!(
        content.metadata.last_modified,
        "Tue, 21 Jul 2026 00:00:00 GMT"
    );
    assert!(content.metadata.range_supported);
    assert_eq!(
        sdk.resolve_requests.lock().expect("resolve requests lock")[0].relative_path,
        "assets/app.js"
    );

    let not_modified = provider
        .resolve_static_path(&resolve_request(WebsiteRequestConditions {
            if_none_match: Some(content.metadata.etag.clone()),
            ..WebsiteRequestConditions::default()
        }))
        .await
        .expect("conditional resolution");
    assert_eq!(not_modified, WebsiteContentResolution::NotModified);

    let failed = provider
        .resolve_static_path(&resolve_request(WebsiteRequestConditions {
            if_match: Some("\"different\"".to_string()),
            ..WebsiteRequestConditions::default()
        }))
        .await
        .expect_err("If-Match mismatch");
    assert_eq!(failed.kind, WebsiteProviderErrorKind::PreconditionFailed);

    let mut invalid = resolve_request(WebsiteRequestConditions::default());
    invalid.provider_relative_path = "/assets/%2e%2e/private".to_string();
    let invalid_path = provider
        .resolve_static_path(&invalid)
        .await
        .expect_err("encoded traversal path");
    assert_eq!(invalid_path.kind, WebsiteProviderErrorKind::InvalidPath);
}

#[tokio::test]
async fn revalidates_path_generation_and_node_version_before_opening_content() {
    let (provider, sdk) = test_provider();
    let handle = resolved_handle(&provider).await;
    let mut opened = provider
        .open_static_content(&open_request(
            handle,
            None,
            WebsiteRequestConditions::default(),
        ))
        .await
        .expect("open complete content");
    assert_eq!(opened.content_length, 10);
    assert_eq!(opened.content_range, None);
    let mut collected = Vec::new();
    while let Some(chunk) = opened.stream.next_chunk().await.expect("content chunk") {
        collected.extend_from_slice(&chunk);
    }
    assert_eq!(collected, b"0123456789".to_vec());
    assert_eq!(opened.stream.next_chunk().await.expect("stream end"), None);

    let request = sdk
        .resolve_requests
        .lock()
        .expect("resolve requests lock")
        .last()
        .expect("revalidation request")
        .clone();
    assert_eq!(request.relative_path, "assets/app.js");
    assert_eq!(request.pinned_generation.as_deref(), Some("3"));
    assert_eq!(
        request.pinned_node_version_id.as_deref(),
        Some("version-app")
    );
    assert_eq!(
        sdk.content_calls.lock().expect("content calls lock")[0],
        ContentCall {
            node_version_id: "version-app".to_string(),
            scope_uuid: WEBSITE_ROOT_UUID.to_string(),
            relative_path: "assets/app.js".to_string(),
            pinned_generation: Some("3".to_string()),
            range: None,
        }
    );
}

#[tokio::test]
async fn serves_ranges_and_falls_back_to_full_content_for_if_range_mismatch() {
    let (provider, sdk) = test_provider();
    let handle = resolved_handle(&provider).await;
    let mut ranged = provider
        .open_static_content(&open_request(
            handle.clone(),
            Some(WebsiteByteRange {
                start: 2,
                end_inclusive: Some(5),
                suffix_bytes: None,
            }),
            WebsiteRequestConditions::default(),
        ))
        .await
        .expect("open range");
    assert_eq!(ranged.content_length, 4);
    assert_eq!(
        ranged.content_range,
        Some(sdkwork_webserver_contract::provider::WebsiteContentRange {
            start: 2,
            end_inclusive: 5,
            complete_length: 10,
        })
    );
    assert_eq!(
        ranged.stream.next_chunk().await.expect("range chunk"),
        Some(b"2345".to_vec())
    );

    let full = provider
        .open_static_content(&open_request(
            handle.clone(),
            Some(WebsiteByteRange {
                start: 2,
                end_inclusive: Some(5),
                suffix_bytes: None,
            }),
            WebsiteRequestConditions {
                if_range: Some("\"different\"".to_string()),
                ..WebsiteRequestConditions::default()
            },
        ))
        .await
        .expect("If-Range mismatch returns the complete representation");
    assert_eq!(full.content_length, 10);
    assert_eq!(full.content_range, None);

    let unsatisfiable = provider
        .open_static_content(&open_request(
            handle.clone(),
            Some(WebsiteByteRange {
                start: 10,
                end_inclusive: None,
                suffix_bytes: None,
            }),
            WebsiteRequestConditions::default(),
        ))
        .await
        .err()
        .expect("out-of-bounds range");
    assert_eq!(
        unsatisfiable.kind,
        WebsiteProviderErrorKind::RangeNotSatisfiable
    );

    sdk.next_content_status.store(416, Ordering::Release);
    let owner_416 = provider
        .open_static_content(&open_request(
            handle,
            Some(WebsiteByteRange {
                start: 2,
                end_inclusive: Some(5),
                suffix_bytes: None,
            }),
            WebsiteRequestConditions::default(),
        ))
        .await
        .err()
        .expect("owner 416");
    assert_eq!(
        owner_416.kind,
        WebsiteProviderErrorKind::RangeNotSatisfiable
    );
}

#[tokio::test]
async fn fails_closed_on_revocation_contract_drift_and_length_mismatch() {
    let (provider, sdk) = test_provider();
    sdk.root.lock().expect("root lock").root_status = "SUSPENDED".to_string();
    let revoked = provider
        .validate_resource(&validate_request(false))
        .await
        .expect_err("suspended root");
    assert_eq!(revoked.kind, WebsiteProviderErrorKind::Revoked);

    sdk.root.lock().expect("root lock").root_status = "ACTIVE".to_string();
    let handle = resolved_handle(&provider).await;
    sdk.resolution
        .lock()
        .expect("resolution lock")
        .scope_generation = "4".to_string();
    let drift = provider
        .open_static_content(&open_request(
            handle,
            None,
            WebsiteRequestConditions::default(),
        ))
        .await
        .err()
        .expect("generation drift");
    assert_eq!(drift.kind, WebsiteProviderErrorKind::ContractMismatch);

    *sdk.resolution.lock().expect("resolution lock") = resource_resolution();
    let handle = resolved_handle(&provider).await;
    *sdk.content.lock().expect("content lock") = b"short".to_vec();
    // A short owner body opens successfully in streaming mode and fails
    // closed while the stream is consumed (exact-length enforcement).
    let mut short = provider
        .open_static_content(&open_request(
            handle,
            None,
            WebsiteRequestConditions::default(),
        ))
        .await
        .expect("streaming open succeeds");
    let mut short_error = None;
    loop {
        match short.stream.next_chunk().await {
            Ok(Some(_)) => {}
            Ok(None) => panic!("short owner body must fail length enforcement before EOF"),
            Err(error) => {
                short_error = Some(error);
                break;
            }
        }
    }
    let short_error = short_error.expect("short owner body fails while streaming");
    assert_eq!(short_error.kind, WebsiteProviderErrorKind::ContractMismatch);
}

#[tokio::test]
async fn maps_sdk_statuses_and_enforces_provider_deadlines() {
    let (provider, sdk) = test_provider();
    for (status, expected) in [
        (404, WebsiteProviderErrorKind::NotFound),
        (410, WebsiteProviderErrorKind::Revoked),
        (429, WebsiteProviderErrorKind::RateLimited),
        (503, WebsiteProviderErrorKind::Unavailable),
        (401, WebsiteProviderErrorKind::ContractMismatch),
    ] {
        sdk.next_root_status.store(status, Ordering::Release);
        let error = provider
            .validate_resource(&validate_request(false))
            .await
            .expect_err("mapped SDK status");
        assert_eq!(error.kind, expected);
    }

    sdk.root_delay_ms.store(25, Ordering::Release);
    let mut request = validate_request(false);
    request.context.deadline_ms = 1;
    let timeout = provider
        .validate_resource(&request)
        .await
        .expect_err("provider deadline");
    assert_eq!(timeout.kind, WebsiteProviderErrorKind::DeadlineExceeded);
}

fn test_provider() -> (DriveWebsiteProvider, Arc<FakeDriveWebsiteSdk>) {
    let sdk = Arc::new(FakeDriveWebsiteSdk::new());
    let client: Arc<dyn DriveWebsiteSdkClient> = sdk.clone();
    let resolver =
        FixedDriveWebsiteSdkClientResolver::new("tenant-scope", client).expect("fixed resolver");
    (DriveWebsiteProvider::new(Arc::new(resolver)), sdk)
}

fn context() -> WebsiteProviderRuntimeContext {
    WebsiteProviderRuntimeContext {
        tenant_scope_hash: "tenant-scope".to_string(),
        site_uuid: "site-1".to_string(),
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
        provider_type: WebsiteProviderType::Drive,
        provider_resource_uuid: WEBSITE_ROOT_UUID.to_string(),
        provider_contract_version: DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION.to_string(),
    }
}

fn validate_request(range_requests: bool) -> ValidateWebsiteResourceRequest {
    ValidateWebsiteResourceRequest {
        context: context(),
        provider: reference(),
        required_capabilities: WebsiteResourceCapabilities {
            static_content: true,
            wiki_routes: false,
            wiki_search: false,
            range_requests,
        },
    }
}

fn resolve_request(conditions: WebsiteRequestConditions) -> ResolveWebsiteStaticPathRequest {
    ResolveWebsiteStaticPathRequest {
        context: context(),
        provider: reference(),
        provider_relative_path: "/assets/app.js".to_string(),
        conditions,
    }
}

fn open_request(
    content_handle: WebsiteProviderContentHandle,
    range: Option<WebsiteByteRange>,
    conditions: WebsiteRequestConditions,
) -> OpenWebsiteContentRequest {
    OpenWebsiteContentRequest {
        context: context(),
        provider: reference(),
        provider_relative_path: "/assets/app.js".to_string(),
        content_handle,
        range,
        conditions,
        maximum_bytes: 1_024,
    }
}

async fn resolved_handle(provider: &DriveWebsiteProvider) -> WebsiteProviderContentHandle {
    let resolved = provider
        .resolve_static_path(&resolve_request(WebsiteRequestConditions::default()))
        .await
        .expect("resolve content handle");
    let WebsiteContentResolution::Found(content) = resolved else {
        panic!("expected content handle")
    };
    content.content_handle
}

fn website_root() -> WebsiteRoot {
    WebsiteRoot {
        uuid: WEBSITE_ROOT_UUID.to_string(),
        space_id: "space-website".to_string(),
        source_root_mode: "SPACE_ROOT".to_string(),
        content_mode: "LIVE_TREE".to_string(),
        active_generation: "3".to_string(),
        root_status: "ACTIVE".to_string(),
        capabilities: vec![
            "STATIC_CONTENT".to_string(),
            "BYTE_RANGE".to_string(),
            "CONDITIONAL_REQUESTS".to_string(),
        ],
        version: "7".to_string(),
        updated_at: "2026-07-21T00:00:00Z".to_string(),
    }
}

fn resource_resolution() -> DriveResourceResolution {
    DriveResourceResolution {
        scope_type: "WEBSITE_ROOT".to_string(),
        scope_uuid: WEBSITE_ROOT_UUID.to_string(),
        scope_generation: "3".to_string(),
        normalized_relative_path: "assets/app.js".to_string(),
        resource_type: "FILE".to_string(),
        node_id: "node-app".to_string(),
        logical_node_version_id: "version-app".to_string(),
        version_no: "5".to_string(),
        checksum_sha256_hex: CHECKSUM.to_string(),
        etag: format!("\"{CHECKSUM}\""),
        content_type: "application/javascript".to_string(),
        content_length: "10".to_string(),
        last_modified: "2026-07-21T00:00:00Z".to_string(),
        scope_status: "ACTIVE".to_string(),
        node_status: "ACTIVE".to_string(),
        eligibility: "ELIGIBLE".to_string(),
    }
}
