//! Cross-repository contract test: sdkwork-deployments compiles the descriptor,
//! sdkwork-webserver serves it.
//!
//! The app-domain fallback only works if the two repositories agree on three
//! things that are declared in **both** of them:
//!
//! 1. the provider contract version each resource carries
//!    (`drive.website-root.v1`, `knowledgebase.wiki-publication.v1`) — the Web
//!    Server adapters reject anything else with `CONTRACT_MISMATCH`, and the
//!    Deploy compiler is the producer;
//! 2. the five lifecycle environments (`development`, `test`, `staging`,
//!    `demo`, `production`) — the platform labels `<appLabel>.app[-<env>].<suffix>`
//!    and the lookup environment are keyed by them;
//! 3. the descriptor/runtime-set schema — the Deploy compiler's
//!    `compile_app_revision` output must be accepted verbatim by the
//!    `compile_website_runtime_set_snapshot` path the fallback uses.
//!
//! The Deploy repository already asserts (3) for a single descriptor. This test
//! adds the Web Server side of (1) and re-asserts (2) and (3) through the
//! runtime-*set* path, with both provider types present, so neither repository
//! can change its half of the contract without turning this red.

use sdkwork_deploy_runtime_compiler::{
    compile_app_revision, AppRuntimeCompilationInput, RuntimeBinding, RuntimeBindingAction,
    RuntimeDeliveryPolicy, RuntimeEnvironment, RuntimeHandler, RuntimeLimits, RuntimeMount,
    RuntimeMountMode, RuntimeMountTranslation, RuntimeObservabilityPolicy,
    RuntimeProviderReference, RuntimeProviderType, RuntimeResource, RuntimeResourceCapabilities,
    RuntimeSecurityPolicy, RuntimeVariant,
    DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION as DEPLOY_DRIVE,
    KNOWLEDGEBASE_WIKI_PUBLICATION_PROVIDER_CONTRACT_VERSION as DEPLOY_WIKI,
};
use sdkwork_webserver_core::website_runtime::{
    compile_website_runtime_set_snapshot, website_runtime_set_snapshot_sha256, WebsiteProviderType,
    WebsiteRequestRoutingContext, WebsiteRouteSelection, WebsiteRuntimeSetSnapshot,
    WEBSITE_RUNTIME_SET_KIND, WEBSITE_RUNTIME_SET_SCHEMA_VERSION,
};
use sdkwork_webserver_drive_provider::DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION as SERVER_DRIVE;
use sdkwork_webserver_knowledgebase_provider::KNOWLEDGEBASE_WIKI_PROVIDER_CONTRACT_VERSION as SERVER_WIKI;

const HOSTNAME: &str = "myapp.app.sdkwork.com";

/// The five lifecycle environments the platform provisions and serves.
const LIFECYCLE_ENVIRONMENTS: [&str; 5] = ["development", "test", "staging", "demo", "production"];

#[test]
fn provider_contract_versions_agree_across_repositories() {
    assert_eq!(
        DEPLOY_DRIVE, SERVER_DRIVE,
        "the Deploy compiler and the Web Server drive adapter must share one contract version"
    );
    assert_eq!(
        DEPLOY_WIKI, SERVER_WIKI,
        "the Deploy compiler and the Web Server knowledgebase adapter must share one contract version"
    );
    // And each side's canonical version is the one it would emit/accept for its
    // own provider type.
    assert_eq!(
        sdkwork_deploy_runtime_compiler::canonical_provider_contract_version(
            RuntimeProviderType::Drive
        ),
        SERVER_DRIVE
    );
    assert_eq!(
        sdkwork_deploy_runtime_compiler::canonical_provider_contract_version(
            RuntimeProviderType::Knowledgebase
        ),
        SERVER_WIKI
    );
}

