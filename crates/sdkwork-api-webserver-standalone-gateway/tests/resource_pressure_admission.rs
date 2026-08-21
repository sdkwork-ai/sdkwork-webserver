#![cfg(any(target_os = "windows", target_os = "linux"))]

use std::{
    fs::{self, File, OpenOptions},
    net::TcpListener as StdTcpListener,
    path::{Path, PathBuf},
    time::Duration,
};

use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use reqwest::{Client, Method, StatusCode, Version};
use sdkwork_api_webserver_standalone_gateway::run_data_plane_from_config_until;
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::oneshot,
    task::JoinHandle,
    time::{sleep, timeout, Instant},
};

const HOST: &str = "pressure.test";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn enforces_resource_pressure_admission_and_supervisor_lifecycle() {
    let baseline_handles = current_open_handles();
    let maximum_handles = baseline_handles
        .checked_add(512)
        .filter(|value| *value <= 1_048_576)
        .expect("test process handle count leaves bounded test headroom");
    let admission_handles = effective_threshold(maximum_handles, 16, 99);
    let recovery_percent = (1_u8..99)
        .rev()
        .find(|percent| {
            let threshold = effective_threshold(maximum_handles, 16, *percent);
            threshold < admission_handles && threshold > baseline_handles.saturating_add(64)
        })
        .expect("derive a distinct recovery threshold above startup overhead");

    let directory = TempDir::new().expect("create resource-pressure test directory");
    write_self_signed_certificate(directory.path(), "pressure", &["localhost", HOST]);
    let port = available_port();
    let config = pressure_config(port, maximum_handles, recovery_percent, "business-v1");
    let config_path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_watched_data_plane(&config_path);
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .http2_prior_knowledge()
        .pool_max_idle_per_host(1)
        .resolve(HOST, ([127, 0, 0, 1], port).into())
        .build()
        .expect("build HTTP/2 pressure test client");
    let base_url = format!("https://{HOST}:{port}");

    let ready = wait_for_status(&client, &base_url, Method::GET, "/business", StatusCode::OK).await;
    assert_eq!(ready.version(), Version::HTTP_2);
    assert_eq!(
        ready.text().await.expect("read initial Body"),
        "business-v1"
    );

    let held = open_handles_until_pressure(
        directory.path().join("held-handles"),
        admission_handles.saturating_add(8),
    );
    assert!(
        current_open_handles() >= admission_handles,
        "real process handles must cross the configured admission threshold"
    );

    let pressured = wait_for_status(
        &client,
        &base_url,
        Method::GET,
        "/business",
        StatusCode::SERVICE_UNAVAILABLE,
    )
    .await;
    assert_eq!(pressured.version(), Version::HTTP_2);
    assert_eq!(pressured.headers()["retry-after"], "1");
    assert_eq!(
        pressured.text().await.expect("read pressure response"),
        "server resource pressure is active\n"
    );

    for (method, path) in [
        (Method::GET, "/readyz"),
        (Method::GET, "/livez"),
        (Method::POST, "/healthz"),
        (Method::GET, "/missing"),
    ] {
        let response = send(&client, &base_url, method, path)
            .await
            .expect("established HTTP/2 request during pressure");
        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{path} must not borrow operations capacity"
        );
        assert_eq!(response.headers()["retry-after"], "1");
        let _ = response.bytes().await.expect("drain rejected response");
    }

    for method in [Method::GET, Method::HEAD] {
        let response = send(&client, &base_url, method, "/healthz")
            .await
            .expect("reserved health operation during pressure");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.version(), Version::HTTP_2);
        let _ = response.bytes().await.expect("drain health response");
    }

    assert_new_socket_is_closed_while_pressured(port).await;

    drop(held);
    let recovered =
        wait_for_status(&client, &base_url, Method::GET, "/business", StatusCode::OK).await;
    assert_eq!(recovered.version(), Version::HTTP_2);
    assert_eq!(
        recovered.text().await.expect("read recovered Body"),
        "business-v1"
    );

    let mut restart_candidate = pressure_config(
        port,
        maximum_handles.saturating_add(1),
        recovery_percent,
        "business-v2",
    );
    restart_candidate["deployment"]["reload"] = config["deployment"]["reload"].clone();
    write_config(directory.path(), &restart_candidate);
    sleep(Duration::from_millis(500)).await;
    let retained = send(&client, &base_url, Method::GET, "/business")
        .await
        .expect("active generation survives restart-only candidate");
    assert_eq!(retained.status(), StatusCode::OK);
    assert_eq!(
        retained.text().await.expect("read retained Body"),
        "business-v1"
    );

    let reloadable = pressure_config(port, maximum_handles, recovery_percent, "business-v3");
    write_config(directory.path(), &reloadable);
    wait_for_body(&client, &base_url, "/business", "business-v3").await;

    drop(client);
    shutdown.send(()).expect("request data-plane shutdown");
    timeout(Duration::from_secs(5), task)
        .await
        .expect("shutdown joins listener and resource sampler tasks")
        .expect("data-plane task joins")
        .expect("data plane shuts down cleanly");
}

