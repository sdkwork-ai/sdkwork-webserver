use std::{io, net::SocketAddr, sync::Arc, time::Duration};

use axum::{http::StatusCode, response::IntoResponse, routing::get, Router};
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

use crate::metric_dimensions::CanonicalMetricDimensions;

use super::{runtime::DataPlaneRuntime, DataPlaneError};

const OPERATIONS_BIND_ENV: &str = "SDKWORK_WEBSERVER_DATA_PLANE_OPERATIONS_BIND";
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
        validate_loopback_bind(bind)?;
        Ok(Some(Self {
            bind,
            dimensions: CanonicalMetricDimensions::from_env()?,
        }))
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
    Ok(PreparedOperationsListener { socket, address })
}

pub(crate) async fn serve_operations_listener(
    prepared: PreparedOperationsListener,
    runtime: Arc<DataPlaneRuntime>,
    website_delivery: Option<Arc<WebsiteDeliveryExecutor>>,
    provider_resources: Option<Arc<AppConfigResourceExecutor>>,
    shutdown: watch::Receiver<bool>,
) -> Result<(), DataPlaneError> {
    let metrics_runtime = runtime.clone();
    let router = Router::new().route(
        "/metrics",
        get(move || {
            let runtime = metrics_runtime.clone();
            let website_delivery = website_delivery.clone();
            let provider_resources = provider_resources.clone();
            async move {
                let provider_resolution_cache = match website_delivery.as_ref() {
                    Some(executor) => Some(executor.provider_resolution_cache_snapshot().await),
                    None => match provider_resources.as_ref() {
                        Some(executor) => Some(executor.provider_resolution_cache_snapshot().await),
                        None => None,
                    },
                };
                (
                    StatusCode::OK,
                    [(
                        axum::http::header::CONTENT_TYPE,
                        "text/plain; version=0.0.4; charset=utf-8",
                    )],
                    runtime
                        .metrics
                        .render_prometheus(&runtime, provider_resolution_cache.as_ref()),
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
                let (stream, _) = accepted?;
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
    if !bind.ip().is_loopback() {
        return Err(format!(
            "{OPERATIONS_BIND_ENV} must use a loopback address until a reviewed authenticated operations profile exists"
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

    #[tokio::test]
    async fn operations_probe_rejects_unreserved_paths_before_network_io() {
        let error = super::probe_data_plane_operations_from_env("/metrics")
            .await
            .expect_err("metrics must not be a health probe target");
        assert!(error.contains("probe path"));
    }
}
