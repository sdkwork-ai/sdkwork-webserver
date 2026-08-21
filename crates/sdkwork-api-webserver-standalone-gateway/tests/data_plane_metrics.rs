use std::{fs, net::TcpListener, sync::Arc, time::Duration};

use sdkwork_api_webserver_standalone_gateway::{
    run_data_plane_with_operations_until, run_website_data_plane_with_operations_until,
    DataPlaneOperationsConfig,
};
use sdkwork_webserver_core::{
    load_and_compile_webserver_config,
    website_runtime::{WebsiteRuntimeEnvironment, WebsiteRuntimeRegistry},
};
use sdkwork_webserver_delivery_runtime::{WebsiteDeliveryExecutor, WebsiteProviderRegistry};
use serde_json::json;
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{oneshot, Notify},
    task::JoinHandle,
};

fn available_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve an available port");
    listener.local_addr().expect("read local address").port()
}

fn write_proxy_config(directory: &TempDir, port: u16, upstream_port: u16) -> std::path::PathBuf {
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-metrics-test-web",
        "limits": {
            "maxRequestBodyBytes": 1048576,
            "requestTimeoutMs": 5000,
            "drainTimeoutMs": 1000,
            "maxConnections": 2,
            "maxConcurrentRequests": 1
        },
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "test-host",
            "maxConnections": 2
        }],
        "resources": [{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "origin"
        }],
        "upstreams": [{
            "id": "origin",
            "targets": [{
                "url": format!("http://127.0.0.1:{upstream_port}"),
                "maxConnections": 2
            }],
            "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]},
            "connectTimeoutMs": 1000,
            "requestTimeoutMs": 5000,
            "maxConnections": 2,
            "maxIdleConnections": 0,
            "maxInFlightRequests": 2
        }],
        "virtualHosts": [{
            "id": "test-host",
            "listenerRefs": ["http"],
            "serverNames": ["test.localhost"],
            "routes": [{
                "id": "stream-route",
                "match": {"pathType": "exact", "path": "/stream"},
                "resourceRef": "proxy"
            }]
        }]
    });
    let path = directory.path().join("sdkwork.webserver.config.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&config).expect("serialize config"),
    )
    .expect("write config");
    path
}

async fn spawn_paused_upstream() -> (u16, Arc<Notify>, oneshot::Sender<()>, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind paused upstream");
    let port = listener.local_addr().expect("upstream address").port();
    let headers_sent = Arc::new(Notify::new());
    let task_headers_sent = headers_sent.clone();
    let (release_tx, release_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept proxy connection");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut buffer).await.expect("read proxy request");
            if read == 0 {
                return;
            }
            request.extend_from_slice(&buffer[..read]);
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n")
            .await
            .expect("write response headers");
        task_headers_sent.notify_waiters();
        let _ = release_rx.await;
        stream
            .write_all(b"done")
            .await
            .expect("write response body");
        let _ = stream.shutdown().await;
    });
    (port, headers_sent, release_tx, task)
}

async fn wait_for_response(client: &reqwest::Client, url: &str) -> reqwest::Response {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match client.get(url).send().await {
            Ok(response) => return response,
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => panic!("listener did not become ready: {error}"),
        }
    }
}

