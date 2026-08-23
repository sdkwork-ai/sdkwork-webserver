//! `proxy_set_header` execution e2e: supported `$variables` expand at
//! request time and the upstream observes them; unsupported variables fail
//! closed at materialization.

use std::{fs, net::TcpListener, path::PathBuf, sync::Arc, time::Duration};

use serde_json::{json, Value};
use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::{load_and_compile_webserver_config, WebServerConfigLoader};
use tokio::sync::oneshot;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

/// Upstream echoing the request head (request line + headers) as the body.
async fn spawn_echo_upstream(port: u16) {
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("bind");
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut socket = socket;
                let mut buffer = vec![0_u8; 8192];
                let Ok(read) = socket.read(&mut buffer).await else {
                    return;
                };
                let head = String::from_utf8_lossy(&buffer[..read])
                    .split("\r\n\r\n")
                    .next()
                    .unwrap_or("")
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

fn write_config(port: u16, upstream_port: u16, set_headers: Value) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("sdkwork-header-e2e-{}", port));
    fs::create_dir_all(&directory).expect("dir");
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-header-e2e",
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
        "upstreams": [{
            "id": "echo",
            "targets": [{ "url": format!("http://127.0.0.1:{upstream_port}") }],
            "addressPolicy": { "allowedCidrs": ["127.0.0.0/8", "::1/128"] }
        }],
        "resources": [{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "echo",
            "requestSetHeaders": set_headers
        }],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["http"],
            "serverNames": ["header.localhost"],
            "routes": [{
                "id": "proxy-route",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "proxy"
            }]
        }]
    });
    let path = directory.join("config.json");
    fs::write(&path, serde_json::to_vec_pretty(&config).expect("serialize")).expect("write");
    path
}

async fn spawn_data_plane(
    config_path: &PathBuf,
) -> (oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let compiled = load_and_compile_webserver_config(config_path).expect("compile");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let _ = run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await;
    });
    (shutdown_tx, task)
}

#[tokio::test]
async fn proxy_set_header_expands_supported_variables_upstream() {
    let port = free_port();
    let upstream_port = free_port();
    spawn_echo_upstream(upstream_port).await;
    let config_path = write_config(
        port,
        upstream_port,
        json!([
            "Host $host",
            "X-Forwarded-For $proxy_add_x_forwarded_for",
            "X-Real-IP $remote_addr",
            "X-Forwarded-Proto $scheme",
            "X-Server-Port $server_port"
        ]),
    );
    let (shutdown_tx, task) = spawn_data_plane(&config_path).await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("client");
    let base = format!("http://127.0.0.1:{port}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if client
            .get(&format!("{base}/"))
            .header("host", "header.localhost")
            .send()
            .await
            .is_ok()
        {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "not ready");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    let body = client
        .get(&format!("{base}/path?q=1"))
        .header("host", "header.localhost")
        .send()
        .await
        .expect("request")
        .text()
        .await
        .expect("body");
    let lowered = body.to_ascii_lowercase();
    // $host expands to the request authority host.
    assert!(lowered.contains("host: header.localhost"), "body: {body}");
    // $proxy_add_x_forwarded_for: the data plane overwrites a client-supplied
    // X-Forwarded-For with the peer IP (spoofing guard), then appends the
    // peer again for the proxy chain.
    assert!(
        lowered.contains("x-forwarded-for: 127.0.0.1, 127.0.0.1"),
        "body: {body}"
    );
    // $remote_addr expands to the client IP.
    assert!(lowered.contains("x-real-ip: 127.0.0.1"), "body: {body}");
    // $scheme expands to http over the plaintext listener.
    assert!(lowered.contains("x-forwarded-proto: http"), "body: {body}");
    // $server_port expands to the listener port.
    assert!(
        lowered.contains(&format!("x-server-port: {port}")),
        "body: {body}"
    );

    let _ = shutdown_tx.send(());
    let _ = task.await;
    fs::remove_dir_all(config_path.parent().expect("dir")).ok();
}

#[tokio::test]
async fn unsupported_proxy_set_header_variables_fail_closed() {
    let directory = std::env::temp_dir().join(format!("sdkwork-header-reject-{}", free_port()));
    fs::create_dir_all(&directory).expect("dir");
    let path = directory.join("server.toml");
    fs::write(
        &path,
        "[[http.upstream]]\nname = \"echo\"\n\n[[http.upstream.target]]\naddress = \"127.0.0.1:1\"\n\n[[http.server]]\nlisten = [\"80\"]\nserverName = [\"x.local\"]\n\n[[http.server.location]]\nmatch = \"/\"\nproxyPass = \"http://echo\"\nproxySetHeader = [\"X-Custom $unsupported_var\"]\n",
    )
    .expect("write");
    let error = WebServerConfigLoader::new()
        .load(&path, &Default::default())
        .expect_err("unsupported header variable must fail closed");
    assert!(
        error.to_string().contains("unsupported"),
        "unexpected error: {error}"
    );
    fs::remove_dir_all(&directory).ok();
}
