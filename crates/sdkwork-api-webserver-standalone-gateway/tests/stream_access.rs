//! nginx stream access phase (`allow`/`deny`) and `return` handler.
//!
//! Differential-probed against nginx 1.29.6:
//! - `deny 127.0.0.1; allow all;` closes the accepted connection with no
//!   bytes (peer reads EOF immediately);
//! - `return 403;` writes the literal value `403` to the connection and then
//!   closes it, without contacting an upstream.

use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    time::Duration,
};

use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener as TokioListener, TcpStream},
    sync::oneshot,
};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

/// TCP echo upstream that records whether it ever received a connection.
fn spawn_echo_upstream(port: u16) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let listener = TokioListener::bind(("127.0.0.1", port))
            .await
            .expect("echo bind");
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buffer = [0_u8; 1024];
                while let Ok(read) = socket.read(&mut buffer).await {
                    if read == 0 {
                        break;
                    }
                    if socket.write_all(&buffer[..read]).await.is_err() {
                        break;
                    }
                }
            });
        }
    })
}

fn write_config(directory: &Path, http_port: u16, streams: &[(u16, serde_json::Value)]) -> PathBuf {
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-stream-access-test",
        "limits": {
            "requestTimeoutMs": 5000,
            "drainTimeoutMs": 1000,
            "maxConnections": 32
        },
        // The app model always carries the HTTP plane; this test only exercises
        // the stream listeners.
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": http_port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "host"
        }],
        "resources": [{
            "id": "ok",
            "type": "respond",
            "status": 200,
            "contentType": "text/plain; charset=utf-8",
            "body": "ok"
        }],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["http"],
            "serverNames": ["stream-test.localhost"],
            "routes": [{
                "id": "ok-route",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "ok"
            }]
        }],
        "streams": streams
            .iter()
            .map(|(port, extra)| {
                let mut stream = json!({
                    "id": format!("stream-{port}"),
                    "bind": "127.0.0.1",
                    "port": port,
                });
                if let Some(object) = extra.as_object() {
                    for (key, value) in object {
                        stream[key] = value.clone();
                    }
                }
                stream
            })
            .collect::<Vec<_>>(),
    });
    let path = directory.join("config.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&config).expect("serialize"),
    )
    .expect("write config");
    path
}

async fn spawn_data_plane(
    config_path: &Path,
) -> (oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let compiled = load_and_compile_webserver_config(config_path).expect("compile");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let _ = run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await;
    });
    (shutdown_tx, task)
}

async fn connect(port: u16) -> TcpStream {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        match TcpStream::connect(("127.0.0.1", port)).await {
            Ok(stream) => return stream,
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => panic!("stream data plane never listened on {port}: {error}"),
        }
    }
}

#[tokio::test]
async fn stream_allow_first_match_proxies_and_deny_first_match_closes() {
    let directory = tempfile::tempdir().expect("temp dir");
    let echo_port = free_port();
    let upstream = spawn_echo_upstream(echo_port);
    let allowed_port = free_port();
    let denied_port = free_port();
    let config = write_config(
        directory.path(),
        free_port(),
        &[
            (
                allowed_port,
                json!({
                    "target": {"type": "literal", "host": "127.0.0.1", "port": echo_port},
                    "access": [
                        {"action": "allow", "network": "127.0.0.1"},
                        {"action": "deny", "network": "all"}
                    ]
                }),
            ),
            (
                denied_port,
                json!({
                    "target": {"type": "literal", "host": "127.0.0.1", "port": echo_port},
                    "access": [
                        {"action": "deny", "network": "127.0.0.1"},
                        {"action": "allow", "network": "all"}
                    ]
                }),
            ),
        ],
    );
    let (shutdown_tx, task) = spawn_data_plane(&config).await;

    // First matching rule allows loopback: the connection is proxied.
    let mut allowed = connect(allowed_port).await;
    allowed.write_all(b"ping-stream\n").await.expect("write");
    let mut echoed = vec![0_u8; 12];
    allowed.read_exact(&mut echoed).await.expect("read echo");
    assert_eq!(&echoed, b"ping-stream\n");
    drop(allowed);

    // First matching rule denies loopback: nginx closes without a byte.
    let mut denied = connect(denied_port).await;
    let mut buffer = [0_u8; 16];
    let read = tokio::time::timeout(Duration::from_secs(5), denied.read(&mut buffer))
        .await
        .expect("deny close")
        .expect("read");
    assert_eq!(read, 0, "denied stream connection must be closed empty");

    let _ = shutdown_tx.send(());
    let _ = task.await;
    upstream.abort();
}

#[tokio::test]
async fn stream_return_writes_the_literal_value_and_closes() {
    let directory = tempfile::tempdir().expect("temp dir");
    let echo_port = free_port();
    let upstream = spawn_echo_upstream(echo_port);
    let return_port = free_port();
    let config = write_config(
        directory.path(),
        free_port(),
        &[(return_port, json!({"returnValue": "403"}))],
    );
    let (shutdown_tx, task) = spawn_data_plane(&config).await;

    let mut socket = connect(return_port).await;
    let mut buffer = Vec::new();
    socket
        .read_to_end(&mut buffer)
        .await
        .expect("read return value");
    assert_eq!(
        buffer, b"403",
        "linux stream `return 403` writes the literal"
    );

    let _ = shutdown_tx.send(());
    let _ = task.await;
    upstream.abort();
}
