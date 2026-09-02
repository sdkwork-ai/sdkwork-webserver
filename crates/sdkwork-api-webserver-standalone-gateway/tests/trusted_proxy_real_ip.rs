use std::{
    fs,
    net::{SocketAddr, TcpListener as StdTcpListener},
    path::Path,
    time::Duration,
};

use axum::{
    body::Body,
    http::{HeaderMap, Response, StatusCode, Version},
    routing::any,
    Router,
};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use sdkwork_api_webserver_standalone_gateway::{run_data_plane_from_config_until, DataPlaneError};
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
    time::timeout,
};

struct Upstream {
    address: SocketAddr,
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

fn available_port() -> u16 {
    let listener = StdTcpListener::bind("127.0.0.1:0").expect("reserve gateway port");
    listener.local_addr().expect("gateway address").port()
}

async fn spawn_upstream(label: &'static str) -> Upstream {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind real-IP upstream");
    let address = listener.local_addr().expect("real-IP upstream address");
    let (shutdown, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let app = Router::new().fallback(any(move |headers: HeaderMap| async move {
            let forwarded_for = headers
                .get("x-forwarded-for")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("missing");
            Response::new(Body::from(format!("{label}|{forwarded_for}")))
        }));
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("serve real-IP upstream");
    });
    Upstream {
        address,
        shutdown,
        task,
    }
}

fn write_self_signed_certificate(directory: &Path) {
    let mut params =
        CertificateParams::new(vec!["localhost".to_owned(), "test.localhost".to_owned()])
            .expect("certificate parameters");
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, "localhost");
    let key = KeyPair::generate().expect("generate key");
    let certificate = params.self_signed(&key).expect("generate certificate");
    fs::write(directory.join("localhost.pem"), certificate.pem()).expect("write certificate");
    fs::write(directory.join("localhost.key"), key.serialize_pem()).expect("write private key");
}

fn trusted_proxy_config(
    http_port: u16,
    https_port: u16,
    upstreams: &[Upstream],
    recursive: bool,
    trusted_cidrs: &[&str],
    load_balancing: &str,
) -> Value {
    let targets = upstreams
        .iter()
        .map(|upstream| json!({"url": format!("http://127.0.0.1:{}", upstream.address.port())}))
        .collect::<Vec<_>>();
    json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "trusted-proxy-real-ip-test",
        "limits": {
            "maxConnections": 64,
            "maxConcurrentRequests": 32,
            "requestTimeoutMs": 2_000,
            "drainTimeoutMs": 1_000
        },
        "deployment": {
            "drainTimeoutMs": 1_000,
            "reload": {"mode": "watch", "pollIntervalMs": 100}
        },
        "listeners": [
            {
                "id": "http",
                "bind": "127.0.0.1",
                "port": http_port,
                "protocols": ["http1"],
                "defaultVirtualHostRef": "test-host",
                "maxConnections": 32,
                "trustedProxy": {
                    "trustedCidrs": trusted_cidrs,
                    "header": "x-forwarded-for",
                    "recursive": recursive,
                    "maxHops": 4,
                    "maxHeaderBytes": 64
                }
            },
            {
                "id": "https",
                "bind": "127.0.0.1",
                "port": https_port,
                "protocols": ["http1", "http2"],
                "tlsPolicyRef": "tls",
                "defaultVirtualHostRef": "test-host",
                "maxConnections": 32,
                "trustedProxy": {
                    "trustedCidrs": trusted_cidrs,
                    "header": "x-forwarded-for",
                    "recursive": recursive,
                    "maxHops": 4,
                    "maxHeaderBytes": 64
                }
            }
        ],
        "certificates": [{
            "id": "cert",
            "serverNames": ["localhost", "test.localhost"],
            "source": {
                "type": "protected-file",
                "certificateFile": "localhost.pem",
                "privateKeyFile": "localhost.key"
            }
        }],
        "tlsPolicies": [{
            "id": "tls",
            "certificateRef": "cert",
            "minimumVersion": "tls1.2",
            "maximumVersion": "tls1.3",
            "alpn": ["h2", "http/1.1"]
        }],
        "resources": [{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "origin"
        }],
        "upstreams": [{
            "id": "origin",
            "loadBalancing": load_balancing,
            "targets": targets,
            "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]},
            "connectTimeoutMs": 1_000,
            "requestTimeoutMs": 2_000,
            "maxConnections": 8,
            "maxIdleConnections": 8,
            "maxInFlightRequests": 16
        }],
        "virtualHosts": [{
            "id": "test-host",
            "listenerRefs": ["http", "https"],
            "serverNames": ["test.localhost", "localhost"],
            "routes": [{
                "id": "proxy-route",
                "match": {"pathType": "prefix", "path": "/"},
                "resourceRef": "proxy"
            }]
        }]
    })
}

