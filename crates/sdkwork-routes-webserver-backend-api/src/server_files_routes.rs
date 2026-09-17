//! Server Files HTTP routes.
//!
//! Exposes the Server Files explorer API backed by
//! [`sdkwork_server_files_service`]. Each deployment node is represented by a
//! [`ServerFilesNode`] with an authorized filesystem root. Every request is:
//!
//! 1. Authenticated through the IAM dual-token session (same as the rest of
//!    the backend API) via `require_backend_context`.
//! 2. Path-resolved by the service's containment layer, so traversal and
//!    symlink escapes are impossible regardless of the caller's permission.
//! 3. Bounded: reads respect the service's maximum file size and directory
//!    entry caps.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Extension, Json, Router,
};
use sdkwork_routes_webserver_common::WebApiError;
use sdkwork_server_files_service::{
    classify_entry_names, command_for, ServerFilesService, ServerFilesServiceConfig,
};
use sdkwork_utils_rust::SdkWorkResultCode;
use sdkwork_webserver_contract::WebBackendRequestContext;
use serde::{Deserialize, Serialize};

use crate::{auth::require_backend_context, paths};

/// A deployment node the Server Files explorer may browse.
///
/// The wire shape is the OpenAPI authority's camelCase contract
/// (`ServerFilesNode`); the browser client reads `filesystemRoot` to seed the
/// explorer's first browse request, so a rename here silently strands the
/// explorer (see the wire-contract tests below).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerFilesNode {
    pub id: String,
    pub name: String,
    pub host: String,
    pub ssh_port: u16,
    pub status: NodeStatus,
    pub filesystem_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    Online,
    Offline,
    Unknown,
}

/// Node configuration registry backing the Server Files API.
#[derive(Clone, Debug, Default)]
pub struct ServerFilesNodeRegistry {
    nodes: Arc<HashMap<String, ServerFilesNode>>,
}

impl ServerFilesNodeRegistry {
    pub fn new(nodes: Vec<ServerFilesNode>) -> Self {
        Self {
            nodes: Arc::new(
                nodes
                    .into_iter()
                    .map(|node| (node.id.clone(), node))
                    .collect(),
            ),
        }
    }

    fn get(&self, node_id: &str) -> Option<&ServerFilesNode> {
        self.nodes.get(node_id)
    }

    fn all(&self) -> Vec<ServerFilesNode> {
        let mut nodes: Vec<ServerFilesNode> = self.nodes.values().cloned().collect();
        nodes.sort_by(|left, right| left.id.cmp(&right.id));
        nodes
    }
}

/// A node derived from the local host: always presents the deployment root.
pub fn local_deployment_node(deployment_root: &str) -> ServerFilesNode {
    ServerFilesNode {
        id: "local".to_string(),
        name: "Local Deployment".to_string(),
        host: hostname(),
        ssh_port: 22,
        status: NodeStatus::Online,
        filesystem_root: deployment_root.to_string(),
        region: Some("local".to_string()),
    }
}

fn hostname() -> String {
    std::env::var("SDKWORK_HOSTNAME").unwrap_or_else(|_| "127.0.0.1".to_string())
}

#[derive(Clone)]
struct ServerFilesState {
    registry: Arc<ServerFilesNodeRegistry>,
}

/// Build the Server Files router. `registry` carries the node inventory the
/// assembly resolves from the managed deployment nodes.
pub fn build_server_files_router(registry: ServerFilesNodeRegistry) -> Router {
    Router::new()
        .route(paths::SERVER_FILES_NODES, get(list_nodes))
        .route(
            paths::SERVER_FILES_NODE_DIRECTORY,
            get(browse_node_directory),
        )
        .route(paths::SERVER_FILES_NODE_FILE, get(read_node_file))
        .route(
            paths::SERVER_FILES_NODE_OPERATIONS,
            get(list_node_operations).post(run_node_operation),
        )
        .with_state(ServerFilesState {
            registry: Arc::new(registry),
        })
}

