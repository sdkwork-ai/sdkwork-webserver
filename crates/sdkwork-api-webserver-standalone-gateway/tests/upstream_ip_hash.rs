use std::{
    fs,
    net::{SocketAddr, TcpListener as StdTcpListener},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use axum::{
    body::Body,
    http::{Response, StatusCode},
    routing::any,
    Router,
};
use sdkwork_api_webserver_standalone_gateway::{run_data_plane_until, DataPlaneError};
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::json;
use tempfile::TempDir;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
    time::timeout,
};

struct LabeledUpstream {
    address: SocketAddr,
    failing: Arc<AtomicBool>,
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

fn available_port() -> u16 {
    let listener = StdTcpListener::bind("127.0.0.1:0").expect("reserve gateway port");
    listener.local_addr().expect("gateway address").port()
}

async fn spawn_upstream(label: &'static str) -> LabeledUpstream {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind labeled upstream");
    let address = listener.local_addr().expect("labeled upstream address");
    let failing = Arc::new(AtomicBool::new(false));
    let handler_failing = failing.clone();
    let (shutdown, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let app = Router::new().fallback(any(move || {
            let failing = handler_failing.clone();
            async move {
                if failing.load(Ordering::Acquire) {
                    Response::builder()
                        .status(StatusCode::SERVICE_UNAVAILABLE)
                        .body(Body::from("failing"))
                        .expect("failure response")
                } else {
                    Response::new(Body::from(label))
                }
            }
        }));
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("serve labeled upstream");
    });
    LabeledUpstream {
        address,
        failing,
        shutdown,
        task,
    }
}

fn write_config(path: &Path, port: u16, targets: [SocketAddr; 2]) {
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "ip-hash-test",
        "limits": {
            "maxConcurrentRequests": 32,
            "requestTimeoutMs": 2_000,
            "drainTimeoutMs": 1_000,
            "maxConnections": 64
        },
        "deployment": {"drainTimeoutMs": 1_000},
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "test-host",
            "maxConnections": 32
        }],
        "resources": [{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "affinity-upstream"
        }],
        "upstreams": [{
            "id": "affinity-upstream",
            "loadBalancing": "ip-hash",
            "targets": targets.map(|address| json!({
                "url": format!("http://127.0.0.1:{}", address.port())
            })),
            "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]},
            "connectTimeoutMs": 1_000,
            "requestTimeoutMs": 2_000,
            "maxConnections": 8,
            "maxIdleConnections": 8,
            "maxInFlightRequests": 16,
            "passiveHealth": {
                "failureThreshold": 1,
                "ejectionTimeMs": 100,
                "failureStatuses": [503]
            }
        }],
        "virtualHosts": [{
            "id": "test-host",
            "listenerRefs": ["http"],
            "serverNames": ["test.localhost"],
            "routes": [{
                "id": "proxy-route",
                "match": {"pathType": "prefix", "path": "/"},
                "resourceRef": "proxy"
            }]
        }]
    });
    fs::write(
        path,
        serde_json::to_vec_pretty(&config).expect("serialize ip-hash config"),
    )
    .expect("write ip-hash config");
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

async fn request(client: &reqwest::Client, port: u16, forwarded_for: &str) -> (StatusCode, String) {
    let response = client
        .get(format!("http://127.0.0.1:{port}/affinity"))
        .header("host", "test.localhost")
        .header("x-forwarded-for", forwarded_for)
        .send()
        .await
        .expect("ip-hash request");
    let status = response.status();
    let body = response.text().await.expect("ip-hash response Body");
    (status, body)
}

async fn stop_upstream(upstream: LabeledUpstream) {
    upstream.shutdown.send(()).expect("stop upstream");
    timeout(Duration::from_secs(3), upstream.task)
        .await
        .expect("upstream stops")
        .expect("upstream task joins");
}

#[tokio::test]
async fn direct_peer_affinity_ignores_spoofed_forwarding_and_recovers_deterministically() {
    let primary = spawn_upstream("primary").await;
    let secondary = spawn_upstream("secondary").await;
    let directory = TempDir::new().expect("create config directory");
    let path = directory.path().join("sdkwork.webserver.config.json");
    let port = available_port();
    write_config(&path, port, [primary.address, secondary.address]);
    let compiled = load_and_compile_webserver_config(&path).expect("compile ip-hash config");
    let (shutdown, shutdown_rx) = oneshot::channel();
    let gateway: JoinHandle<Result<(), DataPlaneError>> = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    wait_for_gateway(port).await;
    let client = reqwest::Client::new();

    for spoofed in [
        "203.0.113.1",
        "198.51.100.254",
        "2001:db8::7",
        "10.0.0.1, 192.0.2.1",
    ] {
        assert_eq!(
            request(&client, port, spoofed).await,
            (StatusCode::OK, "primary".to_owned()),
            "untrusted forwarding metadata must not alter direct-peer affinity"
        );
    }

    primary.failing.store(true, Ordering::Release);
    assert_eq!(
        request(&client, port, "203.0.113.10").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        request(&client, port, "203.0.113.11").await,
        (StatusCode::OK, "secondary".to_owned())
    );

    primary.failing.store(false, Ordering::Release);
    tokio::time::sleep(Duration::from_millis(125)).await;
    assert_eq!(
        request(&client, port, "198.51.100.1").await,
        (StatusCode::OK, "primary".to_owned()),
        "the original direct-peer mapping must return after half-open recovery"
    );

    shutdown.send(()).expect("stop gateway");
    timeout(Duration::from_secs(3), gateway)
        .await
        .expect("gateway stops")
        .expect("gateway task joins")
        .expect("gateway stops cleanly");
    stop_upstream(primary).await;
    stop_upstream(secondary).await;
}
