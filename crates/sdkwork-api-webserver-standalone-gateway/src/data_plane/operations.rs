use std::{io, net::SocketAddr, sync::Arc, time::Duration};

use zeroize::Zeroizing;

use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use hyper_util::{
    rt::{TokioExecutor, TokioIo, TokioTimer},
    server::conn::auto::Builder,
    service::TowerToHyperService,
};
use sdkwork_web_bootstrap::{service_router, ServiceRouterConfig};
use sdkwork_webserver_delivery_runtime::{AppConfigResourceExecutor, WebsiteDeliveryExecutor};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{watch, Semaphore},
    task::JoinSet,
    time::timeout,
};
use tower_http::timeout::TimeoutLayer;

use sdkwork_webserver_tunnel::service::TunnelService;

use crate::metric_dimensions::CanonicalMetricDimensions;

use super::{runtime::DataPlaneRuntime, DataPlaneError};

const OPERATIONS_BIND_ENV: &str = "SDKWORK_WEBSERVER_DATA_PLANE_OPERATIONS_BIND";
const OPERATIONS_AUTH_TOKEN_FILE_ENV: &str = "SDKWORK_WEBSERVER_OPERATIONS_AUTH_TOKEN_FILE";
const OPERATIONS_EXPOSE_ALLOWED_ENV: &str = "SDKWORK_WEBSERVER_OPERATIONS_EXPOSE_ALLOWED";
const MAX_OPERATIONS_CONNECTIONS: usize = 32;
const OPERATIONS_MAX_HEADER_BYTES: usize = 16 * 1024;
const OPERATIONS_HEADER_TIMEOUT: Duration = Duration::from_secs(5);
const OPERATIONS_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const OPERATIONS_CONNECTION_LIFETIME: Duration = Duration::from_secs(60);
const OPERATIONS_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
const OPERATIONS_PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const OPERATIONS_PROBE_MAX_RESPONSE_BYTES: u64 = 16 * 1024;

#[derive(Clone, Debug)]
pub struct DataPlaneOperationsConfig {
    pub(crate) bind: SocketAddr,
    pub(crate) dimensions: CanonicalMetricDimensions,
    /// Bearer token every non-health operations request must present
    /// (`Authorization: Bearer <token>`). Read from
    /// `SDKWORK_WEBSERVER_OPERATIONS_AUTH_TOKEN_FILE` when set.
    pub(crate) auth_token: Option<Arc<Zeroizing<String>>>,
}

impl DataPlaneOperationsConfig {
    pub fn loopback(
        bind: SocketAddr,
        environment: &str,
        deployment_profile: &str,
        runtime_target: &str,
    ) -> Result<Self, String> {
        validate_loopback_bind(bind)?;
        Ok(Self {
            bind,
            dimensions: CanonicalMetricDimensions::new(
                Some(environment),
                Some(deployment_profile),
                Some(runtime_target),
            )?,
            auth_token: None,
        })
    }

    pub fn from_env() -> Result<Option<Self>, String> {
        let Some(bind) = std::env::var(OPERATIONS_BIND_ENV).ok() else {
            return Ok(None);
        };
        let bind = bind.trim();
        if bind.is_empty() {
            return Err(format!("{OPERATIONS_BIND_ENV} must not be empty"));
        }
        let bind = bind
            .parse::<SocketAddr>()
            .map_err(|error| format!("{OPERATIONS_BIND_ENV} is not a socket address: {error}"))?;
        let auth_token = load_operations_auth_token()?;
        // The authenticated operations profile: the listener stays loopback
        // unless an operator both configures a bearer token and explicitly
        // authorizes the exposed bind — the token alone proves callers, the
        // flag records the host policy decision.
        if !bind.ip().is_loopback() {
            let Some(_) = auth_token.as_ref() else {
                return Err(format!(
                    "{OPERATIONS_BIND_ENV} must use a loopback address unless {OPERATIONS_AUTH_TOKEN_FILE_ENV} configures a bearer token"
                ));
            };
            let authorized = std::env::var(OPERATIONS_EXPOSE_ALLOWED_ENV)
                .map(|value| value.trim() == "true" || value.trim() == "1")
                .unwrap_or(false);
            if !authorized {
                return Err(format!(
                    "{OPERATIONS_BIND_ENV} refuses a non-loopback bind; set {OPERATIONS_EXPOSE_ALLOWED_ENV}=true to authorize the exposed operations listener"
                ));
            }
        }
        Ok(Some(Self {
            bind,
            dimensions: CanonicalMetricDimensions::from_env()?,
            auth_token,
        }))
    }
}

