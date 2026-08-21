use std::{
    fs,
    net::TcpListener,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{oneshot, watch},
    task::JoinHandle,
};

#[derive(Clone, Copy)]
enum OriginBehavior {
    Respond(u16),
    RespondWithBody(u16, &'static str),
    Close,
    Hang,
}

struct Origin {
    port: u16,
    requests: Arc<AtomicUsize>,
    shutdown: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl Origin {
    async fn start(behavior: OriginBehavior) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind retry test origin");
        let port = listener.local_addr().expect("origin address").port();
        let requests = Arc::new(AtomicUsize::new(0));
        let task_requests = requests.clone();
        let (shutdown, mut shutdown_rx) = watch::channel(false);
        let task = tokio::spawn(async move {
            loop {
                let accepted = tokio::select! {
                    biased;
                    changed = shutdown_rx.changed() => {
                        if changed.is_err() || *shutdown_rx.borrow() {
                            return;
                        }
                        continue;
                    }
                    accepted = listener.accept() => accepted,
                };
                let (mut stream, _) = accepted.expect("accept retry test request");
                task_requests.fetch_add(1, Ordering::AcqRel);
                match behavior {
                    OriginBehavior::Respond(status) => {
                        read_request(&mut stream).await;
                        let body = if status == 200 { "okay" } else { "fail" };
                        write_response(&mut stream, status, body).await;
                    }
                    OriginBehavior::RespondWithBody(status, body) => {
                        read_request(&mut stream).await;
                        write_response(&mut stream, status, body).await;
                    }
                    OriginBehavior::Close => {}
                    OriginBehavior::Hang => {
                        let _ = shutdown_rx.changed().await;
                        return;
                    }
                }
            }
        });
        Self {
            port,
            requests,
            shutdown,
            task,
        }
    }

    fn count(&self) -> usize {
        self.requests.load(Ordering::Acquire)
    }

    async fn stop(self) {
        let _ = self.shutdown.send(true);
        self.task.await.expect("origin task joins");
    }
}

async fn write_response(stream: &mut tokio::net::TcpStream, status: u16, body: &str) {
    let reason = if status == 200 {
        "OK"
    } else {
        "Service Unavailable"
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .await
        .expect("write retry test response");
}

async fn read_request(stream: &mut tokio::net::TcpStream) {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    let header_end = loop {
        let read = stream.read(&mut buffer).await.expect("read request");
        if read == 0 {
            return;
        }
        request.extend_from_slice(&buffer[..read]);
        if let Some(position) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let content_length = std::str::from_utf8(&request[..header_end])
        .expect("request Headers are UTF-8 test data")
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().expect("Content-Length"))
        })
        .unwrap_or(0);
    while request.len().saturating_sub(header_end) < content_length {
        let read = stream.read(&mut buffer).await.expect("read request Body");
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
    }
}

fn available_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve port");
    listener.local_addr().expect("port").port()
}

fn write_config(
    directory: &TempDir,
    data_port: u16,
    upstreams: Vec<Value>,
    routes: &[(&str, &str)],
) -> std::path::PathBuf {
    let resources = routes
        .iter()
        .map(|(_, upstream)| {
            json!({
                "id": format!("{upstream}-resource"),
                "type": "proxy",
                "upstreamRef": upstream
            })
        })
        .collect::<Vec<_>>();
    let route_values = routes
        .iter()
        .map(|(path, upstream)| {
            json!({
                "id": format!("{upstream}-route"),
                "match": {"pathType": "exact", "path": path},
                "resourceRef": format!("{upstream}-resource")
            })
        })
        .collect::<Vec<_>>();
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-retry-test-web",
        "limits": {"requestTimeoutMs": 2000, "drainTimeoutMs": 1000},
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": data_port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "retry-host",
            "trustedProxy": {
                "trustedCidrs": ["127.0.0.0/8"],
                "maxHops": 4,
                "maxHeaderBytes": 256
            }
        }],
        "resources": resources,
        "upstreams": upstreams,
        "virtualHosts": [{
            "id": "retry-host",
            "listenerRefs": ["http"],
            "serverNames": ["retry.localhost"],
            "routes": route_values
        }]
    });
    let path = directory.path().join("sdkwork.webserver.config.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&config).expect("serialize retry config"),
    )
    .expect("write retry config");
    path
}

fn upstream(id: &str, ports: [u16; 2], retry: Option<Value>, request_timeout_ms: u64) -> Value {
    let mut value = json!({
        "id": id,
        "targets": [
            {"url": format!("http://127.0.0.1:{}", ports[0])},
            {"url": format!("http://127.0.0.1:{}", ports[1])}
        ],
        "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]},
        "connectTimeoutMs": 200,
        "requestTimeoutMs": request_timeout_ms,
        "maxIdleConnections": 0,
        "maxConnections": 2,
        "maxInFlightRequests": 2,
        "passiveHealth": {
            "failureThreshold": 1,
            "ejectionTimeMs": 1000,
            "failureStatuses": [502, 503, 504]
        }
    });
    if let Some(retry) = retry {
        value["retry"] = retry;
    }
    value
}

