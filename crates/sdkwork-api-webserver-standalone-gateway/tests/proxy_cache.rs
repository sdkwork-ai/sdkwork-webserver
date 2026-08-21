//! End-to-end proxy_cache: hit after fill, and stale serve on upstream 5xx.

use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use axum::{
    body::Body,
    http::{Response, StatusCode},
    routing::get,
    Router,
};
use serde_json::json;
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

fn write_config(listener_port: u16, upstream_port: u16, cache_dir: &PathBuf) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sdkwork-proxy-cache-test-{}-{}",
        std::process::id(),
        listener_port
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("config.json");
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-proxy-cache-test",
        "proxyCache": {
            "enabled": true,
            "maxEntries": 64,
            "maxObjectBytes": 65536,
            "defaultTtlSeconds": 2,
            "staleTtlSeconds": 30,
            "diskPath": cache_dir.to_string_lossy()
        },
        "limits": {
            "requestTimeoutMs": 5000,
            "drainTimeoutMs": 1000,
            "maxConnections": 32
        },
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": listener_port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "host"
        }],
        "upstreams": [{
            "id": "origin",
            "targets": [{ "url": format!("http://127.0.0.1:{upstream_port}") }],
            "loadBalancing": "round-robin",
            "addressPolicy": { "allowedCidrs": ["127.0.0.0/8", "::1/128"] }
        }],
        "resources": [{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "origin",
            "stripPrefix": false,
            "requestSetHeaders": ["Host $host", "X-Forwarded-Proto $scheme"]
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
    fs::write(&path, serde_json::to_vec_pretty(&config).expect("serialize")).expect("write");
    path
}

async fn wait_ready(client: &reqwest::Client, url: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match client
            .get(url)
            .header("host", "cache.localhost")
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
async fn proxy_cache_serves_hit_and_stale_on_upstream_failure() {
    let hits = Arc::new(AtomicUsize::new(0));
    let fail = Arc::new(AtomicUsize::new(0));
    let hits_clone = Arc::clone(&hits);
    let fail_clone = Arc::clone(&fail);
    let upstream = Router::new().route(
        "/item",
        get(move || {
            let hits = Arc::clone(&hits_clone);
            let fail = Arc::clone(&fail_clone);
            async move {
                if fail.load(Ordering::SeqCst) > 0 {
                    return Response::builder()
                        .status(StatusCode::BAD_GATEWAY)
                        .body(Body::from("upstream-down"))
                        .expect("response");
                }
                hits.fetch_add(1, Ordering::SeqCst);
                Response::builder()
                    .status(StatusCode::OK)
                    .header("cache-control", "public, max-age=2")
                    .header("content-type", "text/plain")
                    .body(Body::from("cached-body"))
                    .expect("response")
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("upstream bind");
    let upstream_port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        axum::serve(listener, upstream).await.expect("upstream");
    });

    let listener_port = free_port();
    let cache_dir = std::env::temp_dir().join(format!(
        "sdkwork-proxy-cache-disk-{}-{}",
        std::process::id(),
        listener_port
    ));
    fs::create_dir_all(&cache_dir).expect("cache dir");
    let config_path = write_config(listener_port, upstream_port, &cache_dir);
    let app = load_and_compile_webserver_config(&config_path).expect("compile");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let join = tokio::spawn(async move {
        run_data_plane_until(app, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client");
    let base = format!("http://127.0.0.1:{listener_port}/item");
    wait_ready(&client, &base).await;

    let first = client
        .get(&base)
        .header("host", "cache.localhost")
        .send()
        .await
        .expect("first");
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(first.text().await.expect("body"), "cached-body");
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    let second = client
        .get(&base)
        .header("host", "cache.localhost")
        .send()
        .await
        .expect("second");
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(second.text().await.expect("body"), "cached-body");
    assert_eq!(hits.load(Ordering::SeqCst), 1, "second request must be a cache hit");

    // Expire freshness, force upstream 5xx, expect stale body.
    tokio::time::sleep(Duration::from_millis(2_200)).await;
    fail.store(1, Ordering::SeqCst);
    let stale = client
        .get(&base)
        .header("host", "cache.localhost")
        .send()
        .await
        .expect("stale");
    assert_eq!(stale.status(), StatusCode::OK);
    assert_eq!(stale.text().await.expect("body"), "cached-body");

    let _ = shutdown_tx.send(());
    join.await.expect("join").expect("runtime");
}