async fn list_nodes(
    State(state): State<ServerFilesState>,
    context: Option<Extension<WebBackendRequestContext>>,
) -> Result<Response, WebApiError> {
    require_read(context)?;
    Ok(ok_json(
        &serde_json::json!({ "items": state.registry.all() }),
    ))
}

#[derive(Debug, Deserialize)]
struct PathQuery {
    path: String,
}

async fn browse_node_directory(
    State(state): State<ServerFilesState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(node_id): Path<String>,
    Query(query): Query<PathQuery>,
) -> Result<Response, WebApiError> {
    require_read(context)?;
    let service = service_for(state, &node_id)?;
    let listing = service
        .browse_directory(&query.path)
        .await
        .map_err(browse_directory_error)?;
    Ok(ok_json(&listing))
}

async fn read_node_file(
    State(state): State<ServerFilesState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path((node_id, file_path)): Path<(String, String)>,
) -> Result<Response, WebApiError> {
    require_read(context)?;
    let service = service_for(state, &node_id)?;
    let content = service
        .read_file(&file_path)
        .await
        .map_err(read_file_error)?;
    Ok(ok_json(&content))
}

async fn list_node_operations(
    State(state): State<ServerFilesState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(node_id): Path<String>,
    Query(query): Query<PathQuery>,
) -> Result<Response, WebApiError> {
    require_read(context)?;
    let service = service_for(state, &node_id)?;
    let resolved = service
        .contained_path(&query.path)
        .map_err(containment_error)?;
    let classification = classify_entry_names(&entry_names(&resolved));
    let operations = service
        .operations_for(&query.path, &classification)
        .map_err(containment_error)?;
    Ok(ok_json(&operations))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunOperationRequest {
    path: String,
    operation_id: String,
}

async fn run_node_operation(
    State(state): State<ServerFilesState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(node_id): Path<String>,
    Json(request): Json<RunOperationRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    let service = service_for(state, &node_id)?;
    let resolved = service
        .contained_path(&request.path)
        .map_err(containment_error)?;
    let classification = classify_entry_names(&entry_names(&resolved));
    let operations = service
        .operations_for(&request.path, &classification)
        .map_err(containment_error)?;

    let operation = operations
        .operations
        .iter()
        .find(|operation| operation.id == request.operation_id)
        .ok_or_else(not_found)?;
    require_operation_permission(&context, operation)?;

    let command = command_for(classification.project_type, operation.kind).ok_or_else(not_found)?;

    let cwd = resolved.join(&command.cwd);
    let output = tokio::process::Command::new(&command.program)
        .args(&command.args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|_error| {
            WebApiError::new(SdkWorkResultCode::InternalError, "project operation failed")
        })?;

    Ok(ok_json(&serde_json::json!({
        "operationId": operation.id,
        "exitCode": output.status.code(),
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr),
    })))
}

fn service_for(state: ServerFilesState, node_id: &str) -> Result<ServerFilesService, WebApiError> {
    let node = state.registry.get(node_id).ok_or_else(not_found)?;
    ServerFilesService::new(ServerFilesServiceConfig {
        node_id: node.id.clone(),
        filesystem_root: node.filesystem_root.clone(),
        ..ServerFilesServiceConfig::default()
    })
    .map_err(|_error| {
        WebApiError::new(
            SdkWorkResultCode::ValidationError,
            "invalid node filesystem root",
        )
    })
}