/// Reads the optional operations bearer token file: trimmed, non-empty,
/// zeroized in memory. A configured-but-unreadable file fails the boot
/// (fail-closed) instead of silently starting an unauthenticated listener.
fn load_operations_auth_token() -> Result<Option<Arc<Zeroizing<String>>>, String> {
    let Some(path) = std::env::var(OPERATIONS_AUTH_TOKEN_FILE_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let raw = std::fs::read_to_string(&path)
        .map_err(|error| format!("{OPERATIONS_AUTH_TOKEN_FILE_ENV} ({path}) is not readable: {error}"))?;
    let token = raw.trim();
    if token.is_empty() {
        return Err(format!(
            "{OPERATIONS_AUTH_TOKEN_FILE_ENV} ({path}) must contain a non-empty bearer token"
        ));
    }
    Ok(Some(Arc::new(Zeroizing::new(token.to_owned()))))
}

/// What a request to the operations listener may do.
#[derive(Clone)]
pub(crate) struct OperationsAuth {
    token: Option<Arc<Zeroizing<String>>>,
}

impl OperationsAuth {
    pub(crate) fn new(token: Option<Arc<Zeroizing<String>>>) -> Self {
        Self { token }
    }

    fn presented_token(headers: &axum::http::HeaderMap) -> Option<&str> {
        headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
    }

    /// `Ok` when the request may proceed; `Err` carries the response status.
    ///
    /// The three health paths are always open: they reveal nothing and are
    /// what host probes (k8s exec, curl) call. Everything else requires the
    /// bearer token when one is configured. Mutating tunnel control is
    /// fail-closed: with no token configured there is no way to prove the
    /// caller, and a loopback listener is reachable from any local process
    /// and from server-side fetches made by hosted sites, so `POST`/`DELETE`
    /// answer 403 instead of trusting the bind address.
    pub(crate) fn check(
        &self,
        headers: &axum::http::HeaderMap,
        mutation: bool,
    ) -> Result<(), StatusCode> {
        let Some(token) = self.token.as_ref() else {
            return if mutation {
                Err(StatusCode::FORBIDDEN)
            } else {
                Ok(())
            };
        };
        match Self::presented_token(headers) {
            Some(presented)
                if sdkwork_utils_rust::secure_compare(presented, token.as_str()) => {
                    Ok(())
                }
            _ => Err(StatusCode::UNAUTHORIZED),
        }
    }
}

pub async fn probe_data_plane_operations_from_env(path: &str) -> Result<(), String> {
    if !matches!(path, "/healthz" | "/readyz" | "/livez") {
        return Err("operations probe path must be /healthz, /readyz, or /livez".to_owned());
    }
    let config = DataPlaneOperationsConfig::from_env()?
        .ok_or_else(|| format!("{OPERATIONS_BIND_ENV} is required for operations probes"))?;
    timeout(
        OPERATIONS_PROBE_TIMEOUT,
        probe_operations(config.bind, path),
    )
    .await
    .map_err(|_| "operations probe timed out".to_owned())?
}

async fn probe_operations(bind: SocketAddr, path: &str) -> Result<(), String> {
    let mut stream = tokio::net::TcpStream::connect(bind)
        .await
        .map_err(|error| format!("operations probe connection failed: {error}"))?;
    let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|error| format!("operations probe write failed: {error}"))?;
    stream
        .shutdown()
        .await
        .map_err(|error| format!("operations probe request shutdown failed: {error}"))?;

    let mut response = Vec::new();
    stream
        .take(OPERATIONS_PROBE_MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut response)
        .await
        .map_err(|error| format!("operations probe read failed: {error}"))?;
    if response.len() as u64 > OPERATIONS_PROBE_MAX_RESPONSE_BYTES {
        return Err("operations probe response exceeds the bounded limit".to_owned());
    }
    let status_line_end = response
        .windows(2)
        .position(|window| window == b"\r\n")
        .ok_or_else(|| "operations probe response has no HTTP status line".to_owned())?;
    let status_line = std::str::from_utf8(&response[..status_line_end])
        .map_err(|_| "operations probe response status is not UTF-8".to_owned())?;
    if !matches!(status_line, "HTTP/1.0 200 OK" | "HTTP/1.1 200 OK") {
        return Err("operations probe did not return HTTP 200".to_owned());
    }
    Ok(())
}

