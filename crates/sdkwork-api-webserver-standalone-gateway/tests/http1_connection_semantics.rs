use std::{fs, net::TcpListener, path::Path, time::Duration};

use sdkwork_api_webserver_standalone_gateway::{run_data_plane_until, DataPlaneError};
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::json;
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::oneshot,
    task::JoinHandle,
    time::timeout,
};

fn available_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve an available port");
    listener.local_addr().expect("read local address").port()
}

fn write_config(directory: &Path, port: u16) -> std::path::PathBuf {
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-http1-semantics-test",
        "limits": {
            "maxRequestBodyBytes": 4,
            "requestTimeoutMs": 2000,
            "drainTimeoutMs": 1000,
            "maxConnections": 32,
            "maxRequestHeaderBytes": 8192,
            "maxRequestHeaders": 32,
            "requestHeaderTimeoutMs": 1000
        },
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "test-host",
            "maxConnections": 16
        }],
        "resources": [
            {"id": "accepted", "type": "respond", "status": 200, "body": "accepted"},
            {"id": "first", "type": "respond", "status": 200, "body": "first"},
            {"id": "second", "type": "respond", "status": 200, "body": "second"}
        ],
        "virtualHosts": [{
            "id": "test-host",
            "listenerRefs": ["http"],
            "serverNames": ["test.localhost"],
            "routes": [
                {"id": "route-first", "match": {"pathType": "exact", "path": "/first"}, "resourceRef": "first"},
                {"id": "route-second", "match": {"pathType": "exact", "path": "/second"}, "resourceRef": "second"},
                {"id": "route-accepted", "match": {"pathType": "prefix", "path": "/"}, "resourceRef": "accepted"}
            ]
        }]
    });
    let path = directory.join("sdkwork.webserver.config.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&config).expect("serialize config"),
    )
    .expect("write config");
    path
}

fn write_pipeline_config(directory: &Path, port: u16) -> std::path::PathBuf {
    let path = write_config(directory, port);
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("read Pipeline test configuration"))
            .expect("parse Pipeline test configuration");
    config["limits"]["maxConnections"] = json!(1);
    config["limits"]["http1MaxPipelineDepth"] = json!(2);
    config["listeners"][0]["maxConnections"] = json!(1);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&config).expect("serialize Pipeline test configuration"),
    )
    .expect("write Pipeline test configuration");
    path
}

fn spawn_data_plane(path: &Path) -> (oneshot::Sender<()>, JoinHandle<Result<(), DataPlaneError>>) {
    let compiled = load_and_compile_webserver_config(path).expect("compile data-plane config");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    (shutdown_tx, task)
}