#[test]
fn both_repositories_agree_on_the_lifecycle_environments() {
    for environment in LIFECYCLE_ENVIRONMENTS {
        assert!(
            RuntimeEnvironment::parse(environment).is_ok(),
            "the Deploy compiler must accept the {environment} lifecycle environment"
        );
        assert!(
            sdkwork_webserver_core::website_runtime::WebsiteRuntimeEnvironment::parse(environment)
                .is_ok(),
            "the Web Server must accept the {environment} lifecycle environment"
        );
    }
    // The app-domain labels are the control plane's contract; the edge must be
    // able to map every one of them back to an environment.
    for label in ["app", "app-dev", "app-test", "app-staging", "app-demo"] {
        assert!(
            sdkwork_deploy_core::environment_for_app_domain_label(label).is_some(),
            "app domain label {label} must resolve to a lifecycle environment"
        );
    }
    assert!(sdkwork_deploy_core::environment_for_app_domain_label("app-prod").is_none());
}

#[test]
fn deploy_compiled_descriptor_is_served_by_the_fallback_runtime_set() {
    let compiled = compile_app_revision(production_input()).expect("Deploy compiler output");

    // (1) Every resource carries the version the Web Server adapter requires.
    let resources = compiled.descriptor["resources"]
        .as_array()
        .expect("descriptor resources");
    assert_eq!(resources.len(), 2, "test fixture declares drive + wiki");
    for resource in resources {
        let provider_type = resource["provider"]["providerType"]
            .as_str()
            .expect("provider type");
        let expected = match provider_type {
            "DRIVE" => SERVER_DRIVE,
            "KNOWLEDGEBASE" => SERVER_WIKI,
            other => panic!("unexpected provider type in a Deploy descriptor: {other}"),
        };
        assert_eq!(
            resource["provider"]["providerContractVersion"].as_str(),
            Some(expected),
            "resource {provider_type} must carry the contract version the edge requires"
        );
    }

    // (3) The fallback's own path: single-site snapshot → compile → route.
    let snapshot = single_site_snapshot(compiled.descriptor);
    let bytes = serde_json::to_vec(&snapshot).expect("serialize snapshot");
    let set = compile_website_runtime_set_snapshot(&bytes)
        .expect("the fallback runtime-set compiler must accept a Deploy descriptor");
    assert_eq!(set.site_count(), 1);
    assert!(
        set.uses_provider_type(WebsiteProviderType::Drive)
            && set.uses_provider_type(WebsiteProviderType::Knowledgebase),
        "both provider types must survive the runtime-set compile"
    );

    let selection = set
        .select_route(
            HOSTNAME,
            "/index.html",
            WebsiteRequestRoutingContext {
                verified_preferred_variant_uuid: None,
                client_class: None,
                client_classification_source: None,
            },
        )
        .expect("route selection")
        .expect("the binding the control plane created must be routable on the edge");
    let WebsiteRouteSelection::Serve(route) = selection else {
        panic!("the control plane's SERVE binding must select a route, not a redirect");
    };
    assert_eq!(route.app_uuid, "app-contract-1");
    assert_eq!(route.normalized_request_hostname, HOSTNAME);
    assert_eq!(route.binding.hostname, HOSTNAME);
    assert_eq!(route.normalized_request_path, "/index.html");
    assert_eq!(
        route.provider.provider_type,
        WebsiteProviderType::Drive,
        "the root mount serves the Drive website root the descriptor declares"
    );
}

/// Assemble the same single-site snapshot `DeployFallbackResolver::compile_site`
/// builds for a freshly resolved host: the Deploy descriptor with its digest
/// filled in, wrapped in a one-site runtime set.
fn single_site_snapshot(mut descriptor: serde_json::Value) -> serde_json::Value {
    let digest = sdkwork_webserver_core::website_runtime::website_runtime_descriptor_sha256(
        &serde_json::from_value(descriptor.clone()).expect("descriptor parses"),
    )
    .expect("descriptor digest");
    descriptor["descriptorSha256"] = serde_json::Value::String(digest);
    let mut snapshot = serde_json::json!({
        "schemaVersion": WEBSITE_RUNTIME_SET_SCHEMA_VERSION,
        "kind": WEBSITE_RUNTIME_SET_KIND,
        "snapshotUuid": "app-domain-fallback-contract-1",
        "nodeUuid": "node-contract-1",
        "environment": "production",
        "generation": 1,
        "generatedAt": "2026-09-15T00:00:00Z",
        "compilerVersion": "sdkwork-webserver-app-domain-fallback/1",
        "snapshotSha256": "0".repeat(64),
        "maximumSites": 1,
        "descriptors": [descriptor]
    });
    let parsed: WebsiteRuntimeSetSnapshot =
        serde_json::from_value(snapshot.clone()).expect("snapshot parses");
    snapshot["snapshotSha256"] = serde_json::Value::String(
        website_runtime_set_snapshot_sha256(&parsed).expect("snapshot digest"),
    );
    snapshot
}

