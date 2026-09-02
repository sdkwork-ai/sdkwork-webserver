use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU8, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use sdkwork_api_webserver_standalone_gateway::run_data_plane_from_config_until;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::{JoinHandle, JoinSet},
    time::timeout,
};

type ServerTask = JoinHandle<()>;
type DataPlaneTask =
    JoinHandle<Result<(), sdkwork_api_webserver_standalone_gateway::DataPlaneError>>;

const HEALTHY: u8 = 0;
const WRONG_STATUS: u8 = 1;
const TIMEOUT: u8 = 2;
const OVERSIZED_CHUNKED_BODY: u8 = 3;

struct ProbeConcurrency {
    current: AtomicUsize,
    maximum: AtomicUsize,
}

impl ProbeConcurrency {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            current: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
        })
    }

    fn enter(self: &Arc<Self>) -> ProbeGuard {
        let current = self.current.fetch_add(1, Ordering::AcqRel) + 1;
        self.maximum.fetch_max(current, Ordering::AcqRel);
        ProbeGuard {
            concurrency: self.clone(),
        }
    }
}

struct ProbeGuard {
    concurrency: Arc<ProbeConcurrency>,
}

impl Drop for ProbeGuard {
    fn drop(&mut self) {
        self.concurrency.current.fetch_sub(1, Ordering::AcqRel);
    }
}

struct UpstreamState {
    mode: AtomicU8,
    health_checks: AtomicUsize,
    business_requests: AtomicUsize,
    concurrency: Arc<ProbeConcurrency>,
    label: &'static str,
}

impl UpstreamState {
    fn new(label: &'static str, mode: u8, concurrency: Arc<ProbeConcurrency>) -> Arc<Self> {
        Arc::new(Self {
            mode: AtomicU8::new(mode),
            health_checks: AtomicUsize::new(0),
            business_requests: AtomicUsize::new(0),
            concurrency,
            label,
        })
    }
}

async fn spawn_upstream(
    label: &'static str,
    mode: u8,
    concurrency: Arc<ProbeConcurrency>,
) -> (
    SocketAddr,
    Arc<UpstreamState>,
    oneshot::Sender<()>,
    ServerTask,
) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind upstream");
    let address = listener.local_addr().expect("upstream address");
    let state = UpstreamState::new(label, mode, concurrency);
    let state_for_task = state.clone();
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else {
                        continue;
                    };
                    let state = state_for_task.clone();
                    connections.spawn(async move {
                        serve_upstream_connection(stream, state).await;
                    });
                }
                Some(_) = connections.join_next(), if !connections.is_empty() => {}
            }
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    });
    (address, state, shutdown_tx, task)
}

async fn serve_upstream_connection(mut stream: TcpStream, state: Arc<UpstreamState>) {
    loop {
        let Some(headers) = read_headers(&mut stream).await else {
            return;
        };
        let request_line_end = headers
            .windows(2)
            .position(|window| window == b"\r\n")
            .unwrap_or(headers.len());
        let request_line = String::from_utf8_lossy(&headers[..request_line_end]);
        let path = request_line.split_whitespace().nth(1).unwrap_or_default();
        if path.starts_with("/healthz") {
            state.health_checks.fetch_add(1, Ordering::AcqRel);
            let _guard = state.concurrency.enter();
            match state.mode.load(Ordering::Acquire) {
                HEALTHY => write_fixed_response(&mut stream, 200, b"ok").await,
                WRONG_STATUS => write_fixed_response(&mut stream, 503, b"unhealthy").await,
                TIMEOUT => {
                    let mut cancellation = [0_u8; 1];
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                        result = stream.read(&mut cancellation) => {
                            if !matches!(result, Ok(count) if count > 0) {
                                return;
                            }
                        }
                    }
                    continue;
                }
                OVERSIZED_CHUNKED_BODY => write_oversized_chunked_response(&mut stream).await,
                _ => return,
            }
        } else {
            state.business_requests.fetch_add(1, Ordering::AcqRel);
            write_fixed_response(&mut stream, 200, state.label.as_bytes()).await;
        }
        if stream.writable().await.is_err() {
            return;
        }
    }
}

