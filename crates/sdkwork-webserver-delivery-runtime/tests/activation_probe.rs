use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;
use sdkwork_webserver_contract::provider::{
    OpenWebsiteContentRequest, OpenedWebsiteContent, ResolveWebsiteStaticPathRequest,
    ResolvedWebsiteContent, ValidateWebsiteResourceRequest, ValidatedWebsiteResource,
    WebsiteContentMetadata, WebsiteContentResolution, WebsiteProviderContentHandle,
    WebsiteProviderError, WebsiteProviderErrorKind, WebsiteProviderPurpose, WebsiteProviderResult,
    WebsiteResourceProvider, WebsiteStaticContentProvider,
};
use sdkwork_webserver_core::website_runtime::{
    website_runtime_descriptor_sha256, website_runtime_set_snapshot_sha256, WebsiteProviderType,
    WebsiteRuntimeDescriptor, WebsiteRuntimeEnvironment, WebsiteRuntimeRegistry,
    WebsiteRuntimeSetSnapshot,
};
use sdkwork_webserver_delivery_runtime::{
    probe_website_runtime_set_activation, WebsiteProviderRegistry,
    WebsiteRuntimeActivationProbeError,
};
use serde_json::{json, Value};

const NODE_UUID: &str = "activation-node-0001";
const TENANT_SCOPE_HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";

struct StaticProbeProvider {
    paths: HashSet<String>,
    observed: std::sync::Mutex<Vec<(String, WebsiteProviderPurpose)>>,
}

#[async_trait]
impl WebsiteResourceProvider for StaticProbeProvider {
    fn maximum_content_bytes(&self) -> u64 {
        1024
    }

    async fn validate_resource(
        &self,
        request: &ValidateWebsiteResourceRequest,
    ) -> WebsiteProviderResult<ValidatedWebsiteResource> {
        Ok(ValidatedWebsiteResource {
            provider_resource_uuid: request.provider.provider_resource_uuid.clone(),
            provider_generation: "1".to_owned(),
            public_generation: "1".to_owned(),
            capabilities: request.required_capabilities.clone(),
        })
    }
}

#[async_trait]
impl WebsiteStaticContentProvider for StaticProbeProvider {
    async fn resolve_static_path(
        &self,
        request: &ResolveWebsiteStaticPathRequest,
    ) -> WebsiteProviderResult<WebsiteContentResolution> {
        self.observed.lock().unwrap().push((
            request.provider_relative_path.clone(),
            request.context.purpose,
        ));
        if !self.paths.contains(&request.provider_relative_path) {
            return Err(WebsiteProviderError::new(
                WebsiteProviderErrorKind::NotFound,
            ));
        }
        Ok(WebsiteContentResolution::Found(ResolvedWebsiteContent {
            content_handle: WebsiteProviderContentHandle::new("activation-content").unwrap(),
            metadata: WebsiteContentMetadata {
                content_type: "text/html".to_owned(),
                content_length: 16,
                etag: "\"activation\"".to_owned(),
                last_modified: "2026-07-22T00:00:00Z".to_owned(),
                content_version: "1".to_owned(),
                provider_generation: "1".to_owned(),
                range_supported: true,
            },
        }))
    }

    async fn open_static_content(
        &self,
        _request: &OpenWebsiteContentRequest,
    ) -> WebsiteProviderResult<OpenedWebsiteContent> {
        unreachable!("HEAD activation probes must not open response bodies")
    }
}

