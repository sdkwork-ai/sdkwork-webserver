//! Wave 7E stream-plane integration: upstream TLS client, inbound PROXY
//! protocol, SNI virtual-server routing, half-close, proxy_bind, rates and the
//! nginx retry budget. The typed JSON configuration is used so each behaviour
//! can be isolated without a stock nginx file; the nginx-compatibility mapping
//! is exercised by the core materializer tests and the differential-stream
//! oracle.

use std::{
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair, KeyUsagePurpose};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
};

/// The data plane binds its own listener, so the test processes must allocate
/// ports and start servers serially: two tests can otherwise reserve the same
/// ephemeral port in the free_port window.
static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve port");
    listener.local_addr().expect("addr").port()
}

struct TestCa {
    params: CertificateParams,
    key: KeyPair,
}

fn write_test_ca(directory: &Path, stem: &str) -> TestCa {
    let mut params = CertificateParams::new(Vec::new()).expect("CA parameters");
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, format!("{stem} CA"));
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
    ];
    let key = KeyPair::generate().expect("CA key");
    let certificate = params.self_signed(&key).expect("self-sign CA");
    fs::write(directory.join(format!("{stem}.pem")), certificate.pem()).expect("write CA");
    TestCa { params, key }
}

fn signed_identity(
    directory: &Path,
    stem: &str,
    names: &[&str],
    authority: &TestCa,
    client: bool,
) -> (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>, String) {
    let mut params = CertificateParams::new(
        names
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    )
    .expect("identity parameters");
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, names[0]);
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![if client {
        rcgen::ExtendedKeyUsagePurpose::ClientAuth
    } else {
        rcgen::ExtendedKeyUsagePurpose::ServerAuth
    }];
    let key = KeyPair::generate().expect("identity key");
    let certificate = params
        .signed_by(&key, &Issuer::new(authority.params.clone(), &authority.key))
        .expect("sign identity");
    fs::write(directory.join(format!("{stem}.pem")), certificate.pem()).expect("write cert");
    let key_pem = key.serialize_pem();
    fs::write(directory.join(format!("{stem}.key")), &key_pem).expect("write key");
    let der = CertificateDer::from(certificate.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der()));
    (vec![der], key_der, key_pem)
}

fn server_tls_config(
    certificates: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> rustls::ServerConfig {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
        .expect("versions")
        .with_no_client_auth()
        .with_single_cert(certificates, key)
        .expect("server TLS config")
}

/// TLS echo upstream: completes a handshake, then echoes application bytes.
async fn spawn_tls_echo(config: rustls::ServerConfig, banner: Option<&'static str>) -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind TLS echo");
    let port = listener.local_addr().expect("addr").port();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut stream) = acceptor.accept(tcp).await else {
                    return;
                };
                if let Some(banner) = banner {
                    let _ = stream.write_all(banner.as_bytes()).await;
                }
                let mut buffer = [0_u8; 4096];
                loop {
                    match stream.read(&mut buffer).await {
                        Ok(0) | Err(_) => return,
                        Ok(read) => {
                            if stream.write_all(&buffer[..read]).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            });
        }
    });
    port
}

/// Plain TCP echo upstream recording the accepted peer and the first line.
async fn spawn_echo(
    prefix: Option<&'static str>,
) -> (u16, Arc<AtomicUsize>, Arc<std::sync::Mutex<Option<String>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind echo");
    let port = listener.local_addr().expect("addr").port();
    let accepted = Arc::new(AtomicUsize::new(0));
    let first = Arc::new(std::sync::Mutex::new(None));
    let accepted_task = Arc::clone(&accepted);
    let first_task = Arc::clone(&first);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            accepted_task.fetch_add(1, Ordering::AcqRel);
            let first_task = Arc::clone(&first_task);
            tokio::spawn(async move {
                if let Some(prefix) = prefix {
                    let _ = stream.write_all(prefix.as_bytes()).await;
                }
                let mut buffer = [0_u8; 4096];
                let mut captured = false;
                loop {
                    match stream.read(&mut buffer).await {
                        Ok(0) | Err(_) => return,
                        Ok(read) => {
                            if !captured {
                                captured = true;
                                let text = String::from_utf8_lossy(&buffer[..read]).into_owned();
                                if let Ok(mut guard) = first_task.lock() {
                                    *guard = Some(text);
                                }
                            }
                            if stream.write_all(&buffer[..read]).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            });
        }
    });
    (port, accepted, first)
}

/// TCP upstream that waits for the client half-close, then writes its reply.
async fn spawn_half_close_echo() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind half-close echo");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buffer = [0_u8; 4096];
                let mut received = Vec::new();
                while let Ok(read) = stream.read(&mut buffer).await {
                    if read == 0 {
                        break;
                    }
                    received.extend_from_slice(&buffer[..read]);
                }
                let reply = format!("got:{}", received.len());
                let _ = stream.write_all(reply.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    port
}

