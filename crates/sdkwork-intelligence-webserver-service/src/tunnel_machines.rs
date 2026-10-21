//! Tunnel machine management through the data plane's operations API.
//!
//! The machine inventory lives in the tunnel gateway process (the data plane);
//! the management API is a different process. This module is the one place
//! that crosses that boundary: it forwards each operation to the data plane's
//! authenticated operations listener and maps the answer back into the service
//! error model.
//!
//! Why a proxy and not a database: a machine's liveness, routes and traffic are
//! *runtime* facts that only the process holding the sessions can answer. The
//! alternative — periodic reporting into a table — is a later, additive step
//! (it buys history and multi-instance fan-out); it is deliberately not
//! simulated here, because a cached machine list that lags reality is worse
//! than an honest "the data plane is unreachable".
//!
//! Configuration:
//! - `SDKWORK_WEBSERVER_TUNNEL_OPERATIONS_URL` — base URL of the data plane's
//!   operations listener (for example `http://127.0.0.1:9100`). Unset means the
//!   feature is not assembled in this deployment and every operation fails with
//!   `Unavailable`, never with an empty machine list.
//! - `SDKWORK_WEBSERVER_OPERATIONS_AUTH_TOKEN_FILE` — the same bearer token
//!   file the data plane reads for its own listener, so one host configures the
//!   credential once.

use std::path::PathBuf;
use std::time::Duration;

use sdkwork_webserver_contract::{
    CreateTunnelMachineRouteRequest, SetTunnelMachineEnabledRequest, TunnelMachineListResponse,
    TunnelMachineResponse, TunnelMachineRouteDeclarationResponse, WebServiceError,
    WebServiceResult,
};

/// Base-URL environment variable for the data plane operations listener.
pub const TUNNEL_OPERATIONS_URL_ENV: &str = "SDKWORK_WEBSERVER_TUNNEL_OPERATIONS_URL";
/// Bearer-token file the data plane's own listener is configured with.
pub const OPERATIONS_AUTH_TOKEN_FILE_ENV: &str = "SDKWORK_WEBSERVER_OPERATIONS_AUTH_TOKEN_FILE";

/// How long one proxied call may take.
///
/// The data plane answers from memory, so a slow answer means the process is
/// wedged, not busy: the operator gets a clear failure instead of a hung page.
const PROXY_TIMEOUT: Duration = Duration::from_secs(10);

/// One JSON answer from the data plane.
struct ProxyResponse {
    status: u16,
    body: serde_json::Value,
}

impl ProxyResponse {
    fn detail(&self) -> String {
        self.body
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("the data plane refused the request")
            .to_owned()
    }
}

/// The base URL, when this deployment assembled the tunnel data plane.
fn operations_base() -> Option<String> {
    std::env::var(TUNNEL_OPERATIONS_URL_ENV)
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_owned())
        .filter(|value| !value.is_empty())
}

/// The bearer token the data plane expects, read from the shared token file.
fn operations_token() -> WebServiceResult<String> {
    let path = std::env::var(OPERATIONS_AUTH_TOKEN_FILE_ENV)
        .map_err(|_| {
            WebServiceError::unavailable(format!(
                "{OPERATIONS_AUTH_TOKEN_FILE_ENV} is not configured, so the operations listener cannot be called"
            ))
        })?
        .trim()
        .to_owned();
    if path.is_empty() {
        return Err(WebServiceError::unavailable(format!(
            "{OPERATIONS_AUTH_TOKEN_FILE_ENV} is empty"
        )));
    }
    let token = std::fs::read_to_string(PathBuf::from(&path)).map_err(|error| {
        WebServiceError::unavailable(format!(
            "{OPERATIONS_AUTH_TOKEN_FILE_ENV} (`{path}`) could not be read: {error}"
        ))
    })?;
    let token = token.trim().to_owned();
    if token.is_empty() {
        return Err(WebServiceError::unavailable(format!(
            "{OPERATIONS_AUTH_TOKEN_FILE_ENV} (`{path}`) is empty"
        )));
    }
    Ok(token)
}

