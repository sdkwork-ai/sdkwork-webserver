//! Bounded capacity evidence for the data plane.
//!
//! Ignored by default: it is a measurement, not a correctness gate, and the
//! throughput it reports depends on the machine. The release workflow and an
//! operator can run it to produce the current numbers:
//!
//! ```text
//! pnpm test:capacity
//! ```
//!
//! The shape is deliberately the cheapest possible data plane (one `respond`
//! resource, one catch-all route) so the measurement covers the HTTP stack,
//! admission, routing, and response path rather than an upstream.

use std::{
    fs,
    net::{SocketAddr, TcpListener},
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use sdkwork_api_webserver_standalone_gateway::{run_data_plane_until, DataPlaneError};
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::json;
use tempfile::TempDir;
use tokio::{net::TcpSocket, sync::oneshot, task::JoinHandle};

const CONCURRENCY: usize = 32;
const DEFAULT_REQUESTS: usize = 32_000;
const DEFAULT_IDLE_CONNECTIONS: usize = 5_000;

fn available_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve an available port");
    listener.local_addr().expect("read local address").port()
}

fn write_config(directory: &Path, port: u16, max_connections: usize) -> std::path::PathBuf {
    // `maxConnections × http2MaxEncodedHeaderBlockBytes ≤ 1 GiB` is a hard
    // validation budget. Large connection-count runs must lower the per-
    // connection block budget instead of tripping the global cap.
    let encoded_header_block_bytes = ((1024_u64 * 1024 * 1024)
        / u64::try_from(max_connections).unwrap_or(u64::MAX).max(1))
    .clamp(4096, 65_536);
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-capacity-smoke",
        "limits": {
            "maxRequestBodyBytes": 1024,
            "requestTimeoutMs": 5000,
            "drainTimeoutMs": 1000,
            "maxConnections": max_connections,
            "maxRequestHeaderBytes": 8192,
            "maxRequestHeaders": 64,
            "requestHeaderTimeoutMs": 30000,
            "http2MaxEncodedHeaderBlockBytes": encoded_header_block_bytes
        },
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "capacity-host",
            "maxConnections": max_connections
        }],
        "resources": [{ "id": "ok", "type": "respond", "status": 200, "body": "ok" }],
        "virtualHosts": [{
            "id": "capacity-host",
            "listenerRefs": ["http"],
            "serverNames": ["capacity.localhost"],
            "routes": [{
                "id": "all",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "ok"
            }]
        }]
    });
    let path = directory.join("sdkwork.webserver.config.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&config).expect("serialize config"),
    )
    .expect("write config");
    path
}

fn spawn_data_plane(path: &Path) -> (oneshot::Sender<()>, JoinHandle<Result<(), DataPlaneError>>) {
    let compiled = load_and_compile_webserver_config(path).expect("compile data-plane config");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    (shutdown_tx, task)
}

