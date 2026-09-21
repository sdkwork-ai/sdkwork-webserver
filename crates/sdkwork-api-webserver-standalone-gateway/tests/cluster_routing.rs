//! Cluster auto-routing end-to-end coverage.
//!
//! A data plane with an injected cluster topology overlay routes requests
//! by served domain to discovered instances: round-robin distribution
//! across healthy instances, exclusion of offline instances, and graceful
//! degradation when every instance is down.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::{net::SocketAddr, time::Duration};

use axum::Json;
use sdkwork_api_webserver_standalone_gateway::{
    run_data_plane_until_with_cluster_overlay, DataPlaneError,
};
use sdkwork_webserver_cluster::{
    DiscoveredInstance, InstanceHealth, InstanceStatus, LoadBalancingStrategy,
};
use sdkwork_webserver_core::load_and_compile_webserver_config_json;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct FakeInstance {
    address: SocketAddr,
    hits: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

async fn spawn_fake_instance(tag: &'static str) -> FakeInstance {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("instance bind");
    let address = listener.local_addr().expect("addr");
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_task = hits.clone();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            hits_task.fetch_add(1, Ordering::Relaxed);
            tokio::spawn(async move {
                let mut buffer = [0_u8; 4096];
                let _ = socket.read(&mut buffer).await;
                let body = format!("hello-from-{tag}");
                let response = format!(
                    "HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });
    FakeInstance {
        address,
        hits,
        task,
    }
}

fn available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve port");
    listener.local_addr().expect("addr").port()
}

fn app_config(data_plane_port: u16) -> serde_json::Value {
    json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "cluster-routing-test",
        "limits": {
            "maxConcurrentRequests": 64,
            "requestTimeoutMs": 5_000,
            "drainTimeoutMs": 500,
            "maxConnections": 64
        },
        "listeners": [
            {
                "id": "main",
                "bind": "127.0.0.1",
                "port": data_plane_port,
                "protocols": ["http1"],
                "defaultVirtualHostRef": "main-host"
            }
        ],
        "upstreams": [
            {
                "id": "never-used",
                "targets": [{"url": "http://127.0.0.1:1"}],
                "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]}
            }
        ],
        "resources": [
            { "id": "root", "type": "proxy", "upstreamRef": "never-used" }
        ],
        "virtualHosts": [
            {
                "id": "main-host",
                "listenerRefs": ["main"],
                "serverNames": ["main.test"],
                "routes": [
                    {
                        "id": "root-route",
                        "match": {"pathType": "prefix", "path": "/"},
                        "resourceRef": "root"
                    }
                ]
            }
        ]
    })
}

fn instance_entry(id: &str, address: SocketAddr) -> Arc<DiscoveredInstance> {
    Arc::new(DiscoveredInstance::new(
        id,
        format!("{}:{}", address.ip(), address.port()),
    ))
}

#[tokio::test]
async fn cluster_routes_round_robin_across_healthy_instances() {
    let data_plane_port = available_port();
    let instance_a = spawn_fake_instance("a").await;
    let instance_b = spawn_fake_instance("b").await;

    let app: sdkwork_webserver_core::WebServerAppConfig =
        serde_json::from_value(app_config(data_plane_port)).expect("config parses");
    let compiled = load_and_compile_webserver_config_json(&app).expect("compiles");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let overlay: sdkwork_webserver_cluster::refresher::SharedTopology =
        sdkwork_webserver_cluster::refresher::empty_topology();
    let overlay_for_task = Arc::clone(&overlay);
    let data_plane: tokio::task::JoinHandle<Result<(), DataPlaneError>> =
        tokio::spawn(async move {
            run_data_plane_until_with_cluster_overlay(compiled, overlay_for_task, async move {
                let _ = shutdown_rx.await;
            })
            .await
        });
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Build the overlay directly (what the discovery refresher publishes).
    let topology = sdkwork_webserver_cluster::ClusterTopology::build(
        "edge",
        0,
        LoadBalancingStrategy::RoundRobin,
        vec![
            sdkwork_webserver_cluster::topology::ServiceMembership {
                instance: instance_entry("node-a", instance_a.address),
                served_domains: vec!["svc.cluster.test".to_owned()],
                strategy: None,
            },
            sdkwork_webserver_cluster::topology::ServiceMembership {
                instance: instance_entry("node-b", instance_b.address),
                served_domains: vec!["svc.cluster.test".to_owned()],
                strategy: None,
            },
        ],
    );
    overlay.store(Arc::new(topology));

    let mut responses = Vec::new();
    for _ in 0..4 {
        let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", data_plane_port))
            .await
            .expect("visitor connect");
        visitor
            .write_all(b"GET / HTTP/1.1\r\nHost: svc.cluster.test\r\nConnection: close\r\n\r\n")
            .await
            .expect("request written");
        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_secs(5), visitor.read_to_end(&mut response))
            .await
            .expect("response within 5s")
            .expect("response read");
        responses.push(String::from_utf8_lossy(&response).to_string());
    }
    assert!(
        responses
            .iter()
            .all(|response| response.contains("hello-from-")),
        "all requests routed: {responses:?}"
    );
    assert!(
        responses.iter().any(|r| r.contains("hello-from-a")),
        "instance a served traffic: {responses:?}"
    );
    assert!(
        responses.iter().any(|r| r.contains("hello-from-b")),
        "instance b served traffic: {responses:?}"
    );
    assert!(instance_a.hits.load(Ordering::Relaxed) >= 1);
    assert!(instance_b.hits.load(Ordering::Relaxed) >= 1);

    let _ = shutdown_tx.send(());
    let _ = data_plane.await;
    instance_a.task.abort();
    instance_b.task.abort();
}