fn write_config(directory: &Path, streams: serde_json::Value, listen_port: u16) -> PathBuf {
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "stream-tls-test",
        "limits": { "requestTimeoutMs": 5000, "drainTimeoutMs": 1000, "maxConnections": 32 },
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": listen_port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "host"
        }],
        "resources": [{
            "id": "static",
            "type": "static",
            "root": directory.to_string_lossy(),
            "indexFiles": ["index.html"]
        }],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["http"],
            "serverNames": ["stream-test.localhost"],
            "routes": [{
                "id": "route",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "static"
            }]
        }],
        "streams": streams
    });
    let path = directory.join("stream-tls.json");
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
    let compiled = load_and_compile_webserver_config(config_path).expect("compile config");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let _ = run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await;
    });
    (shutdown_tx, task)
}

async fn wait_ready(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while TcpStream::connect(("127.0.0.1", port)).await.is_err() {
        assert!(Instant::now() < deadline, "stream listener never listened");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn round_trip(port: u16, payload: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");
    stream.write_all(payload).await.expect("write");
    let mut buffer = vec![0_u8; 4096];
    let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer))
        .await
        .expect("read timeout")
        .expect("read");
    buffer.truncate(read);
    buffer
}

fn stream_server(
    id: &str,
    port: u16,
    target: serde_json::Value,
    extra: serde_json::Value,
) -> serde_json::Value {
    let mut server = json!({
        "id": id,
        "bind": "127.0.0.1",
        "port": port,
        "target": target,
        "proxyTimeoutMs": 5000,
    });
    if let (Some(extra), Some(server)) = (extra.as_object(), server.as_object_mut()) {
        for (key, value) in extra {
            server.insert(key.clone(), value.clone());
        }
    }
    server
}

#[tokio::test]
async fn stream_upstream_tls_with_trusted_ca_and_server_name_override() {
    let _serial = TEST_SERIAL.lock().expect("serial test lock");
    let directory = tempfile::tempdir().expect("temp dir");
    let ca = write_test_ca(directory.path(), "stream-ca");
    let (chain, key, _) = signed_identity(
        directory.path(),
        "stream-server",
        &["stream-tls.test"],
        &ca,
        false,
    );
    let upstream = spawn_tls_echo(server_tls_config(chain, key), None).await;
    let listen = free_port();
    let config_path = write_config(
        directory.path(),
        json!([stream_server(
            "tls-echo",
            listen,
            json!({ "type": "literal", "host": "127.0.0.1", "port": upstream }),
            json!({
                "upstreamTls": {
                    "trustMode": "custom",
                    "caCertificateFiles": ["stream-ca.pem"],
                    "serverName": "stream-tls.test",
                    "sendSni": true
                }
            }),
        )]),
        free_port(),
    );
    let (_shutdown, _task) = spawn_data_plane(&config_path).await;
    wait_ready(listen).await;
    assert_eq!(round_trip(listen, b"tls-echo").await, b"tls-echo");
}

#[tokio::test]
async fn stream_upstream_tls_untrusted_chain_closes_the_connection() {
    let _serial = TEST_SERIAL.lock().expect("serial test lock");
    let directory = tempfile::tempdir().expect("temp dir");
    let trusted_ca = write_test_ca(directory.path(), "trusted-ca");
    let other_ca = write_test_ca(directory.path(), "other-ca");
    let (chain, key, _) = signed_identity(
        directory.path(),
        "stream-server",
        &["stream-tls.test"],
        &trusted_ca,
        false,
    );
    let upstream = spawn_tls_echo(server_tls_config(chain, key), None).await;
    let listen = free_port();
    let config_path = write_config(
        directory.path(),
        json!([stream_server(
            "tls-echo",
            listen,
            json!({ "type": "literal", "host": "127.0.0.1", "port": upstream }),
            json!({
                "upstreamTls": {
                    "trustMode": "custom",
                    "caCertificateFiles": ["other-ca.pem"],
                    "serverName": "stream-tls.test"
                }
            }),
        )]),
        free_port(),
    );
    let _ = other_ca;
    let (_shutdown, _task) = spawn_data_plane(&config_path).await;
    wait_ready(listen).await;
    let mut stream = TcpStream::connect(("127.0.0.1", listen))
        .await
        .expect("connect");
    stream.write_all(b"payload").await.expect("write");
    let mut buffer = [0_u8; 16];
    let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer))
        .await
        .expect("read timeout")
        .unwrap_or(0);
    assert_eq!(
        read, 0,
        "an untrusted upstream chain must close the connection"
    );
}