async fn wait_until_ready(client: &reqwest::Client, base: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(response) = client
            .get(base)
            .header("host", "capacity.localhost")
            .send()
            .await
        {
            if response.status() == reqwest::StatusCode::OK {
                return;
            }
        }
        assert!(Instant::now() < deadline, "data plane did not become ready");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn percentile(sorted_micros: &[u64], percentile: f64) -> u64 {
    if sorted_micros.is_empty() {
        return 0;
    }
    let index = ((sorted_micros.len() as f64 - 1.0) * percentile).round() as usize;
    sorted_micros[index]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "bounded capacity measurement; run with pnpm test:capacity"]
async fn data_plane_sustains_bounded_load_without_errors() {
    let total_requests = std::env::var("SDKWORK_CAPACITY_REQUESTS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value >= CONCURRENCY)
        .unwrap_or(DEFAULT_REQUESTS);
    // Duration mode turns this into a soak: set
    // `SDKWORK_CAPACITY_DURATION_SECONDS=86400` for the 24h run. Without it the
    // test stops after `total_requests`.
    let duration_seconds = std::env::var("SDKWORK_CAPACITY_DURATION_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0);
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let path = write_config(directory.path(), port, 2048);
    let (shutdown, task) = spawn_data_plane(&path);

    let client = reqwest::Client::builder()
        .no_proxy()
        .pool_max_idle_per_host(CONCURRENCY)
        .build()
        .expect("client");
    let base = format!("http://127.0.0.1:{port}");
    wait_until_ready(&client, &base).await;

    // Warm the connection pool and route caches; warmup is not measured.
    for _ in 0..CONCURRENCY {
        let response = client
            .get(format!("{base}/"))
            .header("host", "capacity.localhost")
            .send()
            .await
            .expect("warmup request");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }

    let latencies = Arc::new(Mutex::new(Vec::<u64>::with_capacity(total_requests)));
    let failures = Arc::new(AtomicU64::new(0));
    let started = Instant::now();
    let soak_deadline = duration_seconds.map(|seconds| started + Duration::from_secs(seconds));
    let mut workers = Vec::with_capacity(CONCURRENCY);
    for worker in 0..CONCURRENCY {
        let client = client.clone();
        let base = base.clone();
        let latencies = Arc::clone(&latencies);
        let failures = Arc::clone(&failures);
        workers.push(tokio::spawn(async move {
            let per_worker = total_requests / CONCURRENCY;
            let mut local = Vec::with_capacity(per_worker + 1);
            // Spread the extra requests across the first workers.
            let extra = usize::from(worker < total_requests % CONCURRENCY);
            let mut issued = 0_usize;
            loop {
                if let Some(deadline) = soak_deadline {
                    if Instant::now() >= deadline {
                        break;
                    }
                } else if issued >= per_worker + extra {
                    break;
                }
                issued += 1;
                let request_started = Instant::now();
                match client
                    .get(format!("{base}/"))
                    .header("host", "capacity.localhost")
                    .send()
                    .await
                {
                    Ok(response) if response.status() == reqwest::StatusCode::OK => {
                        local.push(request_started.elapsed().as_micros() as u64);
                    }
                    _ => {
                        failures.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            latencies.lock().expect("latency buffer").extend(local);
        }));
    }
    for worker in workers {
        worker.await.expect("load worker");
    }
    let elapsed = started.elapsed();

    let mut latencies = Arc::try_unwrap(latencies)
        .expect("single latency owner")
        .into_inner()
        .expect("latency buffer");
    latencies.sort_unstable();
    let failures = failures.load(Ordering::Relaxed);
    let measured = latencies.len() as u64;
    let per_second = if elapsed.is_zero() {
        0.0
    } else {
        measured as f64 / elapsed.as_secs_f64()
    };
    eprintln!(
        "capacity smoke: requests={total_requests} measured={measured} failures={failures} \
         elapsed_ms={} throughput_rps={per_second:.0} p50_us={} p95_us={} p99_us={} max_us={}",
        elapsed.as_millis(),
        percentile(&latencies, 0.50),
        percentile(&latencies, 0.95),
        percentile(&latencies, 0.99),
        latencies.last().copied().unwrap_or(0),
    );

    shutdown.send(()).expect("send shutdown");
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("data plane drains")
        .expect("data-plane task joins")
        .expect("data plane stops cleanly");

    assert_eq!(failures, 0, "every request must succeed");
    if soak_deadline.is_none() {
        assert_eq!(measured, total_requests as u64, "every request is measured");
    }
    // Generous portable ceiling: the gate catches a stack that collapses
    // (seconds per request), not machine-to-machine variance.
    assert!(
        percentile(&latencies, 0.99) < 1_000_000,
        "p99 latency exceeded one second"
    );
}

/// Idle keep-alive connections are the shape a real edge holds between requests;
/// this measures how many the listener admits without dropping or refusing, and
/// proves a fresh request still succeeds while they are held. The count is
/// configurable because the OS ephemeral-port/file-descriptor ceiling differs per
/// host: `SDKWORK_CAPACITY_CONNECTIONS=100000 pnpm test:connections`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "bounded idle-connection measurement; run with pnpm test:connections"]
async fn data_plane_accepts_many_idle_connections() {
    let requested = std::env::var("SDKWORK_CAPACITY_CONNECTIONS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value >= 16)
        .unwrap_or(DEFAULT_IDLE_CONNECTIONS);
    let directory = TempDir::new().expect("create temp directory");
    let port = available_port();
    let max_connections = requested.saturating_add(1024).max(2048);
    let path = write_config(directory.path(), port, max_connections);
    let (shutdown, task) = spawn_data_plane(&path);

    let client = reqwest::Client::builder()
        .no_proxy()
        .pool_max_idle_per_host(4)
        .build()
        .expect("client");
    let base = format!("http://127.0.0.1:{port}");
    wait_until_ready(&client, &base).await;

    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut sockets = Vec::with_capacity(requested);
    let started = Instant::now();
    // One loopback source address has roughly 16k ephemeral ports on Windows.
    // Beyond that, rotate across 127.0.0.0/8 so 100k idle connections are a
    // property of the server, not of one address's port budget.
    let per_source_address = 10_000;
    for index in 0..requested {
        let source_index = index / per_source_address;
        let source_ip = [127, 0, 0, u8::try_from(source_index % 200 + 1).unwrap_or(1)];
        let socket = TcpSocket::new_v4().expect("create loopback socket");
        socket
            .bind(SocketAddr::from((source_ip, 0)))
            .unwrap_or_else(|cause| {
                panic!(
                    "bind source {} for connection {}: {cause}",
                    SocketAddr::from((source_ip, 0)),
                    index + 1
                )
            });
        match socket.connect(address).await {
            Ok(socket) => sockets.push(socket),
            Err(cause) => panic!(
                "connection {} of {requested} failed after {} were established: {cause}",
                index + 1,
                sockets.len()
            ),
        }
    }
    eprintln!(
        "idle connections: held={} elapsed_ms={}",
        sockets.len(),
        started.elapsed().as_millis()
    );

    // Holding the sockets must not block the accept path for a new request.
    let response = client
        .get(format!("{base}/"))
        .header("host", "capacity.localhost")
        .send()
        .await
        .expect("request while idle connections are held");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    tokio::time::sleep(Duration::from_millis(500)).await;

    drop(sockets);
    shutdown.send(()).expect("send shutdown");
    tokio::time::timeout(Duration::from_secs(20), task)
        .await
        .expect("data plane drains")
        .expect("data-plane task joins")
        .expect("data plane stops cleanly");
}

/// The PRD's route-compile budget: 10,000 routes in ≤ 1 s. This measures the
/// real config load + compile path (JSON parse, model validation, route table
/// build) and prints the elapsed time. The default assertion is generous
/// because a debug-profiled developer machine is not the release reference;
/// run with `SDKWORK_ROUTE_COMPILE_MAX_MS=1000` on the target host.
#[tokio::test]
#[ignore = "route-compile benchmark; run with pnpm test:route-compile"]
async fn compiles_ten_thousand_routes_within_budget() {
    let route_count = std::env::var("SDKWORK_ROUTE_COMPILE_ROUTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value >= 100)
        .unwrap_or(10_000);
    let max_millis = std::env::var("SDKWORK_ROUTE_COMPILE_MAX_MS")
        .ok()
        .and_then(|value| value.parse::<u128>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(5_000);
    let routes: Vec<serde_json::Value> = (0..route_count)
        .map(|index| {
            json!({
                "id": format!("route-{index}"),
                "match": { "pathType": "exact", "path": format!("/r{index}") },
                "resourceRef": "ok"
            })
        })
        .collect();
    let port = available_port();
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-route-compile",
        "limits": {
            "maxRequestBodyBytes": 1024,
            "requestTimeoutMs": 5000,
            "drainTimeoutMs": 1000,
            "maxConnections": 1024,
            "maxRequestHeaderBytes": 8192,
            "maxRequestHeaders": 64,
            "requestHeaderTimeoutMs": 5000
        },
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "compile-host",
            "maxConnections": 1024
        }],
        "resources": [{ "id": "ok", "type": "respond", "status": 200, "body": "ok" }],
        "virtualHosts": [{
            "id": "compile-host",
            "listenerRefs": ["http"],
            "serverNames": ["compile.localhost"],
            "routes": routes
        }]
    });
    let directory = TempDir::new().expect("create temp directory");
    let path = directory
        .path()
        .join("sdkwork.webserver.route-compile.json");
    fs::write(
        &path,
        serde_json::to_vec(&config).expect("serialize route-compile config"),
    )
    .expect("write route-compile config");

    let started = Instant::now();
    let compiled = load_and_compile_webserver_config(&path).expect("compile route-heavy config");
    let elapsed = started.elapsed();
    eprintln!(
        "route compile: routes={route_count} elapsed_ms={} budget_ms={max_millis}",
        elapsed.as_millis()
    );
    assert_eq!(
        compiled.config().virtual_hosts[0].routes.len(),
        route_count,
        "every route must survive compilation"
    );
    assert!(
        elapsed.as_millis() <= max_millis,
        "route compile exceeded {max_millis} ms: {elapsed:?}"
    );
}