/// Sends one request to the data plane's operations listener.
async fn call(
    method: &str,
    path: &str,
    body: Option<serde_json::Value>,
) -> WebServiceResult<ProxyResponse> {
    let Some(base) = operations_base() else {
        return Err(WebServiceError::unavailable(format!(
            "{TUNNEL_OPERATIONS_URL_ENV} is not configured, so tunnel machines cannot be managed from here"
        )));
    };
    let token = operations_token()?;
    let base = base
        .strip_prefix("http://")
        .ok_or_else(|| {
            WebServiceError::unavailable(format!(
                "{TUNNEL_OPERATIONS_URL_ENV} must be an http:// URL on the local host"
            ))
        })?
        .to_owned();
    let payload = body.map(|value| value.to_string()).unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {base}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let exchange = async {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut socket = tokio::net::TcpStream::connect(&base).await?;
        socket.write_all(request.as_bytes()).await?;
        socket.flush().await?;
        let mut response = Vec::new();
        socket.read_to_end(&mut response).await?;
        Ok::<Vec<u8>, std::io::Error>(response)
    };
    let raw = tokio::time::timeout(PROXY_TIMEOUT, exchange)
        .await
        .map_err(|_| {
            WebServiceError::unavailable(format!(
                "the tunnel data plane at {base} did not answer within {}s",
                PROXY_TIMEOUT.as_secs()
            ))
        })?
        .map_err(|error| {
            WebServiceError::unavailable(format!(
                "the tunnel data plane at {base} is unreachable: {error}"
            ))
        })?;

    let text = String::from_utf8_lossy(&raw).to_string();
    let mut sections = text.splitn(2, "\r\n\r\n");
    let head = sections.next().unwrap_or_default();
    let body_text = sections.next().unwrap_or_default();
    let status: u16 = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| {
            WebServiceError::Internal(
                "the tunnel data plane answered with an unusable status line".to_owned(),
            )
        })?;
    // The operations listener answers with `Connection: close` and a
    // Content-Length body; a missing body is legitimate for 204.
    let body = if body_text.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(body_text).map_err(|error| {
            WebServiceError::Internal(format!(
                "the tunnel data plane answered with a body that is not JSON: {error}"
            ))
        })?
    };
    Ok(ProxyResponse { status, body })
}

/// Maps one proxied answer onto the service error model.
fn expect_ok(response: ProxyResponse) -> WebServiceResult<serde_json::Value> {
    match response.status {
        200..=204 => Ok(response.body),
        400 => Err(WebServiceError::Validation(response.detail())),
        401 | 403 => Err(WebServiceError::Forbidden),
        404 => Err(WebServiceError::not_found(response.detail())),
        409 => Err(WebServiceError::conflict(response.detail())),
        503 => Err(WebServiceError::unavailable(response.detail())),
        _ => Err(WebServiceError::Internal(format!(
            "the tunnel data plane answered {}: {}",
            response.status,
            response.detail()
        ))),
    }
}

/// JSON → contract, refusing a shape the console cannot render.
fn machine_from_json(value: &serde_json::Value) -> WebServiceResult<TunnelMachineResponse> {
    serde_json::from_value(value.clone()).map_err(|error| {
        WebServiceError::Internal(format!(
            "machine payload does not match the contract: {error}"
        ))
    })
}

/// Lists managed machines.
pub(crate) async fn list_machines() -> WebServiceResult<TunnelMachineListResponse> {
    let body = expect_ok(call("GET", "/tunnel/machines", None).await?)?;
    let machines = body
        .get("machines")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    let machines: Vec<TunnelMachineResponse> =
        serde_json::from_value(machines).map_err(|error| {
            WebServiceError::Internal(format!("machine list does not match the contract: {error}"))
        })?;
    Ok(TunnelMachineListResponse {
        machines,
        page_info: None,
    })
}

