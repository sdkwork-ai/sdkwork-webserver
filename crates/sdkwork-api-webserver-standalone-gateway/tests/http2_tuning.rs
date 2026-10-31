//! HTTP/2 ingress tuning (`http2_max_concurrent_streams`,
//! `http2_chunk_size`) over a real TLS/ALPN h2 connection.
//!
//! nginx's `http2_max_concurrent_streams` maps onto the runtime ingress
//! profile (`limits.http2MaxConcurrentStreams`), which the pinned hyper/h2
//! stack publishes as `SETTINGS_MAX_CONCURRENT_STREAMS`. The nginx directive
//! itself is covered by the mapping unit tests in
//! `sdkwork-webserver-core`; this test proves the compiled limit actually
//! reaches a live HTTP/2 connection:
//!
//! 1. ALPN negotiates `h2`;
//! 2. the server's first SETTINGS frame advertises the configured stream
//!    ceiling (parsed from the wire, not from our own config);
//! 3. a functional request over h2 succeeds;
//! 4. a request whose header list exceeds the configured
//!    `http2MaxHeaderListBytes` is rejected instead of being served.
//!
//! nginx's `http2_chunk_size` is not exposed by hyper/h2's server API, so the
//! runtime re-chunks the response body itself (`data_plane/http2_chunk.rs`).
//! The `http2_data_frames_respect_the_configured_chunk_size` test acts as an h2
//! frame-capturing client: it tees the decrypted server bytes, parses every
//! raw DATA frame, and asserts each frame is at most the configured chunk while
//! the reassembled body stays byte-identical.

use std::{
    fs, io,
    net::TcpListener,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration,
};

use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose};
use rustls::pki_types::{CertificateDer, ServerName};
use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::json;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf},
    net::TcpStream,
    sync::oneshot,
};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

/// Self-signed server identity plus the DER the h2 client should trust.
fn write_identity(directory: &Path, names: &[&str]) -> (PathBuf, PathBuf, Vec<u8>) {
    let mut ca_params = CertificateParams::new(Vec::new()).expect("CA parameters");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.distinguished_name = Default::default();
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "h2 tuning test CA");
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let ca_key = KeyPair::generate().expect("CA key");
    let ca_certificate = ca_params.self_signed(&ca_key).expect("CA certificate");

    let mut params = CertificateParams::new(
        names
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    )
    .expect("server parameters");
    params.distinguished_name = Default::default();
    params.distinguished_name.push(DnType::CommonName, names[0]);
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    let key = KeyPair::generate().expect("server key");
    let certificate = params
        .signed_by(&key, &Issuer::new(ca_params, &ca_key))
        .expect("sign server certificate");

    let cert_path = directory.join("server.pem");
    let key_path = directory.join("server.key");
    fs::write(&cert_path, certificate.pem()).expect("write certificate");
    fs::write(&key_path, key.serialize_pem()).expect("write key");
    (cert_path, key_path, ca_certificate.der().to_vec())
}

fn write_config(directory: &Path, port: u16, streams: u32, header_list_bytes: u32) -> PathBuf {
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-h2-tuning-test",
        "limits": {
            "requestTimeoutMs": 5000,
            "drainTimeoutMs": 1000,
            "maxConnections": 32,
            "http2MaxConcurrentStreams": streams,
            "http2MaxHeaderListBytes": header_list_bytes
        },
        "listeners": [{
            "id": "h2",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http2"],
            "tlsPolicyRef": "tls",
            "defaultVirtualHostRef": "host"
        }],
        "certificates": [{
            "id": "server-cert",
            "serverNames": ["h2.localhost"],
            "source": {
                "type": "protected-file",
                "certificateFile": "server.pem",
                "privateKeyFile": "server.key"
            }
        }],
        "tlsPolicies": [{
            "id": "tls",
            "certificateRef": "server-cert",
            "minimumVersion": "tls1.2",
            "maximumVersion": "tls1.3",
            "alpn": ["h2"]
        }],
        "resources": [{
            "id": "ok",
            "type": "respond",
            "status": 200,
            "contentType": "text/plain; charset=utf-8",
            "body": "h2-ok"
        }],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["h2"],
            "serverNames": ["h2.localhost"],
            "routes": [{
                "id": "ok-route",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "ok"
            }]
        }]
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

