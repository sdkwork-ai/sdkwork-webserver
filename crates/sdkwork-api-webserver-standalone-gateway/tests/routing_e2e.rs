//! Routing e2e: nginx location selection semantics at the request level —
//! regex locations (`~` / `~*`), `^~` prefix suppression, `rewrite` flags
//! (permanent/redirect/last/break with captures), and `alias` path
//! substitution.

use std::{fs, net::TcpListener, path::PathBuf, time::Duration};

use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::{json, Value};
use tokio::sync::oneshot;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

fn respond_resource(id: &str, body: &str) -> Value {
    json!({
        "id": id,
        "type": "respond",
        "status": 200,
        "contentType": "text/plain; charset=utf-8",
        "body": body
    })
}

fn write_config(
    port: u16,
    resources: Vec<Value>,
    routes: Vec<Value>,
    directory: &PathBuf,
) -> PathBuf {
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-routing-e2e",
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
            "serverNames": ["route.localhost"],
            "routes": routes
        }]
    });
    let path = directory.join("config.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&config).expect("serialize"),
    )
    .expect("write");
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

async fn wait_ready(client: &reqwest::Client, url: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match client
            .get(url)
            .header("host", "route.localhost")
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

async fn body_of(client: &reqwest::Client, url: &str) -> String {
    client
        .get(url)
        .header("host", "route.localhost")
        .send()
        .await
        .expect("request")
        .text()
        .await
        .expect("body")
}

#[tokio::test]
async fn regex_locations_win_over_prefixes_and_prefix_exclusive_suppresses_regex() {
    let port = free_port();
    let directory = std::env::temp_dir().join(format!("sdkwork-routing-e2e-{}", port));
    fs::create_dir_all(&directory).expect("dir");

    // Route table exercising nginx location selection:
    // - prefix /api/  → "prefix"
    // - regex  ~ ^/api/v1/ → "regex-v1" (must beat the /api/ prefix)
    // - regex  ~* /images/.*\.png$ → "regex-png-ci" (case-insensitive)
    // - prefix ^~ /static/ → "static-exclusive" (suppresses regex evaluation)
    // - regex  ~ ^/static/ → "regex-static" (must never be evaluated for /static/)
    // - prefix /        → "root"
    let config_path = write_config(
        port,
        vec![
            respond_resource("prefix", "prefix"),
            respond_resource("regex-v1", "regex-v1"),
            respond_resource("regex-png-ci", "regex-png-ci"),
            respond_resource("static-exclusive", "static-exclusive"),
            respond_resource("regex-static", "regex-static"),
            respond_resource("root", "root"),
        ],
        vec![
            json!({
                "id": "r-prefix",
                "match": { "pathType": "prefix", "path": "/api/" },
                "resourceRef": "prefix"
            }),
            json!({
                "id": "r-regex-v1",
                "match": { "pathType": "regex", "path": "^/api/v1/" },
                "resourceRef": "regex-v1"
            }),
            json!({
                "id": "r-regex-png",
                "match": { "pathType": "regex", "path": "(?i)/images/.*\\.png$" },
                "resourceRef": "regex-png-ci"
            }),
            json!({
                "id": "r-static",
                "match": { "pathType": "prefix-exclusive", "path": "/static/" },
                "resourceRef": "static-exclusive"
            }),
            json!({
                "id": "r-regex-static",
                "match": { "pathType": "regex", "path": "^/static/" },
                "resourceRef": "regex-static"
            }),
            json!({
                "id": "r-root",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "root"
            }),
        ],
        &directory,
    );
    let (shutdown_tx, task) = spawn_data_plane(&config_path).await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("client");
    let base = format!("http://127.0.0.1:{port}");
    wait_ready(&client, &format!("{base}/")).await;

    // Regex beats the longer-declared prefix when both match.
    assert_eq!(
        body_of(&client, &format!("{base}/api/v1/users")).await,
        "regex-v1"
    );
    // Plain prefix still wins for paths only the prefix matches.
    assert_eq!(
        body_of(&client, &format!("{base}/api/other")).await,
        "prefix"
    );
    // Case-insensitive regex matches mixed-case paths.
    assert_eq!(
        body_of(&client, &format!("{base}/images/IMG.PNG")).await,
        "regex-png-ci"
    );
    // ^~ prefix suppresses regex evaluation entirely.
    assert_eq!(
        body_of(&client, &format!("{base}/static/app.js")).await,
        "static-exclusive"
    );
    // Fallback prefix.
    assert_eq!(body_of(&client, &format!("{base}/anything")).await, "root");

    let _ = shutdown_tx.send(());
    let _ = task.await;
    fs::remove_dir_all(&directory).ok();
}

#[tokio::test]
async fn rewrite_flags_and_captures_execute_at_request_time() {
    let port = free_port();
    let directory = std::env::temp_dir().join(format!("sdkwork-routing-e2e-{}", port));
    fs::create_dir_all(&directory).expect("dir");

    // - /old/*: rewrite permanent with a capture → 301 to /new/$1
    // - /temp/*: rewrite redirect → 302
    // - /docs/*: rewrite last → re-routes to the /final/ prefix route
    // - /break/*: rewrite break keeps the current route's resource
    let config_path = write_config(
        port,
        vec![
            respond_resource("new", "new"),
            respond_resource("final", "final"),
            respond_resource("broken", "broken"),
            respond_resource("root", "root"),
        ],
        vec![
            json!({
                "id": "r-old",
                "match": { "pathType": "prefix", "path": "/old/" },
                "resourceRef": "root",
                "rewrite": [{
                    "pattern": "^/old/(.*)$",
                    "replacement": "/new/$1",
                    "flag": "permanent"
                }]
            }),
            json!({
                "id": "r-temp",
                "match": { "pathType": "prefix", "path": "/temp/" },
                "resourceRef": "root",
                "rewrite": [{
                    "pattern": "^/temp/(.*)$",
                    "replacement": "/new/$1",
                    "flag": "redirect"
                }]
            }),
            json!({
                "id": "r-docs",
                "match": { "pathType": "prefix", "path": "/docs/" },
                "resourceRef": "root",
                "rewrite": [{
                    "pattern": "^/docs/(.*)$",
                    "replacement": "/final/$1",
                    "flag": "last"
                }]
            }),
            json!({
                "id": "r-break",
                "match": { "pathType": "prefix", "path": "/break/" },
                "resourceRef": "broken",
                "rewrite": [{
                    "pattern": "^/break/(.*)$",
                    "replacement": "/ignored/$1",
                    "flag": "break"
                }]
            }),
            json!({
                "id": "r-final",
                "match": { "pathType": "prefix", "path": "/final/" },
                "resourceRef": "final"
            }),
            json!({
                "id": "r-root",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "root"
            }),
        ],
        &directory,
    );
    let (shutdown_tx, task) = spawn_data_plane(&config_path).await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");
    let base = format!("http://127.0.0.1:{port}");
    wait_ready(&client, &format!("{base}/")).await;

    // permanent → 301 with the capture substituted.
    let permanent = client
        .get(&format!("{base}/old/page.html"))
        .header("host", "route.localhost")
        .send()
        .await
        .expect("permanent");
    assert_eq!(permanent.status(), 301);
    assert_eq!(
        permanent
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok()),
        Some("/new/page.html")
    );

    // redirect → 302.
    let redirect = client
        .get(&format!("{base}/temp/page.html"))
        .header("host", "route.localhost")
        .send()
        .await
        .expect("redirect");
    assert_eq!(redirect.status(), 302);

    // last → internal re-route to /final/.
    assert_eq!(
        body_of(&client, &format!("{base}/docs/guide")).await,
        "final"
    );

    // break → the current route's resource is served unchanged.
    assert_eq!(body_of(&client, &format!("{base}/break/x")).await, "broken");

    let _ = shutdown_tx.send(());
    let _ = task.await;
    fs::remove_dir_all(&directory).ok();
}

