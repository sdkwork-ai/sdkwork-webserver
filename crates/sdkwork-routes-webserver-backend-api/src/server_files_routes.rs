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
#[derive(Debug, Clone, Serialize)]
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
        .map_err(server_files_error)?;
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
        "operation_id": operation.id,
        "exit_code": output.status.code(),
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
    use sdkwork_server_files_service::ProjectOperationKind;

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