async fn start_data_plane(
    path: std::path::PathBuf,
    port: u16,
) -> (oneshot::Sender<()>, JoinHandle<()>) {
    let compiled = load_and_compile_webserver_config(path).expect("compile retry config");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
        .expect("retry data plane exits cleanly");
    });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
            Ok(stream) => {
                drop(stream);
                break;
            }
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(error) => panic!("retry listener did not start: {error}"),
        }
    }
    (shutdown_tx, task)
}

async fn stop_data_plane(shutdown: oneshot::Sender<()>, task: JoinHandle<()>) {
    shutdown.send(()).expect("stop retry data plane");
    task.await.expect("retry data-plane task joins");
}

#[tokio::test]
async fn explicit_status_retry_fails_over_while_omitted_policy_stays_single_attempt() {
    let retry_failure = Origin::start(OriginBehavior::Respond(503)).await;
    let retry_success = Origin::start(OriginBehavior::Respond(200)).await;
    let single_failure = Origin::start(OriginBehavior::Respond(503)).await;
    let single_unused = Origin::start(OriginBehavior::Respond(200)).await;
    let transport_failure = Origin::start(OriginBehavior::Close).await;
    let transport_success = Origin::start(OriginBehavior::Respond(200)).await;
    let backup_failure = Origin::start(OriginBehavior::Respond(503)).await;
    let backup_success = Origin::start(OriginBehavior::Respond(200)).await;
    let exhausted_first = Origin::start(OriginBehavior::RespondWithBody(503, "first")).await;
    let exhausted_final = Origin::start(OriginBehavior::RespondWithBody(503, "final")).await;
    let directory = TempDir::new().expect("retry temp directory");
    let port = available_port();
    let mut backup_retry = upstream(
        "backup-retry",
        [backup_failure.port, backup_success.port],
        Some(json!({
            "maxAttempts": 2,
            "timeoutMs": 1000,
            "retryOn": ["http_503"]
        })),
        500,
    );
    backup_retry["loadBalancing"] = json!("least-connections");
    backup_retry["targets"][1]["backup"] = json!(true);
    let mut status_retry = upstream(
        "retry",
        [retry_failure.port, retry_success.port],
        Some(json!({
            "maxAttempts": 2,
            "timeoutMs": 1000,
            "retryOn": ["http_503"]
        })),
        500,
    );
    status_retry["loadBalancing"] = json!("ip-hash");
    let mut transport_retry = upstream(
        "transport",
        [transport_failure.port, transport_success.port],
        Some(json!({
            "maxAttempts": 2,
            "timeoutMs": 1000,
            "retryOn": ["error"]
        })),
        500,
    );
    transport_retry["loadBalancing"] = json!("least-connections");
    let path = write_config(
        &directory,
        port,
        vec![
            status_retry,
            upstream(
                "single",
                [single_failure.port, single_unused.port],
                None,
                500,
            ),
            transport_retry,
            backup_retry,
            upstream(
                "exhausted",
                [exhausted_first.port, exhausted_final.port],
                Some(json!({
                    "maxAttempts": 2,
                    "timeoutMs": 1000,
                    "retryOn": ["http_503"]
                })),
                500,
            ),
        ],
        &[
            ("/retry", "retry"),
            ("/single", "single"),
            ("/transport", "transport"),
            ("/backup-retry", "backup-retry"),
            ("/exhausted", "exhausted"),
        ],
    );
    let (shutdown, task) = start_data_plane(path, port).await;
    let client = reqwest::Client::new();

    let retried = client
        .get(format!("http://127.0.0.1:{port}/retry"))
        .header("host", "retry.localhost")
        .header("x-forwarded-for", "192.0.3.1")
        .send()
        .await
        .expect("retry request");
    assert_eq!(retried.status(), reqwest::StatusCode::OK);
    assert_eq!(retried.text().await.expect("retry response"), "okay");
    assert_eq!(retry_failure.count(), 1);
    assert_eq!(retry_success.count(), 1);

    let single = client
        .get(format!("http://127.0.0.1:{port}/single"))
        .header("host", "retry.localhost")
        .send()
        .await
        .expect("single attempt request");
    assert_eq!(single.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(single_failure.count(), 1);
    assert_eq!(single_unused.count(), 0);

    let transported = client
        .get(format!("http://127.0.0.1:{port}/transport"))
        .header("host", "retry.localhost")
        .send()
        .await
        .expect("transport retry request");
    assert_eq!(transported.status(), reqwest::StatusCode::OK);
    assert_eq!(transport_failure.count(), 1);
    assert_eq!(transport_success.count(), 1);

    let backup_retried = client
        .get(format!("http://127.0.0.1:{port}/backup-retry"))
        .header("host", "retry.localhost")
        .send()
        .await
        .expect("backup retry request");
    assert_eq!(backup_retried.status(), reqwest::StatusCode::OK);
    assert_eq!(
        backup_retried.text().await.expect("backup retry response"),
        "okay"
    );
    assert_eq!(backup_failure.count(), 1);
    assert_eq!(backup_success.count(), 1);

    let exhausted = client
        .get(format!("http://127.0.0.1:{port}/exhausted"))
        .header("host", "retry.localhost")
        .send()
        .await
        .expect("exhausted retry request");
    assert_eq!(exhausted.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        exhausted.text().await.expect("final retry response"),
        "final"
    );
    assert_eq!(exhausted_first.count(), 1);
    assert_eq!(exhausted_final.count(), 1);

    stop_data_plane(shutdown, task).await;
    retry_failure.stop().await;
    retry_success.stop().await;
    single_failure.stop().await;
    single_unused.stop().await;
    transport_failure.stop().await;
    transport_success.stop().await;
    backup_failure.stop().await;
    backup_success.stop().await;
    exhausted_first.stop().await;
    exhausted_final.stop().await;
}

#[tokio::test]
async fn non_idempotent_body_and_total_deadline_never_replay() {
    let post_failure = Origin::start(OriginBehavior::Respond(503)).await;
    let post_unused = Origin::start(OriginBehavior::Respond(200)).await;
    let body_failure = Origin::start(OriginBehavior::Respond(503)).await;
    let body_unused = Origin::start(OriginBehavior::Respond(200)).await;
    let timeout_origin = Origin::start(OriginBehavior::Hang).await;
    let timeout_unused = Origin::start(OriginBehavior::Respond(200)).await;
    let timeout_retry_origin = Origin::start(OriginBehavior::Hang).await;
    let timeout_retry_success = Origin::start(OriginBehavior::Respond(200)).await;
    let directory = TempDir::new().expect("retry temp directory");
    let port = available_port();
    let retry = || {
        Some(json!({
            "maxAttempts": 2,
            "timeoutMs": 200,
            "retryOn": ["timeout", "http_503"]
        }))
    };
    let path = write_config(
        &directory,
        port,
        vec![
            upstream("post", [post_failure.port, post_unused.port], retry(), 500),
            upstream("body", [body_failure.port, body_unused.port], retry(), 500),
            upstream(
                "timeout",
                [timeout_origin.port, timeout_unused.port],
                retry(),
                500,
            ),
            upstream(
                "timeout-retry",
                [timeout_retry_origin.port, timeout_retry_success.port],
                Some(json!({
                    "maxAttempts": 2,
                    "timeoutMs": 500,
                    "retryOn": ["timeout"]
                })),
                250,
            ),
        ],
        &[
            ("/post", "post"),
            ("/body", "body"),
            ("/timeout", "timeout"),
            ("/timeout-retry", "timeout-retry"),
        ],
    );
    let (shutdown, task) = start_data_plane(path, port).await;
    let client = reqwest::Client::new();

    let post = client
        .post(format!("http://127.0.0.1:{port}/post"))
        .header("host", "retry.localhost")
        .send()
        .await
        .expect("POST request");
    assert_eq!(post.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(post_failure.count(), 1);
    assert_eq!(post_unused.count(), 0);

    let body = client
        .get(format!("http://127.0.0.1:{port}/body"))
        .header("host", "retry.localhost")
        .body("must-not-replay")
        .send()
        .await
        .expect("body-bearing GET request");
    assert_eq!(body.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body_failure.count(), 1);
    assert_eq!(body_unused.count(), 0);

    let timeout = client
        .get(format!("http://127.0.0.1:{port}/timeout"))
        .header("host", "retry.localhost")
        .send()
        .await
        .expect("bounded retry timeout request");
    assert_eq!(timeout.status(), reqwest::StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(timeout_origin.count(), 1);
    assert_eq!(timeout_unused.count(), 0);

    let timeout_retry = client
        .get(format!("http://127.0.0.1:{port}/timeout-retry"))
        .header("host", "retry.localhost")
        .send()
        .await
        .expect("timeout failover request");
    assert_eq!(timeout_retry.status(), reqwest::StatusCode::OK);
    assert_eq!(
        timeout_retry.text().await.expect("timeout retry response"),
        "okay"
    );
    assert_eq!(timeout_retry_origin.count(), 1);
    assert_eq!(timeout_retry_success.count(), 1);

    stop_data_plane(shutdown, task).await;
    post_failure.stop().await;
    post_unused.stop().await;
    body_failure.stop().await;
    body_unused.stop().await;
    timeout_origin.stop().await;
    timeout_unused.stop().await;
    timeout_retry_origin.stop().await;
    timeout_retry_success.stop().await;
}