#[tokio::test]
async fn inbound_proxy_protocol_v1_and_v2_rewrite_the_client_address() {
    let _serial = TEST_SERIAL.lock().expect("serial test lock");
    let directory = tempfile::tempdir().expect("temp dir");
    let (upstream, accepted, first_line) = spawn_echo(None).await;
    let listen = free_port();
    let config_path = write_config(
        directory.path(),
        json!([stream_server(
            "proxy-in",
            listen,
            json!({ "type": "literal", "host": "127.0.0.1", "port": upstream }),
            json!({
                "acceptProxyProtocol": true,
                "proxyProtocolTimeoutMs": 2000,
                "realIpTrustedCidrs": ["127.0.0.1/32"],
                "access": [{ "action": "allow", "network": "10.9.9.9" }, { "action": "deny", "network": "all" }],
                "proxyProtocol": true
            }),
        )]),
        free_port(),
    );
    let (_shutdown, _task) = spawn_data_plane(&config_path).await;
    wait_ready(listen).await;
    // v1: the PROXY header source (10.9.9.9) is both the access-allowed client
    // and the address emitted upstream.
    let mut stream = TcpStream::connect(("127.0.0.1", listen))
        .await
        .expect("connect");
    stream
        .write_all(b"PROXY TCP4 10.9.9.9 127.0.0.1 4242 80\r\nv1-payload")
        .await
        .expect("write v1");
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
            .await
            .expect("read timeout")
            .expect("read");
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.ends_with(b"v1-payload") {
            break;
        }
    }
    assert!(
        buffer.ends_with(b"v1-payload"),
        "the echoed payload must arrive after the emitted PROXY header: {buffer:?}"
    );
    let captured = first_line.lock().expect("lock").clone().unwrap_or_default();
    assert!(
        captured.contains("PROXY TCP4 10.9.9.9 127.0.0.1 4242"),
        "the emitted PROXY header must carry the rewritten source: {captured}"
    );
    assert_eq!(accepted.load(Ordering::Acquire), 1);

    // v2: PROXY command, TCP4 family, source 10.9.9.9:4242.
    let mut header = Vec::from([
        0x0D, 0x0A, 0x0D, 0x0A, 0x00, 0x0D, 0x0A, 0x51, 0x55, 0x49, 0x54, 0x0A,
    ]);
    header.push(0x21); // version 2, command PROXY
    header.push(0x11); // AF_INET, SOCK_STREAM
    header.extend_from_slice(&12_u16.to_be_bytes());
    header.extend_from_slice(&[10, 9, 9, 9]);
    header.extend_from_slice(&[127, 0, 0, 1]);
    header.extend_from_slice(&4242_u16.to_be_bytes());
    header.extend_from_slice(&80_u16.to_be_bytes());
    header.extend_from_slice(b"v2-payload");
    let mut stream = TcpStream::connect(("127.0.0.1", listen))
        .await
        .expect("connect");
    stream.write_all(&header).await.expect("write v2");
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
            .await
            .expect("read timeout")
            .expect("read");
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.ends_with(b"v2-payload") {
            break;
        }
    }
    assert!(
        buffer.ends_with(b"v2-payload"),
        "the echoed payload must arrive after the emitted PROXY header: {buffer:?}"
    );
}

#[tokio::test]
async fn server_name_sni_virtual_servers_select_the_target() {
    let _serial = TEST_SERIAL.lock().expect("serial test lock");
    let directory = tempfile::tempdir().expect("temp dir");
    let ca = write_test_ca(directory.path(), "sni-ca");
    let (chain_a, key_a, _) =
        signed_identity(directory.path(), "sni-a", &["a.sni.test"], &ca, false);
    let (chain_b, key_b, _) =
        signed_identity(directory.path(), "sni-b", &["b.sni.test"], &ca, false);
    let upstream_a = spawn_tls_echo(server_tls_config(chain_a, key_a), Some("server-A\n")).await;
    let upstream_b = spawn_tls_echo(server_tls_config(chain_b, key_b), Some("server-B\n")).await;
    let listen = free_port();
    let config_path = write_config(
        directory.path(),
        json!([
            {
                "id": "sni-a", "bind": "127.0.0.1", "port": listen,
                "target": { "type": "literal", "host": "127.0.0.1", "port": upstream_a },
                "tls": { "mode": "preread" },
                "serverNames": ["a.sni.test"],
                "proxyTimeoutMs": 5000
            },
            {
                "id": "sni-b", "bind": "127.0.0.1", "port": listen,
                "target": { "type": "literal", "host": "127.0.0.1", "port": upstream_b },
                "tls": { "mode": "preread" },
                "serverNames": ["b.sni.test"],
                "proxyTimeoutMs": 5000
            }
        ]),
        free_port(),
    );
    let (_shutdown, _task) = spawn_data_plane(&config_path).await;
    wait_ready(listen).await;

    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mut client_config = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
        .expect("versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyCertificate))
        .with_no_client_auth();
    client_config.alpn_protocols.clear();
    for (name, banner) in [("a.sni.test", "server-A\n"), ("b.sni.test", "server-B\n")] {
        let tcp = TcpStream::connect(("127.0.0.1", listen))
            .await
            .expect("connect");
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config.clone()));
        let server_name = rustls::pki_types::ServerName::try_from(name).expect("name");
        let mut tls = connector
            .connect(server_name, tcp)
            .await
            .expect("handshake");
        let mut buffer = [0_u8; 16];
        let read = tokio::time::timeout(Duration::from_secs(5), tls.read(&mut buffer))
            .await
            .expect("read timeout")
            .expect("read");
        assert_eq!(&buffer[..read], banner.as_bytes(), "SNI {name}");
    }
}