/// TLS client config that trusts the test CA and offers `h2`.
fn h2_client_config(ca_der: Vec<u8>) -> Arc<rustls::ClientConfig> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(CertificateDer::from(ca_der))
        .expect("trust test CA");
    let mut config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"h2".to_vec()];
    Arc::new(config)
}

async fn connect_h2(
    port: u16,
    config: Arc<rustls::ClientConfig>,
) -> tokio_rustls::client::TlsStream<TcpStream> {
    let connector = tokio_rustls::TlsConnector::from(config);
    // The data plane binds asynchronously after the task starts; a complete
    // workspace test run competes for CPU, so allow a generous readiness window.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let stream = loop {
        match TcpStream::connect(("127.0.0.1", port)).await {
            Ok(stream) => break stream,
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => panic!("data plane never listened on {port}: {error}"),
        }
    };
    let server_name = ServerName::try_from("h2.localhost").expect("server name");
    let tls = connector
        .connect(server_name, stream)
        .await
        .expect("tls handshake");
    assert_eq!(
        tls.get_ref().1.alpn_protocol(),
        Some(b"h2".as_ref()),
        "ALPN must negotiate h2"
    );
    tls
}

/// Read the server's initial SETTINGS frame and return
/// `SETTINGS_MAX_CONCURRENT_STREAMS` / `SETTINGS_MAX_HEADER_LIST_SIZE`.
async fn read_server_settings<IO>(io: &mut IO) -> (Option<u32>, Option<u32>)
where
    IO: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;

    io.write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")
        .await
        .expect("preface");
    // An empty client SETTINGS frame (type 0x4, flags 0, stream 0).
    io.write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0])
        .await
        .expect("client settings");
    let mut max_concurrent_streams = None;
    let mut max_header_list_size = None;
    let mut payload = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline {
        let mut header = [0_u8; 9];
        io.read_exact(&mut header).await.expect("frame header");
        let length =
            usize::from(header[0]) << 16 | usize::from(header[1]) << 8 | usize::from(header[2]);
        let frame_type = header[3];
        let flags = header[4];
        payload.resize(length, 0);
        if length > 0 {
            io.read_exact(&mut payload).await.expect("frame payload");
        }
        if frame_type == 0x4 && flags & 0x1 == 0 {
            let mut index = 0;
            while index + 6 <= payload.len() {
                let id = u16::from_be_bytes([payload[index], payload[index + 1]]);
                let value = u32::from_be_bytes([
                    payload[index + 2],
                    payload[index + 3],
                    payload[index + 4],
                    payload[index + 5],
                ]);
                match id {
                    0x3 => max_concurrent_streams = Some(value),
                    0x6 => max_header_list_size = Some(value),
                    _ => {}
                }
                index += 6;
            }
            // ACK the server SETTINGS so the connection can proceed.
            io.write_all(&[0, 0, 0, 4, 1, 0, 0, 0, 0])
                .await
                .expect("settings ack");
            break;
        }
    }
    (max_concurrent_streams, max_header_list_size)
}

#[tokio::test]
async fn http2_settings_publish_the_configured_stream_ceiling() {
    let directory = tempfile::tempdir().expect("temp dir");
    let port = free_port();
    let (_cert, _key, ca_der) = write_identity(directory.path(), &["h2.localhost"]);
    let config_path = write_config(directory.path(), port, 7, 64 * 1024);
    let (shutdown_tx, task) = spawn_data_plane(&config_path).await;
    let tls = connect_h2(port, h2_client_config(ca_der)).await;
    let mut tls = tls;
    let (streams, header_list) = read_server_settings(&mut tls).await;
    assert_eq!(
        streams,
        Some(7),
        "SETTINGS_MAX_CONCURRENT_STREAMS must carry http2MaxConcurrentStreams"
    );
    assert_eq!(
        header_list,
        Some(64 * 1024),
        "SETTINGS_MAX_HEADER_LIST_SIZE must carry http2MaxHeaderListBytes"
    );
    let _ = shutdown_tx.send(());
    let _ = task.await;
}

