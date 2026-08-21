//! Response gzip execution for http.gzip / gzipTypes / gzipMinLength.

use std::{fs, net::TcpListener, path::PathBuf, time::Duration};

use flate2::read::GzDecoder;
use serde_json::{json, Value};
use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::load_and_compile_webserver_config;
use std::io::Read;
use tokio::sync::oneshot;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

fn write_config(port: u16, gzip: Value) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sdkwork-gzip-response-test-{}-{}",
        std::process::id(),
        port
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("config.json");
    let body = "x".repeat(200);
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-gzip-test",
        "gzip": gzip,
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
            "id": "json",
            "type": "respond",
            "status": 200,
            "contentType": "application/json",
            "body": body
        }],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["http"],
            "serverNames": ["gzip.localhost"],
            "routes": [{
                "id": "json-route",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "json"
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
            .header("host", "gzip.localhost")
            .send()
            .await
        {
            Ok(_) => return,
            Err(error) if tokio::time::Instant::now() < deadline => {
                let _ = error;
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => panic!("data plane not ready: {error}"),
        }
    }
}

#[tokio::test]
async fn gzip_compresses_configured_json_responses() {
    let port = free_port();
    let config_path = write_config(
        port,
        json!({
            "enabled": true,
            "types": ["application/json"],
            "minLength": 20
        }),
    );
    let compiled = load_and_compile_webserver_config(&config_path).expect("compile");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    let client = reqwest::Client::builder()
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .build()
        .expect("client");
    let url = format!("http://127.0.0.1:{port}/");
    wait_ready(&client, &url).await;
    let response = client
        .get(&url)
        .header("host", "gzip.localhost")
        .header("accept-encoding", "gzip")
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .headers()
            .get("content-encoding")
            .and_then(|value| value.to_str().ok()),
        Some("gzip")
    );
    let compressed = response.bytes().await.expect("body");
    let mut decoder = GzDecoder::new(compressed.as_ref());
    let mut plain = String::new();
    decoder.read_to_string(&mut plain).expect("inflate");
    assert_eq!(plain, "x".repeat(200));
    let _ = shutdown_tx.send(());
    let _ = task.await;
    let _ = fs::remove_dir_all(config_path.parent().expect("dir"));
}

#[tokio::test]
async fn gzip_skips_when_disabled() {
    let port = free_port();
    let config_path = write_config(
        port,
        json!({
            "enabled": false,
            "types": ["application/json"],
            "minLength": 20
        }),
    );
    let compiled = load_and_compile_webserver_config(&config_path).expect("compile");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    let client = reqwest::Client::builder()
        .no_gzip()
        .build()
        .expect("client");
    let url = format!("http://127.0.0.1:{port}/");
    wait_ready(&client, &url).await;
    let response = client
        .get(&url)
        .header("host", "gzip.localhost")
        .header("accept-encoding", "gzip")
        .send()
        .await
        .expect("request");
    assert!(response.headers().get("content-encoding").is_none());
    let body = response.text().await.expect("body");
    assert_eq!(body, "x".repeat(200));
    let _ = shutdown_tx.send(());
    let _ = task.await;
    let _ = fs::remove_dir_all(config_path.parent().expect("dir"));
}
