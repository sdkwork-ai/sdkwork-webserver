use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use async_trait::async_trait;
use sdkwork_webserver_contract::provider::{
    OpenWebsiteContentRequest, OpenedWebsiteContent, ResolveWebsiteStaticPathRequest,
    ValidateWebsiteResourceRequest, ValidatedWebsiteResource, WebsiteContentResolution,
    WebsiteProviderError, WebsiteProviderErrorKind, WebsiteProviderResult, WebsiteResourceProvider,
    WebsiteStaticContentProvider,
};
use sdkwork_webserver_core::website_runtime::{
    compile_website_runtime_set_snapshot, website_runtime_descriptor_sha256,
    website_runtime_set_snapshot_sha256, WebsiteProviderType, WebsiteRuntimeDescriptor,
    WebsiteRuntimeSetSnapshot,
};
use sdkwork_webserver_delivery_runtime::{
    WebsiteProviderRegistry, WebsiteRuntimeProviderValidationError,
};
use serde_json::{json, Value};

const TENANT_SCOPE_HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";

#[derive(Clone, Copy)]
enum ValidationBehavior {
    Valid,
    Delayed,
    WrongIdentity,
}

struct FakeStaticProvider {
    behavior: ValidationBehavior,
    validations: AtomicUsize,
}

impl FakeStaticProvider {
    fn new(behavior: ValidationBehavior) -> Self {
        Self {
            behavior,
            validations: AtomicUsize::new(0),
        }
    }
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
        self.validations.fetch_add(1, Ordering::Relaxed);
        if matches!(self.behavior, ValidationBehavior::Delayed) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok(ValidatedWebsiteResource {
            provider_resource_uuid: if matches!(self.behavior, ValidationBehavior::WrongIdentity) {
                "different-root".to_owned()
            } else {
                request.provider.provider_resource_uuid.clone()
            },
            provider_generation: "7".to_owned(),
            public_generation: "generation=7;rootVersion=2".to_owned(),
            capabilities: request.required_capabilities.clone(),
        })
    }
}

#[async_trait]
impl WebsiteStaticContentProvider for FakeStaticProvider {
    async fn resolve_static_path(
        &self,
        _request: &ResolveWebsiteStaticPathRequest,
    ) -> WebsiteProviderResult<WebsiteContentResolution> {
        Err(WebsiteProviderError::new(
            WebsiteProviderErrorKind::NotFound,
        ))
    }

    async fn open_static_content(
        &self,
        _request: &OpenWebsiteContentRequest,
    ) -> WebsiteProviderResult<OpenedWebsiteContent> {
        Err(WebsiteProviderError::new(
            WebsiteProviderErrorKind::NotFound,
        ))
    }
}

#[tokio::test]
async fn validates_each_logical_resource_once_per_required_provider_port() {
    let runtime_set = runtime_set(2_500);
    let provider = Arc::new(FakeStaticProvider::new(ValidationBehavior::Valid));
    let mut registry = WebsiteProviderRegistry::new();
    registry
        .register_static(WebsiteProviderType::Drive, provider.clone())
        .unwrap();

    let report = registry
        .validate_runtime_set(&runtime_set, 4)
        .await
        .unwrap();

    assert_eq!(report.validated_resources, 1);
    assert_eq!(provider.validations.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn rejects_candidates_without_the_handler_specific_provider_port() {
    let error = WebsiteProviderRegistry::new()
        .validate_runtime_set(&runtime_set(2_500), 1)
        .await
        .unwrap_err();

    assert_eq!(
        error,
        WebsiteRuntimeProviderValidationError::ProviderNotRegistered {
            provider_type: WebsiteProviderType::Drive,
            capability: "static-content",
        }
    );
}

#[tokio::test]
async fn enforces_activation_deadlines_outside_provider_implementations() {
    let provider = Arc::new(FakeStaticProvider::new(ValidationBehavior::Delayed));
    let mut registry = WebsiteProviderRegistry::new();
    registry
        .register_static(WebsiteProviderType::Drive, provider)
        .unwrap();

    let error = registry
        .validate_runtime_set(&runtime_set(5), 1)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        WebsiteRuntimeProviderValidationError::Provider {
            provider_type: WebsiteProviderType::Drive,
            kind: WebsiteProviderErrorKind::DeadlineExceeded,
            ..
        }
    ));
}

#[tokio::test]
async fn rejects_provider_validation_responses_with_wrong_resource_identity() {
    let provider = Arc::new(FakeStaticProvider::new(ValidationBehavior::WrongIdentity));
    let mut registry = WebsiteProviderRegistry::new();
    registry
        .register_static(WebsiteProviderType::Drive, provider)
        .unwrap();

    let error = registry
        .validate_runtime_set(&runtime_set(2_500), 1)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        WebsiteRuntimeProviderValidationError::Provider {
            provider_type: WebsiteProviderType::Drive,
            kind: WebsiteProviderErrorKind::ContractMismatch,
            ..
        }
    ));
}