pub(crate) struct PreparedOperationsListener {
    socket: TcpListener,
    address: SocketAddr,
    auth: OperationsAuth,
}

pub(crate) async fn prepare_operations_listener(
    config: &DataPlaneOperationsConfig,
) -> Result<PreparedOperationsListener, DataPlaneError> {
    let socket = TcpListener::bind(config.bind).await.map_err(|source| {
        DataPlaneError::OperationsListener {
            address: config.bind,
            source,
        }
    })?;
    let address = socket
        .local_addr()
        .map_err(|source| DataPlaneError::OperationsListener {
            address: config.bind,
            source,
        })?;
    Ok(PreparedOperationsListener {
        socket,
        address,
        auth: OperationsAuth::new(config.auth_token.clone()),
    })
}

pub(crate) async fn serve_operations_listener(
    prepared: PreparedOperationsListener,
    runtime: Arc<DataPlaneRuntime>,
    website_delivery: Option<Arc<WebsiteDeliveryExecutor>>,
    provider_resources: Option<Arc<AppConfigResourceExecutor>>,
    shutdown: watch::Receiver<bool>,
) -> Result<(), DataPlaneError> {
    let metrics_runtime = runtime.clone();
    let auth = prepared.auth.clone();
    let mut router = Router::new();
    // Tunnel control surface (PRD SS35/SS82): served only when the app
    // config enables the tunnel; otherwise the paths 404 like any other.
    // Every handler passes the request headers through `OperationsAuth`:
    // mutations are authenticated unconditionally, reads only when a token
    // is configured.
    if runtime.tunnel.is_some() {
        let status_runtime = runtime.clone();
        let status_auth = auth.clone();
        let list_runtime = runtime.clone();
        let list_auth = auth.clone();
        let create_runtime = runtime.clone();
        let create_auth = auth.clone();
        let remove_runtime = runtime.clone();
        let remove_auth = auth.clone();
        router = router
            .route(
                "/tunnel/status",
                get(move |headers: axum::http::HeaderMap| async move {
                    if let Err(status) = status_auth.check(&headers, false) {
                        return operations_auth_rejected(status).into_response();
                    }
                    tunnel_status_response(&status_runtime).await
                }),
            )
            .route(
                "/tunnel/routes",
                get(move |headers: axum::http::HeaderMap| async move {
                    if let Err(status) = list_auth.check(&headers, false) {
                        return operations_auth_rejected(status).into_response();
                    }
                    tunnel_routes_response(&list_runtime).await
                }),
            )
            .route(
                "/tunnel/routes",
                post(move |headers: axum::http::HeaderMap, body: String| async move {
                    if let Err(status) = create_auth.check(&headers, true) {
                        return operations_auth_rejected(status).into_response();
                    }
                    tunnel_route_create(&create_runtime, &body).await
                }),
            )
            .route(
                "/tunnel/routes/:id",
                delete(
                    move |headers: axum::http::HeaderMap,
                          axum::extract::Path(route_id): axum::extract::Path<String>| async move {
                        if let Err(status) = remove_auth.check(&headers, true) {
                            return operations_auth_rejected(status).into_response();
                        }
                        tunnel_route_remove(&remove_runtime, &route_id).await
                    },
                ),
            );
    }
    let metrics_auth = auth.clone();
    let router = router.route(
        "/metrics",
        get(move |headers: axum::http::HeaderMap| {
            let runtime = metrics_runtime.clone();
            let website_delivery = website_delivery.clone();
            let provider_resources = provider_resources.clone();
            async move {
                if let Err(status) = metrics_auth.check(&headers, false) {
                    return operations_auth_rejected(status).into_response();
                }
                let provider_resolution_cache = match website_delivery.as_ref() {
                    Some(executor) => Some(executor.provider_resolution_cache_snapshot().await),
                    None => match provider_resources.as_ref() {
                        Some(executor) => Some(executor.provider_resolution_cache_snapshot().await),
                        None => None,
                    },
                };
                let mut text = runtime
                    .metrics
                    .render_prometheus(&runtime, provider_resolution_cache.as_ref());
                if let Some(tunnel) = runtime.tunnel.as_ref() {
                    text.push_str(&tunnel.metrics.render_prometheus());
                }
                text.push_str(&super::certificates_metrics::render_prometheus());
                // The audit-persistence counter lives on the shared contract
                // plane because the management service writes it while this
                // scrape reads it: a nonzero value is a permanent audit gap
                // for the affected operations and is alerted on.
                text.push_str("# HELP sdkwork_webserver_audit_persistence_failures_total Audit rows that could not be persisted since process start; nonzero is a permanent audit gap.\n");
                text.push_str("# TYPE sdkwork_webserver_audit_persistence_failures_total counter\n");
                text.push_str(&format!(
                    "sdkwork_webserver_audit_persistence_failures_total {}\n",
                    sdkwork_webserver_contract::observability::audit_persistence_failures_total()
                ));
                (
                    StatusCode::OK,
                    [(
                        axum::http::header::CONTENT_TYPE,
                        "text/plain; version=0.0.4; charset=utf-8",
                    )],
                    text,
                )
                    .into_response()
            }
        }),
    );
    let router = service_router(
        router,
        ServiceRouterConfig::default()
            .with_always_ready()
            .skip_metrics(),
    )
    .layer(TimeoutLayer::with_status_code(
        StatusCode::REQUEST_TIMEOUT,
        OPERATIONS_REQUEST_TIMEOUT,
    ));
    tracing::info!(
        address = %prepared.address,
        "loopback data-plane operations listener started"
    );
    serve_bounded_operations(prepared.socket, router, shutdown)
        .await
        .map_err(|source| DataPlaneError::OperationsListener {
            address: prepared.address,
            source,
        })
}