async fn read_headers(stream: &mut TcpStream) -> Option<Vec<u8>> {
    let mut headers = Vec::with_capacity(1024);
    let mut byte = [0_u8; 1];
    while headers.len() <= 16 * 1024 {
        match stream.read_exact(&mut byte).await {
            Ok(_) => {
                headers.push(byte[0]);
                if headers.ends_with(b"\r\n\r\n") {
                    return Some(headers);
                }
            }
            Err(_) => return None,
        }
    }
    None
}

async fn write_fixed_response(stream: &mut TcpStream, status: u16, body: &[u8]) {
    let reason = if status == 200 {
        "OK"
    } else {
        "Service Unavailable"
    };
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n",
        body.len()
    );
    if stream.write_all(headers.as_bytes()).await.is_ok() {
        let _ = stream.write_all(body).await;
    }
}

async fn write_oversized_chunked_response(stream: &mut TcpStream) {
    let body = [b'x'; 128];
    let headers =
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n80\r\n";
    if stream.write_all(headers).await.is_ok() && stream.write_all(&body).await.is_ok() {
        let _ = stream.write_all(b"\r\n0\r\n\r\n").await;
    }
}

fn available_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind available port")
        .local_addr()
        .expect("available address")
        .port()
}

fn active_health_config(
    port: u16,
    targets: &[SocketAddr],
    maximum_concurrency: usize,
    mode: &str,
) -> Value {
    let targets = targets
        .iter()
        .map(|address| json!({"url": format!("http://{address}")}))
        .collect::<Vec<_>>();
    json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-active-health-test",
        "limits": {
            "maxConcurrentHealthChecks": maximum_concurrency,
            "requestTimeoutMs": 2_000
        },
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "default"
        }],
        "resources": [{
            "id": "proxy",
            "type": "proxy",
            "upstreamRef": "checked",
            "stripPrefix": false
        }],
        "upstreams": [{
            "id": "checked",
            "addressPolicy": {"allowedCidrs": ["127.0.0.1/32"]},
            "targets": targets,
            "maxInFlightRequests": 32,
            "activeHealth": {
                "method": "GET",
                "uri": "/healthz?source=sdkwork",
                "intervalMs": 100,
                "timeoutMs": 100,
                "unhealthyThreshold": 1,
                "healthyThreshold": 2,
                "successStatusMin": 200,
                "successStatusMax": 299,
                "maxResponseBodyBytes": 16
            }
        }],
        "virtualHosts": [{
            "id": "default",
            "listenerRefs": ["http"],
            "serverNames": ["example.test"],
            "routes": [{
                "id": "proxy-route",
                "match": {"pathType": "prefix", "path": "/"},
                "resourceRef": "proxy"
            }]
        }],
        "deployment": {
            "reload": {"mode": mode, "pollIntervalMs": 100}
        }
    })
}

fn write_config(directory: &Path, config: &Value) -> PathBuf {
    let path = directory.join("sdkwork.webserver.config.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(config).expect("serialize config"),
    )
    .expect("write config");
    path
}

fn spawn_data_plane(config_path: PathBuf) -> (oneshot::Sender<()>, DataPlaneTask) {
    sdkwork_webserver_core::load_and_compile_webserver_config(&config_path)
        .expect("active health test config compiles before runtime start");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_from_config_until(config_path, async {
            let _ = shutdown_rx.await;
        })
        .await
    });
    (shutdown_tx, task)
}