async fn wait_until_ready(port: u16) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(mut stream) = tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
            if stream
                .write_all(b"GET / HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n")
                .await
                .is_ok()
            {
                let mut response = Vec::new();
                if stream.read_to_end(&mut response).await.is_ok()
                    && response.starts_with(b"HTTP/1.1 200")
                {
                    return;
                }
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "HTTP/1 data plane did not become ready"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn stop_data_plane(
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<Result<(), DataPlaneError>>,
) {
    shutdown.send(()).expect("send shutdown");
    timeout(Duration::from_secs(3), task)
        .await
        .expect("data plane drains")
        .expect("data-plane task joins")
        .expect("data plane stops cleanly");
}

async fn raw_response(port: u16, request: &[u8]) -> Vec<u8> {
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect raw HTTP client");
    stream.write_all(request).await.expect("write raw request");
    let mut response = Vec::new();
    timeout(Duration::from_secs(3), stream.read_to_end(&mut response))
        .await
        .expect("raw response completes")
        .expect("read raw response");
    response
}

async fn read_header_block(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut response = Vec::with_capacity(256);
    timeout(Duration::from_secs(1), async {
        loop {
            let byte = stream.read_u8().await.expect("read response header byte");
            response.push(byte);
            assert!(response.len() <= 8192, "response header block is bounded");
            if response.ends_with(b"\r\n\r\n") {
                return;
            }
        }
    })
    .await
    .expect("response header block arrives");
    response
}

fn count_occurrences(bytes: &[u8], needle: &[u8]) -> usize {
    bytes
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

#[tokio::test]
async fn handles_expect_continue_and_rejects_unsupported_expectations() {
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let path = write_config(directory.path(), port);
    let (shutdown, task) = spawn_data_plane(&path);
    wait_until_ready(port).await;

    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect Expect client");
    stream
        .write_all(
            b"POST / HTTP/1.1\r\nHost: test.localhost\r\nContent-Length: 4\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n",
        )
        .await
        .expect("write Expect headers");
    let informational = read_header_block(&mut stream).await;
    assert_eq!(informational, b"HTTP/1.1 100 Continue\r\n\r\n");
    stream.write_all(b"four").await.expect("write request body");
    let mut final_response = Vec::new();
    stream
        .read_to_end(&mut final_response)
        .await
        .expect("read final response");
    assert!(final_response.starts_with(b"HTTP/1.1 200"));
    assert!(final_response.ends_with(b"accepted"));

    let oversized = raw_response(
        port,
        b"POST / HTTP/1.1\r\nHost: test.localhost\r\nContent-Length: 5\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(oversized.starts_with(b"HTTP/1.1 413"));
    assert!(!oversized.windows(12).any(|value| value == b"100 Continue"));

    let unsupported = raw_response(
        port,
        b"POST / HTTP/1.1\r\nHost: test.localhost\r\nContent-Length: 4\r\nExpect: sdkwork-magic\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(unsupported.starts_with(b"HTTP/1.1 417"));

    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn serves_http10_default_host_keep_alive_and_ordered_pipelines() {
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let path = write_config(directory.path(), port);
    let (shutdown, task) = spawn_data_plane(&path);
    wait_until_ready(port).await;

    let default_host = raw_response(port, b"GET /first HTTP/1.0\r\n\r\n").await;
    assert!(
        default_host.starts_with(b"HTTP/1.0 200"),
        "HTTP/1.0 response version is preserved by Hyper: {}",
        String::from_utf8_lossy(&default_host)
    );
    assert!(default_host.ends_with(b"first"));
    assert!(!default_host
        .windows(b"transfer-encoding".len())
        .any(|value| value.eq_ignore_ascii_case(b"transfer-encoding")));

    let pipelined = raw_response(
        port,
        b"GET /first HTTP/1.0\r\nHost: test.localhost\r\nConnection: keep-alive\r\n\r\nGET /second HTTP/1.0\r\nHost: test.localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert_eq!(count_occurrences(&pipelined, b"HTTP/1.0 200"), 2);
    let first = pipelined
        .windows(b"first".len())
        .position(|value| value == b"first")
        .expect("first response body");
    let second = pipelined
        .windows(b"second".len())
        .position(|value| value == b"second")
        .expect("second response body");
    assert!(first < second, "pipelined responses preserve request order");

    let invalid_chunked = raw_response(
        port,
        b"POST / HTTP/1.0\r\nHost: test.localhost\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n0\r\n\r\n",
    )
    .await;
    assert!(
        !invalid_chunked.starts_with(b"HTTP/1.0 2"),
        "HTTP/1.0 Transfer-Encoding must not reach a successful route"
    );

    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn bounds_pipeline_read_ahead_and_recovers_the_connection_permit() {
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let path = write_pipeline_config(directory.path(), port);
    let (shutdown, task) = spawn_data_plane(&path);
    wait_until_ready(port).await;

    let accepted = raw_response(
        port,
        b"GET /first HTTP/1.1\r\nHost: test.localhost\r\nConnection: keep-alive\r\n\r\nGET /second HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert_eq!(count_occurrences(&accepted, b"HTTP/1.1 200"), 2);

    let mut excessive = Vec::with_capacity(6 * 1024);
    for index in 0..64 {
        let connection = if index == 63 { "close" } else { "keep-alive" };
        excessive.extend_from_slice(
            format!(
                "GET /first HTTP/1.1\r\nHost: test.localhost\r\nConnection: {connection}\r\n\r\n"
            )
            .as_bytes(),
        );
    }
    let rejected = raw_response(port, &excessive).await;
    assert!(
        count_occurrences(&rejected, b"HTTP/1.1 200") < 64,
        "a single connection must not dispatch an over-depth Pipeline"
    );
    assert!(
        rejected.len() < 64 * 1024,
        "rejection output remains bounded"
    );

    let recovered = raw_response(
        port,
        b"GET /second HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(recovered.starts_with(b"HTTP/1.1 200"));
    assert!(recovered.ends_with(b"second"));

    stop_data_plane(shutdown, task).await;
}

#[tokio::test]
async fn completes_valid_half_closed_requests_and_rejects_truncated_bodies() {
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let path = write_config(directory.path(), port);
    let (shutdown, task) = spawn_data_plane(&path);
    wait_until_ready(port).await;

    let mut valid = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect half-close client");
    valid
        .write_all(b"POST / HTTP/1.1\r\nHost: test.localhost\r\nContent-Length: 4\r\n\r\nfour")
        .await
        .expect("write complete request");
    valid
        .shutdown()
        .await
        .expect("half-close request write side");
    let mut valid_response = Vec::new();
    timeout(
        Duration::from_secs(2),
        valid.read_to_end(&mut valid_response),
    )
    .await
    .expect("half-closed response completes")
    .expect("read half-closed response");
    assert!(valid_response.starts_with(b"HTTP/1.1 200"));
    assert!(valid_response.ends_with(b"accepted"));

    let mut truncated = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect truncated client");
    truncated
        .write_all(b"POST / HTTP/1.1\r\nHost: test.localhost\r\nContent-Length: 4\r\n\r\nxx")
        .await
        .expect("write truncated request");
    truncated
        .shutdown()
        .await
        .expect("half-close truncated request");
    let mut truncated_response = Vec::new();
    timeout(
        Duration::from_secs(2),
        truncated.read_to_end(&mut truncated_response),
    )
    .await
    .expect("truncated connection terminates")
    .expect("read truncated response");
    assert!(
        !truncated_response.starts_with(b"HTTP/1.1 2"),
        "a truncated request body must never execute the route successfully"
    );

    stop_data_plane(shutdown, task).await;
}
