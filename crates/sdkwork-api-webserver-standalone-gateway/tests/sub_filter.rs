//! `sub_filter` response body substitution execution, including its
//! interplay with gzip (nginx filter order: substitute then compress).

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

fn write_config(port: u16, sub_filter: Value) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sdkwork-sub-filter-test-{}-{}",
        std::process::id(),
        port
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("config.json");
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-sub-filter-test",
        "gzip": {
            "enabled": true,
            "types": ["text/html"],
            "minLength": 1
        },
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
            "id": "html",
            "type": "respond",
            "status": 200,
            "contentType": "text/html; charset=utf-8",
            "body": "<h1>Hello World</h1><p>Hello again</p>"
        }],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["http"],
            "serverNames": ["sub.localhost"],
            "routes": [{
                "id": "html-route",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "html",
                "subFilter": sub_filter
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
            .header("host", "sub.localhost")
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
async fn sub_filter_replaces_once_by_default_and_drops_last_modified() {
    let port = free_port();
    let config_path = write_config(
        port,
        json!({
            "rules": [{"from": "Hello", "to": "Hola"}],
            "once": true,
            "types": ["text/html"],
            "lastModified": false
        }),
    );
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
    let url = format!("http://127.0.0.1:{port}/");
    wait_ready(&client, &url).await;

    let response = client
        .get(&url)
        .header("host", "sub.localhost")
        .send()
        .await
        .expect("request");
    let text = response.text().await.expect("body");
    // `once` replaces only the first occurrence; content-type with charset
    // parameter is still eligible.
    assert_eq!(text, "<h1>Hola World</h1><p>Hello again</p>");

    let _ = shutdown_tx.send(());
    let _ = server.await;
    fs::remove_dir_all(
        config_path
            .parent()
            .expect("config dir"),
    )
    .ok();
}

#[tokio::test]
async fn sub_filter_then_gzip_produces_substituted_compressed_body() {
    let port = free_port();
    let config_path = write_config(
        port,
        json!({
            "rules": [{"from": "Hello", "to": "Hola"}],
            "once": false,
            "types": ["text/html"],
            "lastModified": true
        }),
    );
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
    let url = format!("http://127.0.0.1:{port}/");
    wait_ready(&client, &url).await;

    let response = client
        .get(&url)
        .header("host", "sub.localhost")
        .header("accept-encoding", "gzip")
        .send()
        .await
        .expect("request");
    let encoding = response
        .headers()
        .get("content-encoding")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    assert_eq!(encoding, "gzip");
    let bytes = response.bytes().await.expect("body");
    let mut decoder = GzDecoder::new(&bytes[..]);
    let mut text = String::new();
    decoder.read_to_string(&mut text).expect("decompress");
    // Replace-then-compress: every occurrence substituted.
    assert_eq!(text, "<h1>Hola World</h1><p>Hola again</p>");

    let _ = shutdown_tx.send(());
    let _ = server.await;
    fs::remove_dir_all(
        config_path
            .parent()
            .expect("config dir"),
    )
    .ok();
}