async fn wait_for_status(client: &reqwest::Client, url: &str, expected: u16) {
    timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(response) = client.get(url).header("host", "example.test").send().await {
                if response.status().as_u16() == expected {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("observe expected gateway status");
}

async fn wait_for_body(client: &reqwest::Client, url: &str, expected: &str) {
    timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(response) = client.get(url).header("host", "example.test").send().await {
                if response.status().is_success()
                    && response.text().await.ok().as_deref() == Some(expected)
                {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("observe expected gateway body");
}

async fn wait_for_checks(state: &UpstreamState, expected: usize) {
    timeout(Duration::from_secs(5), async {
        while state.health_checks.load(Ordering::Acquire) < expected {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("observe expected active health checks");
}

async fn stop_data_plane(shutdown: oneshot::Sender<()>, task: DataPlaneTask) {
    let _ = shutdown.send(());
    timeout(Duration::from_secs(5), task)
        .await
        .expect("data plane stops before deadline")
        .expect("data plane task joins")
        .expect("data plane exits cleanly");
}

async fn stop_upstream(shutdown: oneshot::Sender<()>, task: ServerTask) {
    let _ = shutdown.send(());
    timeout(Duration::from_secs(2), task)
        .await
        .expect("upstream stops before deadline")
        .expect("upstream task joins");
}

#[tokio::test]
async fn wrong_status_ejects_target_and_consecutive_successes_restore_traffic() {
    let concurrency = ProbeConcurrency::new();
    let (address, state, upstream_shutdown, upstream_task) =
        spawn_upstream("primary", WRONG_STATUS, concurrency).await;
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let path = write_config(
        directory.path(),
        &active_health_config(port, &[address], 4, "disabled"),
    );
    let (gateway_shutdown, gateway_task) = spawn_data_plane(path);
    let client = reqwest::Client::builder().no_proxy().build().expect("client");
    let url = format!("http://127.0.0.1:{port}/service");

    wait_for_status(&client, &url, 503).await;
    assert_eq!(state.business_requests.load(Ordering::Acquire), 0);
    state.mode.store(HEALTHY, Ordering::Release);
    let checks_before_recovery = state.health_checks.load(Ordering::Acquire);
    wait_for_checks(&state, checks_before_recovery + 2).await;
    wait_for_body(&client, &url, "primary").await;

    stop_data_plane(gateway_shutdown, gateway_task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn timeout_and_oversized_chunked_body_remain_unhealthy_until_bounded_recovery() {
    let concurrency = ProbeConcurrency::new();
    let (address, state, upstream_shutdown, upstream_task) =
        spawn_upstream("bounded", TIMEOUT, concurrency.clone()).await;
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let path = write_config(
        directory.path(),
        &active_health_config(port, &[address], 2, "disabled"),
    );
    let (gateway_shutdown, gateway_task) = spawn_data_plane(path);
    let client = reqwest::Client::builder().no_proxy().build().expect("client");
    let url = format!("http://127.0.0.1:{port}/service");

    wait_for_status(&client, &url, 503).await;
    assert!(
        concurrency.maximum.load(Ordering::Acquire) <= 1,
        "one target must never receive overlapping probes"
    );
    state.mode.store(OVERSIZED_CHUNKED_BODY, Ordering::Release);
    let checks_before_oversized = state.health_checks.load(Ordering::Acquire);
    wait_for_checks(&state, checks_before_oversized + 2).await;
    wait_for_status(&client, &url, 503).await;
    state.mode.store(HEALTHY, Ordering::Release);
    let checks_before_recovery = state.health_checks.load(Ordering::Acquire);
    wait_for_checks(&state, checks_before_recovery + 2).await;
    wait_for_body(&client, &url, "bounded").await;

    stop_data_plane(gateway_shutdown, gateway_task).await;
    stop_upstream(upstream_shutdown, upstream_task).await;
}

#[tokio::test]
async fn global_probe_concurrency_is_bounded_and_shutdown_cancels_future_checks() {
    let concurrency = ProbeConcurrency::new();
    let mut addresses = Vec::new();
    let mut states = Vec::new();
    let mut servers = Vec::new();
    for label in ["one", "two", "three", "four"] {
        let (address, state, shutdown, task) =
            spawn_upstream(label, TIMEOUT, concurrency.clone()).await;
        addresses.push(address);
        states.push(state);
        servers.push((shutdown, task));
    }
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let path = write_config(
        directory.path(),
        &active_health_config(port, &addresses, 2, "disabled"),
    );
    let (gateway_shutdown, gateway_task) = spawn_data_plane(path);

    timeout(Duration::from_secs(5), async {
        while states
            .iter()
            .map(|state| state.health_checks.load(Ordering::Acquire))
            .sum::<usize>()
            < 4
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("all targets receive a bounded probe");
    assert!(
        concurrency.maximum.load(Ordering::Acquire) <= 2,
        "active health operations exceed configured global concurrency"
    );

    stop_data_plane(gateway_shutdown, gateway_task).await;
    let checks_after_shutdown = states
        .iter()
        .map(|state| state.health_checks.load(Ordering::Acquire))
        .sum::<usize>();
    tokio::time::sleep(Duration::from_millis(350)).await;
    assert_eq!(
        states
            .iter()
            .map(|state| state.health_checks.load(Ordering::Acquire))
            .sum::<usize>(),
        checks_after_shutdown,
        "shutdown must leave no detached scheduler or future probes"
    );

    for (shutdown, task) in servers {
        stop_upstream(shutdown, task).await;
    }
}

#[tokio::test]
async fn watch_replacement_stops_old_generation_before_only_new_target_continues() {
    let concurrency = ProbeConcurrency::new();
    let (old_address, old_state, old_shutdown, old_task) =
        spawn_upstream("old", HEALTHY, concurrency.clone()).await;
    let (new_address, new_state, new_shutdown, new_task) =
        spawn_upstream("new", HEALTHY, concurrency).await;
    let directory = TempDir::new().expect("create config directory");
    let port = available_port();
    let path = write_config(
        directory.path(),
        &active_health_config(port, &[old_address], 2, "watch"),
    );
    let (gateway_shutdown, gateway_task) = spawn_data_plane(path.clone());
    let client = reqwest::Client::builder().no_proxy().build().expect("client");
    let url = format!("http://127.0.0.1:{port}/service");

    wait_for_body(&client, &url, "old").await;
    wait_for_checks(&old_state, 2).await;
    let old_checks_before_invalid = old_state.health_checks.load(Ordering::Acquire);
    let mut invalid_candidate = active_health_config(port, &[new_address], 2, "watch");
    invalid_candidate["upstreams"][0]["activeHealth"]["uri"] = json!("//attacker.example/health");
    write_config(directory.path(), &invalid_candidate);
    tokio::time::sleep(Duration::from_millis(350)).await;
    wait_for_body(&client, &url, "old").await;
    assert!(
        old_state.health_checks.load(Ordering::Acquire) > old_checks_before_invalid,
        "invalid candidate must retain the active generation and its scheduler"
    );
    assert_eq!(new_state.health_checks.load(Ordering::Acquire), 0);

    write_config(
        directory.path(),
        &active_health_config(port, &[new_address], 2, "watch"),
    );
    wait_for_body(&client, &url, "new").await;
    wait_for_checks(&new_state, 2).await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    let old_checks_after_replacement = old_state.health_checks.load(Ordering::Acquire);
    let new_checks_before = new_state.health_checks.load(Ordering::Acquire);
    tokio::time::sleep(Duration::from_millis(350)).await;
    assert_eq!(
        old_state.health_checks.load(Ordering::Acquire),
        old_checks_after_replacement,
        "replaced generation must stop probing its old target"
    );
    assert!(
        new_state.health_checks.load(Ordering::Acquire) > new_checks_before,
        "published generation must continue active checks"
    );

    stop_data_plane(gateway_shutdown, gateway_task).await;
    let new_checks_after_shutdown = new_state.health_checks.load(Ordering::Acquire);
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(
        new_state.health_checks.load(Ordering::Acquire),
        new_checks_after_shutdown
    );
    stop_upstream(old_shutdown, old_task).await;
    stop_upstream(new_shutdown, new_task).await;
}