fn pressure_config(
    port: u16,
    maximum_handles: u64,
    recovery_percent: u8,
    business_body: &str,
) -> Value {
    json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-resource-pressure-test",
        "limits": {
            "maxRequestBodyBytes": 1_048_576,
            "requestTimeoutMs": 5_000,
            "drainTimeoutMs": 1_000,
            "maxConnections": 32,
            "maxConcurrentRequests": 8
        },
        "listeners": [{
            "id": "https",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http2"],
            "tlsPolicyRef": "tls",
            "defaultVirtualHostRef": "pressure-host",
            "maxConnections": 16
        }],
        "certificates": [{
            "id": "pressure-certificate",
            "serverNames": ["localhost", HOST],
            "source": {
                "type": "protected-file",
                "certificateFile": "pressure.pem",
                "privateKeyFile": "pressure.key"
            }
        }],
        "tlsPolicies": [{
            "id": "tls",
            "certificateRef": "pressure-certificate",
            "minimumVersion": "tls1.2",
            "maximumVersion": "tls1.3",
            "alpn": ["h2"]
        }],
        "resources": [
            {
                "id": "health-response",
                "type": "respond",
                "status": 200,
                "body": "ok\n"
            },
            {
                "id": "business-response",
                "type": "respond",
                "status": 200,
                "body": business_body
            },
            {
                "id": "ready-response",
                "type": "respond",
                "status": 200,
                "body": "not-reserved\n"
            },
            {
                "id": "live-proxy",
                "type": "proxy",
                "upstreamRef": "unreachable"
            }
        ],
        "upstreams": [{
            "id": "unreachable",
            "targets": [{"url": "http://127.0.0.1:9"}],
            "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]},
            "connectTimeoutMs": 100,
            "requestTimeoutMs": 500
        }],
        "virtualHosts": [{
            "id": "pressure-host",
            "listenerRefs": ["https"],
            "serverNames": [HOST],
            "routes": [
                {
                    "id": "health",
                    "match": {
                        "pathType": "exact",
                        "path": "/healthz",
                        "methods": ["GET", "HEAD", "POST"]
                    },
                    "resourceRef": "health-response"
                },
                {
                    "id": "ready-prefix",
                    "match": {"pathType": "prefix", "path": "/readyz"},
                    "resourceRef": "ready-response"
                },
                {
                    "id": "live-route",
                    "match": {"pathType": "exact", "path": "/livez"},
                    "resourceRef": "live-proxy"
                },
                {
                    "id": "business",
                    "match": {"pathType": "exact", "path": "/business"},
                    "resourceRef": "business-response"
                }
            ]
        }],
        "deployment": {
            "drainTimeoutMs": 1_000,
            "reload": {"mode": "watch", "pollIntervalMs": 100},
            "resourcePressure": {
                "sampleIntervalMs": 50,
                "maximumProcessMemoryBytes": 17_592_186_044_416_u64,
                "memoryReserveBytes": 16_777_216,
                "memoryAdmissionPercent": 99,
                "memoryRecoveryPercent": 95,
                "maximumOpenHandles": maximum_handles,
                "openHandleReserve": 16,
                "openHandleAdmissionPercent": 99,
                "openHandleRecoveryPercent": recovery_percent,
                "eventLoopLagAdmissionMs": 10_000,
                "eventLoopLagRecoveryMs": 9_999,
                "consecutivePressureSamples": 2,
                "consecutiveRecoverySamples": 2,
                "operationsReserveRequests": 2,
                "sampleFailurePolicy": "fail-closed"
            }
        }
    })
}

fn write_config(directory: &Path, config: &Value) -> PathBuf {
    let path = directory.join("sdkwork.webserver.config.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(config).expect("serialize pressure config"),
    )
    .expect("write pressure config");
    path
}

