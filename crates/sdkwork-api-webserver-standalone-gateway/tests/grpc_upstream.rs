//! End-to-end gRPC upstream adapter test (`grpc_pass`).
//!
//! A minimal in-process HTTP/2 gRPC responder covers the runtime path without
//! the differential slice's nginx/python dependencies: unary response
//! body+trailers, a trailers-only error response, and a non-200 HTTP status
//! pass-through. The client is a raw HTTP/1.1 socket so the trailing header
//! section is observable exactly as a gRPC client sees it over HTTP/1.1.

use std::{
    fs,
    net::{SocketAddr, TcpListener},
    path::Path,
    time::Duration,
};

use bytes::Bytes;
use h2::server;
use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::config::{ConfigFormat, ConfigLoadOptions, WebServerConfigLoader};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener as TokioTcpListener, TcpStream},
};

fn available_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    port
}

fn frame(payload: &[u8]) -> Bytes {
    let mut bytes = Vec::with_capacity(5 + payload.len());
    bytes.push(0);
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(payload);
    Bytes::from(bytes)
}

fn trailers(status: u32) -> http::HeaderMap {
    let mut map = http::HeaderMap::new();
    map.insert(
        "grpc-status",
        http::HeaderValue::from_str(&status.to_string()).expect("status"),
    );
    map
}

/// Start the deterministic HTTP/2 responder. Returns its address.
async fn spawn_grpc_responder() -> SocketAddr {
    let listener = TokioTcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind responder");
    let address = listener.local_addr().expect("responder addr");
    tokio::spawn(async move {
        loop {
            let Ok((stream, _peer)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let Ok(mut connection) = server::handshake(stream).await else {
                    return;
                };
                while let Some(Ok((request, mut respond))) = connection.accept().await {
                    let path = request.uri().path().to_owned();
                    let mut body = request.into_body();
                    while let Some(Ok(_chunk)) = body.data().await {}
                    if path.ends_with("/Unary") {
                        let mut response = http::Response::new(());
                        *response.status_mut() = http::StatusCode::OK;
                        response.headers_mut().insert(
                            "content-type",
                            http::HeaderValue::from_static("application/grpc"),
                        );
                        let Ok(mut send) = respond.send_response(response, false) else {
                            continue;
                        };
                        let _ = send.send_data(frame(b"pong"), false);
                        let _ = send.send_trailers(trailers(0));
                    } else if path.ends_with("/Error") {
                        let mut response = http::Response::new(());
                        *response.status_mut() = http::StatusCode::OK;
                        response.headers_mut().insert(
                            "content-type",
                            http::HeaderValue::from_static("application/grpc"),
                        );
                        response
                            .headers_mut()
                            .insert("grpc-status", http::HeaderValue::from_static("5"));
                        let _ = respond.send_response(response, true);
                    } else {
                        let mut response = http::Response::new(());
                        *response.status_mut() = http::StatusCode::SERVICE_UNAVAILABLE;
                        response.headers_mut().insert(
                            "content-type",
                            http::HeaderValue::from_static("application/grpc"),
                        );
                        let Ok(mut send) = respond.send_response(response, false) else {
                            continue;
                        };
                        let _ = send.send_data(Bytes::from_static(b"busy"), true);
                    }
                }
            });
        }
    });
    address
}

