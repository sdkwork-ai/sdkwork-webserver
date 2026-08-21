//! Variable `proxy_pass` (`http://$host:$server_port`) execution and
//! `proxy_pass_request_headers off` header stripping.

use std::{fs, net::TcpListener, path::PathBuf, time::Duration};

use serde_json::{json, Value};
use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::load_and_compile_webserver_config;
use tokio::{io::AsyncReadExt, io::AsyncWriteExt, sync::oneshot};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

/// Echo upstream: responds with the raw request head (request line +
/// headers) as the body, so tests can observe what was forwarded.
async fn spawn_echo_upstream(port: u16) {
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("upstream bind");
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut socket = socket;
                let mut buffer = vec![0_u8; 8192];
                let Ok(read) = socket.read(&mut buffer).await else {
                    return;
                };
                let request_head = String::from_utf8_lossy(&buffer[..read]).into_owned();
                let head = request_head
                    .split("\r\n\r\n")
                    .next()
                    .unwrap_or(&request_head)
                    .to_owned();
                let body = head.replace("\r\n", "\\n");
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });
}

fn write_config(port: u16, resource: Value) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sdkwork-dynamic-proxy-test-{}-{}",
        std::process::id(),
        port
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("config.json");
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-dynamic-proxy-test",
        "limits": {
            "requestTimeoutMs": 5000,
            "drainTimeoutMs": 1000,
            "maxConnections": 32
        },
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "host"
        }],
        "resources": [resource],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["http"],
            "serverNames": ["dynamic.localhost"],
            "routes": [{
                "id": "proxy-route",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "proxy"
            }]
        }]
    });
    fs::write(&path, serde_json::to_vec_pretty(&config).expect("serialize")).expect("write");
    path
}

async fn wait_ready(client: &reqwest::Client, url: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match client
            .get(url)
            .header("host", "dynamic.localhost")
            .send()
            .await
        {
            Ok(_) => return,
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => panic!("data plane not ready: {error}"),
        }
    }
}

#[tokio::test]
async fn dynamic_proxy_pass_resolves_the_request_host_per_request() {
    let port = free_port();
    let upstream_port = free_port();
    spawn_echo_upstream(upstream_port).await;
    let resource = json!({
        "id": "proxy",
        "type": "proxy",
        "dynamicTarget": "http://$host:$server_port",
        "proxyPassRequestHeaders": true
    });
    let config_path = write_config(port, resource);
    let compiled = load_and_compile_webserver_config(&config_path).expect("compile");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("client");
    let base = format!("http://127.0.0.1:{port}");
    wait_ready(&client, &format!("{base}/")).await;

    // The Host header selects the upstream host and port.
    let response = client
        .get(&format!("{base}/hello.txt"))
        .header("host", format!("127.0.0.1:{upstream_port}"))
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), 200);
    let body = response.text().await.expect("body");
    assert!(
        body.contains("GET /hello.txt"),
        "upstream must receive the request URI: {body}"
    );

    let _ = shutdown_tx.send(());
    let _ = server.await;
    fs::remove_dir_all(config_path.parent().expect("config dir")).ok();
}

#[tokio::test]
async fn proxy_pass_request_headers_off_strips_client_headers() {
    let port = free_port();
    let upstream_port = free_port();
    spawn_echo_upstream(upstream_port).await;
    let resource = json!({
        "id": "proxy",
        "type": "proxy",
        "dynamicTarget": "http://$host:$server_port",
        "proxyPassRequestHeaders": false
    });
    let config_path = write_config(port, resource);
    let compiled = load_and_compile_webserver_config(&config_path).expect("compile");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("client");
    let base = format!("http://127.0.0.1:{port}");
    wait_ready(&client, &format!("{base}/")).await;

    let response = client
        .get(&format!("{base}/secret"))
        .header("host", format!("127.0.0.1:{upstream_port}"))
        .header("x-client-secret", "top-secret")
        .header("authorization", "Bearer abc")
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), 200);
    let body = response.text().await.expect("body");
    assert!(
        !body.to_ascii_lowercase().contains("x-client-secret"),
        "client headers must not be forwarded: {body}"
    );
    assert!(
        !body.to_ascii_lowercase().contains("authorization"),
        "authorization must not be forwarded: {body}"
    );

    let _ = shutdown_tx.send(());
    let _ = server.await;
    fs::remove_dir_all(config_path.parent().expect("config dir")).ok();
}