async fn wait_for_metric(client: &reqwest::Client, url: &str, sample: &str) -> String {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(response) = client.get(url).send().await {
            if let Ok(body) = response.text().await {
                if body.contains(sample) {
                    return body;
                }
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "metric sample did not converge: {sample}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn isolated_operations_listener_reports_real_stream_lifetimes() {
    let directory = TempDir::new().expect("temporary config directory");
    let data_port = available_port();
    let operations_port = available_port();
    let (upstream_port, headers_sent, release_upstream, upstream_task) =
        spawn_paused_upstream().await;
    let config_path = write_proxy_config(&directory, data_port, upstream_port);
    let compiled = load_and_compile_webserver_config(&config_path).expect("compile config");
    let operations = DataPlaneOperationsConfig::loopback(
        format!("127.0.0.1:{operations_port}")
            .parse()
            .expect("operations address"),
        "test",
        "standalone",
        "server",
    )
    .expect("valid operations config");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let data_plane = tokio::spawn(async move {
        run_data_plane_with_operations_until(compiled, Some(operations), async move {
            let _ = shutdown_rx.await;
        })
        .await
    });

    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(0)
        .build()
        .expect("HTTP client");
    let operations_url = format!("http://127.0.0.1:{operations_port}");
    for path in ["/healthz", "/livez", "/readyz"] {
        let response = wait_for_response(&client, &format!("{operations_url}{path}")).await;
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }

    let application_metrics =
        wait_for_response(&client, &format!("http://127.0.0.1:{data_port}/metrics")).await;
    assert_eq!(application_metrics.status(), reqwest::StatusCode::NOT_FOUND);
    assert!(!application_metrics
        .text()
        .await
        .expect("application response body")
        .contains("sdkwork_web_data_plane_"));

    let common = "service=\"sdkwork-api-webserver-standalone-gateway\",environment=\"test\",deployment_profile=\"standalone\",runtime_target=\"server\"";
    wait_for_metric(
        &client,
        &format!("{operations_url}/metrics"),
        &format!("sdkwork_web_data_plane_connections_active{{{common}}} 0"),
    )
    .await;
    let first_idle = tokio::net::TcpStream::connect(("127.0.0.1", data_port))
        .await
        .expect("first idle data-plane connection");
    let second_idle = tokio::net::TcpStream::connect(("127.0.0.1", data_port))
        .await
        .expect("second idle data-plane connection");
    wait_for_metric(
        &client,
        &format!("{operations_url}/metrics"),
        &format!("sdkwork_web_data_plane_connections_active{{{common}}} 2"),
    )
    .await;
    let third_idle = tokio::net::TcpStream::connect(("127.0.0.1", data_port))
        .await
        .expect("kernel accepts connection before runtime admission");
    wait_for_metric(
        &client,
        &format!("{operations_url}/metrics"),
        &format!(
            "sdkwork_web_data_plane_connection_rejections_total{{{common},reason=\"capacity\"}} 1"
        ),
    )
    .await;
    drop((first_idle, second_idle, third_idle));
    wait_for_metric(
        &client,
        &format!("{operations_url}/metrics"),
        &format!("sdkwork_web_data_plane_connections_active{{{common}}} 0"),
    )
    .await;

    let stream_request = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .get(format!("http://127.0.0.1:{data_port}/stream"))
                .header("host", "test.localhost")
                .send()
                .await
                .expect("start streaming proxy response")
        }
    });
    tokio::time::timeout(Duration::from_secs(5), headers_sent.notified())
        .await
        .expect("upstream response headers");
    let response = stream_request.await.expect("stream request task");
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let rejected = client
        .get(format!("http://127.0.0.1:{data_port}/stream"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("request capacity rejection");
    assert_eq!(rejected.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    drop(rejected);

    let body = wait_for_metric(
        &client,
        &format!("{operations_url}/metrics"),
        &format!("sdkwork_web_data_plane_requests_active{{{common}}} 1"),
    )
    .await;
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_upstream_attempts_total{{{common}}} 1"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_upstream_results_total{{{common},result=\"response\"}} 1"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_upstream_targets{{{common},state=\"healthy\"}} 1"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_request_rejections_total{{{common},reason=\"capacity\"}} 1"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_request_duration_seconds_count{{{common},status_class=\"2xx\"}} 0"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_upstream_duration_seconds_count{{{common},result=\"response\"}} 1"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_upstream_duration_seconds_bucket{{{common},result=\"response\",le=\"+Inf\"}} 1"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_dns_lookups_active{{{common}}} 0"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_dns_results_total{{{common},result=\"success\"}} 1"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_upstream_selection_contentions_total{{{common}}} 0"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_upstream_request_capacity{{{common},state=\"configured\"}} 2"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_upstream_request_capacity{{{common},state=\"in_use\"}} 1"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_upstream_request_capacity{{{common},state=\"available\"}} 1"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_upstream_connection_capacity{{{common},state=\"configured\"}} 2"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_upstream_connection_capacity{{{common},state=\"in_use\"}} 1"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_upstream_connection_capacity{{{common},state=\"available\"}} 1"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_upstream_target_connection_capacity{{{common},state=\"configured\"}} 2"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_upstream_target_connection_capacity{{{common},state=\"in_use\"}} 1"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_upstream_target_connection_capacity{{{common},state=\"available\"}} 1"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_protocol_errors_total{{{common},kind=\"http1_wire\"}} 0"
    )));
    assert!(body.contains(&format!(
        "sdkwork_web_data_plane_websocket_bytes_total{{{common},direction=\"downstream_to_upstream\"}} 0"
    )));
    assert!(!body.contains("test.localhost"));
    assert!(!body.contains("127.0.0.1"));
    assert!(!body.contains("stream-route"));

    release_upstream.send(()).expect("release upstream body");
    assert_eq!(
        response.bytes().await.expect("consume streaming response"),
        "done"
    );
    wait_for_metric(
        &client,
        &format!("{operations_url}/metrics"),
        &format!("sdkwork_web_data_plane_requests_active{{{common}}} 0"),
    )
    .await;
    let completed = wait_for_metric(
        &client,
        &format!("{operations_url}/metrics"),
        &format!(
            "sdkwork_web_data_plane_request_duration_seconds_count{{{common},status_class=\"2xx\"}} 1"
        ),
    )
    .await;
    assert!(completed.contains(&format!(
        "sdkwork_web_data_plane_request_body_bytes_total{{{common}}} 0"
    )));
    assert!(completed.contains("sdkwork_web_data_plane_response_body_bytes_total{"));

    let mut operations_connections = Vec::new();
    for _ in 0..32 {
        operations_connections.push(
            tokio::net::TcpStream::connect(("127.0.0.1", operations_port))
                .await
                .expect("bounded operations connection"),
        );
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut excess_operations = tokio::net::TcpStream::connect(("127.0.0.1", operations_port))
        .await
        .expect("kernel accepts excess operations connection");
    let mut byte = [0_u8; 1];
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), excess_operations.read(&mut byte))
            .await
            .expect("excess operations socket closes promptly")
            .expect("read excess operations socket"),
        0
    );
    drop(operations_connections.pop());
    let health = wait_for_response(&client, &format!("{operations_url}/healthz")).await;
    assert_eq!(health.status(), reqwest::StatusCode::OK);
    drop(operations_connections);

    shutdown_tx.send(()).expect("stop data plane");
    data_plane
        .await
        .expect("data-plane task joins")
        .expect("data plane shuts down cleanly");
    upstream_task.await.expect("upstream task joins");
}

#[tokio::test]
async fn website_operations_listener_exports_executor_cache_metrics() {
    let directory = TempDir::new().expect("temporary config directory");
    let data_port = available_port();
    let operations_port = available_port();
    let unused_upstream_port = available_port();
    let config_path = write_proxy_config(&directory, data_port, unused_upstream_port);
    let compiled = load_and_compile_webserver_config(&config_path).expect("compile config");
    let operations = DataPlaneOperationsConfig::loopback(
        format!("127.0.0.1:{operations_port}")
            .parse()
            .expect("operations address"),
        "test",
        "standalone",
        "server",
    )
    .expect("valid operations config");
    let executor = Arc::new(
        WebsiteDeliveryExecutor::with_provider_runtime_limits(
            Arc::new(WebsiteRuntimeRegistry::new(
                "node-1",
                WebsiteRuntimeEnvironment::Test,
            )),
            Arc::new(WebsiteProviderRegistry::new()),
            1024,
            7,
        )
        .expect("website executor"),
    );
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let data_plane = tokio::spawn(async move {
        run_website_data_plane_with_operations_until(
            compiled,
            executor,
            Some(operations),
            async move {
                let _ = shutdown_rx.await;
            },
        )
        .await
    });

    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(0)
        .build()
        .expect("HTTP client");
    let common = "service=\"sdkwork-api-webserver-standalone-gateway\",environment=\"test\",deployment_profile=\"standalone\",runtime_target=\"server\"";
    let metrics = wait_for_metric(
        &client,
        &format!("http://127.0.0.1:{operations_port}/metrics"),
        &format!("sdkwork_web_data_plane_provider_resolution_cache_capacity_entries{{{common}}} 7"),
    )
    .await;
    assert!(metrics.contains(&format!(
        "sdkwork_web_data_plane_provider_resolution_cache_entries{{{common}}} 0"
    )));
    assert!(metrics.contains(&format!(
        "sdkwork_web_data_plane_provider_resolution_cache_lookups_total{{{common},result=\"miss\"}} 0"
    )));

    shutdown_tx.send(()).expect("stop data plane");
    data_plane
        .await
        .expect("data-plane task joins")
        .expect("data plane shuts down cleanly");
}
