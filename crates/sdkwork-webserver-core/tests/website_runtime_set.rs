use sdkwork_webserver_core::website_runtime::{
    compile_website_runtime_set_snapshot, website_runtime_descriptor_sha256,
    website_runtime_set_snapshot_sha256, WebsiteRequestRoutingContext, WebsiteRouteSelection,
    WebsiteRuntimeDescriptor, WebsiteRuntimeEnvironment, WebsiteRuntimeRegistry,
    WebsiteRuntimeSetError, WebsiteRuntimeSetSnapshot, MAX_WEBSITE_RUNTIME_SET_BYTES,
};
use serde_json::{json, Value};

const NODE_UUID: &str = "node-0001";

fn descriptor_fixture(
    app_uuid: &str,
    hostname: &str,
    path_prefix: &str,
    provider_resource_uuid: &str,
) -> Value {
    json!({
        "schemaVersion": "sdkwork.website-runtime.v1",
        "kind": "sdkwork.website-runtime.descriptor",
        "revisionUuid": format!("revision-{app_uuid}"),
        "appUuid": app_uuid,
        "tenantScopeHash": "1".repeat(64),
        "environment": "production",
        "generatedAt": "2026-07-21T00:00:00Z",
        "compilerVersion": "deploy-descriptor-compiler/1",
        "descriptorSha256": "0".repeat(64),
        "appDefaultVariantUuid": "variant-default",
        "bindings": [
            {
                "bindingUuid": format!("binding-{app_uuid}"),
                "hostname": hostname,
                "pathPrefix": path_prefix,
                "action": {
                    "type": "SERVE"
                }
            }
        ],
        "variants": [
            {
                "variantUuid": "variant-default",
                "label": "Default"
            }
        ],
        "variantRules": [],
        "resources": [
            {
                "resourceUuid": "resource-default",
                "provider": {
                    "providerType": "DRIVE",
                    "providerResourceUuid": provider_resource_uuid,
                    "providerContractVersion": "drive.website-root.v1"
                },
                "capabilities": {
                    "staticContent": true,
                    "wikiRoutes": false,
                    "wikiSearch": false,
                    "rangeRequests": true
                }
            }
        ],
        "mounts": [
            {
                "mountUuid": "mount-default",
                "variantUuid": "variant-default",
                "pathPrefix": "/",
                "resourceUuid": "resource-default",
                "handler": "STATIC",
                "translation": {
                    "mode": "ROOT",
                    "resourceSubpath": "/"
                },
                "indexFiles": ["index.html"]
            }
        ],
        "deliveryPolicy": {
            "providerTimeoutMs": 3000,
            "metadataCacheTtlSeconds": 60,
            "negativeCacheTtlSeconds": 5,
            "staleWhileRevalidateSeconds": 30,
            "maximumObjectBytes": 1073741824
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

fn signed_descriptor(mut value: Value) -> Value {
    let descriptor: WebsiteRuntimeDescriptor = serde_json::from_value(value.clone()).unwrap();
    value["descriptorSha256"] =
        Value::String(website_runtime_descriptor_sha256(&descriptor).unwrap());
    value
}

fn runtime_set_fixture(descriptors: Vec<Value>) -> Value {
    json!({
        "schemaVersion": "sdkwork.website-runtime-set.v1",
        "kind": "sdkwork.website-runtime-set.snapshot",
        "snapshotUuid": "snapshot-0001",
        "nodeUuid": NODE_UUID,
        "environment": "production",
        "generation": 1,
        "generatedAt": "2026-07-21T00:00:00Z",
        "compilerVersion": "deploy-runtime-set-compiler/1",
        "snapshotSha256": "0".repeat(64),
        "maximumSites": 100,
        "descriptors": descriptors
    })
}

fn signed_runtime_set(mut value: Value) -> Vec<u8> {
    let snapshot: WebsiteRuntimeSetSnapshot = serde_json::from_value(value.clone()).unwrap();
    value["snapshotSha256"] =
        Value::String(website_runtime_set_snapshot_sha256(&snapshot).unwrap());
    serde_json::to_vec(&value).unwrap()
}

fn two_site_runtime_set() -> Vec<u8> {
    signed_runtime_set(runtime_set_fixture(vec![
        signed_descriptor(descriptor_fixture(
            "site-a",
            "example.com",
            "/",
            "website-root-a",
        )),
        signed_descriptor(descriptor_fixture(
            "site-b",
            "example.com",
            "/mobile",
            "website-root-b",
        )),
    ]))
}

fn selected_provider(bytes: &[u8], host: &str, path: &str) -> Option<String> {
    let compiled = compile_website_runtime_set_snapshot(bytes).unwrap();
    let selected = compiled
        .select_route(host, path, WebsiteRequestRoutingContext::default())
        .unwrap()?;
    let WebsiteRouteSelection::Serve(selected) = selected else {
        panic!("expected a served route");
    };
    Some(selected.provider.provider_resource_uuid.clone())
}

#[test]
fn selects_the_longest_cross_site_path_without_retargeting_provider_roots() {
    let bytes = two_site_runtime_set();
    let compiled = compile_website_runtime_set_snapshot(&bytes).unwrap();
    assert!(compiled.contains_tenant_scope_hash(&"1".repeat(64)));
    assert!(!compiled.contains_tenant_scope_hash(&"2".repeat(64)));
    assert_eq!(
        selected_provider(&bytes, "example.com", "/mobile/app.js").as_deref(),
        Some("website-root-b")
    );
    assert_eq!(
        selected_provider(&bytes, "example.com", "/desktop/app.js").as_deref(),
        Some("website-root-a")
    );
}

#[test]
fn identifies_empty_single_tenant_and_multi_tenant_runtime_sets() {
    let expected = "1".repeat(64);
    let empty = signed_runtime_set(runtime_set_fixture(Vec::new()));
    let empty = compile_website_runtime_set_snapshot(&empty).unwrap();
    assert!(empty.is_empty_or_single_tenant_scope(&expected));

    let single = compile_website_runtime_set_snapshot(&two_site_runtime_set()).unwrap();
    assert!(single.is_empty_or_single_tenant_scope(&expected));
    assert!(!single.is_empty_or_single_tenant_scope(&"2".repeat(64)));

    let mut second_tenant = descriptor_fixture("site-b", "example.net", "/", "website-root-b");
    second_tenant["tenantScopeHash"] = Value::String("2".repeat(64));
    let multi = signed_runtime_set(runtime_set_fixture(vec![
        signed_descriptor(descriptor_fixture(
            "site-a",
            "example.com",
            "/",
            "website-root-a",
        )),
        signed_descriptor(second_tenant),
    ]));
    let multi = compile_website_runtime_set_snapshot(&multi).unwrap();
    assert_eq!(multi.tenant_scope_count(), 2);
    assert!(!multi.is_empty_or_single_tenant_scope(&expected));
}

#[test]
fn exact_host_ownership_does_not_fall_back_to_a_wildcard_site() {
    let bytes = signed_runtime_set(runtime_set_fixture(vec![
        signed_descriptor(descriptor_fixture(
            "site-a",
            "example.com",
            "/private",
            "website-root-a",
        )),
        signed_descriptor(descriptor_fixture(
            "site-b",
            "*.example.com",
            "/",
            "website-root-b",
        )),
    ]));
    assert_eq!(selected_provider(&bytes, "example.com", "/"), None);
    assert_eq!(
        selected_provider(&bytes, "preview.example.com", "/").as_deref(),
        Some("website-root-b")
    );
}

#[test]
fn rejects_cross_site_host_and_path_conflicts() {
    let bytes = signed_runtime_set(runtime_set_fixture(vec![
        signed_descriptor(descriptor_fixture(
            "site-a",
            "example.com",
            "/",
            "website-root-a",
        )),
        signed_descriptor(descriptor_fixture(
            "site-b",
            "example.com",
            "/",
            "website-root-b",
        )),
    ]));
    assert!(matches!(
        compile_website_runtime_set_snapshot(&bytes),
        Err(WebsiteRuntimeSetError::RouteConflict { .. })
    ));
}

#[test]
fn invalid_candidates_do_not_change_the_current_snapshot() {
    let registry = WebsiteRuntimeRegistry::new(NODE_UUID, WebsiteRuntimeEnvironment::Production);
    let first = two_site_runtime_set();
    let activated = registry.compile_and_activate(&first).unwrap();
    assert!(activated.changed);
    assert!(registry.is_ready());
    let current_hash = registry.current().unwrap().snapshot_sha256().to_owned();

    let mut invalid_descriptor = signed_descriptor(descriptor_fixture(
        "site-a",
        "example.com",
        "/",
        "website-root-a",
    ));
    invalid_descriptor["descriptorSha256"] = Value::String("f".repeat(64));
    let invalid = signed_runtime_set(runtime_set_fixture(vec![invalid_descriptor]));
    assert!(matches!(
        registry.compile_and_activate(&invalid),
        Err(WebsiteRuntimeSetError::Descriptor { .. })
    ));
    assert_eq!(registry.current().unwrap().snapshot_sha256(), current_hash);

    let unchanged = registry.compile_and_activate(&first).unwrap();
    assert!(!unchanged.changed);
    assert_eq!(unchanged.previous_snapshot_sha256, None);
}

#[test]
fn activation_swaps_one_complete_snapshot_and_retains_one_rollback_generation() {
    let registry = WebsiteRuntimeRegistry::new(NODE_UUID, WebsiteRuntimeEnvironment::Production);
    let first = two_site_runtime_set();
    let first_report = registry.compile_and_activate(&first).unwrap();

    let mut second_value: Value =
        serde_json::from_slice(&two_site_runtime_set()).expect("fixture is JSON");
    second_value["snapshotUuid"] = json!("snapshot-0002");
    second_value["generation"] = json!(2);
    let second = signed_runtime_set(second_value);
    let second_report = registry.compile_and_activate(&second).unwrap();
    assert!(second_report.changed);
    assert_eq!(second_report.generation, 2);
    assert_eq!(second_report.previous_generation, Some(1));
    assert_eq!(
        second_report.previous_snapshot_sha256.as_deref(),
        Some(first_report.snapshot_sha256.as_str())
    );
    assert_eq!(
        registry.current().unwrap().snapshot_sha256(),
        second_report.snapshot_sha256
    );

    let rollback = registry.rollback().expect("one generation is retained");
    assert_eq!(rollback.generation, 1);
    assert_eq!(rollback.rolled_back_from_generation, 2);
    assert_eq!(rollback.snapshot_sha256, first_report.snapshot_sha256);
    assert_eq!(
        rollback.rolled_back_from_sha256,
        second_report.snapshot_sha256
    );
    assert_eq!(
        registry.current().unwrap().snapshot_sha256(),
        rollback.snapshot_sha256
    );
    assert!(registry.rollback().is_none());

    assert!(matches!(
        registry.compile_and_activate(&second),
        Err(WebsiteRuntimeSetError::StaleGeneration {
            candidate_generation: 2,
            highest_generation: 2
        })
    ));
}

#[test]
fn rejects_stale_and_same_generation_conflicting_snapshots() {
    let registry = WebsiteRuntimeRegistry::new(NODE_UUID, WebsiteRuntimeEnvironment::Production);
    let mut newest: Value = serde_json::from_slice(&two_site_runtime_set()).unwrap();
    newest["snapshotUuid"] = json!("snapshot-0003");
    newest["generation"] = json!(3);
    registry
        .compile_and_activate(&signed_runtime_set(newest))
        .unwrap();

    assert!(matches!(
        registry.compile_and_activate(&two_site_runtime_set()),
        Err(WebsiteRuntimeSetError::StaleGeneration {
            candidate_generation: 1,
            highest_generation: 3
        })
    ));

    let mut conflicting: Value = serde_json::from_slice(&two_site_runtime_set()).unwrap();
    conflicting["snapshotUuid"] = json!("snapshot-conflicting");
    conflicting["generation"] = json!(3);
    assert!(matches!(
        registry.compile_and_activate(&signed_runtime_set(conflicting)),
        Err(WebsiteRuntimeSetError::GenerationConflict { generation: 3, .. })
    ));
    assert_eq!(registry.current().unwrap().generation(), 3);
}

#[test]
fn registry_rejects_snapshots_assigned_to_another_node_or_environment() {
    let registry = WebsiteRuntimeRegistry::new(NODE_UUID, WebsiteRuntimeEnvironment::Production);
    let mut other_node: Value = serde_json::from_slice(&two_site_runtime_set()).unwrap();
    other_node["nodeUuid"] = json!("node-0002");
    let bytes = signed_runtime_set(other_node);
    assert!(matches!(
        registry.compile_and_activate(&bytes),
        Err(WebsiteRuntimeSetError::ScopeMismatch { .. })
    ));
    assert!(!registry.is_ready());
}

#[test]
fn rejects_outer_hash_tampering_environment_mismatch_and_noncanonical_site_order() {
    let unsigned = runtime_set_fixture(vec![signed_descriptor(descriptor_fixture(
        "site-a",
        "example.com",
        "/",
        "website-root-a",
    ))]);
    assert!(matches!(
        compile_website_runtime_set_snapshot(&serde_json::to_vec(&unsigned).unwrap()),
        Err(WebsiteRuntimeSetError::HashMismatch { .. })
    ));

    let mut environment_mismatch = signed_descriptor(descriptor_fixture(
        "site-a",
        "example.com",
        "/",
        "website-root-a",
    ));
    environment_mismatch["environment"] = json!("staging");
    let environment_mismatch = signed_runtime_set(runtime_set_fixture(vec![environment_mismatch]));
    let error = compile_website_runtime_set_snapshot(&environment_mismatch)
        .expect_err("descriptor environment must match the set");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path == "/descriptors/0/environment"));

    let noncanonical = signed_runtime_set(runtime_set_fixture(vec![
        signed_descriptor(descriptor_fixture(
            "site-b",
            "example.net",
            "/",
            "website-root-b",
        )),
        signed_descriptor(descriptor_fixture(
            "site-a",
            "example.com",
            "/",
            "website-root-a",
        )),
    ]));
    let error = compile_website_runtime_set_snapshot(&noncanonical)
        .expect_err("Site descriptors must use canonical order");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path == "/descriptors"));
}

#[test]
fn accepts_an_empty_assigned_set_and_rejects_oversized_input() {
    let registry = WebsiteRuntimeRegistry::new(NODE_UUID, WebsiteRuntimeEnvironment::Production);
    let empty = signed_runtime_set(runtime_set_fixture(Vec::new()));
    registry.compile_and_activate(&empty).unwrap();
    assert!(registry.is_ready());
    assert_eq!(registry.current().unwrap().site_count(), 0);

    let oversized = vec![b' '; MAX_WEBSITE_RUNTIME_SET_BYTES + 1];
    assert!(matches!(
        compile_website_runtime_set_snapshot(&oversized),
        Err(WebsiteRuntimeSetError::TooLarge { .. })
    ));
}
