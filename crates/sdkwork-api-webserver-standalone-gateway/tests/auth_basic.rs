//! Location auth_basic challenge and credential acceptance.

use std::{fs, net::TcpListener, path::PathBuf, time::Duration};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde_json::json;
use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::{apr1_hash, load_and_compile_webserver_config};
use tokio::sync::oneshot;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

fn write_config(port: u16, password_hash: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sdkwork-auth-basic-test-{}-{}",
        std::process::id(),
        port
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("config.json");
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-auth-basic-test",
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
            "serverNames": ["auth.localhost"],
            "routes": [{
                "id": "root",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "ok",
                "authBasic": {
                    "realm": "Restricted",
                    "users": [{
                        "username": "alice",
                        "passwordHash": password_hash
                    }]
                }
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
            .header("host", "auth.localhost")
            .send()
            .await
        {
            Ok(_) => return,
            Err(error) if tokio::time::Instant::now() < deadline => {
                let _ = error;
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => panic!("data plane did not become ready: {error}"),
        }
    }
}

#[tokio::test]
async fn auth_basic_challenges_then_accepts_valid_credentials() {
    let port = free_port();
    let hash = apr1_hash("secret", "testsalt");
    let config_path = write_config(port, &hash);
    let compiled = load_and_compile_webserver_config(&config_path).expect("compile");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let join = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("client");
    let url = format!("http://127.0.0.1:{port}/");
    wait_ready(&client, &url).await;

    let challenge = client
        .get(&url)
        .header("host", "auth.localhost")
        .send()
        .await
        .expect("challenge");
    assert_eq!(challenge.status(), reqwest::StatusCode::UNAUTHORIZED);
    let www = challenge
        .headers()
        .get("www-authenticate")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    assert!(www.contains("Basic realm=\"Restricted\""), "{www}");

    let authorized = client
        .get(&url)
        .header("host", "auth.localhost")
        .header(
            "authorization",
            format!("Basic {}", BASE64.encode("alice:secret")),
        )
        .send()
        .await
        .expect("authorized");
    assert_eq!(authorized.status(), reqwest::StatusCode::OK);
    assert_eq!(authorized.text().await.expect("body"), "ok");

    let rejected = client
        .get(&url)
        .header("host", "auth.localhost")
        .header(
            "authorization",
            format!("Basic {}", BASE64.encode("alice:wrong")),
        )
        .send()
        .await
        .expect("rejected");
    assert_eq!(rejected.status(), reqwest::StatusCode::UNAUTHORIZED);

    let _ = shutdown_tx.send(());
    join.await.expect("join").expect("data plane");
}