fn operations_auth_rejected(status: StatusCode) -> axum::response::Response {
    let detail = if status == StatusCode::FORBIDDEN {
        format!(
            "operations mutations require {OPERATIONS_AUTH_TOKEN_FILE_ENV} to configure a bearer token"
        )
    } else {
        "operations bearer token missing or invalid".to_owned()
    };
    (status, detail).into_response()
}

fn tunnel_service(
    runtime: &Arc<DataPlaneRuntime>,
) -> Option<Arc<sdkwork_webserver_tunnel::service::GatewayTunnelService>> {
    runtime.tunnel.as_ref().map(|shared| {
        Arc::new(sdkwork_webserver_tunnel::service::GatewayTunnelService {
            shared: shared.clone(),
        })
    })
}

async fn tunnel_status_response(runtime: &Arc<DataPlaneRuntime>) -> axum::response::Response {
    match tunnel_service(runtime) {
        Some(service) => match service.status().await {
            Ok(status) => (StatusCode::OK, Json(status)).into_response(),
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            "tunnel is not enabled
",
        )
            .into_response(),
    }
}

async fn tunnel_routes_response(runtime: &Arc<DataPlaneRuntime>) -> axum::response::Response {
    match tunnel_service(runtime) {
        Some(service) => match service.list_routes().await {
            Ok(routes) => (StatusCode::OK, Json(routes)).into_response(),
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            "tunnel is not enabled
",
        )
            .into_response(),
    }
}

/// `POST /tunnel/routes` body (PRD SS36 + device binding):
/// `{"deviceId": "dev_x", "name": "web", "protocol": "http", "domain":
/// "demo.x", "port": 7000, "target": "127.0.0.1:3000", "allowPublic":
/// true}`. The target is advisory: the agent activates only templates it
/// recognizes and always dials its own configured target (PRD SS45).
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TunnelRouteCreateBody {
    device_id: String,
    name: String,
    protocol: String,
    domain: Option<String>,
    port: Option<u16>,
    target: String,
    #[serde(default)]
    allow_public: bool,
}

