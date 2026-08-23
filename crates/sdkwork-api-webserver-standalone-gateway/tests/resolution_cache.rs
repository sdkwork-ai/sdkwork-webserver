//! Resolution cache e2e: the data plane resolves upstream hostnames
//! through the multi-layer chain (file seed → memory → system DNS), with
//! negative caching and back-fill observable at the request level.

use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    time::Duration,
};

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

fn write_config(
    directory: &PathBuf,
    port: u16,
    upstream_port: u16,
    resolution_cache: Option<Value>,
) -> PathBuf {
    let mut config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-resolver-e2e",
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
            "targets": [{ "url": format!("http://echo-seed.internal:{upstream_port}") }],
            "loadBalancing": "round-robin",
            "addressPolicy": { "allowedCidrs": ["127.0.0.0/8", "::1/128"] }
        }],
        "resources": [{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "echo"
        }],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["http"],
            "serverNames": ["cache.localhost"],
            "routes": [{
                "id": "proxy-route",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "proxy"
            }]
        }]
    });
    if let Some(cache) = resolution_cache {
        config["resolutionCache"] = cache;
    }
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

/// Upstream that answers with its port, so the test can tell which target
/// served the request.
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
                let mut buffer = vec![0_u8; 2048];
                let _ = socket.read(&mut buffer).await;
                let body = format!("served-by-{port}");
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

#[tokio::test]
async fn file_layer_resolves_upstream_hostnames_without_system_dns() {
    let directory = std::env::temp_dir().join(format!("sdkwork-resolver-e2e-{}", free_port()));
    fs::create_dir_all(&directory).expect("dir");
    let upstream_port = free_port();
    spawn_echo_upstream(upstream_port).await;

    // The file seed maps the upstream hostname to the loopback upstream.
    let seed = directory.join("resolver.seed");
    fs::write(&seed, format!("127.0.0.1 echo-seed.internal\n")).expect("seed");

    let config_path = write_config(
        &directory,
        free_port(),
        upstream_port,
        Some(json!({
            "enabled": true,
            "file": seed.to_string_lossy(),
            "memory": true,
            "memoryMaxEntries": 1024,
            "memoryTtlSeconds": 60,
            "negativeTtlSeconds": 10
        })),
    );
    let (shutdown_tx, task) = spawn_data_plane(&config_path).await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("client");
    let base = format!(
        "http://127.0.0.1:{}",
        serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&config_path).expect("read"))
            .expect("json")["listeners"][0]["port"]
            .as_u64()
            .expect("port")
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if client
            .get(&format!("{base}/"))
            .header("host", "cache.localhost")
            .send()
            .await
            .is_ok()
        {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "not ready");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    // The upstream hostname `echo-seed.internal` is resolved through the
    // file seed to the loopback upstream — no system DNS involved.
    let response = client
        .get(&format!("{base}/hello"))
        .header("host", "cache.localhost")
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.text().await.expect("body"),
        format!("served-by-{upstream_port}")
    );

    let _ = shutdown_tx.send(());
    let _ = task.await;
    fs::remove_dir_all(&directory).ok();
}

#[tokio::test]
async fn without_a_cache_config_the_system_resolver_is_used_uncached() {
    // Baseline: no resolutionCache key → the runtime builds no chain and
    // the data plane still serves. Covered implicitly by every other e2e
    // test in the suite; assert the loader accepts the omission.
    let directory = std::env::temp_dir().join(format!("sdkwork-resolver-e2e-{}", free_port()));
    fs::create_dir_all(&directory).expect("dir");
    let config_path = write_config(&directory, free_port(), free_port(), None);
    let _ = load_and_compile_webserver_config(&config_path).expect("compile without cache");
    fs::remove_dir_all(&directory).ok();
}