/// Offset page over the gateway's machine list.
///
/// The gateway returns the complete inventory (bounded by the tunnel
/// configuration's `maxDevices`), so slicing here is a presentation concern,
/// not a database scan. `pageInfo` carries the exact total so the console no
/// longer fabricates `hasMore: false`.
pub(crate) async fn list_machines_page(
    page: i32,
    page_size: i32,
) -> WebServiceResult<TunnelMachineListResponse> {
    let page = page.max(1);
    let page_size = page_size.clamp(1, 200);
    let response = list_machines().await?;
    let total = response.machines.len();
    let total_pages = total.div_ceil(usize::try_from(page_size).unwrap_or(1));
    let skip = usize::try_from(page - 1)
        .unwrap_or(usize::MAX)
        .saturating_mul(usize::try_from(page_size).unwrap_or(usize::MAX));
    let machines = response
        .machines
        .into_iter()
        .skip(skip)
        .take(usize::try_from(page_size).unwrap_or(usize::MAX))
        .collect::<Vec<_>>();
    Ok(TunnelMachineListResponse {
        machines,
        page_info: Some(sdkwork_utils_rust::PageInfo {
            mode: sdkwork_utils_rust::PageMode::Offset,
            page: Some(page),
            page_size: Some(page_size),
            total_items: Some(total.to_string()),
            total_pages: i32::try_from(total_pages).ok(),
            next_cursor: None,
            has_more: Some(usize::try_from(page).unwrap_or(usize::MAX) < total_pages),
        }),
    })
}

/// One machine's detail.
pub(crate) async fn retrieve_machine(device_id: &str) -> WebServiceResult<TunnelMachineResponse> {
    let body = expect_ok(call("GET", &format!("/tunnel/machines/{device_id}"), None).await?)?;
    machine_from_json(&body)
}

/// Switches a machine on or off.
pub(crate) async fn set_machine_enabled(
    device_id: &str,
    request: &SetTunnelMachineEnabledRequest,
) -> WebServiceResult<TunnelMachineResponse> {
    let body = expect_ok(
        call(
            "PUT",
            &format!("/tunnel/machines/{device_id}/enabled"),
            Some(serde_json::json!({ "enabled": request.enabled })),
        )
        .await?,
    )?;
    machine_from_json(&body)
}

/// Declares a route for one machine.
pub(crate) async fn create_machine_route(
    device_id: &str,
    request: &CreateTunnelMachineRouteRequest,
) -> WebServiceResult<TunnelMachineRouteDeclarationResponse> {
    let response = expect_ok(
        call(
            "POST",
            "/tunnel/routes",
            Some(serde_json::json!({
                "deviceId": device_id,
                "name": request.name,
                "protocol": request.protocol,
                "domain": request.domain,
                "port": request.port,
                "target": "",
                "allowPublic": request.allow_public,
                "allowedIps": request.allowed_ips,
                "auth": request.auth,
                "visitorTokens": request.visitor_tokens,
                "locations": request.locations,
                "bandwidthLimitBytesPerSecond": request.bandwidth_limit_bytes_per_second,
                "proxyProtocol": request.proxy_protocol,
                "loadBalancerGroup": request.load_balancer_group,
            })),
        )
        .await?,
    )?;
    // The data plane answers with the registration outcome; an inactive
    // declaration is a normal answer (queued for an offline machine, or a
    // template the machine does not recognize), so it is reported, not raised.
    let route_id = response
        .get("route_id")
        .or_else(|| response.get("routeId"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let active = response
        .get("active")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let public_url = response
        .get("public_url")
        .or_else(|| response.get("publicUrl"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let detail = response
        .get("error")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Ok(TunnelMachineRouteDeclarationResponse {
        route_id,
        active,
        public_url,
        detail,
    })
}

/// Removes one route.
pub(crate) async fn delete_machine_route(route_id: &str) -> WebServiceResult<()> {
    expect_ok(call("DELETE", &format!("/tunnel/routes/{route_id}"), None).await?)?;
    Ok(())
}