#[tokio::test]
async fn http2_requests_succeed_and_oversized_header_lists_are_rejected() {
    let directory = tempfile::tempdir().expect("temp dir");
    let port = free_port();
    let (_cert, _key, ca_der) = write_identity(directory.path(), &["h2.localhost"]);
    let config_path = write_config(directory.path(), port, 7, 1024);
    let (shutdown_tx, task) = spawn_data_plane(&config_path).await;
    let tls = connect_h2(port, h2_client_config(ca_der)).await;
    let (mut client, connection) = h2::client::handshake(tls).await.expect("h2 handshake");
    let connection_task = tokio::spawn(async move {
        let _ = connection.await;
    });

    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri("https://h2.localhost/")
        .body(())
        .expect("request");
    let (response, send_stream) = client.send_request(request, true).expect("send");
    let response = response.await.expect("response");
    assert_eq!(response.status(), http::StatusCode::OK);
    drop(send_stream);

    // A header list above the configured 1 KiB ceiling must not be served.
    let large_value = "x".repeat(4096);
    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri("https://h2.localhost/")
        .header("x-huge-header", large_value)
        .body(())
        .expect("request");
    let oversized = match client.send_request(request, true) {
        Ok((response, _send_stream)) => response.await.map(|response| response.status()),
        Err(_) => {
            // The client may refuse locally once the server's settings (or a
            // stream reset) reject the header block.
            Ok(http::StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE)
        }
    };
    if let Ok(status) = oversized {
        assert_ne!(
            status,
            http::StatusCode::OK,
            "an oversized header list must not be served"
        );
    }

    drop(client);
    connection_task.abort();
    let _ = shutdown_tx.send(());
    let _ = task.await;
}

/// The 64 KiB body served by the `http2_chunk_size` routes.
fn chunk_body() -> Vec<u8> {
    (0..64 * 1024).map(|index| (index % 251) as u8).collect()
}

/// Static fixture + typed config for the DATA-chunking test.
///
/// `/chunked/` carries route-level `http2ChunkSize: 1024`; `/default/` carries
/// none, so nginx's 8 KiB default applies.
fn write_chunk_config(directory: &Path, port: u16) -> PathBuf {
    let public = directory.join("public");
    fs::create_dir_all(public.join("chunked")).expect("chunked dir");
    fs::create_dir_all(public.join("default")).expect("default dir");
    fs::write(public.join("chunked/big.bin"), chunk_body()).expect("write chunked body");
    fs::write(public.join("default/big.bin"), chunk_body()).expect("write default body");

    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-h2-chunk-test",
        "limits": {
            "requestTimeoutMs": 5000,
            "drainTimeoutMs": 1000,
            "maxConnections": 32
        },
        "listeners": [{
            "id": "h2",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http2"],
            "tlsPolicyRef": "tls",
            "defaultVirtualHostRef": "host"
        }],
        "certificates": [{
            "id": "server-cert",
            "serverNames": ["h2.localhost"],
            "source": {
                "type": "protected-file",
                "certificateFile": "server.pem",
                "privateKeyFile": "server.key"
            }
        }],
        "tlsPolicies": [{
            "id": "tls",
            "certificateRef": "server-cert",
            "minimumVersion": "tls1.2",
            "maximumVersion": "tls1.3",
            "alpn": ["h2"]
        }],
        "resources": [{
            "id": "big",
            "type": "static",
            "root": "public",
            "indexFiles": ["index.html"],
            "followSymlinks": false
        }],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["h2"],
            "serverNames": ["h2.localhost"],
            "routes": [
                {
                    "id": "chunked",
                    "match": { "pathType": "prefix", "path": "/chunked/" },
                    "resourceRef": "big",
                    "http2ChunkSize": 1024
                },
                {
                    "id": "default",
                    "match": { "pathType": "prefix", "path": "/default/" },
                    "resourceRef": "big"
                }
            ]
        }]
    });
    let path = directory.join("chunk-config.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&config).expect("serialize"),
    )
    .expect("write config");
    path
}

/// A stream wrapper that records every byte the server writes.
struct RecordingStream<S> {
    inner: S,
    recorded: Arc<Mutex<Vec<u8>>>,
}