#[tokio::test]
async fn rejects_object_limits_above_the_registered_provider_capability() {
    let provider = Arc::new(FakeStaticProvider::new(ValidationBehavior::Valid));
    let mut registry = WebsiteProviderRegistry::new();
    registry
        .register_static(WebsiteProviderType::Drive, provider)
        .unwrap();

    let error = registry
        .validate_runtime_set(&runtime_set_with_object_limit(2_500, 2_048), 1)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        WebsiteRuntimeProviderValidationError::ObjectLimitUnsupported {
            provider_type: WebsiteProviderType::Drive,
            requested_bytes: 2_048,
            maximum_bytes: 1_024,
            ..
        }
    ));
}

fn runtime_set(
    provider_timeout_ms: u64,
) -> sdkwork_webserver_core::website_runtime::CompiledWebsiteRuntimeSet {
    runtime_set_with_object_limit(provider_timeout_ms, 1_024)
}

fn runtime_set_with_object_limit(
    provider_timeout_ms: u64,
    maximum_object_bytes: u64,
) -> sdkwork_webserver_core::website_runtime::CompiledWebsiteRuntimeSet {
    let mut descriptor = json!({
        "schemaVersion": "sdkwork.website-runtime.v1",
        "kind": "sdkwork.website-runtime.descriptor",
        "revisionUuid": "revision-spa",
        "appUuid": "site-spa",
        "tenantScopeHash": TENANT_SCOPE_HASH,
        "environment": "production",
        "generatedAt": "2026-07-21T00:00:00Z",
        "compilerVersion": "deploy-descriptor-compiler/1",
        "descriptorSha256": "0".repeat(64),
        "appDefaultVariantUuid": "variant-desktop",
        "bindings": [{
            "bindingUuid": "binding-spa",
            "hostname": "example.com",
            "pathPrefix": "/",
            "action": { "type": "SERVE" }
        }],
        "variants": [
            {"variantUuid": "variant-desktop", "label": "Desktop"},
            {"variantUuid": "variant-mobile", "label": "Mobile"}
        ],
        "variantRules": [],
        "resources": [{
            "resourceUuid": "resource-spa",
            "provider": {
                "providerType": "DRIVE",
                "providerResourceUuid": "website-root-0001",
                "providerContractVersion": "drive.website-root.v1"
            },
            "capabilities": {
                "staticContent": true,
                "wikiRoutes": false,
                "wikiSearch": false,
                "rangeRequests": true
            }
        }],
        "mounts": [
            {
                "mountUuid": "mount-desktop",
                "variantUuid": "variant-desktop",
                "pathPrefix": "/",
                "resourceUuid": "resource-spa",
                "handler": "SPA",
                "translation": {"mode": "ROOT", "resourceSubpath": "/web"},
                "indexFiles": ["index.html"],
                "spaFallback": "/web/index.html"
            },
            {
                "mountUuid": "mount-mobile",
                "variantUuid": "variant-mobile",
                "pathPrefix": "/",
                "resourceUuid": "resource-spa",
                "handler": "STATIC",
                "translation": {"mode": "ROOT", "resourceSubpath": "/mobile"},
                "indexFiles": ["index.html"]
            }
        ],
        "deliveryPolicy": {
            "providerTimeoutMs": provider_timeout_ms,
            "metadataCacheTtlSeconds": 60,
            "negativeCacheTtlSeconds": 5,
            "staleWhileRevalidateSeconds": 30,
            "maximumObjectBytes": maximum_object_bytes
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
    });
    let parsed: WebsiteRuntimeDescriptor = serde_json::from_value(descriptor.clone()).unwrap();
    descriptor["descriptorSha256"] =
        Value::String(website_runtime_descriptor_sha256(&parsed).unwrap());
    let mut snapshot = json!({
        "schemaVersion": "sdkwork.website-runtime-set.v1",
        "kind": "sdkwork.website-runtime-set.snapshot",
        "snapshotUuid": "snapshot-0001",
        "nodeUuid": "node-0001",
        "environment": "production",
        "generation": 1,
        "generatedAt": "2026-07-21T00:00:00Z",
        "compilerVersion": "deploy-runtime-set-compiler/1",
        "snapshotSha256": "0".repeat(64),
        "maximumSites": 8,
        "descriptors": [descriptor]
    });
    let parsed: WebsiteRuntimeSetSnapshot = serde_json::from_value(snapshot.clone()).unwrap();
    snapshot["snapshotSha256"] =
        Value::String(website_runtime_set_snapshot_sha256(&parsed).unwrap());
    compile_website_runtime_set_snapshot(&serde_json::to_vec(&snapshot).unwrap()).unwrap()
}