fn spawn_watched_data_plane(
    config_path: &Path,
) -> (
    oneshot::Sender<()>,
    JoinHandle<Result<(), sdkwork_api_webserver_standalone_gateway::DataPlaneError>>,
) {
    load_and_compile_webserver_config(config_path)
        .expect("compile watched resource-pressure configuration before startup");
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

async fn send(
    client: &Client,
    base_url: &str,
    method: Method,
    path: &str,
) -> Result<reqwest::Response, reqwest::Error> {
    client
        .request(method, format!("{base_url}{path}"))
        .send()
        .await
}

async fn wait_for_status(
    client: &Client,
    base_url: &str,
    method: Method,
    path: &str,
    expected: StatusCode,
) -> reqwest::Response {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut last_error = None;
    let mut last_response = None;
    loop {
        match send(client, base_url, method.clone(), path).await {
            Ok(response) if response.status() == expected => return response,
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                last_response = Some((status, body));
            }
            Err(error) => last_error = Some(error),
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {expected} on {path}; last response: {last_response:?}; last client error: {last_error:?}"
        );
        sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_for_body(client: &Client, base_url: &str, path: &str, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(response) = send(client, base_url, Method::GET, path).await {
            if response.status() == StatusCode::OK
                && response.text().await.ok().as_deref() == Some(expected)
            {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for Body {expected}"
        );
        sleep(Duration::from_millis(25)).await;
    }
}

fn open_handles_until_pressure(directory: PathBuf, target: u64) -> Vec<File> {
    fs::create_dir_all(&directory).expect("create held-handle directory");
    let mut files = Vec::with_capacity(768);
    while current_open_handles() < target && files.len() < 768 {
        let path = directory.join(format!("handle-{}.tmp", files.len()));
        files.push(
            OpenOptions::new()
                .create(true)
                .truncate(true)
                .read(true)
                .write(true)
                .open(path)
                .expect("open held file handle"),
        );
    }
    assert!(
        current_open_handles() >= target,
        "bounded test handle set did not reach the configured threshold"
    );
    files
}

async fn assert_new_socket_is_closed_while_pressured(port: u16) {
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("TCP handshake can complete before the pressured accept loop closes it");
    let _ = stream.write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n").await;
    let mut byte = [0_u8; 1];
    match timeout(Duration::from_secs(2), stream.read(&mut byte)).await {
        Ok(Ok(0)) | Ok(Err(_)) => {}
        Ok(Ok(_)) => panic!("pressured new socket unexpectedly reached HTTP handling"),
        Err(_) => panic!("pressured new socket was not closed promptly"),
    }
}

fn effective_threshold(limit: u64, reserve: u64, percent: u8) -> u64 {
    (((limit as u128 * percent as u128) / 100).min(u64::MAX as u128) as u64)
        .min(limit.saturating_sub(reserve))
}

fn available_port() -> u16 {
    StdTcpListener::bind(("127.0.0.1", 0))
        .expect("bind ephemeral test port")
        .local_addr()
        .expect("read ephemeral test port")
        .port()
}

fn write_self_signed_certificate(directory: &Path, stem: &str, names: &[&str]) {
    let mut params = CertificateParams::new(
        names
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    )
    .expect("certificate parameters");
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, names[0]);
    let key = KeyPair::generate().expect("generate test key");
    let certificate = params.self_signed(&key).expect("generate test certificate");
    fs::write(directory.join(format!("{stem}.pem")), certificate.pem())
        .expect("write test certificate");
    fs::write(directory.join(format!("{stem}.key")), key.serialize_pem())
        .expect("write test private key");
}

#[cfg(target_os = "windows")]
fn current_open_handles() -> u64 {
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};

    // SAFETY: GetCurrentProcess returns a process-local pseudo handle with no ownership transfer.
    let process = unsafe { GetCurrentProcess() };
    let mut handles = 0_u32;
    assert_ne!(
        // SAFETY: `process` is the valid current-process pseudo handle and `handles` is writable.
        unsafe { GetProcessHandleCount(process, &mut handles) },
        0,
        "sample current process handle count"
    );
    handles as u64
}

#[cfg(target_os = "linux")]
fn current_open_handles() -> u64 {
    fs::read_dir("/proc/self/fd")
        .expect("read current process file descriptors")
        .count() as u64
}
