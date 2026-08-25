//! `secure_link` verification execution: `secure_link_secret` URI rewrites
//! and `secure_link`/`secure_link_md5` query digests with expiry.

use std::{fs, net::TcpListener, path::PathBuf, time::Duration};

use serde_json::{json, Value};
use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::{load_and_compile_webserver_config, md5_hex};
use tokio::sync::oneshot;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

fn write_config(port: u16, route: Value, resource: Option<Value>, root: Option<&str>) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sdkwork-secure-link-test-{}-{}",
        std::process::id(),
        port
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("config.json");
    let mut resources = Vec::new();
    if let Some(resource) = resource {
        resources.push(resource);
    }
    if let Some(root) = root {
        resources.push(json!({
            "id": "files",
            "type": "static",
            "root": root,
            "indexFiles": ["index.html"],
            "followSymlinks": false,
            "stripPrefix": true
        }));
    }
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-secure-link-test",
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
        "resources": resources,
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["http"],
            "serverNames": ["link.localhost"],
            "routes": [route]
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
            .header("host", "link.localhost")
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
async fn secure_link_secret_rewrites_and_serves_the_stripped_uri() {
    let port = free_port();
    let config_dir = std::env::temp_dir().join(format!(
        "sdkwork-secure-link-test-{}-{}",
        std::process::id(),
        port
    ));
    let static_root = config_dir.join("public");
    fs::create_dir_all(&static_root).expect("static dir");
    fs::write(static_root.join("report.pdf"), "PDF-DATA").expect("static file");
    fs::write(static_root.join("index.html"), "INDEX").expect("index file");

    let secret = "s3cret";
    let hash = md5_hex(format!("{secret}report.pdf").as_bytes());
    let route = json!({
        "id": "files-route",
        "match": { "pathType": "prefix", "path": "/files/" },
        "resourceRef": "files",
        "secureLink": { "mode": "secret", "secret": secret }
    });
    let config_path = write_config(port, route, None, Some("public"));
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
    wait_ready(&client, &format!("{base}/files/report.pdf")).await;

    // Valid link: the hash segment is stripped before static serving.
    let valid = client
        .get(&format!("{base}/files/{hash}/report.pdf"))
        .header("host", "link.localhost")
        .send()
        .await
        .expect("valid link");
    assert_eq!(valid.status(), 200);
    assert_eq!(valid.text().await.expect("body"), "PDF-DATA");

    // Invalid hash is rejected with 403.
    let invalid = client
        .get(&format!("{base}/files/beef/report.pdf"))
        .header("host", "link.localhost")
        .send()
        .await
        .expect("invalid link");
    assert_eq!(invalid.status(), 403);

    let _ = shutdown_tx.send(());
    let _ = server.await;
    fs::remove_dir_all(&config_dir).ok();
}

#[tokio::test]
async fn secure_link_md5_accepts_valid_digests_and_rejects_wrong_or_expired() {
    let port = free_port();
    let secret = "key";
    let template = "$secure_link_expires$uri$remote_addr key";
    let uri = "/links/file.txt";
    let expires = "4102444800"; // 2100-01-01
    let digest = md5_hex(format!("{expires}{uri}127.0.0.1 {secret}").as_bytes());
    let route = json!({
        "id": "links-route",
        "match": { "pathType": "prefix", "path": "/links/" },
        "resourceRef": "ok",
        "secureLink": {
            "mode": "md5",
            "argument": "st",
            "template": template,
            "expiresArgument": "e"
        }
    });
    let resource = json!({
        "id": "ok",
        "type": "respond",
        "status": 200,
        "contentType": "text/plain; charset=utf-8",
        "body": "ok"
    });
    let config_path = write_config(port, route, Some(resource), None);
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
    wait_ready(&client, &format!("{base}/links/file.txt")).await;

    let valid = client
        .get(&format!("{base}{uri}?st={digest}&e={expires}"))
        .header("host", "link.localhost")
        .send()
        .await
        .expect("valid digest");
    assert_eq!(valid.status(), 200);

    let wrong = client
        .get(&format!("{base}{uri}?st=beef&e={expires}"))
        .header("host", "link.localhost")
        .send()
        .await
        .expect("wrong digest");
    assert_eq!(wrong.status(), 403);

    let expired = client
        .get(&format!("{base}{uri}?st={digest}&e=100"))
        .header("host", "link.localhost")
        .send()
        .await
        .expect("expired link");
    assert_eq!(expired.status(), 403);

    let missing = client
        .get(&format!("{base}{uri}"))
        .header("host", "link.localhost")
        .send()
        .await
        .expect("missing digest");
    assert_eq!(missing.status(), 403);

    let _ = shutdown_tx.send(());
    let _ = server.await;
    fs::remove_dir_all(config_path.parent().expect("config dir")).ok();
}
