//! Cluster auto-routing end-to-end coverage.
//!
//! A data plane with an injected cluster topology overlay routes requests
//! by served domain to discovered instances: round-robin distribution
//! across healthy instances, exclusion of offline instances, and graceful
//! degradation when every instance is down. The east-west hop marker that
//! makes a relay happen at most once is covered here too, because its two
//! failure modes are only observable end to end: a visitor presenting a
//! forged marker must stay in the balancing pool, and a sibling that
//! receives a hop must serve it instead of forwarding it on.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::{net::SocketAddr, time::Duration};

use sdkwork_api_webserver_standalone_gateway::{
    run_data_plane_until_with_cluster_overlay, DataPlaneError,
};
use sdkwork_webserver_cluster::{
    refresher::SharedTopology, topology::ServiceMembership, ClusterTopology, DiscoveredInstance,
    InstanceHealth, InstanceStatus, LoadBalancingStrategy,
};
use sdkwork_webserver_core::load_and_compile_webserver_config_json;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The east-west hop marker; the data plane strips it from every inbound
/// request and sets it only when relaying to a sibling.
const HOP_HEADER: &str = "x-served-by-cluster-hop";

const SERVED_DOMAIN: &str = "svc.cluster.test";

struct FakeInstance {
    address: SocketAddr,
    hits: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl FakeInstance {
    fn hits(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }
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

/// The base application configuration: one listener, one upstream, and one
/// catch-all virtual host for `main.test` (which the cluster domain never
/// matches, so local routing only ever serves the cluster domain when a test
/// declares it explicitly).
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

/// `app_config` plus a virtual host that serves the cluster domain locally,
/// proxying to `local_target`. Models an instance that owns the domain's
/// content: a request it serves itself answers from here instead of being
/// forwarded on.
fn app_config_serving_domain(data_plane_port: u16, local_target: SocketAddr) -> serde_json::Value {
    let mut config = app_config(data_plane_port);
    let upstream = format!("http://{local_target}");
    config["upstreams"]
        .as_array_mut()
        .expect("upstreams")
        .push(json!({
            "id": "local-domain",
            "targets": [{"url": upstream}],
            "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]}
        }));
    config["resources"]
        .as_array_mut()
        .expect("resources")
        .push(json!({
            "id": "local-domain-root",
            "type": "proxy",
            "upstreamRef": "local-domain"
        }));
    config["virtualHosts"]
        .as_array_mut()
        .expect("virtualHosts")
        .push(json!({
            "id": "served-host",
            "listenerRefs": ["main"],
            "serverNames": [SERVED_DOMAIN],
            "routes": [
                {
                    "id": "served-route",
                    "match": {"pathType": "prefix", "path": "/"},
                    "resourceRef": "local-domain-root"
                }
            ]
        }));
    config
}

/// One running data plane plus the overlay slot its cluster routes read from.
struct DataPlane {
    port: u16,
    overlay: SharedTopology,
    shutdown: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<Result<(), DataPlaneError>>,
}

impl DataPlane {
    /// Sends one request the way a visitor would and returns the raw response.
    async fn request(&self, host: &str, extra_headers: &str) -> String {
        let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", self.port))
            .await
            .expect("visitor connect");
        let request =
            format!("GET / HTTP/1.1\r\nHost: {host}\r\n{extra_headers}Connection: close\r\n\r\n");
        visitor
            .write_all(request.as_bytes())
            .await
            .expect("request written");
        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_secs(5), visitor.read_to_end(&mut response))
            .await
            .expect("response within 5s")
            .expect("response read");
        String::from_utf8_lossy(&response).to_string()
    }

    /// Publishes a topology snapshot, as the discovery refresher would.
    fn publish(&self, topology: ClusterTopology) {
        self.overlay.store(Arc::new(topology));
    }

    async fn shutdown(self) {
        let _ = self.shutdown.send(());
        let _ = self.task.await;
    }
}

async fn spawn_data_plane(config: serde_json::Value) -> DataPlane {
    let port = u16::try_from(
        config["listeners"][0]["port"]
            .as_u64()
            .expect("listener port"),
    )
    .expect("port fits u16");
    let app: sdkwork_webserver_core::WebServerAppConfig =
        serde_json::from_value(config).expect("config parses");
    let compiled = load_and_compile_webserver_config_json(&app).expect("compiles");
    let (shutdown, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let overlay = sdkwork_webserver_cluster::refresher::empty_topology();
    let overlay_for_task = Arc::clone(&overlay);
    let task = tokio::spawn(async move {
        run_data_plane_until_with_cluster_overlay(compiled, overlay_for_task, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    DataPlane {
        port,
        overlay,
        shutdown,
        task,
    }
}

fn membership(id: &str, address: SocketAddr) -> ServiceMembership {
    ServiceMembership {
        instance: Arc::new(DiscoveredInstance::new(
            id,
            format!("{}:{}", address.ip(), address.port()),
        )),
        served_domains: vec![SERVED_DOMAIN.to_owned()],
        strategy: None,
    }
}

fn topology_of(members: Vec<ServiceMembership>) -> ClusterTopology {
    ClusterTopology::build("edge", 0, LoadBalancingStrategy::RoundRobin, members)
}

#[tokio::test]
async fn cluster_routes_round_robin_across_healthy_instances() {
    let instance_a = spawn_fake_instance("a").await;
    let instance_b = spawn_fake_instance("b").await;
    let plane = spawn_data_plane(app_config(available_port())).await;
    plane.publish(topology_of(vec![
        membership("node-a", instance_a.address),
        membership("node-b", instance_b.address),
    ]));

    let mut responses = Vec::new();
    for _ in 0..4 {
        responses.push(plane.request(SERVED_DOMAIN, "").await);
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
    assert!(instance_a.hits() >= 1);
    assert!(instance_b.hits() >= 1);

    plane.shutdown().await;
    instance_a.task.abort();
    instance_b.task.abort();
}

#[tokio::test]
async fn offline_instance_is_excluded_and_unhealthy_cluster_returns_503() {
    let healthy = spawn_fake_instance("healthy").await;
    let plane = spawn_data_plane(app_config(available_port())).await;
    plane.publish(topology_of(vec![
        ServiceMembership {
            instance: Arc::new(
                DiscoveredInstance::new(
                    "node-offline",
                    format!("{}:{}", healthy.address.ip(), healthy.address.port()),
                )
                .with_state(InstanceStatus::Offline, InstanceHealth::Healthy),
            ),
            served_domains: vec![SERVED_DOMAIN.to_owned()],
            strategy: None,
        },
        membership("node-healthy", healthy.address),
    ]));

    // The offline instance would collide with the healthy one's port space in
    // a real deployment; here the healthy instance serves every request.
    for _ in 0..3 {
        let text = plane.request(SERVED_DOMAIN, "").await;
        assert!(text.contains("hello-from-healthy"), "got: {text}");
    }

    // Swap in an all-offline topology: routing degrades to 503, not a panic.
    plane.publish(topology_of(vec![ServiceMembership {
        instance: Arc::new(
            DiscoveredInstance::new("node-down", "127.0.0.1:1")
                .with_state(InstanceStatus::Offline, InstanceHealth::Healthy),
        ),
        served_domains: vec![SERVED_DOMAIN.to_owned()],
        strategy: None,
    }]));
    let text = plane.request(SERVED_DOMAIN, "").await;
    assert!(text.contains("503"), "got: {text}");

    plane.shutdown().await;
    healthy.task.abort();
}

/// The marker's trust model, pinned. One listener accepts both visitor and
/// sibling traffic, so a visitor that presents the marker is treated as the
/// last hop and served locally instead of being balanced onward. The effect is
/// confined to the request that presents it — the instance is not contacted —
/// and the marker never grants access to content this instance would not
/// already serve.
#[tokio::test]
async fn a_presented_hop_marker_makes_this_instance_the_last_hop() {
    let instance = spawn_fake_instance("target").await;
    let plane = spawn_data_plane(app_config(available_port())).await;
    plane.publish(topology_of(vec![membership(
        "node-target",
        instance.address,
    )]));

    let text = plane
        .request(SERVED_DOMAIN, &format!("{HOP_HEADER}: 1\r\n"))
        .await;
    assert!(
        !text.contains("hello-from-target"),
        "a request that already carries a hop is not relayed again: {text}"
    );
    assert_eq!(
        instance.hits(),
        0,
        "the marked request never reached the instance"
    );

    // The same request without the marker is balanced to the instance, which
    // proves the marker — not the topology — is what changed the outcome.
    let text = plane.request(SERVED_DOMAIN, "").await;
    assert!(text.contains("hello-from-target"), "got: {text}");
    assert_eq!(instance.hits(), 1);

    plane.shutdown().await;
    instance.task.abort();
}

/// The availability half of the hop rule: the instance that receives a hop
/// serves it instead of forwarding it on. Both planes here see the domain in
/// their own overlay, so an instance that ignored the marker would hand the
/// request to the instance whose address its overlay names — in this
/// deployment that is the edge that just sent it, which is exactly the cycle
/// the marker prevents. The two outcomes are distinguishable: serving locally
/// answers `hello-from-sibling-local`, while bouncing sends the request back
/// to the edge, which has no local content for the domain and answers 404.
#[tokio::test]
async fn a_sibling_that_already_carried_the_hop_serves_locally_without_looping() {
    let sibling_local = spawn_fake_instance("sibling-local").await;
    let sibling = spawn_data_plane(app_config_serving_domain(
        available_port(),
        sibling_local.address,
    ))
    .await;
    let edge = spawn_data_plane(app_config(available_port())).await;

    // Each plane discovers the other: the edge relays the domain to the
    // sibling, and the sibling's own overlay names the edge.
    edge.publish(topology_of(vec![membership(
        "node-sibling",
        SocketAddr::from(([127, 0, 0, 1], sibling.port)),
    )]));
    sibling.publish(topology_of(vec![membership(
        "node-edge",
        SocketAddr::from(([127, 0, 0, 1], edge.port)),
    )]));

    let text = edge.request(SERVED_DOMAIN, "").await;
    assert!(
        text.contains("hello-from-sibling-local"),
        "the sibling must serve the hop it received, not forward it on: {text}"
    );
    assert_eq!(
        sibling_local.hits(),
        1,
        "the sibling's local content answered exactly once"
    );

    edge.shutdown().await;
    sibling.shutdown().await;
    sibling_local.task.abort();
}

/// One upgraded instance: a raw HTTP/1.1 peer that accepts a WebSocket
/// handshake and then echoes every byte it receives. It keeps the handshake it
/// was sent, because that head is the only part of a relayed WebSocket the
/// edge gets to rewrite — everything after the `101` is opaque bytes.
struct UpgradingInstance {
    address: SocketAddr,
    heads: Arc<std::sync::Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}

impl UpgradingInstance {
    fn heads(&self) -> Vec<String> {
        self.heads.lock().expect("handshake log lock").clone()
    }
}

async fn spawn_upgrading_instance() -> UpgradingInstance {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("instance bind");
    let address = listener.local_addr().expect("addr");
    let heads = Arc::new(std::sync::Mutex::new(Vec::new()));
    let heads_for_task = Arc::clone(&heads);
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let heads = Arc::clone(&heads_for_task);
            tokio::spawn(async move {
                let mut bytes = Vec::with_capacity(1024);
                let mut buffer = [0_u8; 1024];
                let tail = loop {
                    let Ok(read) = socket.read(&mut buffer).await else {
                        return;
                    };
                    if read == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&buffer[..read]);
                    if let Some(position) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        break bytes.split_off(position + 4);
                    }
                    if bytes.len() > 64 * 1024 {
                        return;
                    }
                };
                heads
                    .lock()
                    .expect("handshake log lock")
                    .push(String::from_utf8_lossy(&bytes).to_string());

                // RFC 6455's own example key/accept pair.
                let response = "HTTP/1.1 101 Switching Protocols\r\nConnection: upgrade\r\n\
                    Upgrade: websocket\r\nSec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\r\n";
                if socket.write_all(response.as_bytes()).await.is_err() {
                    return;
                }
                if !tail.is_empty() && socket.write_all(&tail).await.is_err() {
                    return;
                }
                loop {
                    match socket.read(&mut buffer).await {
                        Ok(0) | Err(_) => {
                            let _ = socket.shutdown().await;
                            return;
                        }
                        Ok(read) if socket.write_all(&buffer[..read]).await.is_err() => return,
                        Ok(_) => {}
                    }
                }
            });
        }
    });
    UpgradingInstance {
        address,
        heads,
        task,
    }
}

/// Reads one HTTP head (`\r\n\r\n` terminated) and returns it with whatever
/// bytes of the next frame arrived in the same read.
async fn read_head<S>(stream: &mut S) -> std::io::Result<(String, Vec<u8>)>
where
    S: tokio::io::AsyncRead + Unpin,
{
    let mut bytes = Vec::with_capacity(1024);
    let mut buffer = [0_u8; 1024];
    loop {
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
        }
        bytes.extend_from_slice(&buffer[..read]);
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let tail = bytes.split_off(position + 4);
            return Ok((String::from_utf8_lossy(&bytes).to_string(), tail));
        }
        if bytes.len() > 64 * 1024 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "head exceeds test bound",
            ));
        }
    }
}

