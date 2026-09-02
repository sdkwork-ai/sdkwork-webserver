//! `limit_conn` per-key concurrent connection admission execution.

use std::{fs, net::TcpListener, path::PathBuf, time::Duration};

use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::{json, Value};
use tokio::{io::AsyncReadExt, io::AsyncWriteExt, net::TcpStream, sync::oneshot};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

/// A slow HTTP/1.1 upstream: reads the request, sleeps, then responds.
async fn spawn_slow_upstream(port: u16, delay: Duration) {
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
                let mut buffer = [0_u8; 2048];
                let _ = socket.read(&mut buffer).await;
                tokio::time::sleep(delay).await;
                let _ = socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok",
                    )
                    .await;
            });
        }
    });
}

fn write_config(port: u16, upstream_port: u16) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sdkwork-limit-conn-test-{}-{}",
        std::process::id(),
        port
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("config.json");
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-limit-conn-test",
        "limitConnZones": [{
            "name": "perip",
            "key": "$binary_remote_addr",
            "maxKeys": 1024
        }],
        "limits": {
            "requestTimeoutMs": 10000,
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
            "id": "slow",
            "targets": [{ "url": format!("http://127.0.0.1:{upstream_port}") }],
            "loadBalancing": "round-robin",
            "addressPolicy": { "allowedCidrs": ["127.0.0.0/8", "::1/128"] }
        }],
        "resources": [{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "slow"
        }],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["http"],
            "serverNames": ["conn.localhost"],
            "routes": [{
                "id": "proxy-route",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "proxy",
                "limitConn": [{ "zone": "perip", "maxConnections": 1 }]
            }]
        }]
    });
    fs::write(
        &path,
        serde_json::to_vec_pretty(&config).expect("serialize"),
    )
    .expect("write");
    path
}

#[tokio::test]
async fn rejects_concurrent_connections_per_key_and_releases_on_completion() {
    let port = free_port();
    let upstream_port = free_port();
    spawn_slow_upstream(upstream_port, Duration::from_millis(800)).await;
    let config_path = write_config(port, upstream_port);
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

    // Wait until the data plane answers.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if client
            .get(&url)
            .header("host", "conn.localhost")
            .send()
            .await
            .is_ok()
        {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "data plane not ready"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    // First request holds the lease while the upstream sleeps.
    let first = tokio::spawn({
        let client = client.clone();
        let url = url.clone();
        async move {
            client
                .get(&url)
                .header("host", "conn.localhost")
                .send()
                .await
                .expect("first request")
        }
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Second concurrent request from the same key must be rejected.
    let second = client
        .get(&url)
        .header("host", "conn.localhost")
        .send()
        .await
        .expect("second request");
    assert_eq!(
        second.status(),
        503,
        "concurrent connection must be limited"
    );

    // Complete the first response; the lease releases when the body ends.
    let first = first.await.expect("join");
    assert_eq!(first.status(), 200);
    let _ = first.bytes().await.expect("first body");
    tokio::time::sleep(Duration::from_millis(100)).await;

    // A later request is admitted again.
    let third = client
        .get(&url)
        .header("host", "conn.localhost")
        .send()
        .await
        .expect("third request");
    assert_eq!(
        third.status(),
        200,
        "slot must be released after completion"
    );

    let _ = shutdown_tx.send(());
    let _ = server.await;
    let _ = TcpStream::connect(("127.0.0.1", upstream_port)).await;
    fs::remove_dir_all(config_path.parent().expect("config dir")).ok();
}
