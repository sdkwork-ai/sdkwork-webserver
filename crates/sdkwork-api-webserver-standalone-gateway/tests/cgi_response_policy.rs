//! Wave 7D end-to-end: the CGI-family response policy executes `*_store`
//! (atomic bounded write), `*_force_ranges`, `*_limit_rate` and
//! `*_pass_request_body off` against a bounded in-process FastCGI responder.
//!
//! The byte-level nginx oracle for the same forms lives in
//! `tests/nginx/differential-protocols` (77/77 cases); this test pins the
//! runtime contract that a byte differential cannot observe: the exact file
//! that appears on disk, its mode, and the pacing window.

use std::{
    net::TcpListener as StdTcpListener,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
    time::timeout,
};

const READ_TIMEOUT: Duration = Duration::from_secs(10);

fn free_port() -> u16 {
    StdTcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sdkwork-cgi-policy-{tag}-{}-{}",
        std::process::id(),
        free_port()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

struct HttpReply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpReply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

async fn http_request(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> HttpReply {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stream = loop {
        match TcpStream::connect(("127.0.0.1", port)).await {
            Ok(stream) => break stream,
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => panic!("cannot connect: {error}"),
        }
    };
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: cgi.localhost\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await.expect("send");
    stream.write_all(body).await.expect("send body");
    let mut raw = Vec::new();
    timeout(READ_TIMEOUT, stream.read_to_end(&mut raw))
        .await
        .expect("read response")
        .expect("read");
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("header terminator");
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|value| value.parse().ok())
        .expect("status");
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .collect();
    HttpReply {
        status,
        headers,
        body: raw[split + 4..].to_vec(),
    }
}

/// One-record FastCGI responder driven by the request target/path body mode.
async fn start_fastcgi_responder(bytes_seen: Arc<std::sync::Mutex<Vec<usize>>>) -> (u16, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let bytes_seen = Arc::clone(&bytes_seen);
            tokio::spawn(async move {
                loop {
                    let mut header = [0_u8; 8];
                    if stream.read_exact(&mut header).await.is_err() {
                        return;
                    }
                    let record_type = header[1];
                    let length = u16::from_be_bytes([header[4], header[5]]) as usize;
                    let padding = header[6] as usize;
                    let mut content = vec![0_u8; length];
                    if stream.read_exact(&mut content).await.is_err() {
                        return;
                    }
                    let mut pad = vec![0_u8; padding];
                    if stream.read_exact(&mut pad).await.is_err() {
                        return;
                    }
                    if record_type == 1 {
                        // BEGIN_REQUEST: read PARAMS then the STDIN stream.
                        let mut params: Vec<(String, String)> = Vec::new();
                        let mut stdin_bytes = 0usize;
                        loop {
                            let mut next = [0_u8; 8];
                            if stream.read_exact(&mut next).await.is_err() {
                                return;
                            }
                            let next_type = next[1];
                            let next_length = u16::from_be_bytes([next[4], next[5]]) as usize;
                            let next_padding = next[6] as usize;
                            let mut payload = vec![0_u8; next_length];
                            if stream.read_exact(&mut payload).await.is_err() {
                                return;
                            }
                            let mut extra = vec![0_u8; next_padding];
                            if stream.read_exact(&mut extra).await.is_err() {
                                return;
                            }
                            match next_type {
                                4 => {
                                    let mut offset = 0usize;
                                    while offset < payload.len() {
                                        let key_len = decode_length(&payload, &mut offset);
                                        let value_len = decode_length(&payload, &mut offset);
                                        let key = String::from_utf8_lossy(
                                            &payload[offset..offset + key_len],
                                        )
                                        .into_owned();
                                        offset += key_len;
                                        let value = String::from_utf8_lossy(
                                            &payload[offset..offset + value_len],
                                        )
                                        .into_owned();
                                        offset += value_len;
                                        params.push((key, value));
                                    }
                                }
                                5 => {
                                    if next_length == 0 {
                                        break;
                                    }
                                    stdin_bytes += next_length;
                                }
                                _ => {}
                            }
                        }
                        bytes_seen.lock().expect("lock").push(stdin_bytes);
                        let uri = params
                            .iter()
                            .find(|(name, _)| name == "REQUEST_URI")
                            .map(|(_, value)| value.as_str())
                            .unwrap_or("/");
                        let (payload, content_type) = if uri.contains("/store") {
                            (b"stored-content-123".to_vec(), "text/plain")
                        } else if uri.contains("/range") {
                            (b"0123456789abcdef".to_vec(), "text/plain")
                        } else if uri.contains("/rate") {
                            (b"RRRRRRRRRRRRRRRRRRRR".repeat(1000), "text/plain")
                        } else {
                            let echoed = format!("stdin={stdin_bytes}");
                            (echoed.into_bytes(), "text/plain")
                        };
                        let response = format!(
                            "Content-Type: {content_type}\r\nContent-Length: {}\r\n\r\n",
                            payload.len()
                        );
                        let mut body = response.into_bytes();
                        body.extend_from_slice(&payload);
                        let record = fastcgi_record(6, &body);
                        let _ = stream.write_all(&record).await;
                        let _ = stream.write_all(&fastcgi_record(6, b"")).await;
                        let _ = stream
                            .write_all(&fastcgi_record(3, &[0, 0, 0, 0, 0, 0, 0, 0]))
                            .await;
                    }
                }
            });
        }
    });
    (port, 0)
}

