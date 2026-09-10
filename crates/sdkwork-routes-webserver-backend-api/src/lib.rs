//! Backend API route boundary for SDKWork Web Server.

pub mod agent_routes;
pub mod auth;
pub mod http_route_manifest;
pub mod paths;
pub mod routes;
pub mod server_files_routes;
pub mod web_bootstrap;
pub mod webserver_config_routes;

pub use http_route_manifest::backend_route_manifest;
pub use routes::{
    build_agent_router_with_shared_backend_api, build_router_with_backend_api,
    build_router_with_shared_backend_api,
};
pub use sdkwork_server_files_service::{
    ensure_space_repository, SpaceCloneError, SDKWORK_SPACE_DIRECTORY, SDKWORK_SPACE_REPOSITORY,
};
pub use sdkwork_webserver_contract::{WebBackendApi, WebBackendRequestContext};
pub use server_files_routes::{
    build_server_files_router, local_deployment_node, NodeStatus, ServerFilesNode,
    ServerFilesNodeRegistry,
};
pub use web_bootstrap::{
    domain_context_injectors as web_backend_domain_context_injectors,
    wrap_agent_router_with_web_framework_from_env, wrap_router_with_web_framework_from_env,
    wrap_router_with_web_framework_from_env_and_metrics,
};
pub use webserver_config_routes::{
    build_webserver_config_router, default_webserver_config_root, webserver_config_service_from_env,
};

use sdkwork_web_core::HttpRouteManifest;
use std::sync::Arc;

pub fn gateway_route_manifest() -> HttpRouteManifest {
    backend_route_manifest()
}

pub fn gateway_mount(api: Arc<dyn WebBackendApi>) -> axum::Router {
    gateway_mount_with_server_files(api, None)
}

/// Compose the backend router with the Server Files explorer API and the
/// Web Server configuration management API.
///
/// `server_files_nodes` is an optional explicit node inventory. When omitted,
/// a single local deployment node is derived from `SDKWORK_DEPLOY_ROOT`
/// (default `/opt/deploy`), so the explorer is immediately browsable. The
/// configuration management router resolves its roots from
/// `SDKWORK_WEBSERVER_CONFIG_ROOT` (default `/etc/sdkwork/webserver`) and
/// `SDKWORK_DEPLOY_ROOT` (enables the read-only sibling-module sidecar
/// group).
pub fn gateway_mount_with_server_files(
    api: Arc<dyn WebBackendApi>,
    server_files_nodes: Option<Vec<ServerFilesNode>>,
) -> axum::Router {
    let deployment_root =
        std::env::var("SDKWORK_DEPLOY_ROOT").unwrap_or_else(|_| "/opt/deploy".to_string());
    let registry = ServerFilesNodeRegistry::new(
        server_files_nodes.unwrap_or_else(|| vec![local_deployment_node(&deployment_root)]),
    );
    let webserver_config_router = match webserver_config_service_from_env() {
        Ok(service) => build_webserver_config_router(service),
        Err(_) => axum::Router::new(),
    };
    build_router_with_shared_backend_api(api)
        .merge(build_server_files_router(registry))
        .merge(webserver_config_router)
}

/// Agent-only router for machine-only composition on the standalone gateway.
pub fn agent_gateway_mount(api: Arc<dyn WebBackendApi>) -> axum::Router {
    build_agent_router_with_shared_backend_api(api)
}
