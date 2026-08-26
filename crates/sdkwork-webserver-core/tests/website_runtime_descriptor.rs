use sdkwork_webserver_core::website_runtime::{
    compile_website_runtime_descriptor, website_runtime_descriptor_sha256, WebsiteClientClass,
    WebsiteClientClassificationSource, WebsiteRequestRoutingContext, WebsiteRouteSelection,
    WebsiteRouteSelectionError, WebsiteRuntimeDescriptor, WebsiteRuntimeDescriptorError,
    WebsiteVariantSelectionReason, MAX_WEBSITE_RUNTIME_DESCRIPTOR_BYTES,
};
use serde_json::{json, Value};

fn descriptor_fixture() -> Value {
    json!({
        "schemaVersion": "sdkwork.website-runtime.v1",
        "kind": "sdkwork.website-runtime.descriptor",
        "revisionUuid": "revision-0001",
        "appUuid": "site-0001",
        "tenantScopeHash": "1".repeat(64),
        "environment": "production",
        "generatedAt": "2026-07-21T00:00:00Z",
        "compilerVersion": "deploy-descriptor-compiler/1",
        "descriptorSha256": "0".repeat(64),
        "appDefaultVariantUuid": "variant-desktop",
        "bindings": [
            {
                "bindingUuid": "binding-docs",
                "hostname": "example.com",
                "pathPrefix": "/docs",
                "action": {
                    "type": "SERVE",
                    "defaultVariantUuid": "variant-desktop"
                }
            },
            {
                "bindingUuid": "binding-root",
                "hostname": "example.com",
                "pathPrefix": "/",
                "action": {
                    "type": "SERVE"
                }
            },
            {
                "bindingUuid": "binding-wildcard",
                "hostname": "*.example.com",
                "pathPrefix": "/",
                "action": {
                    "type": "REDIRECT",
                    "statusCode": 308,
                    "scheme": "https",
                    "hostname": "example.com",
                    "pathPrefix": "/from-subdomain",
                    "preservePath": true,
                    "preserveQuery": true
                }
            }
        ],
        "variants": [
            {
                "variantUuid": "variant-desktop",
                "label": "Desktop"
            },
            {
                "variantUuid": "variant-mobile",
                "label": "Mobile"
            },
            {
                "variantUuid": "variant-tv",
                "label": "TV"
            }
        ],
        "variantRules": [
            {
                "ruleUuid": "rule-mobile-client",
                "variantUuid": "variant-mobile",
                "priority": 100,
                "match": {
                    "type": "CLIENT_CLASS",
                    "clientClass": "MOBILE"
                }
            },
            {
                "ruleUuid": "rule-mobile-path",
                "variantUuid": "variant-mobile",
                "priority": 200,
                "match": {
                    "type": "PATH_PREFIX",
                    "pathPrefix": "/m"
                }
            },
            {
                "ruleUuid": "rule-tv-client",
                "variantUuid": "variant-tv",
                "priority": 100,
                "match": {
                    "type": "CLIENT_CLASS",
                    "clientClass": "TV"
                }
            }
        ],
        "resources": [
            {
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
            },
            {
                "resourceUuid": "resource-wiki",
                "provider": {
                    "providerType": "KNOWLEDGEBASE",
                    "providerResourceUuid": "wiki-publication-0001",
                    "providerContractVersion": "knowledgebase.wiki-publication.v1"
                },
                "capabilities": {
                    "staticContent": true,
                    "wikiRoutes": true,
                    "wikiSearch": true,
                    "rangeRequests": true
                }
            }
        ],
        "mounts": [
            {
                "mountUuid": "mount-desktop-assets",
                "variantUuid": "variant-desktop",
                "pathPrefix": "/assets",
                "resourceUuid": "resource-drive",
                "handler": "STATIC",
                "translation": {
                    "mode": "ALIAS",
                    "resourceSubpath": "/public"
                },
                "indexFiles": ["index.html"]
            },
            {
                "mountUuid": "mount-desktop-root",
                "variantUuid": "variant-desktop",
                "pathPrefix": "/",
                "resourceUuid": "resource-wiki",
                "handler": "WIKI",
                "translation": {
                    "mode": "ROOT",
                    "resourceSubpath": "/"
                },
                "indexFiles": []
            },
            {
                "mountUuid": "mount-mobile-root",
                "variantUuid": "variant-mobile",
                "pathPrefix": "/",
                "resourceUuid": "resource-drive",
                "handler": "SPA",
                "translation": {
                    "mode": "ROOT",
                    "resourceSubpath": "/mobile"
                },
                "indexFiles": ["index.html"],
                "spaFallback": "/index.html"
            },
            {
                "mountUuid": "mount-tv-root",
                "variantUuid": "variant-tv",
                "pathPrefix": "/",
                "resourceUuid": "resource-drive",
                "handler": "SPA",
                "translation": {
                    "mode": "ROOT",
                    "resourceSubpath": "/tv"
                },
                "indexFiles": ["index.html"],
                "spaFallback": "/index.html"
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
            "deniedPathPrefixes": ["/.git", "/.well-known/private"]
        },
        "limits": {
            "maximumBindings": 32,
            "maximumVariants": 8,
            "maximumVariantRules": 32,
            "maximumResources": 16,
            "maximumMounts": 64,
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

fn signed_descriptor(mut value: Value) -> Vec<u8> {
    let descriptor: WebsiteRuntimeDescriptor = serde_json::from_value(value.clone()).unwrap();
    value["descriptorSha256"] =
        Value::String(website_runtime_descriptor_sha256(&descriptor).unwrap());
    serde_json::to_vec(&value).unwrap()
}

#[test]
fn compiles_hash_verified_descriptor_and_routes_without_control_plane_lookups() {
    let compiled = compile_website_runtime_descriptor(&signed_descriptor(descriptor_fixture()))
        .expect("fixture must compile");

    let selected = compiled
        .select_route(
            "EXAMPLE.COM.:443",
            "/docs/assets/app.js",
            WebsiteRequestRoutingContext::default(),
        )
        .unwrap()
        .unwrap();
    let WebsiteRouteSelection::Serve(selected) = selected else {
        panic!("expected a served route");
    };
    assert_eq!(selected.binding.binding_uuid, "binding-docs");
    assert_eq!(selected.variant.variant_uuid, "variant-desktop");
    assert_eq!(
        selected.variant_reason,
        WebsiteVariantSelectionReason::BindingDefault
    );
    assert_eq!(selected.mount.mount_uuid, "mount-desktop-assets");
    assert_eq!(selected.binding_relative_path, "/assets/app.js");
    assert_eq!(selected.provider_relative_path, "/public/app.js");
    assert_eq!(
        selected.provider.provider_resource_uuid,
        "website-root-0001"
    );
    assert_eq!(compiled.descriptor_sha256().len(), 64);
}

#[test]
fn applies_variant_precedence_and_keeps_root_translation_inside_provider_scope() {
    let compiled = compile_website_runtime_descriptor(&signed_descriptor(descriptor_fixture()))
        .expect("fixture must compile");
    let selected = compiled
        .select_route(
            "example.com",
            "/m/dashboard",
            WebsiteRequestRoutingContext {
                verified_preferred_variant_uuid: None,
                client_class: Some(WebsiteClientClass::Desktop),
                client_classification_source: Some(WebsiteClientClassificationSource::UserAgent),
            },
        )
        .unwrap()
        .unwrap();
    let WebsiteRouteSelection::Serve(selected) = selected else {
        panic!("expected a served route");
    };
    assert_eq!(selected.variant.variant_uuid, "variant-mobile");
    assert_eq!(
        selected.variant_reason,
        WebsiteVariantSelectionReason::PathRule
    );
    assert_eq!(selected.provider_relative_path, "/mobile/m/dashboard");

    let preferred = compiled
        .select_route(
            "example.com",
            "/m/dashboard",
            WebsiteRequestRoutingContext {
                verified_preferred_variant_uuid: Some("variant-desktop"),
                client_class: Some(WebsiteClientClass::Mobile),
                client_classification_source: Some(WebsiteClientClassificationSource::ClientHint),
            },
        )
        .unwrap()
        .unwrap();
    let WebsiteRouteSelection::Serve(preferred) = preferred else {
        panic!("expected a served route");
    };
    assert_eq!(preferred.variant.variant_uuid, "variant-desktop");
    assert_eq!(
        preferred.variant_reason,
        WebsiteVariantSelectionReason::Preference
    );
}

#[test]
fn selects_tv_variant_from_the_runtime_descriptor_contract() {
    let compiled = compile_website_runtime_descriptor(&signed_descriptor(descriptor_fixture()))
        .expect("fixture must compile");
    let selected = compiled
        .select_route(
            "example.com",
            "/dashboard",
            WebsiteRequestRoutingContext {
                verified_preferred_variant_uuid: None,
                client_class: Some(WebsiteClientClass::Tv),
                client_classification_source: Some(WebsiteClientClassificationSource::UserAgent),
            },
        )
        .unwrap()
        .unwrap();
    let WebsiteRouteSelection::Serve(selected) = selected else {
        panic!("expected a served TV route");
    };
    assert_eq!(selected.variant.variant_uuid, "variant-tv");
    assert_eq!(
        selected.variant_reason,
        WebsiteVariantSelectionReason::UserAgent
    );
    assert_eq!(selected.mount.mount_uuid, "mount-tv-root");
    assert_eq!(selected.provider_relative_path, "/tv/dashboard");
}

#[test]
fn exact_hosts_are_isolated_from_wildcards_and_wildcard_redirects_are_structured() {
    let compiled = compile_website_runtime_descriptor(&signed_descriptor(descriptor_fixture()))
        .expect("fixture must compile");
    let selected = compiled
        .select_route(
            "preview.example.com",
            "/guide/start",
            WebsiteRequestRoutingContext::default(),
        )
        .unwrap()
        .unwrap();
    let WebsiteRouteSelection::Redirect(selected) = selected else {
        panic!("expected a redirect");
    };
    assert_eq!(selected.binding.binding_uuid, "binding-wildcard");
    assert_eq!(selected.hostname, "example.com");
    assert_eq!(selected.path, "/from-subdomain/guide/start");
    assert_eq!(selected.status_code, 308);
    assert!(selected.preserve_query);

    assert!(compiled
        .select_route(
            "nested.preview.example.com",
            "/guide/start",
            WebsiteRequestRoutingContext::default(),
        )
        .unwrap()
        .is_none());
}

#[test]
fn segment_aware_mount_matching_does_not_treat_prefix_text_as_a_path_segment() {
    let compiled = compile_website_runtime_descriptor(&signed_descriptor(descriptor_fixture()))
        .expect("fixture must compile");
    let selected = compiled
        .select_route(
            "example.com",
            "/assetshop/logo.svg",
            WebsiteRequestRoutingContext::default(),
        )
        .unwrap()
        .unwrap();
    let WebsiteRouteSelection::Serve(selected) = selected else {
        panic!("expected a served route");
    };
    assert_eq!(selected.mount.mount_uuid, "mount-desktop-root");
    assert_eq!(
        selected.provider.provider_resource_uuid,
        "wiki-publication-0001"
    );
}

#[test]
fn rejects_hash_tampering_and_unknown_provider_topology_fields() {
    let bytes = serde_json::to_vec(&descriptor_fixture()).unwrap();
    assert!(matches!(
        compile_website_runtime_descriptor(&bytes),
        Err(WebsiteRuntimeDescriptorError::HashMismatch { .. })
    ));

    let mut with_object_key: Value =
        serde_json::from_slice(&signed_descriptor(descriptor_fixture())).unwrap();
    with_object_key["resources"][0]["provider"]["objectKey"] =
        Value::String("tenant/private/index.html".into());
    let error = compile_website_runtime_descriptor(&serde_json::to_vec(&with_object_key).unwrap())
        .expect_err("provider object keys must fail closed");
    assert!(matches!(
        error,
        WebsiteRuntimeDescriptorError::Validation { .. }
    ));
}

#[test]
fn rejects_non_canonical_collection_order_and_incoherent_provider_capabilities() {
    let mut unsorted = descriptor_fixture();
    unsorted["variants"].as_array_mut().unwrap().reverse();
    let error = compile_website_runtime_descriptor(&signed_descriptor(unsorted))
        .expect_err("non-canonical collection order must fail");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path == "/variants"));

    let mut invalid_drive = descriptor_fixture();
    invalid_drive["resources"][0]["capabilities"]["wikiRoutes"] = Value::Bool(true);
    let error = compile_website_runtime_descriptor(&signed_descriptor(invalid_drive))
        .expect_err("Drive cannot claim Wiki route capability");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path == "/resources/0/capabilities"));
}

#[test]
fn denies_reserved_prefixes_and_dotfiles_before_provider_resolution() {
    let compiled = compile_website_runtime_descriptor(&signed_descriptor(descriptor_fixture()))
        .expect("fixture must compile");
    for path in ["/.git/config", "/docs/.env", "/.well-known/private/token"] {
        assert!(
            matches!(
                compiled
                    .select_route("example.com", path, WebsiteRequestRoutingContext::default(),),
                Err(WebsiteRouteSelectionError::DeniedPath)
            ),
            "path {path} must fail closed"
        );
    }
}

#[test]
fn rejects_redirect_cycles_noncanonical_timestamps_and_oversized_payloads() {
    let mut cycle = descriptor_fixture();
    cycle["bindings"][0]["action"] = json!({
        "type": "REDIRECT",
        "statusCode": 308,
        "scheme": "https",
        "hostname": "loop.example.com",
        "pathPrefix": "/",
        "preservePath": true,
        "preserveQuery": true
    });
    cycle["bindings"][2]["action"]["pathPrefix"] = json!("/docs");
    let error = compile_website_runtime_descriptor(&signed_descriptor(cycle))
        .expect_err("redirect cycles must fail closed");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("cycle")));

    let mut timestamp = descriptor_fixture();
    timestamp["generatedAt"] = json!("2026-07-21T00:00:00.000Z");
    let error = compile_website_runtime_descriptor(&signed_descriptor(timestamp))
        .expect_err("producer timestamps must use one canonical representation");
    assert!(error
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.path == "/generatedAt"));

    let oversized = vec![b' '; MAX_WEBSITE_RUNTIME_DESCRIPTOR_BYTES + 1];
    assert!(matches!(
        compile_website_runtime_descriptor(&oversized),
        Err(WebsiteRuntimeDescriptorError::TooLarge { .. })
    ));
}