/// A WebSocket session across a cluster hop (PRD §124): the edge relays the
/// handshake to the discovered instance, and once the instance answers `101`
/// it pumps raw bytes in both directions. The handshake is asserted at the
/// instance, so the east-west marker and the visitor's real address are
/// proven to travel with the upgrade — a relay that dropped them would leave
/// the instance unable to tell a hop from a direct visitor.
#[tokio::test]
async fn websocket_upgrade_is_pumped_through_a_cluster_hop() {
    let instance = spawn_upgrading_instance().await;
    let plane = spawn_data_plane(app_config(available_port())).await;
    plane.publish(topology_of(vec![membership("node-ws", instance.address)]));

    let mut visitor = tokio::net::TcpStream::connect(("127.0.0.1", plane.port))
        .await
        .expect("visitor connect");
    let handshake = format!(
        "GET /socket HTTP/1.1\r\nHost: {SERVED_DOMAIN}\r\nConnection: keep-alive, Upgrade\r\n\
         Upgrade: websocket\r\nSec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Key: MDEyMzQ1Njc4OWFiY2RlZg==\r\n\r\n"
    );
    visitor
        .write_all(handshake.as_bytes())
        .await
        .expect("handshake written");
    let (head, buffered) = tokio::time::timeout(Duration::from_secs(5), read_head(&mut visitor))
        .await
        .expect("the 101 arrives within 5s")
        .expect("read the relayed 101");
    assert!(head.starts_with("HTTP/1.1 101"), "{head}");
    let lowered = head.to_ascii_lowercase();
    assert!(lowered.contains("connection: upgrade"), "{head}");
    assert!(lowered.contains("upgrade: websocket"), "{head}");

    // Everything after the 101 is opaque: it must round trip through the pump.
    let payload = b"ping-through-the-hop";
    visitor
        .write_all(payload)
        .await
        .expect("payload written into the upgraded socket");
    let mut echoed = buffered;
    while echoed.len() < payload.len() {
        let mut chunk = [0_u8; 64];
        let read = tokio::time::timeout(Duration::from_secs(5), visitor.read(&mut chunk))
            .await
            .expect("the echo arrives within 5s")
            .expect("read echoed bytes");
        assert_ne!(read, 0, "the pump closed before the payload round tripped");
        echoed.extend_from_slice(&chunk[..read]);
    }
    assert_eq!(&echoed[..payload.len()], payload);

    let heads = instance.heads();
    assert_eq!(
        heads.len(),
        1,
        "exactly one handshake reached the instance: {heads:?}"
    );
    let forwarded = &heads[0];
    assert!(
        forwarded.contains(&format!("{HOP_HEADER}: 1")),
        "the relayed handshake carries the east-west marker: {forwarded}"
    );
    assert!(
        forwarded
            .to_ascii_lowercase()
            .contains("x-forwarded-for: 127.0.0.1"),
        "the relayed handshake carries the visitor's real address: {forwarded}"
    );

    drop(visitor);
    plane.shutdown().await;
    instance.task.abort();
}
