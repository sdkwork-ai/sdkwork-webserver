use std::{
    fs,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener as StdTcpListener},
    path::Path,
    sync::Arc,
    time::Duration,
};

use axum::{
    body::Body,
    http::{HeaderMap, Response},
    routing::any,
    Router,
};
use crc::{Crc, CRC_32_ISCSI};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use rustls::{
    pki_types::{CertificateDer, ServerName},
    ClientConfig, RootCertStore,
};
use sdkwork_api_webserver_standalone_gateway::{run_data_plane_from_config_until, DataPlaneError};
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
    time::timeout,
};
use tokio_rustls::TlsConnector;

const V2_SIGNATURE: &[u8; 12] = b"\r\n\r\n\0\r\nQUIT\n";
const CRC32C: Crc<u32> = Crc::<u32>::new(&CRC_32_ISCSI);

struct Upstream {
    address: SocketAddr,
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

fn available_port() -> u16 {
    StdTcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn spawn_upstream() -> Upstream {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let app = Router::new().fallback(any(|headers: HeaderMap| async move {
            let value = headers
                .get("x-forwarded-for")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("missing");
            Response::new(Body::from(value.to_owned()))
        }));
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });
    Upstream {
        address,
        shutdown,
        task,
    }
}

fn write_certificate(directory: &Path) -> CertificateDer<'static> {
    let mut params = CertificateParams::new(vec!["localhost".to_owned()]).unwrap();
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, "localhost");
    let key = KeyPair::generate().unwrap();
    let certificate = params.self_signed(&key).unwrap();
    fs::write(directory.join("localhost.pem"), certificate.pem()).unwrap();
    fs::write(directory.join("localhost.key"), key.serialize_pem()).unwrap();
    certificate.der().clone()
}

fn config(
    http_port: u16,
    https_port: u16,
    upstream: SocketAddr,
    trusted: &[&str],
    versions: &[&str],
) -> Value {
    json!({
        "schemaVersion": 1, "kind": "sdkwork.webserver.app", "appKey": "proxy-protocol-test",
        "limits": {"maxConnections": 64, "maxConcurrentRequests": 32, "requestTimeoutMs": 2000, "drainTimeoutMs": 1000},
        "deployment": {"drainTimeoutMs": 1000, "reload": {"mode": "watch", "pollIntervalMs": 100}},
        "listeners": [
            {"id": "http", "bind": "127.0.0.1", "port": http_port, "protocols": ["http1"], "defaultVirtualHostRef": "host", "proxyProtocol": {"trustedSourceCidrs": trusted, "versions": versions, "timeoutMs": 150, "maxHeaderBytes": 536}},
            {"id": "https", "bind": "127.0.0.1", "port": https_port, "protocols": ["http1", "http2"], "tlsPolicyRef": "tls", "defaultVirtualHostRef": "host", "proxyProtocol": {"trustedSourceCidrs": trusted, "versions": versions, "timeoutMs": 150, "maxHeaderBytes": 536}}
        ],
        "certificates": [{"id": "cert", "serverNames": ["localhost"], "source": {"type": "protected-file", "certificateFile": "localhost.pem", "privateKeyFile": "localhost.key"}}],
        "tlsPolicies": [{"id": "tls", "certificateRef": "cert", "minimumVersion": "tls1.2", "maximumVersion": "tls1.3", "alpn": ["h2", "http/1.1"]}],
        "resources": [{"id": "proxy", "type": "proxy", "upstreamRef": "origin"}],
        "upstreams": [{"id": "origin", "targets": [{"url": format!("http://127.0.0.1:{}", upstream.port())}], "addressPolicy": {"allowedCidrs": ["127.0.0.0/8"]}, "maxConnections": 8, "maxIdleConnections": 8, "maxInFlightRequests": 16}],
        "virtualHosts": [{"id": "host", "listenerRefs": ["http", "https"], "serverNames": ["localhost"], "routes": [{"id": "route", "match": {"pathType": "prefix", "path": "/"}, "resourceRef": "proxy"}]}]
    })
}