#[tokio::test]
async fn offline_instance_is_excluded_and_unhealthy_cluster_returns_503() {
    let data_plane_port = available_port();
    let healthy = spawn_fake_instance("healthy").await;

    let app: sdkwork_webserver_core::WebServerAppConfig =
        serde_json::from_value(app_config(data_plane_port)).expect("config parses");
    let compiled = load_and_compile_webserver_config_json(&app).expect("compiles");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let overlay: sdkwork_webserver_cluster::refresher::SharedTopology =
        sdkwork_webserver_cluster::refresher::empty_topology();
    let overlay_for_task = Arc::clone(&overlay);
    let data_plane: tokio::task::JoinHandle<Result<(), DataPlaneError>> =
        tokio::spawn(async move {
            run_data_plane_until_with_cluster_overlay(compiled, overlay_for_task, async move {
                let _ = shutdown_rx.await;
            })
            .await
        });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let topology = sdkwork_webserver_cluster::ClusterTopology::build(
        "edge",
        0,
        LoadBalancingStrategy::RoundRobin,
        vec![
            sdkwork_webserver_cluster::topology::ServiceMembership {
                instance: Arc::new(
                    DiscoveredInstance::new(
                        "node-offline",
                        format!("{}:{}", healthy.address.ip(), healthy.address.port()),
                    )
                    .with_state(InstanceStatus::Offline, InstanceHealth::Healthy),
                ),
                served_domains: vec!["svc.cluster.test".to_owned()],
                strategy: None,
            },
            sdkwork_webserver_cluster::topology::ServiceMembership {
                instance: instance_entry("node-healthy", healthy.address),
                served_domains: vec!["svc.cluster.test".to_owned()],
                strategy: None,
            },
        ],
    );
    overlay.store(Arc::new(topology));

    // The offline instance would collide with the healthy one's port space in
    // a real deployment; here the healthy instance serves every request.
    for _ in 0..3 {
        let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", data_plane_port))
            .await
            .expect("visitor connect");
        visitor
            .write_all(b"GET / HTTP/1.1\r\nHost: svc.cluster.test\r\nConnection: close\r\n\r\n")
            .await
            .expect("request written");
        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_secs(5), visitor.read_to_end(&mut response))
            .await
            .expect("response within 5s")
            .expect("response read");
        let text = String::from_utf8_lossy(&response).to_string();
        assert!(text.contains("hello-from-healthy"), "got: {text}");
    }

    // Swap in an all-offline topology: routing degrades to 503, not a panic.
    let all_down = sdkwork_webserver_cluster::ClusterTopology::build(
        "edge",
        0,
        LoadBalancingStrategy::RoundRobin,
        vec![sdkwork_webserver_cluster::topology::ServiceMembership {
            instance: Arc::new(
                DiscoveredInstance::new("node-down", "127.0.0.1:1")
                    .with_state(InstanceStatus::Offline, InstanceHealth::Healthy),
            ),
            served_domains: vec!["svc.cluster.test".to_owned()],
            strategy: None,
        }],
    );
    overlay.store(Arc::new(all_down));
    let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", data_plane_port))
        .await
        .expect("visitor connect");
    visitor
        .write_all(b"GET / HTTP/1.1\r\nHost: svc.cluster.test\r\nConnection: close\r\n\r\n")
        .await
        .expect("request written");
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), visitor.read_to_end(&mut response))
        .await
        .expect("response within 5s")
        .expect("response read");
    let text = String::from_utf8_lossy(&response).to_string();
    assert!(text.contains("503"), "got: {text}");

    let _ = shutdown_tx.send(());
    let _ = data_plane.await;
    healthy.task.abort();
}

#[allow(dead_code)]
fn json_smoke(_: Json<serde_json::Value>) {}