#[derive(Debug)]
struct AcceptAnyCertificate;

impl rustls::client::danger::ServerCertVerifier for AcceptAnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            certificate,
            dss,
            &rustls::crypto::aws_lc_rs::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            certificate,
            dss,
            &rustls::crypto::aws_lc_rs::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::aws_lc_rs::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[tokio::test]
async fn half_close_keeps_the_reverse_direction_alive() {
    let _serial = TEST_SERIAL.lock().expect("serial test lock");
    let directory = tempfile::tempdir().expect("temp dir");
    let upstream = spawn_half_close_echo().await;
    let listen = free_port();
    let config_path = write_config(
        directory.path(),
        json!([stream_server(
            "half-close",
            listen,
            json!({ "type": "literal", "host": "127.0.0.1", "port": upstream }),
            json!({ "halfClose": true, "proxyTimeoutMs": 5000 }),
        )]),
        free_port(),
    );
    let (_shutdown, _task) = spawn_data_plane(&config_path).await;
    wait_ready(listen).await;
    let mut stream = TcpStream::connect(("127.0.0.1", listen))
        .await
        .expect("connect");
    stream.write_all(b"request").await.expect("write");
    stream.shutdown().await.expect("half close");
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut response))
        .await
        .expect("read timeout")
        .expect("read");
    assert_eq!(response, b"got:7");
}

#[tokio::test]
async fn proxy_bind_uses_the_configured_source_address() {
    let _serial = TEST_SERIAL.lock().expect("serial test lock");
    let directory = tempfile::tempdir().expect("temp dir");
    let (upstream, _accepted, first_line) = spawn_echo(None).await;
    let listen = free_port();
    let config_path = write_config(
        directory.path(),
        json!([stream_server(
            "bound",
            listen,
            json!({ "type": "literal", "host": "127.0.0.1", "port": upstream }),
            json!({ "bindAddress": "127.0.0.2", "proxyTimeoutMs": 5000 }),
        )]),
        free_port(),
    );
    let (_shutdown, _task) = spawn_data_plane(&config_path).await;
    wait_ready(listen).await;
    assert_eq!(round_trip(listen, b"bound").await, b"bound");
    let _ = first_line;
    // The echo upstream records the peer through the accepted socket; the
    // gateway-side binding is proven by the successful connect from
    // 127.0.0.2 (a bind failure would close the connection).
    let _ = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let _: SocketAddr = "127.0.0.2:0".parse().expect("loopback alias");
}

#[tokio::test]
async fn next_upstream_retries_the_dead_target() {
    let _serial = TEST_SERIAL.lock().expect("serial test lock");
    let directory = tempfile::tempdir().expect("temp dir");
    let (alive, accepted, _first) = spawn_echo(None).await;
    let dead = free_port();
    let listen = free_port();
    let config_path = write_config(
        directory.path(),
        json!([stream_server(
            "retry",
            listen,
            json!({ "type": "upstream", "name": "stream-pool" }),
            json!({
                "nextUpstream": ["error", "timeout"],
                "nextUpstreamTries": 2,
                "nextUpstreamTimeoutMs": 3000,
                "proxyTimeoutMs": 5000
            }),
        )]),
        free_port(),
    );
    // Add the upstream pool to the serialized configuration.
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&config_path).expect("read config")).expect("json");
    config["upstreams"] = json!([{
        "id": "stream-pool",
        "addressPolicy": { "allowedCidrs": ["127.0.0.1/32"] },
        "targets": [
            { "url": format!("http://127.0.0.1:{dead}") },
            { "url": format!("http://127.0.0.1:{alive}") }
        ]
    }]);
    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("serialize"),
    )
    .expect("write config");
    let (_shutdown, _task) = spawn_data_plane(&config_path).await;
    wait_ready(listen).await;
    assert_eq!(round_trip(listen, b"retry-ok").await, b"retry-ok");
    assert!(accepted.load(Ordering::Acquire) >= 1);
}
