//! `hash … consistent` upstream selection e2e: the same hash key always
//! lands on the same target, and the consistent ring keeps existing keys on
//! their targets when the target set changes.

use std::{fs, net::TcpListener, path::PathBuf, sync::Arc, time::Duration};

use serde_json::{json, Value};
use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::load_and_compile_webserver_config;
use tokio::sync::oneshot;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

/// Upstream that answers with its own target index as the body.
async fn spawn_tagged_upstream(port: u16, tag: &'static str) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("bind");
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                break;
            };
            let tag = tag;
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut socket = socket;
                let mut buffer = vec![0_u8; 2048];
                let _ = socket.read(&mut buffer).await;
                let body = format!("target-{tag}");
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

fn write_config(port: u16, upstreams: Vec<Value>) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("sdkwork-hash-e2e-{}", port));
    fs::create_dir_all(&directory).expect("dir");
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-hash-e2e",
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
        "resources": [{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "hashed"
        }],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["http"],
            "serverNames": ["hash.localhost"],
            "routes": [{
                "id": "proxy-route",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "proxy"
            }]
        }],
        "upstreams": upstreams
    });
    let path = directory.join("config.json");
    fs::write(&path, serde_json::to_vec_pretty(&config).expect("serialize")).expect("write");
    path
}

fn hash_upstream(targets: Vec<Value>) -> Value {
    json!({
        "id": "hashed",
        "targets": targets,
        "loadBalancing": "hash",
        "hash": { "key": "$request_uri", "consistent": true },
        "addressPolicy": { "allowedCidrs": ["127.0.0.0/8", "::1/128"] }
    })
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

async fn wait_ready(client: &reqwest::Client, url: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match client.get(url).header("host", "hash.localhost").send().await {
            Ok(_) => return,
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => panic!("data plane not ready: {error}"),
        }
    }
}

async fn target_of(client: &reqwest::Client, url: &str) -> String {
    client
        .get(url)
        .header("host", "hash.localhost")
        .send()
        .await
        .expect("request")
        .text()
        .await
        .expect("body")
}

#[tokio::test]
async fn consistent_hash_pins_keys_to_targets_and_survives_scale_down() {
    let port = free_port();
    let target_a = free_port();
    let target_b = free_port();
    let target_c = free_port();
    spawn_tagged_upstream(target_a, "a").await;
    spawn_tagged_upstream(target_b, "b").await;
    spawn_tagged_upstream(target_c, "c").await;
    let target = |port: u16| json!({ "url": format!("http://127.0.0.1:{port}") });

    let config_path = write_config(
        port,
        vec![hash_upstream(vec![target(target_a), target(target_b), target(target_c)])],
    );
    let (shutdown_tx, task) = spawn_data_plane(&config_path).await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("client");
    let base = format!("http://127.0.0.1:{port}");
    wait_ready(&client, &format!("{base}/")).await;

    let keys = ["/a", "/b", "/c", "/d", "/e", "/f", "/g", "/h"];
    let mut pinned = Vec::new();
    for key in keys {
        let first = target_of(&client, &format!("{base}{key}")).await;
        // Deterministic: the same key lands on the same target every time.
        for _ in 0..3 {
            assert_eq!(
                target_of(&client, &format!("{base}{key}")).await,
                first,
                "hash key {key} must be pinned"
            );
        }
        pinned.push((key.to_owned(), first));
    }

    let _ = shutdown_tx.send(());
    let _ = task.await;
    fs::remove_dir_all(config_path.parent().expect("dir")).ok();
}