fn write_nginx_config(directory: &Path, listen: u16, upstream: SocketAddr) -> std::path::PathBuf {
    let path = directory.join("grpc.conf");
    fs::write(
        &path,
        format!(
            r#"
server {{
    listen 127.0.0.1:{listen};
    server_name grpc.test;
    location = /grpc.test.Echo/Unary {{ grpc_pass grpc://{upstream}; }}
    location = /grpc.test.Echo/Error {{ grpc_pass grpc://{upstream}; }}
    location = /grpc.test.Echo/Http503 {{ grpc_pass grpc://{upstream}; }}
}}
"#
        ),
    )
    .expect("write nginx config");
    path
}

fn spawn_data_plane(
    path: &Path,
) -> (
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<(), sdkwork_api_webserver_standalone_gateway::DataPlaneError>>,
) {
    let compiled = WebServerConfigLoader::new()
        .load_and_compile(
            path,
            &ConfigLoadOptions::with_format(ConfigFormat::NginxConf),
        )
        .expect("compile nginx config");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    (shutdown_tx, task)
}

/// One raw HTTP/1.1 request, returning status, decoded body bytes and the
/// chunked trailer section.
async fn raw_request(port: u16, path: &str, body: &[u8]) -> (u16, Vec<u8>, Vec<(String, String)>) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match TcpStream::connect(("127.0.0.1", port)).await {
            Ok(stream) => break stream,
            Err(error) if tokio::time::Instant::now() < deadline => {
                let _ = error;
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => panic!("data plane did not listen: {error}"),
        }
    };
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: grpc.test\r\nConnection: close\r\n\
         content-type: application/grpc\r\nte: trailers\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write request head");
    stream.write_all(body).await.expect("write request body");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("read response");
    let head_end = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("response header terminator");
    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .expect("status line");
    let chunked = lines.any(|line| {
        line.to_ascii_lowercase()
            .starts_with("transfer-encoding: chunked")
    });
    let payload = &raw[head_end + 4..];
    let mut body_out = Vec::new();
    let mut trailers = Vec::new();
    if chunked {
        let mut rest = payload;
        while let Some(newline) = rest.windows(2).position(|window| window == b"\r\n") {
            let size_line = String::from_utf8_lossy(&rest[..newline]).into_owned();
            let size =
                usize::from_str_radix(size_line.split(';').next().unwrap_or("0"), 16).unwrap_or(0);
            rest = &rest[newline + 2..];
            if size == 0 {
                for line in String::from_utf8_lossy(rest).split("\r\n") {
                    if let Some((name, value)) = line.split_once(':') {
                        trailers.push((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
                    }
                }
                break;
            }
            body_out.extend_from_slice(&rest[..size]);
            rest = &rest[size + 2..];
        }
    } else {
        body_out.extend_from_slice(payload);
    }
    (status, body_out, trailers)
}

#[tokio::test]
async fn grpc_pass_preserves_unary_body_and_trailers() {
    let directory = TempDir::new().expect("temp dir");
    let responder = spawn_grpc_responder().await;
    let listen = available_port();
    let config = write_nginx_config(directory.path(), listen, responder);
    let (shutdown_tx, task) = spawn_data_plane(&config);
    let body = frame(b"ping");

    let (status, response_body, trailers) =
        raw_request(listen, "/grpc.test.Echo/Unary", &body).await;
    assert_eq!(status, 200);
    assert_eq!(response_body, frame(b"pong").to_vec());
    assert_eq!(
        trailers,
        vec![("grpc-status".to_owned(), "0".to_owned())],
        "downstream must receive the upstream trailing status"
    );

    // Trailers-only response: nginx renders the trailing fields as response
    // headers with an empty body.
    let (status, response_body, trailers) =
        raw_request(listen, "/grpc.test.Echo/Error", &body).await;
    assert_eq!(status, 200);
    assert!(response_body.is_empty());
    assert!(trailers.is_empty());
    let mut stream = TcpStream::connect(("127.0.0.1", listen))
        .await
        .expect("reconnect");
    stream
        .write_all(
            format!(
                "POST /grpc.test.Echo/Error HTTP/1.1\r\nHost: grpc.test\r\nConnection: close\r\n\
                 content-type: application/grpc\r\nte: trailers\r\nContent-Length: {}\r\n\r\n",
                body.len()
            )
            .as_bytes(),
        )
        .await
        .expect("write request");
    stream.write_all(&body).await.expect("write body");
    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .await
        .expect("read trailers-only");
    let head = String::from_utf8_lossy(&raw).to_ascii_lowercase();
    assert!(head.contains("grpc-status: 5"), "headers: {head}");
    assert!(head.contains("content-length: 0"), "headers: {head}");

    // A non-200 upstream HTTP status passes through (nginx mapping).
    let (status, _body, _trailers) = raw_request(listen, "/grpc.test.Echo/Http503", &body).await;
    assert_eq!(status, 503);

    stop_data_plane(shutdown_tx, task).await;
}

async fn stop_data_plane(
    shutdown: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<
        Result<(), sdkwork_api_webserver_standalone_gateway::DataPlaneError>,
    >,
) {
    let _ = shutdown.send(());
    let outcome = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("data plane stopped");
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => panic!("data plane failed: {error}"),
        Err(error) => panic!("data plane task panicked: {error}"),
    }
}
