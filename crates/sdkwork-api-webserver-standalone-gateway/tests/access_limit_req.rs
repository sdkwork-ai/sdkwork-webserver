//! Location allow/deny ACL and limit_req execution.

use std::{fs, net::TcpListener, path::PathBuf, time::Duration};

use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::json;
use tokio::sync::oneshot;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

fn write_config(
    port: u16,
    access: serde_json::Value,
    limit_req_zones: serde_json::Value,
    limit_req: serde_json::Value,
) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sdkwork-access-limit-test-{}-{}",
        std::process::id(),
        port
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("config.json");
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-access-limit-test",
        "limitReqZones": limit_req_zones,
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
            "id": "ok",
            "type": "respond",
            "status": 200,
            "contentType": "text/plain; charset=utf-8",
            "body": "ok"
        }],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["http"],
            "serverNames": ["acl.localhost"],
            "routes": [{
                "id": "root",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "ok",
                "access": access,
                "limitReq": limit_req
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

async fn wait_ready(client: &reqwest::Client, url: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match client.get(url).header("host", "acl.localhost").send().await {
            Ok(_) => return,
            Err(error) if tokio::time::Instant::now() < deadline => {
                let _ = error;
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => panic!("ready wait failed: {error}"),
        }
    }
}

#[tokio::test]
async fn allow_deny_rejects_unmatched_clients() {
    let port = free_port();
    let config_path = write_config(
        port,
        json!([
            { "action": "allow", "network": "10.0.0.0/8" },
            { "action": "deny", "network": "all" }
        ]),
        json!([]),
        json!([]),
    );
    let app = load_and_compile_webserver_config(&config_path).expect("compile");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let join = tokio::spawn(async move {
        run_data_plane_until(app, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    let client = reqwest::Client::builder().no_proxy().build().expect("client");
    let url = format!("http://127.0.0.1:{port}/");
    wait_ready(&client, &url).await;
    // Loopback is outside 10.0.0.0/8 → deny all.
    let response = client
        .get(&url)
        .header("host", "acl.localhost")
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
    let _ = shutdown_tx.send(());
    join.await.expect("join").expect("runtime");
}

#[tokio::test]
async fn limit_req_returns_503_beyond_burst() {
    let port = free_port();
    let config_path = write_config(
        port,
        json!([]),
        json!([{
            "name": "one",
            "key": "$binary_remote_addr",
            "maxKeys": 32,
            "ratePerSecond": 1.0
        }]),
        json!([{ "zone": "one", "burst": 1, "nodelay": true }]),
    );
    let app = load_and_compile_webserver_config(&config_path).expect("compile");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let join = tokio::spawn(async move {
        run_data_plane_until(app, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    let client = reqwest::Client::builder().no_proxy().build().expect("client");
    let url = format!("http://127.0.0.1:{port}/");
    wait_ready(&client, &url).await;
    // Drain the readiness probe's token at 1r/s before asserting burst.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let first = client
        .get(&url)
        .header("host", "acl.localhost")
        .send()
        .await
        .expect("first");
    let second = client
        .get(&url)
        .header("host", "acl.localhost")
        .send()
        .await
        .expect("second");
    let third = client
        .get(&url)
        .header("host", "acl.localhost")
        .send()
        .await
        .expect("third");
    assert_eq!(first.status(), reqwest::StatusCode::OK);
    assert_eq!(second.status(), reqwest::StatusCode::OK);
    assert_eq!(third.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    let _ = shutdown_tx.send(());
    join.await.expect("join").expect("runtime");
}