impl<S> RecordingStream<S> {
    fn new(inner: S, recorded: Arc<Mutex<Vec<u8>>>) -> Self {
        Self { inner, recorded }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for RecordingStream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let before = buf.filled().len();
        match Pin::new(&mut this.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                let filled = buf.filled();
                if filled.len() > before {
                    if let Ok(mut log) = this.recorded.lock() {
                        log.extend_from_slice(&filled[before..]);
                    }
                }
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for RecordingStream<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

/// Parse a captured HTTP/2 byte stream and return every DATA-frame payload
/// length on the server-to-client direction.
fn data_frame_lengths(bytes: &[u8]) -> Vec<usize> {
    let mut lengths = Vec::new();
    let mut offset = 0_usize;
    while offset + 9 <= bytes.len() {
        let length = usize::from(bytes[offset]) << 16
            | usize::from(bytes[offset + 1]) << 8
            | usize::from(bytes[offset + 2]);
        let frame_type = bytes[offset + 3];
        // Stop at a partially captured trailing frame: the socket tee records
        // reads, so the last frame may have been cut off when the body ended.
        if offset + 9 + length > bytes.len() {
            break;
        }
        if frame_type == 0 {
            lengths.push(length);
        }
        offset += 9 + length;
    }
    lengths
}

/// Perform one h2 request and return `(DATA frame lengths, reassembled body)`.
async fn capture_data_frames(port: u16, ca_der: Vec<u8>, path: &str) -> (Vec<usize>, Vec<u8>) {
    let tls = connect_h2(port, h2_client_config(ca_der)).await;
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let stream = RecordingStream::new(tls, Arc::clone(&recorded));
    let (mut client, connection) = h2::client::handshake(stream).await.expect("h2 handshake");
    let connection_task = tokio::spawn(async move {
        let _ = connection.await;
    });

    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri(format!("https://h2.localhost{path}"))
        .body(())
        .expect("request");
    let (response, send_stream) = client.send_request(request, true).expect("send");
    let response = response.await.expect("response");
    assert_eq!(response.status(), http::StatusCode::OK);
    drop(send_stream);

    let mut body = response.into_body();
    let mut collected = Vec::new();
    while let Some(chunk) = body.data().await {
        let chunk = chunk.expect("data frame");
        collected.extend_from_slice(&chunk);
        // h2's receive window is not released automatically: without this the
        // connection stalls once the initial 64 KiB window is exhausted, which
        // is exactly this test's body size.
        body.flow_control()
            .release_capacity(chunk.len())
            .expect("release h2 flow-control capacity");
    }
    drop(client);
    connection_task.abort();

    let bytes = recorded.lock().expect("recorded bytes").clone();
    (data_frame_lengths(&bytes), collected)
}

/// `http2_chunk_size` is not wire-observable over HTTP/1 (the differential
/// harness), so this test captures the real h2 DATA frames instead.
#[tokio::test]
async fn http2_data_frames_respect_the_configured_chunk_size() {
    let directory = tempfile::tempdir().expect("temp dir");
    let port = free_port();
    let (_cert, _key, ca_der) = write_identity(directory.path(), &["h2.localhost"]);
    let config_path = write_chunk_config(directory.path(), port);
    let (shutdown_tx, task) = spawn_data_plane(&config_path).await;

    let (chunked_frames, chunked_body) =
        capture_data_frames(port, ca_der.clone(), "/chunked/big.bin").await;
    assert!(!chunked_frames.is_empty(), "no DATA frames were captured");
    assert!(
        chunked_frames.iter().all(|length| *length <= 1024),
        "every DATA frame must be at most the configured 1 KiB chunk: {chunked_frames:?}"
    );
    assert!(
        chunked_frames.iter().any(|length| *length == 1024),
        "the 1 KiB boundary should actually be used: {chunked_frames:?}"
    );
    assert_eq!(
        chunked_body,
        chunk_body(),
        "reassembled body must be byte-identical"
    );

    let (default_frames, default_body) =
        capture_data_frames(port, ca_der, "/default/big.bin").await;
    assert!(!default_frames.is_empty(), "no DATA frames were captured");
    assert!(
        default_frames.iter().all(|length| *length <= 8 * 1024),
        "every DATA frame must be at most nginx's 8 KiB default: {default_frames:?}"
    );
    assert!(
        default_frames.iter().any(|length| *length > 1024),
        "the default-route frames must exceed the 1 KiB route, proving the \
         directive changes the boundary: {default_frames:?}"
    );
    assert!(
        default_frames.len() < chunked_frames.len(),
        "the 1 KiB route must produce more frames than the 8 KiB default: \
         {chunked_frames:?} vs {default_frames:?}"
    );
    assert_eq!(
        default_body,
        chunk_body(),
        "reassembled default body must be byte-identical"
    );

    let _ = shutdown_tx.send(());
    let _ = task.await;
}