fn decode_length(payload: &[u8], offset: &mut usize) -> usize {
    let first = payload[*offset];
    *offset += 1;
    if first < 0x80 {
        return first as usize;
    }
    let length = (((first & 0x7f) as usize) << 24)
        | ((payload[*offset] as usize) << 16)
        | ((payload[*offset + 1] as usize) << 8)
        | (payload[*offset + 2] as usize);
    *offset += 3;
    length
}

fn fastcgi_record(record_type: u8, content: &[u8]) -> Vec<u8> {
    let padding = (8 - content.len() % 8) % 8;
    let mut record = vec![
        1,
        record_type,
        0,
        1,
        (content.len() >> 8) as u8,
        content.len() as u8,
        padding as u8,
        0,
    ];
    record.extend_from_slice(content);
    record.extend(std::iter::repeat_n(0_u8, padding));
    record
}

fn write_config(path: &Path, listener_port: u16, upstream_port: u16, store: &Path) -> PathBuf {
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-cgi-response-policy",
        "limits": {
            "requestTimeoutMs": 10_000,
            "drainTimeoutMs": 1_000,
            "maxConnections": 32
        },
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": listener_port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "host"
        }],
        "upstreams": [{
            "id": "fcgi",
            "targets": [{ "url": format!("http://127.0.0.1:{upstream_port}") }],
            "loadBalancing": "round-robin",
            "addressPolicy": { "allowedCidrs": ["127.0.0.0/8", "::1/128"] }
        }],
        "resources": [{
            "id": "cgi",
            "type": "fast-cgi",
            "upstreamRef": "fcgi",
            "params": [
                { "name": "REQUEST_URI", "value": "$request_uri" },
                { "name": "REQUEST_METHOD", "value": "$request_method" },
                { "name": "CONTENT_LENGTH", "value": "$content_length" }
            ],
            "store": {
                "path": format!("{}$uri", store.to_string_lossy()),
                "accessMode": 420
            },
            "limitRateBytesPerSecond": 20000,
            "forceRanges": true,
            "passRequestBody": false
        }],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["http"],
            "serverNames": ["cgi.localhost"],
            "routes": [{
                "id": "cgi-route",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "cgi"
            }]
        }]
    });
    let path = path.join("config.json");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&config).expect("serialize"),
    )
    .expect("write config");
    path
}

fn spawn_data_plane(config: &Path) -> (oneshot::Sender<()>, JoinHandle<()>) {
    let compiled = load_and_compile_webserver_config(config).expect("compile");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let _ = run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await;
    });
    (shutdown_tx, task)
}

#[tokio::test]
async fn cgi_response_policy_executes_store_ranges_rate_and_body_suppression() {
    let stdin_sizes = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (upstream_port, _) = start_fastcgi_responder(Arc::clone(&stdin_sizes)).await;
    let listener_port = free_port();
    let directory = temp_dir("store");
    let store = directory.join("store");
    std::fs::create_dir_all(&store).expect("store dir");
    let config = write_config(&directory, listener_port, upstream_port, &store);
    let (shutdown, task) = spawn_data_plane(&config);

    // `*_store`: the upstream body is written atomically under the store root
    // with the configured access mode, and the client still gets the body.
    let stored = http_request(listener_port, "GET", "/store/f1", &[], b"").await;
    assert_eq!(stored.status, 200);
    assert_eq!(stored.body, b"stored-content-123");
    let stored_path = store.join("store").join("f1");
    let stored_body = std::fs::read(&stored_path).expect("stored file exists");
    assert_eq!(stored_body, b"stored-content-123");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&stored_path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o644);
    }

    // `*_force_ranges on`: a single range is served from the buffered response.
    let ranged = http_request(
        listener_port,
        "GET",
        "/range",
        &[("Range", "bytes=2-5")],
        b"",
    )
    .await;
    assert_eq!(ranged.status, 206);
    assert_eq!(ranged.body, b"2345");
    assert_eq!(ranged.header("content-range"), Some("bytes 2-5/16"));

    // `*_limit_rate`: 20 000 bytes at 20 KiB/s is paced, not delivered at once.
    let started = Instant::now();
    let paced = http_request(listener_port, "GET", "/rate", &[], b"").await;
    let elapsed = started.elapsed();
    assert_eq!(paced.body.len(), 20_000);
    assert!(
        elapsed >= Duration::from_millis(450),
        "limit_rate did not pace the relay: {elapsed:?}"
    );

    // `*_pass_request_body off`: the client body never reaches FastCGI STDIN.
    let uploaded = http_request(
        listener_port,
        "POST",
        "/echo",
        &[("Content-Type", "text/plain")],
        b"should-not-forward",
    )
    .await;
    assert_eq!(uploaded.status, 200);
    assert_eq!(uploaded.body, b"stdin=0");
    let sizes = stdin_sizes.lock().expect("lock").clone();
    assert_eq!(sizes.last().copied(), Some(0));

    shutdown.send(()).ok();
    let _ = task.await;
}