fn entry_names(path: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(path)
        .map(|read_dir| {
            read_dir
                .filter_map(|result| result.ok())
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Wrap a serializable value in the canonical success envelope.
pub(crate) fn ok_json<T: Serialize>(data: &T) -> Response {
    // The OpenAPI authority declares these payloads as `data: <T>` (no
    // `{item}` resource wrapper) with a generated trace id, matching the
    // canonical SDKWork success envelope.
    let body = sdkwork_utils_rust::SdkWorkApiResponse::success(
        data,
        sdkwork_routes_webserver_common::correlation::resolved_trace_id(),
    );
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        Json(body),
    )
        .into_response()
}

fn not_found() -> WebApiError {
    WebApiError::new(SdkWorkResultCode::NotFound, "resource not found")
}

fn server_files_error(error: impl std::fmt::Display) -> WebApiError {
    WebApiError::new(SdkWorkResultCode::InternalError, error.to_string())
}

/// Map a browse failure onto the matching result code.
///
/// A containment violation or a missing directory is the caller's problem, not
/// the server's: reporting it as `INTERNAL_ERROR`/`500` (whose detail the
/// problem envelope masks) tells the operator nothing and hides a real
/// mis-supplied path. This mirrors `read_file_error` for the file route.
fn browse_directory_error(
    error: sdkwork_server_files_service::BrowseDirectoryError,
) -> WebApiError {
    use sdkwork_server_files_service::BrowseDirectoryError;

    match error {
        BrowseDirectoryError::Containment(_) => containment_error_from_browse(),
        BrowseDirectoryError::Io { kind, .. } => match kind {
            std::io::ErrorKind::NotFound => {
                WebApiError::new(SdkWorkResultCode::NotFound, "directory not found")
            }
            std::io::ErrorKind::PermissionDenied => WebApiError::new(
                SdkWorkResultCode::PermissionRequired,
                "the directory is not readable",
            ),
            // Anything else (I/O faults, an unmounted volume) stays an internal
            // error: the detail is withheld by the problem envelope.
            _ => WebApiError::new(
                SdkWorkResultCode::InternalError,
                "the directory could not be read",
            ),
        },
    }
}

fn containment_error_from_browse() -> WebApiError {
    // Do not echo the offending path back to the caller.
    WebApiError::new(
        SdkWorkResultCode::ValidationError,
        "path is outside the authorized directory",
    )
}

fn read_file_error(error: sdkwork_server_files_service::ReadFileError) -> WebApiError {
    match &error {
        sdkwork_server_files_service::ReadFileError::Sensitive => WebApiError::new(
            SdkWorkResultCode::PermissionRequired,
            "credential and secret files cannot be read through the file explorer",
        ),
        sdkwork_server_files_service::ReadFileError::Containment(containment) => {
            let _ = containment;
            // Do not echo the offending path back to the caller.
            WebApiError::new(
                SdkWorkResultCode::ValidationError,
                "path is outside the authorized directory",
            )
        }
        _ => server_files_error(error),
    }
}

fn containment_error(_error: sdkwork_server_files_service::PathContainmentError) -> WebApiError {
    // Do not echo the offending path back to the caller.
    WebApiError::new(
        SdkWorkResultCode::ValidationError,
        "path is outside the authorized directory",
    )
}

fn require_read(context: Option<Extension<WebBackendRequestContext>>) -> Result<(), WebApiError> {
    require_backend_context(context)?;
    Ok(())
}

/// Deploy-class operations escalate beyond the route's coarse
/// `web.servers.files.write` gate: they execute lifecycle commands on the node
/// and must satisfy the per-operation permission declared by the operations
/// manifest (PRD-FR-029 route-level authorization). Wildcard grants are
/// honored through the shared framework matcher; an empty grant list fails
/// closed.
fn require_operation_permission(
    context: &WebBackendRequestContext,
    operation: &sdkwork_server_files_service::ProjectOperation,
) -> Result<(), WebApiError> {
    if sdkwork_web_core::request_context::permission_scope_matches_any(
        &context.permission_scope,
        &operation.permission,
    ) {
        Ok(())
    } else {
        Err(WebApiError::new(
            SdkWorkResultCode::PermissionRequired,
            format!(
                "operation '{}' requires the {} permission",
                operation.id, operation.permission
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_server_files_service::{
        BrowseDirectoryError, PathContainmentError, ProjectOperationKind,
    };

    fn deploy_operation() -> sdkwork_server_files_service::ProjectOperation {
        sdkwork_server_files_service::ProjectOperation {
            id: "deploy".to_owned(),
            kind: ProjectOperationKind::Deploy,
            label: "Deploy".to_owned(),
            permission: "web.servers.files.deploy".to_owned(),
            description: None,
            dangerous: true,
        }
    }

    /// The browser explorer seeds its first browse request from
    /// `filesystemRoot`; if the node payload renames that field the request
    /// falls back to `"/"` and the explorer answers a masked `500` for the
    /// node's own root.
    #[test]
    fn node_payload_matches_the_openapi_camel_case_contract() {
        let node = local_deployment_node("/opt/deploy");
        let value = serde_json::to_value(&node).unwrap();

        assert_eq!(value["id"], serde_json::json!("local"));
        assert_eq!(value["sshPort"], serde_json::json!(22));
        assert_eq!(value["filesystemRoot"], serde_json::json!("/opt/deploy"));
        assert!(value.get("ssh_port").is_none(), "got {value}");
        assert!(value.get("filesystem_root").is_none(), "got {value}");
    }

    #[test]
    fn run_operation_request_accepts_the_generated_sdk_body() {
        let request: RunOperationRequest =
            serde_json::from_str(r#"{"path":"/opt/deploy/app","operationId":"build"}"#).unwrap();
        assert_eq!(request.operation_id, "build");
    }

    #[test]
    fn browse_containment_violation_is_a_caller_error_not_an_internal_error() {
        let error = browse_directory_error(BrowseDirectoryError::Containment(
            PathContainmentError::EscapesRoot,
        ));
        assert_eq!(error.code(), SdkWorkResultCode::ValidationError);
    }

    #[test]
    fn missing_directory_is_not_found() {
        let error = browse_directory_error(BrowseDirectoryError::Io {
            kind: std::io::ErrorKind::NotFound,
            detail: "the system cannot find the file specified".to_owned(),
        });
        assert_eq!(error.code(), SdkWorkResultCode::NotFound);
    }

    #[test]
    fn unreadable_directory_requires_permission() {
        let error = browse_directory_error(BrowseDirectoryError::Io {
            kind: std::io::ErrorKind::PermissionDenied,
            detail: "access is denied".to_owned(),
        });
        assert_eq!(error.code(), SdkWorkResultCode::PermissionRequired);
    }

    #[test]
    fn unexpected_io_fault_stays_internal() {
        let error = browse_directory_error(BrowseDirectoryError::Io {
            kind: std::io::ErrorKind::Other,
            detail: "the device is not ready".to_owned(),
        });
        assert_eq!(error.code(), SdkWorkResultCode::InternalError);
    }

    /// A scratch node root with one file and one classifiable project
    /// directory, so the browse test never depends on the host's real
    /// deployment tree.
    fn scratch_node_root() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "sdkwork-server-files-routes-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("sdkwork-service")).unwrap();
        std::fs::write(root.join("README.md"), "# scratch\n").unwrap();
        std::fs::write(
            root.join("sdkwork-service/Cargo.toml"),
            "[package]\nname = \"scratch\"\n",
        )
        .unwrap();
        root
    }

    fn entry<'a>(json: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
        json["data"]["entries"]
            .as_array()
            .expect("entries array")
            .iter()
            .find(|entry| entry["name"] == serde_json::json!(name))
            .unwrap_or_else(|| panic!("entry {name} missing from {json}"))
    }

    fn scratch_state(root: &std::path::Path) -> State<ServerFilesState> {
        State(ServerFilesState {
            registry: Arc::new(ServerFilesNodeRegistry::new(vec![ServerFilesNode {
                id: "local".to_owned(),
                name: "Local Deployment".to_owned(),
                host: "127.0.0.1".to_owned(),
                ssh_port: 22,
                status: NodeStatus::Online,
                filesystem_root: root.to_string_lossy().into_owned(),
                region: Some("local".to_owned()),
            }])),
        })
    }

    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .expect("bounded response body");
        serde_json::from_slice(&bytes).expect("JSON response body")
    }

    /// The node list is the explorer's seed: this asserts the field the client
    /// actually reads, end to end through the handler's envelope.
    #[tokio::test]
    async fn node_list_envelope_exposes_filesystem_root() {
        let root = scratch_node_root();
        let response = list_nodes(
            scratch_state(&root),
            Some(Extension(context_with(&["web.servers.files.read"]))),
        )
        .await
        .expect("node list succeeds");

        let json = body_json(response).await;
        assert_eq!(
            json["data"]["items"][0]["filesystemRoot"],
            serde_json::json!(root.to_string_lossy().into_owned())
        );
        assert_eq!(json["data"]["items"][0]["sshPort"], serde_json::json!(22));
    }

    /// The explorer's first browse request is `path=<filesystemRoot>`: it must
    /// answer `200` with the camelCase listing contract, including byte sizes
    /// as decimal strings.
    #[tokio::test]
    async fn browsing_the_node_root_answers_a_camel_case_listing() {
        let root = scratch_node_root();
        let response = browse_node_directory(
            scratch_state(&root),
            Some(Extension(context_with(&["web.servers.files.read"]))),
            Path("local".to_owned()),
            Query(PathQuery {
                path: root.to_string_lossy().into_owned(),
            }),
        )
        .await
        .expect("browsing the node root succeeds");

        let json = body_json(response).await;
        assert_eq!(json["data"]["nodeId"], serde_json::json!("local"));
        assert!(
            json["data"].get("parentPath").is_some(),
            "camelCase parentPath missing: {json}"
        );

        let file = entry(&json, "README.md");
        assert_eq!(file["kind"], serde_json::json!("file"));
        assert!(
            file["size"].is_string(),
            "byte size must be an int64 decimal string: {file}"
        );

        let directory = entry(&json, "sdkwork-service");
        assert_eq!(directory["kind"], serde_json::json!("directory"));
        assert_eq!(directory["projectType"], serde_json::json!("rust-backend"));
        assert_eq!(directory["isProjectRoot"], serde_json::json!(true));
        assert!(directory.get("size").is_none(), "got {directory}");
    }

    /// A path outside the node root is the caller's mistake and must surface
    /// as a diagnosable `422`, never as the masked `500` that hid this defect.
    #[tokio::test]
    async fn path_outside_the_node_root_answers_422_not_500() {
        let root = scratch_node_root();
        let error = browse_node_directory(
            scratch_state(&root),
            Some(Extension(context_with(&["web.servers.files.read"]))),
            Path("local".to_owned()),
            Query(PathQuery {
                path: "/".to_owned(),
            }),
        )
        .await
        .expect_err("the filesystem root is outside the node root");

        let response = error.into_response();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY
        );
        let json = body_json(response).await;
        assert_eq!(
            json["detail"],
            serde_json::json!("path is outside the authorized directory")
        );
    }

    fn context_with(grants: &[&str]) -> WebBackendRequestContext {
        WebBackendRequestContext {
            operator_id: Some(7),
            tenant_id: Some(42),
            subject_id: Some("7".to_owned()),
            idempotency_key: None,
            permission_scope: grants.iter().map(|grant| (*grant).to_owned()).collect(),
        }
    }

    #[test]
    fn deploy_operation_requires_deploy_permission() {
        let operation = deploy_operation();
        // The coarse file-write permission does not authorize deploy-class
        // lifecycle commands.
        let denied = require_operation_permission(
            &context_with(&["web.servers.files.write", "web.servers.files.read"]),
            &operation,
        )
        .unwrap_err();
        assert_eq!(denied.code(), SdkWorkResultCode::PermissionRequired);

        // An empty grant list fails closed.
        let anonymous = require_operation_permission(&context_with(&[]), &operation).unwrap_err();
        assert_eq!(anonymous.code(), SdkWorkResultCode::PermissionRequired);

        // The exact deploy grant authorizes the operation.
        assert!(require_operation_permission(
            &context_with(&["web.servers.files.deploy"]),
            &operation
        )
        .is_ok());

        // Domain wildcard grants authorize the operation.
        assert!(
            require_operation_permission(&context_with(&["web.servers.*"]), &operation).is_ok()
        );
    }
}
