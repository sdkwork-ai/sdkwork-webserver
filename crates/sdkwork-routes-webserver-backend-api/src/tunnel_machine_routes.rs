//! Tunnel machine (connector node) management handlers.
//!
//! Thin: authorization and the port call live here, the state lives behind
//! [`WebBackendApi`]. The machine inventory belongs to the tunnel gateway
//! process, so the service implementation reaches it over the data plane's
//! operations API rather than from this process's memory — that indirection is
//! the service's concern, not this layer's.

use axum::{
    extract::{Path, State},
    response::Response,
    Extension, Json,
};
use sdkwork_webserver_contract::{
    CreateTunnelMachineRouteRequest, SetTunnelMachineEnabledRequest, WebBackendRequestContext,
};

use crate::auth::require_backend_context;
use crate::routes::BackendState;
use sdkwork_routes_webserver_common::{created_resource, no_content, ok_resource, WebApiError};

pub(crate) async fn list_tunnel_machines(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(state.api.list_tunnel_machines(&context).await)
}

pub(crate) async fn retrieve_tunnel_machine(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(device_id): Path<String>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .retrieve_tunnel_machine(&context, &device_id)
            .await,
    )
}

pub(crate) async fn set_tunnel_machine_enabled(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(device_id): Path<String>,
    Json(request): Json<SetTunnelMachineEnabledRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .set_tunnel_machine_enabled(&context, &device_id, &request)
            .await,
    )
}

pub(crate) async fn create_tunnel_machine_route(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path(device_id): Path<String>,
    Json(request): Json<CreateTunnelMachineRouteRequest>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    created_resource(
        state
            .api
            .create_tunnel_machine_route(&context, &device_id, &request)
            .await,
    )
}

pub(crate) async fn delete_tunnel_machine_route(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Path((device_id, route_id)): Path<(String, String)>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    no_content(
        state
            .api
            .delete_tunnel_machine_route(&context, &device_id, &route_id)
            .await,
    )
}
