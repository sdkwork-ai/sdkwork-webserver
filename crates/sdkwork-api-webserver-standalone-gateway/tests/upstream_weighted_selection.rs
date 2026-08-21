use std::{
    fs,
    net::{SocketAddr, TcpListener as StdTcpListener},
    path::{Path, PathBuf},
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
use sdkwork_api_webserver_standalone_gateway::{
    run_data_plane_from_config_until, run_data_plane_until, DataPlaneError,
};
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
    time::timeout,
};

type DataPlaneTask = JoinHandle<Result<(), DataPlaneError>>;
type UpstreamTask = JoinHandle<()>;

struct LabeledUpstream {
    address: SocketAddr,
    failing: Arc<AtomicBool>,
    shutdown: oneshot::Sender<()>,
    task: UpstreamTask,
}

fn available_port() -> u16 {
    let listener = StdTcpListener::bind("127.0.0.1:0").expect("reserve an available port");
    listener.local_addr().expect("read reserved port").port()
}

fn write_config(directory: &Path, config: &Value) -> PathBuf {
    let path = directory.join("sdkwork.webserver.config.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(config).expect("serialize weighted config"),
    )
    .expect("write weighted config");
    path
}

fn weighted_config(port: u16, targets: &[(SocketAddr, u16)]) -> Value {
    json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "weighted-upstream-test",
        "limits": {
            "maxConcurrentRequests": 32,
            "requestTimeoutMs": 5_000,
            "drainTimeoutMs": 1_000,
            "maxConnections": 64
        },
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
            "upstreamRef": "weighted-upstream"
        }],
        "upstreams": [{
            "id": "weighted-upstream",
            "targets": targets
                .iter()
                .map(|(address, weight)| json!({
                    "url": format!("http://127.0.0.1:{}", address.port()),
                    "weight": weight
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
                "failureThreshold": 1,
                "ejectionTimeMs": 200,
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
    })
}

fn spawn_data_plane(config_path: &Path) -> (oneshot::Sender<()>, DataPlaneTask) {
    let compiled = load_and_compile_webserver_config(config_path).expect("compile weighted config");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    (shutdown_tx, task)
}

fn spawn_watched_data_plane(config_path: &Path) -> (oneshot::Sender<()>, DataPlaneTask) {
    let config_path = config_path.to_path_buf();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_from_config_until(config_path, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    (shutdown_tx, task)
}

async fn wait_for_gateway(port: u16) {
    timeout(Duration::from_secs(5), async {
        loop {
            if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("gateway becomes reachable");
}

async fn stop_data_plane(shutdown: oneshot::Sender<()>, task: DataPlaneTask) {
    shutdown.send(()).expect("signal data-plane shutdown");
    timeout(Duration::from_secs(3), task)
        .await
        .expect("data plane stops within its drain budget")
        .expect("data-plane task joins")
        .expect("data plane stops cleanly");
}

async fn spawn_labeled_upstream(label: &'static str) -> LabeledUpstream {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind labeled upstream");
    let address = listener.local_addr().expect("labeled upstream address");
    let failing = Arc::new(AtomicBool::new(false));
    let task_failing = failing.clone();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let app = Router::new().fallback(any(move || {
            let failing = task_failing.load(Ordering::Acquire);
            async move {
                let mut response = Response::new(Body::from(label));
                if failing {
                    *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
                }
                response
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
        shutdown: shutdown_tx,
        task,
    }
}

async fn stop_upstream(upstream: LabeledUpstream) {
    upstream
        .shutdown
        .send(())
        .expect("signal upstream shutdown");
    timeout(Duration::from_secs(3), upstream.task)
        .await
        .expect("upstream stops")
        .expect("upstream task joins");
}

async fn request_label(client: &reqwest::Client, port: u16) -> (StatusCode, String) {
    let response = client
        .get(format!("http://127.0.0.1:{port}/weighted"))
        .header("host", "test.localhost")
        .send()
        .await
        .expect("weighted request completes");
    let status = response.status();
    let body = response.text().await.expect("read weighted response");
    (status, body)
}

async fn observe_cycle(client: &reqwest::Client, port: u16, requests: usize) -> (usize, usize) {
    let mut primary = 0;
    let mut secondary = 0;
    for _ in 0..requests {
        let (status, body) = request_label(client, port).await;
        assert_eq!(status, StatusCode::OK);
        match body.as_str() {
            "primary" => primary += 1,
            "secondary" => secondary += 1,
            other => panic!("unexpected upstream label {other}"),
        }
    }
    (primary, secondary)
}

async fn observe_sequence(client: &reqwest::Client, port: u16, requests: usize) -> Vec<String> {
    let mut labels = Vec::with_capacity(requests);
    for _ in 0..requests {
        let (status, body) = request_label(client, port).await;
        assert_eq!(status, StatusCode::OK);
        labels.push(body);
    }
    labels
}

#[tokio::test]
async fn weights_drive_real_traffic_and_health_exclusion_then_recovery() {
    let primary = spawn_labeled_upstream("primary").await;
    let secondary = spawn_labeled_upstream("secondary").await;
    let directory = TempDir::new().expect("create test directory");
    let port = available_port();
    let config = weighted_config(port, &[(primary.address, 3), (secondary.address, 1)]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    wait_for_gateway(port).await;
    let client = reqwest::Client::new();

    assert_eq!(
        observe_sequence(&client, port, 4).await,
        ["primary", "primary", "secondary", "primary"]
    );
    assert_eq!(observe_cycle(&client, port, 8).await, (6, 2));

    primary.failing.store(true, Ordering::Release);
    let (status, body) = request_label(&client, port).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, "primary");
    for _ in 0..4 {
        let (status, body) = request_label(&client, port).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "secondary");
    }

    primary.failing.store(false, Ordering::Release);
    tokio::time::sleep(Duration::from_millis(250)).await;
    let (status, body) = request_label(&client, port).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "primary", "one half-open request restores the target");
    assert_eq!(observe_cycle(&client, port, 8).await, (6, 2));

    stop_data_plane(shutdown, task).await;
    stop_upstream(primary).await;
    stop_upstream(secondary).await;
}

#[tokio::test]
async fn passive_recovery_ramps_effective_weight_before_restoring_nominal_distribution() {
    let primary = spawn_labeled_upstream("primary").await;
    let secondary = spawn_labeled_upstream("secondary").await;
    let directory = TempDir::new().expect("create test directory");
    let port = available_port();
    let mut config = weighted_config(port, &[(primary.address, 4), (secondary.address, 1)]);
    config["upstreams"][0]["targets"][0]["slowStartMs"] = json!(1_000);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    wait_for_gateway(port).await;
    let client = reqwest::Client::new();

    assert_eq!(observe_cycle(&client, port, 10).await, (8, 2));
    primary.failing.store(true, Ordering::Release);
    assert_eq!(
        request_label(&client, port).await,
        (StatusCode::SERVICE_UNAVAILABLE, "primary".to_owned())
    );

    primary.failing.store(false, Ordering::Release);
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(
        request_label(&client, port).await,
        (StatusCode::OK, "primary".to_owned()),
        "half-open success starts slow start"
    );
    assert_eq!(
        observe_cycle(&client, port, 4).await,
        (2, 2),
        "the first integer slow-start slot is 1:1"
    );

    tokio::time::sleep(Duration::from_millis(1_050)).await;
    assert_eq!(
        observe_cycle(&client, port, 10).await,
        (8, 2),
        "the configured deadline restores nominal 4:1 weight"
    );

    stop_data_plane(shutdown, task).await;
    stop_upstream(primary).await;
    stop_upstream(secondary).await;
}

#[tokio::test]
async fn backup_receives_only_fallback_traffic_and_primary_probe_restores_the_tier() {
    let primary = spawn_labeled_upstream("primary").await;
    let backup = spawn_labeled_upstream("backup").await;
    let directory = TempDir::new().expect("create test directory");
    let port = available_port();
    let mut config = weighted_config(port, &[(primary.address, 1), (backup.address, 1)]);
    config["upstreams"][0]["targets"][1]["backup"] = json!(true);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);
    wait_for_gateway(port).await;
    let client = reqwest::Client::new();

    for _ in 0..4 {
        let (status, body) = request_label(&client, port).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "primary");
    }

    primary.failing.store(true, Ordering::Release);
    let (status, body) = request_label(&client, port).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, "primary");
    for _ in 0..4 {
        let (status, body) = request_label(&client, port).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "backup");
    }

    primary.failing.store(false, Ordering::Release);
    tokio::time::sleep(Duration::from_millis(250)).await;
    let (status, body) = request_label(&client, port).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "primary", "half-open primary precedes healthy backup");
    for _ in 0..3 {
        let (status, body) = request_label(&client, port).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "primary");
    }

    stop_data_plane(shutdown, task).await;
    stop_upstream(primary).await;
    stop_upstream(backup).await;
}

#[tokio::test]
async fn watch_replaces_weights_and_rejects_invalid_candidate() {
    let primary = spawn_labeled_upstream("primary").await;
    let secondary = spawn_labeled_upstream("secondary").await;
    let directory = TempDir::new().expect("create test directory");
    let port = available_port();
    let mut initial = weighted_config(port, &[(primary.address, 3), (secondary.address, 1)]);
    initial["deployment"] = json!({
        "drainTimeoutMs": 1_000,
        "reload": {"mode": "watch", "pollIntervalMs": 100}
    });
    let path = write_config(directory.path(), &initial);
    let (shutdown, task) = spawn_watched_data_plane(&path);
    wait_for_gateway(port).await;
    let client = reqwest::Client::new();
    assert_eq!(observe_cycle(&client, port, 8).await, (6, 2));

    let mut replacement = weighted_config(port, &[(primary.address, 1), (secondary.address, 3)]);
    replacement["deployment"] = initial["deployment"].clone();
    write_config(directory.path(), &replacement);
    timeout(Duration::from_secs(5), async {
        loop {
            if observe_cycle(&client, port, 8).await == (2, 6) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("Watch publishes replacement target weights");

    let mut invalid = weighted_config(port, &[(primary.address, 0), (secondary.address, 1)]);
    invalid["deployment"] = initial["deployment"].clone();
    write_config(directory.path(), &invalid);
    for _ in 0..3 {
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            observe_cycle(&client, port, 8).await,
            (2, 6),
            "invalid candidate must retain the active weighted generation"
        );
    }

    let mut invalid_slow_start = replacement.clone();
    invalid_slow_start["upstreams"][0]["targets"][0]["slowStartMs"] = json!(99);
    write_config(directory.path(), &invalid_slow_start);
    for _ in 0..3 {
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            observe_cycle(&client, port, 8).await,
            (2, 6),
            "invalid slow-start candidate must retain the active weighted generation"
        );
    }

    stop_data_plane(shutdown, task).await;
    stop_upstream(primary).await;
    stop_upstream(secondary).await;
}

#[tokio::test]
async fn watch_replaces_backup_roles_and_rejects_an_all_backup_candidate() {
    let primary = spawn_labeled_upstream("primary").await;
    let secondary = spawn_labeled_upstream("secondary").await;
    let directory = TempDir::new().expect("create test directory");
    let port = available_port();
    let mut initial = weighted_config(port, &[(primary.address, 1), (secondary.address, 1)]);
    initial["upstreams"][0]["targets"][1]["backup"] = json!(true);
    initial["deployment"] = json!({
        "drainTimeoutMs": 1_000,
        "reload": {"mode": "watch", "pollIntervalMs": 100}
    });
    let path = write_config(directory.path(), &initial);
    let (shutdown, task) = spawn_watched_data_plane(&path);
    wait_for_gateway(port).await;
    let client = reqwest::Client::new();
    for _ in 0..4 {
        assert_eq!(
            request_label(&client, port).await,
            (StatusCode::OK, "primary".to_owned())
        );
    }

    let mut replacement = weighted_config(port, &[(primary.address, 1), (secondary.address, 1)]);
    replacement["upstreams"][0]["targets"][0]["backup"] = json!(true);
    replacement["deployment"] = initial["deployment"].clone();
    write_config(directory.path(), &replacement);
    timeout(Duration::from_secs(5), async {
        loop {
            if request_label(&client, port).await == (StatusCode::OK, "secondary".to_owned()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("Watch publishes replacement backup roles");

    let mut invalid = replacement.clone();
    invalid["upstreams"][0]["targets"][1]["backup"] = json!(true);
    write_config(directory.path(), &invalid);
    for _ in 0..3 {
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            request_label(&client, port).await,
            (StatusCode::OK, "secondary".to_owned()),
            "all-backup candidate must retain the active tier roles"
        );
    }

    stop_data_plane(shutdown, task).await;
    stop_upstream(primary).await;
    stop_upstream(secondary).await;
}