fn write_config(path: &Path, config: &Value) {
    fs::write(
        path,
        serde_json::to_vec_pretty(config).expect("serialize real-IP config"),
    )
    .expect("write real-IP config");
}

fn spawn_gateway(path: &Path) -> (oneshot::Sender<()>, JoinHandle<Result<(), DataPlaneError>>) {
    let path = path.to_path_buf();
    let (shutdown, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_from_config_until(path, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    (shutdown, task)
}

async fn wait_for_gateway(port: u16) {
    timeout(Duration::from_secs(5), async {
        loop {
            if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("gateway becomes reachable");
}

async fn get(
    client: &reqwest::Client,
    url: &str,
    host: &str,
    forwarded_for: &str,
) -> reqwest::Response {
    client
        .get(url)
        .header("host", host)
        .header("x-forwarded-for", forwarded_for)
        .send()
        .await
        .expect("real-IP request")
}

async fn wait_for_body(
    client: &reqwest::Client,
    url: &str,
    host: &str,
    forwarded_for: &str,
    expected: &str,
) {
    timeout(Duration::from_secs(5), async {
        loop {
            let response = get(client, url, host, forwarded_for).await;
            if response.status() == StatusCode::OK
                && response.text().await.expect("read watched response") == expected
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("Watch publishes trusted-proxy policy");
}

async fn stop_gateway(shutdown: oneshot::Sender<()>, task: JoinHandle<Result<(), DataPlaneError>>) {
    shutdown.send(()).expect("stop gateway");
    timeout(Duration::from_secs(3), task)
        .await
        .expect("gateway stops")
        .expect("gateway task joins")
        .expect("gateway stops cleanly");
}

async fn stop_upstream(upstream: Upstream) {
    upstream.shutdown.send(()).expect("stop upstream");
    timeout(Duration::from_secs(3), upstream.task)
        .await
        .expect("upstream stops")
        .expect("upstream task joins");
}

#[tokio::test]
async fn trusted_policy_is_bounded_across_http_https_h2_and_watch_generations() {
    let upstream = spawn_upstream("origin").await;
    let directory = TempDir::new().expect("create config directory");
    write_self_signed_certificate(directory.path());
    let path = directory.path().join("sdkwork.webserver.config.json");
    let http_port = available_port();
    let https_port = available_port();
    let mut config = trusted_proxy_config(
        http_port,
        https_port,
        std::slice::from_ref(&upstream),
        true,
        &["127.0.0.0/8", "10.0.0.0/8"],
        "round-robin",
    );
    write_config(&path, &config);
    let (shutdown, task) = spawn_gateway(&path);
    wait_for_gateway(http_port).await;
    wait_for_gateway(https_port).await;

    let http = reqwest::Client::builder().no_proxy().build().expect("client");
    let https = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("build HTTPS client");
    let chain = "203.0.113.20, 10.0.0.7, 10.0.0.8";
    let http_url = format!("http://127.0.0.1:{http_port}/inspect");
    let https_url = format!("https://localhost:{https_port}/inspect");

    let response = get(&http, &http_url, "test.localhost", chain).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.text().await.unwrap(), "origin|203.0.113.20");

    let response = get(&https, &https_url, "localhost", chain).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.version(), Version::HTTP_2);
    assert_eq!(response.text().await.unwrap(), "origin|203.0.113.20");

    for invalid in [
        "invalid, 203.0.113.20, 10.0.0.8",
        "203.0.113.1, 10.0.0.1, 10.0.0.2, 10.0.0.3, 10.0.0.4",
        "203.0.113.123456789012345678901234567890123456789012345678901234567890",
    ] {
        let response = get(&http, &http_url, "test.localhost", invalid).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.text().await.unwrap(),
            "forwarded client identity is invalid\n"
        );
    }

    let duplicate = http
        .get(&http_url)
        .header("host", "test.localhost")
        .header("x-forwarded-for", "203.0.113.20")
        .header("x-forwarded-for", "203.0.113.21")
        .send()
        .await
        .expect("duplicate forwarding request");
    assert_eq!(duplicate.status(), StatusCode::BAD_REQUEST);

    config["listeners"][0]["trustedProxy"]["recursive"] = json!(false);
    config["listeners"][1]["trustedProxy"]["recursive"] = json!(false);
    write_config(&path, &config);
    wait_for_body(&http, &http_url, "test.localhost", chain, "origin|10.0.0.8").await;

    config["listeners"][0]["trustedProxy"]["trustedCidrs"] = json!(["10.0.0.0/8"]);
    config["listeners"][1]["trustedProxy"]["trustedCidrs"] = json!(["10.0.0.0/8"]);
    write_config(&path, &config);
    wait_for_body(
        &http,
        &http_url,
        "test.localhost",
        "198.51.100.99",
        "origin|127.0.0.1",
    )
    .await;

    stop_gateway(shutdown, task).await;
    stop_upstream(upstream).await;
}

#[tokio::test]
async fn trusted_effective_ip_controls_stable_ip_hash_affinity() {
    let first = spawn_upstream("first").await;
    let second = spawn_upstream("second").await;
    let upstreams = [first, second];
    let directory = TempDir::new().expect("create config directory");
    write_self_signed_certificate(directory.path());
    let path = directory.path().join("sdkwork.webserver.config.json");
    let http_port = available_port();
    let https_port = available_port();
    let config = trusted_proxy_config(
        http_port,
        https_port,
        &upstreams,
        false,
        &["127.0.0.0/8"],
        "ip-hash",
    );
    write_config(&path, &config);
    let (shutdown, task) = spawn_gateway(&path);
    wait_for_gateway(http_port).await;
    let client = reqwest::Client::builder().no_proxy().build().expect("client");
    let url = format!("http://127.0.0.1:{http_port}/affinity");

    let first_response = get(&client, &url, "test.localhost", "192.0.2.1")
        .await
        .text()
        .await
        .unwrap();
    let first_repeat = get(&client, &url, "test.localhost", "192.0.2.1")
        .await
        .text()
        .await
        .unwrap();
    let second_response = get(&client, &url, "test.localhost", "192.0.3.1")
        .await
        .text()
        .await
        .unwrap();
    let second_repeat = get(&client, &url, "test.localhost", "192.0.3.1")
        .await
        .text()
        .await
        .unwrap();

    assert_eq!(first_response, first_repeat);
    assert_eq!(second_response, second_repeat);
    assert_ne!(
        first_response.split_once('|').unwrap().0,
        second_response.split_once('|').unwrap().0
    );
    assert!(first_response.ends_with("|192.0.2.1"));
    assert!(second_response.ends_with("|192.0.3.1"));

    stop_gateway(shutdown, task).await;
    let [first, second] = upstreams;
    stop_upstream(first).await;
    stop_upstream(second).await;
}
