//! UDP stream proxying (`listen … udp`): datagrams are NAT-forwarded to the
//! configured target and replies come back to the originating client.

use std::{fs, net::TcpListener, path::PathBuf, time::Duration};

use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::{json, Value};
use tokio::{net::UdpSocket, sync::oneshot};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

/// UDP echo server: replies with the received datagram prefixed by "ECHO:".
async fn spawn_udp_echo(port: u16) {
    tokio::spawn(async move {
        let socket = UdpSocket::bind(("127.0.0.1", port))
            .await
            .expect("echo bind");
        let mut buffer = vec![0_u8; 4096];
        loop {
            let Ok((length, peer)) = socket.recv_from(&mut buffer).await else {
                break;
            };
            let reply = format!("ECHO:{}", String::from_utf8_lossy(&buffer[..length]));
            let _ = socket.send_to(reply.as_bytes(), peer).await;
        }
    });
}

fn write_config(port: u16, upstream_port: u16) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sdkwork-udp-stream-test-{}-{}",
        std::process::id(),
        port
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("config.json");
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-udp-stream-test",
        "limits": {
            "requestTimeoutMs": 5000,
            "drainTimeoutMs": 1000,
            "maxConnections": 32
        },
        "listeners": [],
        "resources": [],
        "virtualHosts": [],
        "streams": [{
            "id": "udp-dns",
            "bind": "127.0.0.1",
            "port": port,
            "protocol": "udp",
            "target": { "type": "literal", "host": "127.0.0.1", "port": upstream_port },
            "proxyTimeoutMs": 5000
        }]
    });
    fs::write(
        &path,
        serde_json::to_vec_pretty(&config).expect("serialize"),
    )
    .expect("write");
    path
}

#[tokio::test]
async fn udp_stream_forwards_datagrams_both_directions() {
    let port = free_port();
    let upstream_port = free_port();
    spawn_udp_echo(upstream_port).await;
    let config_path = write_config(port, upstream_port);
    let compiled = load_and_compile_webserver_config(&config_path).expect("compile");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    let client = UdpSocket::bind("127.0.0.1:0").await.expect("client bind");

    // Wait until the data plane UDP listener is ready.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let _ = client.send_to(b"ping", ("127.0.0.1", port)).await;
        let mut buffer = vec![0_u8; 4096];
        match tokio::time::timeout(Duration::from_millis(200), client.recv_from(&mut buffer)).await
        {
            Ok(Ok(_)) => break,
            _ if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            _ => panic!("udp stream listener not ready"),
        }
    }

    // Round-trip through the data plane to the echo upstream.
    let mut buffer = vec![0_u8; 4096];
    client
        .send_to(b"hello-udp", ("127.0.0.1", port))
        .await
        .expect("send");
    let (length, _) = tokio::time::timeout(Duration::from_secs(3), client.recv_from(&mut buffer))
        .await
        .expect("reply timeout")
        .expect("recv");
    assert_eq!(String::from_utf8_lossy(&buffer[..length]), "ECHO:hello-udp");

    let _ = shutdown_tx.send(());
    let _ = server.await;
    fs::remove_dir_all(config_path.parent().expect("config dir")).ok();
}