async fn tunnel_route_create(
    runtime: &Arc<DataPlaneRuntime>,
    body: &str,
) -> axum::response::Response {
    let Some(service) = tunnel_service(runtime) else {
        return (
            StatusCode::NOT_FOUND,
            "tunnel is not enabled
",
        )
            .into_response();
    };
    let parsed: TunnelRouteCreateBody = match serde_json::from_str(body) {
        Ok(parsed) => parsed,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response()
        }
    };
    let template = sdkwork_webserver_tunnel_core::TunnelRouteTemplate {
        name: parsed.name,
        protocol: match parsed.protocol.as_str() {
            "http" => sdkwork_webserver_tunnel_core::TunnelProtocolKind::Http,
            "tcp" => sdkwork_webserver_tunnel_core::TunnelProtocolKind::Tcp,
            other => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("protocol `{other}` must be http or tcp")
                    })),
                )
                    .into_response()
            }
        },
        domain: parsed.domain,
        port: parsed.port,
        target: parsed.target,
        policy: Some(sdkwork_webserver_tunnel_core::RoutePolicy {
            allow_public: parsed.allow_public,
            ..sdkwork_webserver_tunnel_core::RoutePolicy::private()
        }),
    };
    let device_id = match sdkwork_webserver_tunnel_core::DeviceId::parse(&parsed.device_id) {
        Ok(device_id) => device_id,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response()
        }
    };
    match service.create_route(&device_id, &template).await {
        Ok(registration) => (StatusCode::ACCEPTED, Json(registration)).into_response(),
        Err(error) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": error.to_string() })),
        )
            .into_response(),
    }
}

async fn tunnel_route_remove(
    runtime: &Arc<DataPlaneRuntime>,
    route_id: &str,
) -> axum::response::Response {
    let Some(service) = tunnel_service(runtime) else {
        return (
            StatusCode::NOT_FOUND,
            "tunnel is not enabled
",
        )
            .into_response();
    };
    match sdkwork_webserver_tunnel_core::RouteId::parse(route_id) {
        Ok(route_id) => match service.remove_route(&route_id).await {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(sdkwork_webserver_tunnel_core::TunnelError::RouteNotFound) => (
                StatusCode::NOT_FOUND,
                "route not found
",
            )
                .into_response(),
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response(),
        },
        Err(_) => (
            StatusCode::BAD_REQUEST,
            "invalid route id
",
        )
            .into_response(),
    }
}

