//! Unified config source behavior equivalence e2e: the same logical
//! configuration authored as nginx.conf, single-file TOML, and JSON serves
//! identical responses through the unified loader.

use std::{fs, net::TcpListener, path::PathBuf, time::Duration};

use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::{ConfigLoadOptions, WebServerConfigLoader};
use tokio::sync::oneshot;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

/// The logical configuration: one proxy route to an echo upstream plus one
/// fixed response, served on `port`.
fn write_nginx_conf(directory: &PathBuf, port: u16, upstream_port: u16) -> PathBuf {
    let path = directory.join("nginx.conf");
    fs::write(
        &path,
        format!(
            "http {{\n    upstream echo {{\n        server 127.0.0.1:{upstream_port};\n    }}\n\n    server {{\n        listen {port};\n        server_name src.localhost;\n        location /api/ {{\n            proxy_pass http://echo;\n        }}\n        location = /healthz {{\n            return 200 \"healthy\";\n        }}\n        location / {{\n            return 404 \"miss\";\n        }}\n    }}\n}}\n"
        ),
    )
    .expect("write nginx.conf");
    path
}

fn write_toml(directory: &PathBuf, port: u16, upstream_port: u16) -> PathBuf {
    let path = directory.join("server.toml");
    fs::write(
        &path,
        format!(
            "[[http.upstream]]\nname = \"echo\"\n\n[[http.upstream.target]]\naddress = \"127.0.0.1:{upstream_port}\"\n\n[[http.server]]\nlisten = [\"{port}\"]\nserverName = [\"src.localhost\"]\n\n[[http.server.location]]\nmatch = \"/api/\"\nproxyPass = \"http://echo\"\n\n[[http.server.location]]\nmatch = \"= /healthz\"\nreturnStatus = 200\nreturnBody = \"healthy\"\n\n[[http.server.location]]\nmatch = \"/\"\nreturnStatus = 404\nreturnBody = \"miss\"\n"
        ),
    )
    .expect("write server.toml");
    path
}

fn write_json(directory: &PathBuf, port: u16, upstream_port: u16) -> PathBuf {
    let path = directory.join("config.json");
    let config = serde_json::json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-src-e2e",
        "limits": { "requestTimeoutMs": 5000, "drainTimeoutMs": 1000, "maxConnections": 32 },
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
        "resources": [
            { "id": "proxy", "type": "proxy", "upstreamRef": "echo" },
            { "id": "healthz", "type": "respond", "status": 200, "contentType": "text/plain; charset=utf-8", "body": "healthy" },
            { "id": "miss", "type": "respond", "status": 404, "contentType": "text/plain; charset=utf-8", "body": "miss" }
        ],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["http"],
            "serverNames": ["src.localhost"],
            "routes": [
                { "id": "api", "match": { "pathType": "prefix", "path": "/api/" }, "resourceRef": "proxy" },
                { "id": "healthz-route", "match": { "pathType": "exact", "path": "/healthz" }, "resourceRef": "healthz" },
                { "id": "root", "match": { "pathType": "prefix", "path": "/" }, "resourceRef": "miss" }
            ]
        }]
    });
    fs::write(&path, serde_json::to_vec_pretty(&config).expect("serialize")).expect("write");
    path
}

/// Echo upstream answering with the request path.
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
                let Ok(read) = socket.read(&mut buffer).await else {
                    return;
                };
                let head = String::from_utf8_lossy(&buffer[..read]);
                let path = head
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_owned();
                let body = format!("echo:{path}");
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

async fn spawn_data_plane(
    path: &PathBuf,
) -> (oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let loader = WebServerConfigLoader::new();
    let compiled = loader
        .load_and_compile(path, &ConfigLoadOptions::default())
        .expect("unified loader compiles the source");
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
async fn nginx_toml_and_json_sources_serve_identical_responses() {
    let upstream_port = free_port();
    spawn_echo_upstream(upstream_port).await;
    let directory = std::env::temp_dir().join(format!("sdkwork-src-e2e-{}", free_port()));
    fs::create_dir_all(&directory).expect("dir");

    let sources = [
        ("nginx", write_nginx_conf(&directory, free_port(), upstream_port)),
        ("toml", write_toml(&directory, free_port(), upstream_port)),
        ("json", write_json(&directory, free_port(), upstream_port)),
    ];

    for (name, path) in &sources {
        let (shutdown_tx, task) = spawn_data_plane(path).await;
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client");
        let port = {
            let text = fs::read_to_string(path).expect("read");
            if name == &"json" {
                let config: serde_json::Value = serde_json::from_str(&text).expect("json");
                config["listeners"][0]["port"].as_u64().expect("port") as u16
            } else if name == &"toml" {
                text.lines()
                    .find_map(|line| line.strip_prefix("listen = [\""))
                    .and_then(|line| line.split('"').next())
                    .and_then(|value| value.parse::<u16>().ok())
                    .expect("toml port")
            } else {
                text.lines()
                    .find_map(|line| line.trim().strip_prefix("listen "))
                    .and_then(|line| line.split(';').next())
                    .and_then(|value| value.parse::<u16>().ok())
                    .expect("nginx port")
            }
        };
        let base = format!("http://127.0.0.1:{port}");

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if client
                .get(&format!("{base}/healthz"))
                .header("host", "src.localhost")
                .send()
                .await
                .is_ok()
            {
                break;
            }
            assert!(tokio::time::Instant::now() < deadline, "{name} not ready");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        // Identical behavior across all three sources.
        let healthz = client
            .get(&format!("{base}/healthz"))
            .header("host", "src.localhost")
            .send()
            .await
            .expect("healthz");
        assert_eq!(healthz.status(), 200);
        assert_eq!(healthz.text().await.expect("body"), "healthy");

        let proxied = client
            .get(&format!("{base}/api/items"))
            .header("host", "src.localhost")
            .send()
            .await
            .expect("proxy");
        assert_eq!(proxied.status(), 200);
        assert_eq!(proxied.text().await.expect("body"), "echo:/api/items");

        let miss = client
            .get(&format!("{base}/other"))
            .header("host", "src.localhost")
            .send()
            .await
            .expect("miss");
        assert_eq!(miss.status(), 404);
        assert_eq!(miss.text().await.expect("body"), "miss");

        let _ = shutdown_tx.send(());
        let _ = task.await;
    }

    fs::remove_dir_all(&directory).ok();
}