#[tokio::test]
async fn alias_substitutes_the_matched_prefix_before_static_serving() {
    let port = free_port();
    let directory = std::env::temp_dir().join(format!("sdkwork-routing-e2e-{}", port));
    fs::create_dir_all(&directory).expect("dir");
    let static_dir = directory.join("assets");
    fs::create_dir_all(&static_dir).expect("assets dir");
    fs::write(static_dir.join("deep.txt"), "aliased-content").expect("file");

    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-alias-e2e",
        "limits": { "maxConnections": 32 },
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "host"
        }],
        "resources": [{
            "id": "aliased",
            "type": "static",
            "root": "assets",
            "indexFiles": ["index.html"],
            "followSymlinks": false,
            "stripPrefix": true
        }],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["http"],
            "serverNames": ["alias.localhost"],
            "routes": [{
                "id": "alias-route",
                "match": { "pathType": "prefix", "path": "/files/" },
                "resourceRef": "aliased"
            }]
        }]
    });
    let config_path = directory.join("config.json");
    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("serialize"),
    )
    .expect("write");
    let (shutdown_tx, task) = spawn_data_plane(&config_path).await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("client");
    let base = format!("http://127.0.0.1:{port}");
    wait_ready(&client, &format!("{base}/files/")).await;

    // /files/deep.txt → assets/deep.txt (matched prefix stripped, nginx alias).
    let response = client
        .get(&format!("{base}/files/deep.txt"))
        .header("host", "alias.localhost")
        .send()
        .await
        .expect("alias");
    assert_eq!(response.status(), 200);
    assert_eq!(response.text().await.expect("body"), "aliased-content");

    let _ = shutdown_tx.send(());
    let _ = task.await;
    fs::remove_dir_all(&directory).ok();
}