#[tokio::test]
async fn probes_default_and_selectable_device_variants_with_activation_purpose() {
    let provider = Arc::new(StaticProbeProvider {
        paths: HashSet::from([
            "/desktop/index.html".to_owned(),
            "/mobile/index.html".to_owned(),
        ]),
        observed: std::sync::Mutex::new(Vec::new()),
    });
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_static(WebsiteProviderType::Drive, provider.clone())
        .unwrap();

    let report =
        probe_website_runtime_set_activation(compiled_runtime_set(2), Arc::new(providers), 2)
            .await
            .unwrap();

    assert_eq!(report.probed_bindings, 1);
    assert_eq!(report.probed_variants, 2);
    assert_eq!(report.probed_routes, 2);
    let observed = provider.observed.lock().unwrap();
    assert_eq!(observed.len(), 2);
    assert!(observed
        .iter()
        .all(|(_, purpose)| *purpose == WebsiteProviderPurpose::Activation));
    let paths = observed
        .iter()
        .map(|(path, _)| path.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(
        paths,
        HashSet::from(["/desktop/index.html", "/mobile/index.html"])
    );
}

#[tokio::test]
async fn failed_candidate_probe_does_not_replace_the_live_runtime() {
    let live_registry = Arc::new(WebsiteRuntimeRegistry::new(
        NODE_UUID,
        WebsiteRuntimeEnvironment::Production,
    ));
    live_registry.activate(compiled_runtime_set(1)).unwrap();
    let provider = Arc::new(StaticProbeProvider {
        paths: HashSet::from(["/desktop/index.html".to_owned()]),
        observed: std::sync::Mutex::new(Vec::new()),
    });
    let mut providers = WebsiteProviderRegistry::new();
    providers
        .register_static(WebsiteProviderType::Drive, provider)
        .unwrap();

    let error =
        probe_website_runtime_set_activation(compiled_runtime_set(2), Arc::new(providers), 2)
            .await
            .unwrap_err();

    assert!(matches!(
        error,
        WebsiteRuntimeActivationProbeError::RouteNotResolved { .. }
    ));
    assert_eq!(live_registry.current().unwrap().generation(), 1);
}

fn compiled_runtime_set(
    generation: u64,
) -> Arc<sdkwork_webserver_core::website_runtime::CompiledWebsiteRuntimeSet> {
    let descriptor = signed_descriptor(descriptor_fixture(generation));
    let mut value = json!({
        "schemaVersion": "sdkwork.website-runtime-set.v1",
        "kind": "sdkwork.website-runtime-set.snapshot",
        "snapshotUuid": format!("snapshot-{generation:04}"),
        "nodeUuid": NODE_UUID,
        "environment": "production",
        "generation": generation,
        "generatedAt": "2026-07-22T00:00:00Z",
        "compilerVersion": "sdkwork-deploy-runtime-set-compiler/1",
        "snapshotSha256": "0".repeat(64),
        "maximumSites": 8,
        "descriptors": [descriptor]
    });
    let snapshot: WebsiteRuntimeSetSnapshot = serde_json::from_value(value.clone()).unwrap();
    value["snapshotSha256"] =
        Value::String(website_runtime_set_snapshot_sha256(&snapshot).unwrap());
    Arc::new(
        sdkwork_webserver_core::website_runtime::compile_website_runtime_set_snapshot(
            &serde_json::to_vec(&value).unwrap(),
        )
        .unwrap(),
    )
}

fn signed_descriptor(mut value: Value) -> Value {
    let descriptor: WebsiteRuntimeDescriptor = serde_json::from_value(value.clone()).unwrap();
    value["descriptorSha256"] =
        Value::String(website_runtime_descriptor_sha256(&descriptor).unwrap());
    value
}

fn descriptor_fixture(generation: u64) -> Value {
    json!({
        "schemaVersion": "sdkwork.website-runtime.v1",
        "kind": "sdkwork.website-runtime.descriptor",
        "revisionUuid": format!("revision-{generation:04}"),
        "appUuid": "site-device-aware",
        "tenantScopeHash": TENANT_SCOPE_HASH,
        "environment": "production",
        "generatedAt": "2026-07-22T00:00:00Z",
        "compilerVersion": "sdkwork-deploy-runtime-compiler/1",
        "descriptorSha256": "0".repeat(64),
        "appDefaultVariantUuid": "variant-desktop",
        "bindings": [{
            "bindingUuid": "binding-primary",
            "hostname": "*.example.com",
            "pathPrefix": "/",
            "action": { "type": "SERVE" }
        }],
        "variants": [
            { "variantUuid": "variant-desktop", "label": "Desktop" },
            { "variantUuid": "variant-mobile", "label": "Mobile" }
        ],
        "variantRules": [{
            "ruleUuid": "rule-mobile",
            "variantUuid": "variant-mobile",
            "priority": 100,
            "match": { "type": "CLIENT_CLASS", "clientClass": "MOBILE" }
        }],
        "resources": [{
            "resourceUuid": "resource-drive",
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
                "resourceUuid": "resource-drive",
                "handler": "STATIC",
                "translation": { "mode": "ROOT", "resourceSubpath": "/desktop" },
                "indexFiles": ["index.html"]
            },
            {
                "mountUuid": "mount-mobile",
                "variantUuid": "variant-mobile",
                "pathPrefix": "/",
                "resourceUuid": "resource-drive",
                "handler": "STATIC",
                "translation": { "mode": "ROOT", "resourceSubpath": "/mobile" },
                "indexFiles": ["index.html"]
            }
        ],
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