fn write_config(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn spawn_gateway(path: &Path) -> (oneshot::Sender<()>, JoinHandle<Result<(), DataPlaneError>>) {
    let path = path.to_owned();
    let (shutdown, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_from_config_until(path, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    (shutdown, task)
}

async fn wait_port(port: u16) {
    timeout(Duration::from_secs(5), async {
        loop {
            if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}

async fn raw_http(port: u16, proxy: &[u8]) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(proxy).await.unwrap();
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut response = Vec::new();
    timeout(Duration::from_secs(3), stream.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    String::from_utf8(response).unwrap()
}

async fn fragmented_http(port: u16, fragments: &[&[u8]]) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    for fragment in fragments {
        stream.write_all(fragment).await.unwrap();
        tokio::task::yield_now().await;
    }
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut response = Vec::new();
    timeout(Duration::from_secs(3), stream.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    String::from_utf8(response).unwrap()
}

async fn assert_connection_closed(mut stream: TcpStream) {
    let mut byte = [0_u8; 1];
    match timeout(Duration::from_secs(1), stream.read(&mut byte)).await {
        Ok(Ok(0)) | Ok(Err(_)) => {}
        Ok(Ok(read)) => panic!("rejected connection returned {read} payload bytes"),
        Err(_) => panic!("rejected connection remained open"),
    }
}

async fn assert_header_rejected(port: u16, header: &[u8]) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(header).await.unwrap();
    assert_connection_closed(stream).await;
}

fn v2_ipv6(source: Ipv6Addr, tlv: &[u8]) -> Vec<u8> {
    let mut header = Vec::with_capacity(52 + tlv.len());
    header.extend_from_slice(V2_SIGNATURE);
    header.extend_from_slice(&[0x21, 0x21]);
    header.extend_from_slice(&((36 + tlv.len()) as u16).to_be_bytes());
    header.extend_from_slice(&source.octets());
    header.extend_from_slice(&Ipv6Addr::LOCALHOST.octets());
    header.extend_from_slice(&44321_u16.to_be_bytes());
    header.extend_from_slice(&443_u16.to_be_bytes());
    header.extend_from_slice(tlv);
    header
}

fn v2_ipv4(source: Ipv4Addr, tlv: &[u8]) -> Vec<u8> {
    let mut header = Vec::with_capacity(28 + tlv.len());
    header.extend_from_slice(V2_SIGNATURE);
    header.extend_from_slice(&[0x21, 0x11]);
    header.extend_from_slice(&((12 + tlv.len()) as u16).to_be_bytes());
    header.extend_from_slice(&source.octets());
    header.extend_from_slice(&Ipv4Addr::LOCALHOST.octets());
    header.extend_from_slice(&44321_u16.to_be_bytes());
    header.extend_from_slice(&80_u16.to_be_bytes());
    header.extend_from_slice(tlv);
    header
}

fn v2_local(tlv: &[u8]) -> Vec<u8> {
    let mut header = Vec::with_capacity(16 + tlv.len());
    header.extend_from_slice(V2_SIGNATURE);
    header.extend_from_slice(&[0x20, 0x00]);
    header.extend_from_slice(&(tlv.len() as u16).to_be_bytes());
    header.extend_from_slice(tlv);
    header
}

fn with_crc32c(mut header: Vec<u8>) -> Vec<u8> {
    let payload_bytes = u16::from_be_bytes([header[14], header[15]]);
    let payload_bytes = payload_bytes
        .checked_add(7)
        .expect("bounded v2 test Header");
    header[14..16].copy_from_slice(&payload_bytes.to_be_bytes());
    header.extend_from_slice(&[0x03, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00]);
    let checksum = CRC32C.checksum(&header);
    let value_offset = header.len() - 4;
    header[value_offset..].copy_from_slice(&checksum.to_be_bytes());
    header
}

fn set_crc32c_policy(config: &mut Value, policy: &str) {
    config["listeners"][0]["proxyProtocol"]["crc32cPolicy"] = json!(policy);
    config["listeners"][1]["proxyProtocol"]["crc32cPolicy"] = json!(policy);
}

fn tls_client(root: CertificateDer<'static>, alpn: &[u8]) -> TlsConnector {
    let mut roots = RootCertStore::empty();
    roots.add(root).unwrap();
    let mut config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![alpn.to_vec()];
    TlsConnector::from(Arc::new(config))
}

async fn proxy_h2(port: u16, header: &[u8], root: CertificateDer<'static>) -> String {
    let mut tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    tcp.write_all(header).await.unwrap();
    let tls = tls_client(root, b"h2")
        .connect(ServerName::try_from("localhost").unwrap(), tcp)
        .await
        .unwrap();
    assert_eq!(tls.get_ref().1.alpn_protocol(), Some(b"h2".as_slice()));
    let (mut sender, connection) = h2::client::handshake(tls).await.unwrap();
    let task = tokio::spawn(async move {
        connection.await.unwrap();
    });
    let request = http::Request::builder()
        .uri("https://localhost/")
        .body(())
        .unwrap();
    let (response, _) = sender.send_request(request, true).unwrap();
    let response = response.await.unwrap();
    assert_eq!(response.status(), http::StatusCode::OK);
    let mut stream = response.into_body();
    let mut body = Vec::new();
    while let Some(chunk) = stream.data().await {
        body.extend_from_slice(&chunk.unwrap());
    }
    drop(sender);
    task.abort();
    let _ = task.await;
    String::from_utf8(body).unwrap()
}

async fn stop_gateway(shutdown: oneshot::Sender<()>, task: JoinHandle<Result<(), DataPlaneError>>) {
    shutdown.send(()).unwrap();
    timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn proxy_v1_http_and_v2_https_h2_preserve_payload_and_effective_identity() {
    let upstream = spawn_upstream().await;
    let directory = TempDir::new().unwrap();
    let root = write_certificate(directory.path());
    let path = directory.path().join("config.json");
    let http_port = available_port();
    let https_port = available_port();
    write_config(
        &path,
        &config(
            http_port,
            https_port,
            upstream.address,
            &["127.0.0.0/8"],
            &["v1", "v2"],
        ),
    );
    load_and_compile_webserver_config(&path).expect("compile PROXY integration config");
    let (shutdown, task) = spawn_gateway(&path);
    wait_port(http_port).await;
    wait_port(https_port).await;

    let response = raw_http(http_port, b"PROXY TCP4 192.0.2.10 127.0.0.1 45678 80\r\n").await;
    assert!(response.ends_with("\r\n\r\n192.0.2.10"));
    let fragmented = fragmented_http(
        http_port,
        &[b"PROXY TCP4 198.", b"51.100.7 127.0.0.1 45679 80\r", b"\n"],
    )
    .await;
    assert!(fragmented.ends_with("\r\n\r\n198.51.100.7"));
    let unknown = raw_http(http_port, b"PROXY UNKNOWN ignored opaque fields\r\n").await;
    assert!(unknown.ends_with("\r\n\r\n127.0.0.1"));
    let ipv6 = raw_http(http_port, b"PROXY TCP6 2001:db8::7 ::1 45680 80\r\n").await;
    assert!(ipv6.ends_with("\r\n\r\n2001:db8::7"));
    let mut local = Vec::from(V2_SIGNATURE.as_slice());
    local.extend_from_slice(&[0x20, 0x00, 0x00, 0x00]);
    let local = raw_http(http_port, &local).await;
    assert!(local.ends_with("\r\n\r\n127.0.0.1"));
    let ipv4 = raw_http(
        http_port,
        &v2_ipv4("203.0.113.19".parse().unwrap(), &[0x05, 0x00, 0x00]),
    )
    .await;
    assert!(ipv4.ends_with("\r\n\r\n203.0.113.19"));
    let source = "2001:db8::1234".parse().unwrap();
    let body = proxy_h2(https_port, &v2_ipv6(source, &[0x04, 0x00, 0x00]), root).await;
    assert_eq!(body, "2001:db8::1234");
    let mut ignored_crc = with_crc32c(v2_ipv4("203.0.113.20".parse().unwrap(), &[]));
    let last = ignored_crc.len() - 1;
    ignored_crc[last] ^= 0x01;
    let ignored_crc = raw_http(http_port, &ignored_crc).await;
    assert!(ignored_crc.ends_with("\r\n\r\n203.0.113.20"));

    stop_gateway(shutdown, task).await;
    upstream.shutdown.send(()).unwrap();
    upstream.task.await.unwrap();
}

#[tokio::test]
async fn proxy_v2_tlv_crc32c_policy_is_bounded_strict_and_restart_only() {
    assert_eq!(CRC32C.checksum(b"123456789"), 0xe306_9283);
    let upstream = spawn_upstream().await;
    let directory = TempDir::new().unwrap();
    let root = write_certificate(directory.path());
    let path = directory.path().join("config.json");
    let http_port = available_port();
    let https_port = available_port();
    let mut value = config(
        http_port,
        https_port,
        upstream.address,
        &["127.0.0.0/8"],
        &["v1", "v2"],
    );
    set_crc32c_policy(&mut value, "validate-if-present");
    write_config(&path, &value);
    load_and_compile_webserver_config(&path).expect("compile CRC validation policy");
    let (shutdown, task) = spawn_gateway(&path);
    wait_port(http_port).await;
    wait_port(https_port).await;

    let valid = with_crc32c(v2_ipv4(
        "198.51.100.45".parse().unwrap(),
        &[0xee, 0x00, 0x03, 0x01, 0x02, 0x03],
    ));
    let split = valid.len() - 2;
    let response =
        fragmented_http(http_port, &[&valid[..5], &valid[5..split], &valid[split..]]).await;
    assert!(response.ends_with("\r\n\r\n198.51.100.45"));

    let source = "2001:db8::46".parse().unwrap();
    let valid_h2 = with_crc32c(v2_ipv6(source, &[0xef, 0x00, 0x00]));
    assert_eq!(
        proxy_h2(https_port, &valid_h2, root.clone()).await,
        source.to_string()
    );

    let missing = raw_http(http_port, &v2_ipv4("198.51.100.47".parse().unwrap(), &[])).await;
    assert!(missing.ends_with("\r\n\r\n198.51.100.47"));

    let mut wrong = with_crc32c(v2_ipv4("198.51.100.48".parse().unwrap(), &[]));
    let last = wrong.len() - 1;
    wrong[last] ^= 0x01;
    assert_header_rejected(http_port, &wrong).await;
    for malformed in [
        v2_ipv4("198.51.100.49".parse().unwrap(), &[0xee, 0x00]),
        v2_ipv4(
            "198.51.100.49".parse().unwrap(),
            &[0xee, 0x00, 0x04, 0x01, 0x02],
        ),
        v2_ipv4(
            "198.51.100.49".parse().unwrap(),
            &[0x03, 0x00, 0x04, 0, 0, 0, 0, 0x03, 0x00, 0x04, 0, 0, 0, 0],
        ),
        v2_ipv4(
            "198.51.100.49".parse().unwrap(),
            &[0x03, 0x00, 0x03, 0, 0, 0],
        ),
    ] {
        assert_header_rejected(http_port, &malformed).await;
    }
    stop_gateway(shutdown, task).await;

    set_crc32c_policy(&mut value, "required");
    write_config(&path, &value);
    let (shutdown, task) = spawn_gateway(&path);
    wait_port(http_port).await;
    let local = raw_http(http_port, &with_crc32c(v2_local(&[]))).await;
    assert!(local.ends_with("\r\n\r\n127.0.0.1"));
    assert_header_rejected(http_port, &v2_ipv4("198.51.100.50".parse().unwrap(), &[])).await;
    assert_header_rejected(http_port, &v2_local(&[])).await;

    set_crc32c_policy(&mut value, "ignore");
    write_config(&path, &value);
    tokio::time::sleep(Duration::from_millis(350)).await;
    assert_header_rejected(http_port, &v2_ipv4("198.51.100.51".parse().unwrap(), &[])).await;
    let still_required = raw_http(
        http_port,
        &with_crc32c(v2_ipv4("198.51.100.52".parse().unwrap(), &[])),
    )
    .await;
    assert!(still_required.ends_with("\r\n\r\n198.51.100.52"));

    stop_gateway(shutdown, task).await;
    upstream.shutdown.send(()).unwrap();
    upstream.task.await.unwrap();
}

#[tokio::test]
async fn proxy_policy_rejects_missing_malformed_untrusted_and_restart_only_changes() {
    let upstream = spawn_upstream().await;
    let directory = TempDir::new().unwrap();
    write_certificate(directory.path());
    let path = directory.path().join("config.json");
    let http_port = available_port();
    let https_port = available_port();
    let mut value = config(
        http_port,
        https_port,
        upstream.address,
        &["127.0.0.0/8"],
        &["v1", "v2"],
    );
    write_config(&path, &value);
    load_and_compile_webserver_config(&path).expect("compile PROXY rejection config");
    let (shutdown, task) = spawn_gateway(&path);
    wait_port(http_port).await;

    for invalid in [
        b"GET / HTTP/1.1\r\n".as_slice(),
        b"PROXY TCP4 999.1.1.1 127.0.0.1 1 80\r\n",
        b"PROXY TCP4 192.0.2.1 127.0.0.1 01 80\r\n",
        b"PROXY TCP4 192.0.2.1 127.0.0.1 1 80\nGET / HTTP/1.1\r\n",
    ] {
        let mut stream = TcpStream::connect(("127.0.0.1", http_port)).await.unwrap();
        stream.write_all(invalid).await.unwrap();
        assert_connection_closed(stream).await;
    }
    let mut partial = TcpStream::connect(("127.0.0.1", http_port)).await.unwrap();
    partial.write_all(b"PROXY ").await.unwrap();
    assert_connection_closed(partial).await;

    for invalid_fixed in [
        [0x22, 0x11, 0x00, 0x0c],
        [0x31, 0x11, 0x00, 0x0c],
        [0x21, 0x11, 0x00, 0x0b],
        [0x21, 0x00, 0x00, 0x00],
    ] {
        let mut invalid_v2 = Vec::from(V2_SIGNATURE.as_slice());
        invalid_v2.extend_from_slice(&invalid_fixed);
        invalid_v2.extend_from_slice(&[0_u8; 12]);
        let mut stream = TcpStream::connect(("127.0.0.1", http_port)).await.unwrap();
        stream.write_all(&invalid_v2).await.unwrap();
        assert_connection_closed(stream).await;
    }
    let mut oversized = Vec::from(V2_SIGNATURE.as_slice());
    oversized.extend_from_slice(&[0x21, 0x11, 0x10, 0x00]);
    let mut stream = TcpStream::connect(("127.0.0.1", http_port)).await.unwrap();
    stream.write_all(&oversized).await.unwrap();
    assert_connection_closed(stream).await;

    value["listeners"][0]["proxyProtocol"]["versions"] = json!(["v2"]);
    write_config(&path, &value);
    tokio::time::sleep(Duration::from_millis(350)).await;
    let response = raw_http(http_port, b"PROXY TCP4 198.51.100.9 127.0.0.1 1234 80\r\n").await;
    assert!(
        response.ends_with("\r\n\r\n198.51.100.9"),
        "restart-only candidate must retain v1 policy"
    );

    stop_gateway(shutdown, task).await;

    value["listeners"][0]["proxyProtocol"]["versions"] = json!(["v1", "v2"]);
    value["listeners"][1]["proxyProtocol"]["versions"] = json!(["v1", "v2"]);
    value["listeners"][0]["proxyProtocol"]["trustedSourceCidrs"] = json!(["10.0.0.0/8"]);
    value["listeners"][1]["proxyProtocol"]["trustedSourceCidrs"] = json!(["10.0.0.0/8"]);
    write_config(&path, &value);
    let (shutdown, task) = spawn_gateway(&path);
    wait_port(http_port).await;
    let mut untrusted = TcpStream::connect(("127.0.0.1", http_port)).await.unwrap();
    untrusted
        .write_all(b"PROXY TCP4 192.0.2.1 127.0.0.1 1234 80\r\n")
        .await
        .unwrap();
    assert_connection_closed(untrusted).await;
    stop_gateway(shutdown, task).await;

    value["listeners"][0]["proxyProtocol"]["trustedSourceCidrs"] = json!(["127.0.0.0/8"]);
    value["listeners"][1]["proxyProtocol"]["trustedSourceCidrs"] = json!(["127.0.0.0/8"]);
    value["listeners"][0]["proxyProtocol"]["versions"] = json!(["v1"]);
    value["listeners"][1]["proxyProtocol"]["versions"] = json!(["v1"]);
    write_config(&path, &value);
    let (shutdown, task) = spawn_gateway(&path);
    wait_port(http_port).await;
    let mut unsupported = TcpStream::connect(("127.0.0.1", http_port)).await.unwrap();
    unsupported
        .write_all(&v2_ipv6("2001:db8::1".parse().unwrap(), &[]))
        .await
        .unwrap();
    assert_connection_closed(unsupported).await;
    stop_gateway(shutdown, task).await;

    upstream.shutdown.send(()).unwrap();
    upstream.task.await.unwrap();
}
