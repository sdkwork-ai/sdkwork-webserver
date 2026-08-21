use std::{
    fs,
    net::{SocketAddr, TcpListener as StdTcpListener},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use axum::{body::Body, http::Response, routing::any, Router};
use bytes::Bytes;
use http_body::Frame;
use http_body_util::channel::Channel;
use sdkwork_api_webserver_standalone_gateway::{
    run_data_plane_from_config_until, run_data_plane_until, DataPlaneError,
};
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{oneshot, watch},
    task::JoinHandle,
    time::timeout,
};

type DataPlaneTask = JoinHandle<Result<(), DataPlaneError>>;

struct StreamingUpstream {
    address: SocketAddr,
    requests: Arc<AtomicUsize>,
    pulse: watch::Sender<u64>,
    release: watch::Sender<bool>,
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

fn available_port() -> u16 {
    let listener = StdTcpListener::bind("127.0.0.1:0").expect("reserve an available port");
    listener.local_addr().expect("read reserved port").port()
}

fn write_config(directory: &Path, config: &Value) -> PathBuf {
    let path = directory.join("sdkwork.webserver.config.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(config).expect("serialize least-connections config"),
    )
    .expect("write least-connections config");
    path
}

fn least_connections_config(
    port: u16,
    targets: &[(SocketAddr, u16, bool)],
    watch_reload: bool,
    revision: usize,
) -> Value {
    let deployment = if watch_reload {
        json!({
            "drainTimeoutMs": 1_000,
            "reload": {"mode": "watch", "pollIntervalMs": 100}
        })
    } else {
        json!({"drainTimeoutMs": 1_000})
    };
    json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "least-connections-test",
        "limits": {
            "maxConcurrentRequests": 32,
            "requestTimeoutMs": 5_000,
            "drainTimeoutMs": 1_000,
            "maxConnections": 64
        },
        "deployment": deployment,
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
            "upstreamRef": "least-connections-upstream"
        }],
        "upstreams": [{
            "id": "least-connections-upstream",
            "loadBalancing": "least-connections",
            "targets": targets
                .iter()
                .map(|(address, weight, backup)| json!({
                    "url": format!("http://127.0.0.1:{}", address.port()),
                    "weight": weight,
                    "backup": backup
                }))
                .collect::<Vec<_>>(),
            "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]},
            "connectTimeoutMs": 2_000,
            "requestTimeoutMs": 5_000,
            "maxConnections": 8,
            "maxIdleConnections": 8,
            "idleConnectionTimeoutMs": 1_000,
            "maxInFlightRequests": 16,
            "passiveHealth": {
                "failureThreshold": 2,
                "ejectionTimeMs": 1_000,
                "failureStatuses": [502, 503, 504]
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
        }],
        "metadata": {"testRevision": revision}
    })
}

async fn spawn_streaming_upstream(label: &'static str) -> StreamingUpstream {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind streaming upstream");
    let address = listener.local_addr().expect("streaming upstream address");
    let requests = Arc::new(AtomicUsize::new(0));
    let handler_requests = requests.clone();
    let (pulse, pulse_rx) = watch::channel(0_u64);
    let (release, release_rx) = watch::channel(false);
    let (shutdown, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let app = Router::new().fallback(any(move || {
            let requests = handler_requests.clone();
            let mut pulse = pulse_rx.clone();
            let mut release = release_rx.clone();
            async move {
                requests.fetch_add(1, Ordering::AcqRel);
                let (mut sender, body) = Channel::<Bytes>::new(1);
                let _ = pulse.borrow_and_update();
                tokio::spawn(async move {
                    if *release.borrow() {
                        let _ =
                            sender.try_send(Frame::data(Bytes::from_static(label.as_bytes())));
                        return;
                    }
                    loop {
                        tokio::select! {
                            changed = release.changed() => {
                                if changed.is_err() {
                                    return;
                                }
                                if *release.borrow() {
                                    let _ = sender.try_send(Frame::data(Bytes::from_static(label.as_bytes())));
                                    return;
                                }
                            }
                            changed = pulse.changed() => {
                                if changed.is_err()
                                    || sender.try_send(Frame::data(Bytes::from_static(b"x"))).is_err()
                                {
                                    return;
                                }
                            }
                        }
                    }
                });
                let mut response = Response::new(Body::new(body));
                response.headers_mut().insert(
                    "x-sdkwork-test-upstream",
                    label.parse().expect("valid test Header value"),
                );
                response
            }
        }));
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("serve streaming upstream");
    });
    StreamingUpstream {
        address,
        requests,
        pulse,
        release,
        shutdown,
        task,
    }
}

