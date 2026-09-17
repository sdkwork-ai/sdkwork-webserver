//! Server Files wire-contract regression tests.
//!
//! The Server Files surface is consumed by the generated SDKs and the admin
//! browser client, all of which are generated from the OpenAPI authority
//! (`apis/backend-api/web/sdkwork-webserver-backend-api.openapi.json`). That
//! authority declares **camelCase** properties and the int64-as-string closure
//! (`API_SPEC.md` §13.6); serde does not translate field names on its own, so
//! every DTO must declare the casing explicitly.
//!
//! When it did not, `GET .../nodes` answered `{"ssh_port":…,"filesystem_root":…}`
//! while the client read `filesystemRoot`. The explorer fell back to its `"/"`
//! seed path, containment refused it, and the route answered a masked `500`
//! (`errors.result.50001`) for the node's own root.

use sdkwork_server_files_service::{
    classify_entry_names, operations_for, resolve_contained_path, validate_allowed_root,
    DirectoryListing, EntryKind, FileContent, ServerEntry,
};

#[test]
fn directory_listing_uses_the_openapi_camel_case_contract() {
    let listing = DirectoryListing {
        node_id: "local".to_owned(),
        path: "/opt/deploy".to_owned(),
        parent_path: Some("/opt".to_owned()),
        entries: Vec::new(),
    };
    let value = serde_json::to_value(&listing).unwrap();

    assert!(value.get("nodeId").is_some(), "got {value}");
    assert!(value.get("parentPath").is_some(), "got {value}");
    assert!(value.get("node_id").is_none(), "got {value}");
    assert!(value.get("parent_path").is_none(), "got {value}");
}

#[test]
fn server_entry_uses_the_openapi_camel_case_contract() {
    let entry = ServerEntry {
        name: "Cargo.toml".to_owned(),
        kind: EntryKind::File,
        path: "/opt/deploy/Cargo.toml".to_owned(),
        size: Some(2048),
        project_type: Some(sdkwork_server_files_service::ProjectType::RustBackend),
        is_project_root: false,
    };
    let value = serde_json::to_value(&entry).unwrap();

    assert_eq!(value["kind"], serde_json::json!("file"));
    assert_eq!(value["projectType"], serde_json::json!("rust-backend"));
    assert_eq!(value["isProjectRoot"], serde_json::json!(false));
    assert!(value.get("project_type").is_none(), "got {value}");
    assert!(value.get("is_project_root").is_none(), "got {value}");
}

#[test]
fn file_content_uses_the_openapi_camel_case_contract() {
    let content = FileContent {
        node_id: "local".to_owned(),
        path: "/opt/deploy/README.md".to_owned(),
        content: "# hi".to_owned(),
        size: 4,
    };
    let value = serde_json::to_value(&content).unwrap();

    assert_eq!(value["nodeId"], serde_json::json!("local"));
    assert!(value.get("node_id").is_none(), "got {value}");
}

/// `size` is declared `type: string, format: int64, x-sdkwork-int64-string: true`
/// (`API_SPEC.md` §13.6): the JSON boundary must carry decimal strings so a
/// generated TypeScript client never loses precision on byte counters.
#[test]
fn byte_sizes_serialize_as_decimal_strings() {
    let entry = ServerEntry {
        name: "archive.tar".to_owned(),
        kind: EntryKind::File,
        path: "/opt/deploy/archive.tar".to_owned(),
        size: Some(1_234_567_890_123),
        project_type: None,
        is_project_root: false,
    };
    let value = serde_json::to_value(&entry).unwrap();
    assert_eq!(value["size"], serde_json::json!("1234567890123"));

    let content = FileContent {
        node_id: "local".to_owned(),
        path: "/opt/deploy/README.md".to_owned(),
        content: "hi".to_owned(),
        size: 42,
    };
    let value = serde_json::to_value(&content).unwrap();
    assert_eq!(value["size"], serde_json::json!("42"));

    // Directories carry no size at all.
    let directory = ServerEntry {
        name: "sdkwork-space".to_owned(),
        kind: EntryKind::Directory,
        path: "/opt/deploy/sdkwork-space".to_owned(),
        size: None,
        project_type: Some(sdkwork_server_files_service::ProjectType::SdkworkWorkspace),
        is_project_root: true,
    };
    let value = serde_json::to_value(&directory).unwrap();
    assert!(value.get("size").is_none(), "got {value}");
}

#[test]
fn project_operations_use_the_openapi_camel_case_contract() {
    let classification = classify_entry_names(&["Cargo.toml".to_owned()]);
    let manifest = operations_for("local", "/opt/deploy/server", &classification);
    let value = serde_json::to_value(&manifest).unwrap();

    assert_eq!(value["nodeId"], serde_json::json!("local"));
    assert_eq!(value["projectType"], serde_json::json!("rust-backend"));
    assert!(value.get("node_id").is_none(), "got {value}");
    assert!(value.get("project_type").is_none(), "got {value}");
    assert_eq!(value["operations"][0]["kind"], serde_json::json!("build"));
}

/// The string the node advertises as `filesystemRoot` is echoed straight back
/// as the `path` query parameter, so it must round-trip through containment.
/// Skips when the host has no deployment root yet, where it is not observable.
#[test]
fn advertised_deployment_root_resolves_to_the_service_root() {
    let advertised =
        std::env::var("SDKWORK_DEPLOY_ROOT").unwrap_or_else(|_| "/opt/deploy".to_string());
    let Ok(root) = validate_allowed_root(&advertised) else {
        eprintln!("skipped: deployment root {advertised:?} is not present on this host");
        return;
    };

    let resolved = resolve_contained_path(&root, &advertised).unwrap_or_else(|error| {
        panic!("advertised root {advertised:?} must stay inside the authorized root: {error}")
    });
    assert_eq!(
        resolved, root,
        "advertised root {advertised:?} resolved to {resolved:?}, not the node root {root:?}"
    );
    assert!(
        resolved.is_dir(),
        "the resolved node root {resolved:?} must be browsable"
    );
}

/// The explorer's `"/"` fallback seed is outside every node root: containment
/// must refuse it so the caller gets a `422` naming the path problem, instead
/// of silently browsing the drive root.
#[test]
fn filesystem_root_seed_is_rejected_as_outside_the_node_root() {
    let advertised =
        std::env::var("SDKWORK_DEPLOY_ROOT").unwrap_or_else(|_| "/opt/deploy".to_string());
    let Ok(root) = validate_allowed_root(&advertised) else {
        eprintln!("skipped: deployment root {advertised:?} is not present on this host");
        return;
    };

    assert_eq!(
        resolve_contained_path(&root, "/").unwrap_err(),
        sdkwork_server_files_service::PathContainmentError::EscapesRoot
    );
}