fn production_input() -> AppRuntimeCompilationInput {
    AppRuntimeCompilationInput {
        revision_uuid: "revision-contract-1".to_owned(),
        app_uuid: "app-contract-1".to_owned(),
        tenant_scope_hash: "1".repeat(64),
        environment: RuntimeEnvironment::Production,
        generated_at: "2026-09-15T00:00:00Z".to_owned(),
        app_default_variant_uuid: "variant-default".to_owned(),
        bindings: vec![RuntimeBinding {
            binding_uuid: "binding-contract-1".to_owned(),
            hostname: HOSTNAME.to_owned(),
            path_prefix: "/".to_owned(),
            action: RuntimeBindingAction::serve(None, None),
        }],
        variants: vec![RuntimeVariant {
            variant_uuid: "variant-default".to_owned(),
            label: "Default".to_owned(),
        }],
        variant_rules: vec![],
        resources: vec![
            RuntimeResource {
                resource_uuid: "resource-drive".to_owned(),
                provider: RuntimeProviderReference {
                    provider_type: RuntimeProviderType::Drive,
                    provider_resource_uuid: "website-root-1".to_owned(),
                    provider_contract_version: DEPLOY_DRIVE.to_owned(),
                },
                capabilities: RuntimeResourceCapabilities {
                    static_content: true,
                    wiki_routes: false,
                    wiki_search: false,
                    range_requests: true,
                },
            },
            RuntimeResource {
                resource_uuid: "resource-wiki".to_owned(),
                provider: RuntimeProviderReference {
                    provider_type: RuntimeProviderType::Knowledgebase,
                    provider_resource_uuid: "wiki-1".to_owned(),
                    provider_contract_version: DEPLOY_WIKI.to_owned(),
                },
                capabilities: RuntimeResourceCapabilities {
                    static_content: false,
                    wiki_routes: true,
                    wiki_search: true,
                    range_requests: false,
                },
            },
        ],
        mounts: vec![
            RuntimeMount {
                mount_uuid: "mount-root".to_owned(),
                variant_uuid: "variant-default".to_owned(),
                path_prefix: "/".to_owned(),
                resource_uuid: "resource-drive".to_owned(),
                handler: RuntimeHandler::Spa,
                translation: RuntimeMountTranslation {
                    mode: RuntimeMountMode::Root,
                    resource_subpath: "/".to_owned(),
                },
                index_files: vec!["index.html".to_owned()],
                spa_fallback: Some("/index.html".to_owned()),
            },
            RuntimeMount {
                mount_uuid: "mount-wiki".to_owned(),
                variant_uuid: "variant-default".to_owned(),
                path_prefix: "/docs".to_owned(),
                resource_uuid: "resource-wiki".to_owned(),
                handler: RuntimeHandler::Wiki,
                translation: RuntimeMountTranslation {
                    mode: RuntimeMountMode::Alias,
                    resource_subpath: "/docs".to_owned(),
                },
                index_files: vec![],
                spa_fallback: None,
            },
        ],
        delivery_policy: RuntimeDeliveryPolicy::default(),
        security_policy: RuntimeSecurityPolicy::default(),
        limits: RuntimeLimits::default(),
        observability_policy: RuntimeObservabilityPolicy::default(),
    }
}