async fn stop_upstream(upstream: StreamingUpstream) {
    let _ = upstream.release.send(true);
    upstream
        .shutdown
        .send(())
        .expect("signal streaming upstream shutdown");
    timeout(Duration::from_secs(3), upstream.task)
        .await
        .expect("streaming upstream stops")
        .expect("streaming upstream task joins");
}

fn spawn_data_plane(config_path: &Path) -> (oneshot::Sender<()>, DataPlaneTask) {
    let compiled =
        load_and_compile_webserver_config(config_path).expect("compile least-connections config");
    let (shutdown, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    (shutdown, task)
}

fn spawn_watched_data_plane(config_path: &Path) -> (oneshot::Sender<()>, DataPlaneTask) {
    let config_path = config_path.to_path_buf();
    let (shutdown, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_from_config_until(config_path, async move {
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

async fn stop_data_plane(shutdown: oneshot::Sender<()>, task: DataPlaneTask) {
    shutdown.send(()).expect("signal data-plane shutdown");
    timeout(Duration::from_secs(3), task)
        .await
        .expect("data plane stops within drain budget")
        .expect("data-plane task joins")
        .expect("data plane stops cleanly");
}

async fn request(client: &reqwest::Client, port: u16, path: &str) -> reqwest::Response {
    client
        .get(format!("http://127.0.0.1:{port}{path}"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("least-connections request receives response Headers")
}

async fn request_head_and_disconnect(port: u16, path: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect raw cancellation client");
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: test.localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .expect("write raw cancellation request");
    let mut response = Vec::with_capacity(1024);
    timeout(Duration::from_secs(3), async {
        let mut buffer = [0_u8; 1024];
        loop {
            let read = stream
                .read(&mut buffer)
                .await
                .expect("read raw response Headers");
            assert_ne!(read, 0, "response ended before Header completion");
            response.extend_from_slice(&buffer[..read]);
            if response.windows(4).any(|window| window == b"\r\n\r\n") {
                return;
            }
        }
    })
    .await
    .expect("raw response Headers arrive");
    #[allow(deprecated)]
    stream
        .set_linger(Some(Duration::ZERO))
        .expect("configure deterministic TCP reset on test disconnect");
    drop(stream);
    let response = String::from_utf8(response).expect("test response Headers are UTF-8");
    response
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("x-sdkwork-test-upstream")
                .then(|| value.trim().to_owned())
        })
        .expect("raw response includes upstream label")
}

fn response_label(response: &reqwest::Response) -> &str {
    response
        .headers()
        .get("x-sdkwork-test-upstream")
        .expect("upstream label Header")
        .to_str()
        .expect("upstream label is text")
}

#[tokio::test]
async fn streaming_activity_drives_weighted_selection_and_cancellation_release() {
    let primary = spawn_streaming_upstream("primary").await;
    let secondary = spawn_streaming_upstream("secondary").await;
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let config = least_connections_config(
        port,
        &[(primary.address, 1, false), (secondary.address, 2, false)],
        false,
        1,
    );
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    wait_for_gateway(port).await;
    let client = reqwest::Client::new();

    assert_eq!(request_head_and_disconnect(port, "/first").await, "primary");
    let _ = primary.pulse.send(1);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let second = request(&client, port, "/second").await;
    assert_eq!(response_label(&second), "secondary");

    let third = request(&client, port, "/after-cancel").await;
    assert_eq!(
        response_label(&third),
        "primary",
        "cancellation must release 0/1 below the secondary's held 1/2 load"
    );

    let _ = primary.release.send(true);
    let _ = secondary.release.send(true);
    assert_eq!(
        second.text().await.expect("complete secondary Body"),
        "secondary"
    );
    assert_eq!(
        third.text().await.expect("complete primary Body"),
        "primary"
    );
    stop_data_plane(shutdown, task).await;
    stop_upstream(primary).await;
    stop_upstream(secondary).await;
}

#[tokio::test]
async fn random_two_routes_around_the_sampled_active_request() {
    let primary = spawn_streaming_upstream("primary").await;
    let secondary = spawn_streaming_upstream("secondary").await;
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let mut config = least_connections_config(
        port,
        &[(primary.address, 1, false), (secondary.address, 1, false)],
        false,
        1,
    );
    config["upstreams"][0]["loadBalancing"] = json!("random-two-least-connections");
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    wait_for_gateway(port).await;
    let client = reqwest::Client::new();

    let first = request(&client, port, "/first").await;
    let first_label = response_label(&first).to_owned();
    let second = request(&client, port, "/second").await;
    assert_ne!(
        response_label(&second),
        first_label,
        "two distinct candidates must route around the active request"
    );
    assert_eq!(primary.requests.load(Ordering::Acquire), 1);
    assert_eq!(secondary.requests.load(Ordering::Acquire), 1);

    let _ = primary.release.send(true);
    let _ = secondary.release.send(true);
    drop((first, second));
    stop_data_plane(shutdown, task).await;
    stop_upstream(primary).await;
    stop_upstream(secondary).await;
}

#[tokio::test]
async fn weighted_ratios_and_primary_tier_remain_authoritative() {
    let primary = spawn_streaming_upstream("primary").await;
    let secondary = spawn_streaming_upstream("secondary").await;
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let config = least_connections_config(
        port,
        &[(primary.address, 2, false), (secondary.address, 1, false)],
        false,
        1,
    );
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    wait_for_gateway(port).await;
    let client = reqwest::Client::new();

    let first = request(&client, port, "/one").await;
    let second = request(&client, port, "/two").await;
    let third = request(&client, port, "/three").await;
    assert_eq!(response_label(&first), "primary");
    assert_eq!(response_label(&second), "secondary");
    assert_eq!(
        response_label(&third),
        "primary",
        "one active request at weight two is below one at weight one"
    );

    let _ = primary.release.send(true);
    let _ = secondary.release.send(true);
    drop((first, second, third));
    stop_data_plane(shutdown, task).await;
    stop_upstream(primary).await;
    stop_upstream(secondary).await;
}

#[tokio::test]
async fn busy_primary_does_not_enable_backup_tier() {
    let primary = spawn_streaming_upstream("primary").await;
    let backup = spawn_streaming_upstream("backup").await;
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let config = least_connections_config(
        port,
        &[(primary.address, 1, false), (backup.address, 1_000, true)],
        false,
        1,
    );
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    wait_for_gateway(port).await;
    let client = reqwest::Client::new();

    let first = request(&client, port, "/first").await;
    let second = request(&client, port, "/second").await;
    assert_eq!(response_label(&first), "primary");
    assert_eq!(response_label(&second), "primary");
    assert_eq!(backup.requests.load(Ordering::Acquire), 0);

    let _ = primary.release.send(true);
    drop((first, second));
    stop_data_plane(shutdown, task).await;
    stop_upstream(primary).await;
    stop_upstream(backup).await;
}

#[tokio::test]
async fn watch_generation_starts_with_fresh_activity_counters() {
    let primary = spawn_streaming_upstream("primary").await;
    let secondary = spawn_streaming_upstream("secondary").await;
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let initial = least_connections_config(
        port,
        &[(primary.address, 1, false), (secondary.address, 2, false)],
        true,
        1,
    );
    let path = write_config(directory.path(), &initial);
    let (shutdown, task) = spawn_watched_data_plane(&path);
    wait_for_gateway(port).await;
    let client = reqwest::Client::new();

    let old_generation = request(&client, port, "/old-generation").await;
    assert_eq!(response_label(&old_generation), "primary");

    let replacement = least_connections_config(
        port,
        &[(primary.address, 1, false), (secondary.address, 3, false)],
        true,
        2,
    );
    write_config(directory.path(), &replacement);
    tokio::time::sleep(Duration::from_millis(350)).await;
    let new_generation = request(&client, port, "/new-generation").await;
    assert_eq!(
        response_label(&new_generation),
        "primary",
        "the replacement generation must not inherit the old primary's active count"
    );

    let _ = primary.release.send(true);
    let _ = secondary.release.send(true);
    drop((old_generation, new_generation));
    stop_data_plane(shutdown, task).await;
    stop_upstream(primary).await;
    stop_upstream(secondary).await;
}