async fn serve_bounded_operations(
    listener: TcpListener,
    router: Router,
    mut shutdown: watch::Receiver<bool>,
) -> io::Result<()> {
    let permits = Arc::new(Semaphore::new(MAX_OPERATIONS_CONNECTIONS));
    let mut builder = Builder::new(TokioExecutor::new());
    builder
        .http1()
        .max_buf_size(OPERATIONS_MAX_HEADER_BYTES)
        .header_read_timeout(OPERATIONS_HEADER_TIMEOUT)
        .timer(TokioTimer::new());
    let builder = Arc::new(builder.http1_only());
    let mut connections = JoinSet::new();
    let mut accept_policy = super::accept::AcceptRetryPolicy::new();

    loop {
        tokio::select! {
            biased;
            () = wait_for_shutdown(&mut shutdown) => break,
            Some(result) = connections.join_next(), if !connections.is_empty() => {
                if let Err(error) = result {
                    tracing::warn!(%error, "operations connection task failed");
                }
            }
            accepted = listener.accept() => {
                let (stream, _) = match accepted {
                    Ok(accepted) => {
                        accept_policy.on_success();
                        accepted
                    }
                    Err(error) => {
                        if accept_policy.on_error(&error) {
                            tokio::time::sleep(super::accept::ACCEPT_RETRY_PAUSE).await;
                            continue;
                        }
                        return Err(error);
                    }
                };
                let Ok(permit) = permits.clone().try_acquire_owned() else {
                    drop(stream);
                    continue;
                };
                let service = router.clone();
                let builder = builder.clone();
                connections.spawn(async move {
                    let _permit = permit;
                    let io = TokioIo::new(stream);
                    let service = TowerToHyperService::new(service);
                    let connection = builder.serve_connection(io, service);
                    if let Ok(Err(error)) = timeout(OPERATIONS_CONNECTION_LIFETIME, connection).await {
                        tracing::debug!(%error, "operations HTTP connection closed with an error");
                    }
                });
            }
        }
    }

    if timeout(OPERATIONS_DRAIN_TIMEOUT, async {
        while let Some(result) = connections.join_next().await {
            if let Err(error) = result {
                tracing::warn!(%error, "operations connection task failed during drain");
            }
        }
    })
    .await
    .is_err()
    {
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
    Ok(())
}

async fn wait_for_shutdown(shutdown: &mut watch::Receiver<bool>) {
    if *shutdown.borrow() {
        return;
    }
    while shutdown.changed().await.is_ok() {
        if *shutdown.borrow() {
            return;
        }
    }
}

fn validate_loopback_bind(bind: SocketAddr) -> Result<(), String> {
    // The explicit `loopback()` constructor pins the host-policy default; the
    // env constructor additionally accepts an exposed bind when a bearer
    // token is configured and `SDKWORK_WEBSERVER_OPERATIONS_EXPOSE_ALLOWED`
    // records the operator's decision.
    if !bind.ip().is_loopback() {
        return Err(format!(
            "{OPERATIONS_BIND_ENV} must use a loopback address unless {OPERATIONS_AUTH_TOKEN_FILE_ENV} configures a bearer token"
        ));
    }
    if bind.port() == 0 {
        return Err(format!("{OPERATIONS_BIND_ENV} must use a non-zero port"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use super::DataPlaneOperationsConfig;

    #[test]
    fn operations_bind_is_loopback_only_and_dimensions_fail_closed() {
        let public = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 3900);
        assert!(
            DataPlaneOperationsConfig::loopback(public, "production", "standalone", "server")
                .is_err()
        );

        let loopback = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3900);
        assert!(DataPlaneOperationsConfig::loopback(
            loopback,
            "production",
            "standalone",
            "docker"
        )
        .is_err());
        assert!(DataPlaneOperationsConfig::loopback(
            loopback,
            "production",
            "standalone",
            "server"
        )
        .is_ok());
    }

    #[test]
    fn operations_auth_gates_mutations_fail_closed_and_tokens_constant_time() {
        use std::sync::Arc;

        use axum::http::HeaderMap;
        use zeroize::Zeroizing;

        use axum::http::StatusCode;

        let bearer = |value: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(
                axum::http::header::AUTHORIZATION,
                axum::http::HeaderValue::from_str(&format!("Bearer {value}")).unwrap(),
            );
            headers
        };

        // No token configured: reads pass on the loopback listener, mutations
        // fail closed (the bind address proves nothing).
        let open = super::OperationsAuth::new(None);
        assert!(open.check(&HeaderMap::new(), false).is_ok());
        assert_eq!(open.check(&HeaderMap::new(), true), Err(StatusCode::FORBIDDEN));

        // Token configured: everything requires the exact bearer; a wrong or
        // missing header is 401, and the comparison is constant-time.
        let token = Arc::new(Zeroizing::new("s3cret-operations-token".to_owned()));
        let guarded = super::OperationsAuth::new(Some(token));
        assert_eq!(
            guarded.check(&HeaderMap::new(), false),
            Err(StatusCode::UNAUTHORIZED)
        );
        assert_eq!(
            guarded.check(&bearer("wrong"), true),
            Err(StatusCode::UNAUTHORIZED)
        );
        assert!(guarded.check(&bearer("s3cret-operations-token"), false).is_ok());
        assert!(guarded.check(&bearer("s3cret-operations-token"), true).is_ok());
    }

    #[tokio::test]
    async fn operations_probe_rejects_unreserved_paths_before_network_io() {
        let error = super::probe_data_plane_operations_from_env("/metrics")
            .await
            .expect_err("metrics must not be a health probe target");
        assert!(error.contains("probe path"));
    }
}
