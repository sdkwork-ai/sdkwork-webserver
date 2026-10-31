//! End-to-end nginx mail client-certificate verification flows (Wave 14B).
//!
//! These tests cover what the byte-level real-nginx differential cannot
//! observe in isolation: the deferred post-handshake verification mode matrix,
//! `ssl_verify_depth` enforcement, the protocol-specific rejection lines, and
//! the exact `auth_http` certificate headers. The real-nginx counterpart lives
//! in `tests/nginx/differential-mail/client-cert`.
//!
//! Expected wire strings and header shapes were probed against nginx 1.29.6
//! (OpenSSL build with `--with-mail --with-mail_ssl_module`).

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use sdkwork_api_webserver_standalone_gateway::{run_data_plane_until, DataPlaneError};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
    time::timeout,
};

const READ_TIMEOUT: Duration = Duration::from_secs(5);

fn available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve a port");
    listener.local_addr().expect("port").port()
}

fn write_config(directory: &Path, config: &Value) -> PathBuf {
    let path = directory.join("sdkwork.webserver.config.json");
    std::fs::write(&path, serde_json::to_vec_pretty(config).expect("serialize"))
        .expect("write config");
    path
}

fn mail_config(servers: Vec<Value>) -> Value {
    json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "mail-client-cert-test",
        "limits": {
            "maxConcurrentRequests": 32,
            "requestTimeoutMs": 5_000,
            "drainTimeoutMs": 1_000,
            "maxConnections": 64
        },
        "listeners": [],
        "resources": [],
        "virtualHosts": [],
        "mail": { "servers": servers }
    })
}

fn mail_server(id: &str, port: u16, protocol: &str, extra: Value) -> Value {
    let mut server = json!({
        "id": id,
        "bind": "127.0.0.1",
        "port": port,
        "protocol": protocol,
        "serverName": "mail.example.com",
        "timeoutMs": 10_000,
    });
    for (key, value) in extra.as_object().expect("extra object") {
        server[key] = value.clone();
    }
    server
}

fn spawn_data_plane(
    config_path: &Path,
) -> (oneshot::Sender<()>, JoinHandle<Result<(), DataPlaneError>>) {
    let compiled = sdkwork_webserver_core::load_and_compile_webserver_config(config_path)
        .expect("compile config");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        run_data_plane_until(compiled, async move {
            let _ = shutdown_rx.await;
        })
        .await
    });
    (shutdown_tx, task)
}

async fn connect(port: u16) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match TcpStream::connect(("127.0.0.1", port)).await {
            Ok(stream) => return stream,
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => panic!("cannot connect to {port}: {error}"),
        }
    }
}

async fn read_available<S>(stream: &mut S) -> Vec<u8>
where
    S: tokio::io::AsyncRead + Unpin,
{
    let mut buffer = [0_u8; 8192];
    match timeout(READ_TIMEOUT, stream.read(&mut buffer)).await {
        Ok(Ok(read)) => buffer[..read].to_vec(),
        Ok(Err(error)) => panic!("read failed: {error}"),
        Err(_) => panic!("timed out waiting for a mail response"),
    }
}

async fn read_until_closed<S>(stream: &mut S) -> Vec<u8>
where
    S: tokio::io::AsyncRead + Unpin,
{
    let mut collected = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        match timeout(Duration::from_secs(6), stream.read(&mut buffer)).await {
            Ok(Ok(0)) => return collected,
            Ok(Ok(read)) => collected.extend_from_slice(&buffer[..read]),
            Ok(Err(_)) => return collected,
            Err(_) => panic!("mail connection did not close"),
        }
    }
}

/// One recorded auth_http request.
#[derive(Clone, Debug)]
struct AuthRequest {
    head: String,
}

impl AuthRequest {
    fn value(&self, name: &str) -> Option<&str> {
        self.head.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }
}

async fn start_auth_http(backend_port: u16) -> (u16, Arc<Mutex<Vec<AuthRequest>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind auth http");
    let port = listener.local_addr().expect("addr").port();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&requests);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let requests = Arc::clone(&recorded);
            tokio::spawn(async move {
                let mut buffer = Vec::new();
                let mut chunk = [0_u8; 4096];
                loop {
                    let Ok(read) = stream.read(&mut chunk).await else {
                        return;
                    };
                    if read == 0 {
                        return;
                    }
                    buffer.extend_from_slice(&chunk[..read]);
                    if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let head = String::from_utf8_lossy(&buffer)
                    .split("\r\n\r\n")
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                requests.lock().expect("lock").push(AuthRequest { head });
                let response = format!(
                    "HTTP/1.0 200 OK\r\nAuth-Status: OK\r\nAuth-Server: 127.0.0.1\r\nAuth-Port: {backend_port}\r\nAuth-User: alice\r\nAuth-Pass: backend-secret\r\n\r\n"
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    (port, requests)
}

async fn start_line_backend(greeting: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind backend");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let _ = stream.write_all(greeting.as_bytes()).await;
                let mut pending = Vec::new();
                let mut chunk = [0_u8; 1024];
                loop {
                    let Ok(read) = stream.read(&mut chunk).await else {
                        return;
                    };
                    if read == 0 {
                        return;
                    }
                    pending.extend_from_slice(&chunk[..read]);
                    while let Some(position) =
                        pending.windows(2).position(|window| window == b"\r\n")
                    {
                        let line = String::from_utf8_lossy(&pending[..position]).into_owned();
                        pending.drain(..position + 2);
                        let reply = if line.to_ascii_uppercase().starts_with("QUIT") {
                            "221 bye\r\n"
                        } else if line.to_ascii_uppercase().starts_with("AUTH") {
                            "235 2.0.0 ok\r\n"
                        } else if line.to_ascii_uppercase().starts_with("USER") {
                            "+OK user\r\n"
                        } else if line.to_ascii_uppercase().starts_with("PASS") {
                            "+OK logged in\r\n"
                        } else if line.to_ascii_uppercase().starts_with("LOGIN") {
                            "+ go\r\n"
                        } else {
                            "250 ok\r\n"
                        };
                        if stream.write_all(reply.as_bytes()).await.is_err() {
                            return;
                        }
                    }
                }
            });
        }
    });
    port
}

// ---------------------------------------------------------------------------
// Client PKI
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ClientIdentity {
    certificate_pem: String,
    key_pem: String,
}

impl ClientIdentity {
    fn certificate_chain(&self) -> Vec<rustls::pki_types::CertificateDer<'static>> {
        use rustls::pki_types::pem::PemObject;
        rustls::pki_types::CertificateDer::pem_slice_iter(self.certificate_pem.as_bytes())
            .collect::<Result<Vec<_>, _>>()
            .expect("client certificate chain")
    }

    fn private_key(&self) -> rustls::pki_types::PrivateKeyDer<'static> {
        use rustls::pki_types::pem::PemObject;
        rustls::pki_types::PrivateKeyDer::from_pem_slice(self.key_pem.as_bytes())
            .expect("client private key")
    }
}

struct TestPki {
    server_certificate_path: PathBuf,
    server_key_path: PathBuf,
    ca_file: PathBuf,
    trusted: ClientIdentity,
    untrusted: ClientIdentity,
    self_signed: ClientIdentity,
    /// Leaf issued by an intermediate CA (depth 1), with the intermediate appended.
    intermediate_chain: ClientIdentity,
    /// The intermediate CA certificate alone, for `ssl_trusted_certificate`.
    intermediate_ca: PathBuf,
    /// Leaf issued by the intermediate CA without the intermediate appended.
    intermediate_leaf: ClientIdentity,
    /// Root-issued leaf carrying a large non-critical extension, used to prove
    /// the auth_http certificate header is not truncated at 8 KiB.
    large: ClientIdentity,
}

fn install_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }
}

fn write_pair(directory: &Path, name: &str, certificate: &str, key: &str) -> (PathBuf, PathBuf) {
    let certificate_path = directory.join(format!("{name}.pem"));
    let key_path = directory.join(format!("{name}.key"));
    std::fs::write(&certificate_path, certificate).expect("write certificate");
    std::fs::write(&key_path, key).expect("write key");
    (certificate_path, key_path)
}

fn generate_pki(directory: &Path) -> TestPki {
    use rcgen::{
        BasicConstraints, CertificateParams, CertifiedIssuer, DnType, ExtendedKeyUsagePurpose,
        IsCa, KeyPair, KeyUsagePurpose,
    };
    install_crypto_provider();
    let root_key = KeyPair::generate().expect("root key");
    let mut root_params = CertificateParams::new(Vec::<String>::new()).expect("root params");
    root_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    root_params
        .distinguished_name
        .push(DnType::CommonName, "SDKWork Test Client Root CA");
    root_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let root = CertifiedIssuer::self_signed(root_params, root_key).expect("root ca");
    let ca_file = directory.join("client-ca.pem");
    std::fs::write(&ca_file, root.pem()).expect("write client CA");

    let other_key = KeyPair::generate().expect("other root key");
    let mut other_params = CertificateParams::new(Vec::<String>::new()).expect("other params");
    other_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    other_params
        .distinguished_name
        .push(DnType::CommonName, "Untrusted Client Root CA");
    other_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let other = CertifiedIssuer::self_signed(other_params, other_key).expect("untrusted ca");

    let intermediate_key = KeyPair::generate().expect("intermediate key");
    let mut intermediate_params =
        CertificateParams::new(Vec::<String>::new()).expect("intermediate");
    intermediate_params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    intermediate_params
        .distinguished_name
        .push(DnType::CommonName, "SDKWork Test Client Intermediate CA");
    intermediate_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let intermediate = CertifiedIssuer::signed_by(intermediate_params, intermediate_key, &root)
        .expect("intermediate ca");

    let leaf = |name: &str, issuer: &rcgen::Issuer<'_, KeyPair>| -> ClientIdentity {
        let key = KeyPair::generate().expect("leaf key");
        let mut params = CertificateParams::new(vec![name.to_owned()]).expect("leaf params");
        params.distinguished_name.push(DnType::CommonName, name);
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        let certificate = params.signed_by(&key, issuer).expect("leaf certificate");
        ClientIdentity {
            certificate_pem: certificate.pem(),
            key_pem: key.serialize_pem(),
        }
    };

    let trusted = leaf("alice@mail.example.com", &root);
    let untrusted = leaf("mallory@untrusted.example.com", &other);
    let mut large_params =
        CertificateParams::new(vec!["large@mail.example.com".to_owned()]).expect("large params");
    large_params
        .distinguished_name
        .push(DnType::CommonName, "large@mail.example.com");
    large_params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    large_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    large_params
        .custom_extensions
        .push(rcgen::CustomExtension::from_oid_content(
            &[1, 2, 3, 4],
            large_extension_content(9000),
        ));
    let large_key = KeyPair::generate().expect("large key");
    let large_certificate = large_params
        .signed_by(&large_key, &root)
        .expect("large certificate");
    let large = ClientIdentity {
        certificate_pem: large_certificate.pem(),
        key_pem: large_key.serialize_pem(),
    };
    let intermediate_leaf = leaf("bob@mail.example.com", &intermediate);
    let intermediate_chain = ClientIdentity {
        certificate_pem: format!(
            "{}{}",
            intermediate_leaf.certificate_pem,
            intermediate.pem()
        ),
        key_pem: intermediate_leaf.key_pem.clone(),
    };

    let self_key = KeyPair::generate().expect("self key");
    let mut self_params =
        CertificateParams::new(vec!["self@mail.example.com".to_owned()]).expect("self params");
    self_params
        .distinguished_name
        .push(DnType::CommonName, "self@mail.example.com");
    self_params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    self_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    let self_certificate = self_params.self_signed(&self_key).expect("self-signed");
    let self_signed = ClientIdentity {
        certificate_pem: self_certificate.pem(),
        key_pem: self_key.serialize_pem(),
    };

    // A self-signed frontend certificate (the client disables verification).
    let mut server_params =
        CertificateParams::new(vec!["mail.example.com".to_owned()]).expect("server params");
    server_params
        .distinguished_name
        .push(DnType::CommonName, "mail.example.com");
    let server_key = KeyPair::generate().expect("server key");
    let server_certificate = server_params
        .self_signed(&server_key)
        .expect("server certificate");
    let intermediate_ca = directory.join("intermediate-ca.pem");
    std::fs::write(&intermediate_ca, intermediate.pem()).expect("write intermediate");

    let (server_certificate_path, server_key_path) = write_pair(
        directory,
        "server",
        &server_certificate.pem(),
        &server_key.serialize_pem(),
    );
    TestPki {
        server_certificate_path,
        server_key_path,
        ca_file,
        trusted,
        untrusted,
        self_signed,
        intermediate_chain,
        intermediate_ca,
        intermediate_leaf,
        large,
    }
}

/// A DER OCTET STRING with `length` bytes of filler, used to inflate a
/// certificate beyond 8 KiB.
fn large_extension_content(length: usize) -> Vec<u8> {
    let mut content = vec![0x04];
    if length < 0x80 {
        content.push(u8::try_from(length).expect("short length"));
    } else if length <= 0xff {
        content.extend_from_slice(&[0x81, u8::try_from(length).expect("medium length")]);
    } else {
        content.extend_from_slice(&[
            0x82,
            u8::try_from((length >> 8) & 0xff).expect("high byte"),
            u8::try_from(length & 0xff).expect("low byte"),
        ]);
    }
    content.extend(std::iter::repeat_n(0xaa, length));
    content
}

#[derive(Debug)]
struct AcceptAnyCertificate;

impl rustls::client::danger::ServerCertVerifier for AcceptAnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _certificate: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _certificate: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

fn client_tls_config(identity: Option<&ClientIdentity>) -> rustls::ClientConfig {
    install_crypto_provider();
    let builder = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyCertificate));
    match identity {
        Some(identity) => builder
            .with_client_auth_cert(identity.certificate_chain(), identity.private_key())
            .expect("client identity"),
        None => builder.with_no_client_auth(),
    }
}

async fn tls_connect(
    port: u16,
    identity: Option<&ClientIdentity>,
) -> tokio_rustls::client::TlsStream<TcpStream> {
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_tls_config(identity)));
    let stream = connect(port).await;
    let name = rustls::pki_types::ServerName::try_from("mail.example.com").expect("server name");
    connector.connect(name, stream).await.expect("TLS connect")
}

fn tls_server_config(pki: &TestPki, implicit: bool) -> Value {
    json!({
        "certificateFile": pki.server_certificate_path.to_string_lossy(),
        "privateKeyFile": pki.server_key_path.to_string_lossy(),
        "implicit": implicit,
        "starttls": if implicit { "off" } else { "on" },
    })
}

fn verify_tls_server_config(pki: &TestPki, verify: &str, depth: u32) -> Value {
    let mut tls = tls_server_config(pki, true);
    tls["verifyClient"] = json!(verify);
    tls["verifyDepth"] = json!(depth);
    tls["clientCertificateFile"] = json!(pki.ca_file.to_string_lossy());
    tls
}

fn decode_mail_auth(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).expect("hex");
            let byte = u8::from_str_radix(hex, 16).expect("percent escape");
            out.push(byte as char);
            index += 3;
        } else {
            out.push(bytes[index] as char);
            index += 1;
        }
    }
    out
}

/// Wait for one recorded auth_http request.
async fn wait_for_request(requests: &Arc<Mutex<Vec<AuthRequest>>>) -> AuthRequest {
    let deadline = Instant::now() + READ_TIMEOUT;
    loop {
        if let Some(request) = requests.lock().expect("lock").last() {
            return request.clone();
        }
        assert!(
            Instant::now() < deadline,
            "the auth_http service received no request"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn smtp_authenticate(stream: &mut tokio_rustls::client::TlsStream<TcpStream>) -> Vec<u8> {
    stream
        .write_all(b"EHLO client.example\r\n")
        .await
        .expect("ehlo");
    let _ = read_available(stream).await;
    stream
        .write_all(b"AUTH PLAIN AGFsaWNlAHNlY3JldA==\r\n")
        .await
        .expect("auth");
    read_available(stream).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn verify_on_accepts_a_trusted_certificate_and_reports_nginx_facts() {
    let directory = tempfile::tempdir().expect("tempdir");
    let pki = generate_pki(directory.path());
    let backend_port = start_line_backend("220 backend ESMTP\r\n").await;
    let (auth_port, requests) = start_auth_http(backend_port).await;
    let port = available_port();
    let tls = verify_tls_server_config(&pki, "on", 1);
    let config = mail_config(vec![mail_server(
        "smtp-tls",
        port,
        "smtp",
        json!({
            "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth"), "passClientCert": true },
            "tls": tls,
        }),
    )]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let mut stream = tls_connect(port, Some(&pki.trusted)).await;
    assert_eq!(
        read_available(&mut stream).await,
        b"220 mail.example.com ESMTP ready\r\n"
    );
    let _ = smtp_authenticate(&mut stream).await;
    let request = wait_for_request(&requests).await;
    assert_eq!(request.value("Auth-SSL"), Some("on"));
    assert_eq!(request.value("Auth-SSL-Verify"), Some("SUCCESS"));
    assert_eq!(
        request.value("Auth-SSL-Subject"),
        Some("CN=alice@mail.example.com")
    );
    assert_eq!(
        request.value("Auth-SSL-Issuer"),
        Some("CN=SDKWork Test Client Root CA")
    );
    let serial = request.value("Auth-SSL-Serial").expect("serial");
    assert!(
        !serial.is_empty() && serial.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "serial is hex: {serial}"
    );
    let fingerprint = request.value("Auth-SSL-Fingerprint").expect("fingerprint");
    assert_eq!(fingerprint.len(), 40, "SHA-1 hex fingerprint");
    assert!(
        fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "fingerprint is lowercase hex: {fingerprint}"
    );
    let certificate = request.value("Auth-SSL-Cert").expect("certificate header");
    assert_eq!(decode_mail_auth(certificate), pki.trusted.certificate_pem);

    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}

#[tokio::test]
async fn a_large_client_certificate_is_forwarded_without_an_eight_kib_refusal() {
    // nginx 1.29.6 does not impose an 8 KiB auth_http certificate-header
    // refusal: the probe forwarded a 13.9 KiB `Auth-SSL-Cert` header in a
    // 14.5 KiB request. Lock the runtime to the same behaviour instead of
    // inventing a limit nginx does not have.
    let directory = tempfile::tempdir().expect("tempdir");
    let pki = generate_pki(directory.path());
    assert!(
        pki.large.certificate_pem.len() > 8 * 1024,
        "the fixture certificate must exceed 8 KiB"
    );
    let backend_port = start_line_backend("220 backend ESMTP\r\n").await;
    let (auth_port, requests) = start_auth_http(backend_port).await;
    let port = available_port();
    let tls = verify_tls_server_config(&pki, "on", 1);
    let config = mail_config(vec![mail_server(
        "smtp-tls",
        port,
        "smtp",
        json!({
            "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth"), "passClientCert": true },
            "tls": tls,
        }),
    )]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let mut stream = tls_connect(port, Some(&pki.large)).await;
    assert_eq!(
        read_available(&mut stream).await,
        b"220 mail.example.com ESMTP ready\r\n"
    );
    let _ = smtp_authenticate(&mut stream).await;
    let request = wait_for_request(&requests).await;
    assert_eq!(request.value("Auth-SSL-Verify"), Some("SUCCESS"));
    let certificate = request.value("Auth-SSL-Cert").expect("certificate header");
    assert!(
        certificate.len() > 8 * 1024,
        "the certificate header is not truncated: {} bytes",
        certificate.len()
    );
    assert_eq!(decode_mail_auth(certificate), pki.large.certificate_pem);

    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}

#[tokio::test]
async fn verify_on_rejects_untrusted_and_missing_certificates_before_auth_http() {
    let directory = tempfile::tempdir().expect("tempdir");
    let pki = generate_pki(directory.path());
    let backend_port = start_line_backend("220 backend ESMTP\r\n").await;
    let (auth_port, requests) = start_auth_http(backend_port).await;
    let port = available_port();
    let tls = verify_tls_server_config(&pki, "on", 1);
    let config = mail_config(vec![mail_server(
        "smtp-tls",
        port,
        "smtp",
        json!({
            "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth"), "passClientCert": true },
            "tls": tls,
        }),
    )]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let mut untrusted = tls_connect(port, Some(&pki.untrusted)).await;
    assert_eq!(
        read_until_closed(&mut untrusted).await,
        b"421 4.7.1 SSL certificate error\r\n"
    );
    let mut missing = tls_connect(port, None).await;
    assert_eq!(
        read_until_closed(&mut missing).await,
        b"421 4.7.1 No required SSL certificate\r\n"
    );
    assert!(
        requests.lock().expect("lock").is_empty(),
        "rejected sessions never reach auth_http"
    );

    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}

#[tokio::test]
async fn optional_and_optional_no_ca_follow_nginx_verification_matrix() {
    let directory = tempfile::tempdir().expect("tempdir");
    let pki = generate_pki(directory.path());
    let backend_port = start_line_backend("220 backend ESMTP\r\n").await;
    let (auth_port, requests) = start_auth_http(backend_port).await;
    let optional_port = available_port();
    let no_ca_port = available_port();

    let optional_tls = verify_tls_server_config(&pki, "optional", 1);
    let mut no_ca_tls = tls_server_config(&pki, true);
    no_ca_tls["verifyClient"] = json!("optional-no-ca");
    no_ca_tls["verifyDepth"] = json!(1);
    let config = mail_config(vec![
        mail_server(
            "smtp-optional",
            optional_port,
            "smtp",
            json!({
                "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth"), "passClientCert": true },
                "tls": optional_tls,
            }),
        ),
        mail_server(
            "smtp-no-ca",
            no_ca_port,
            "smtp",
            json!({
                "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth"), "passClientCert": true },
                "tls": no_ca_tls,
            }),
        ),
    ]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    // optional + absent certificate: accepted, `Auth-SSL-Verify: NONE`.
    let mut missing = tls_connect(optional_port, None).await;
    assert_eq!(
        read_available(&mut missing).await,
        b"220 mail.example.com ESMTP ready\r\n"
    );
    let _ = smtp_authenticate(&mut missing).await;
    let request = wait_for_request(&requests).await;
    assert_eq!(request.value("Auth-SSL-Verify"), Some("NONE"));
    assert_eq!(request.value("Auth-SSL-Subject"), None);
    assert_eq!(request.value("Auth-SSL-Cert"), None);
    requests.lock().expect("lock").clear();

    // optional + untrusted certificate: rejected.
    let mut untrusted = tls_connect(optional_port, Some(&pki.untrusted)).await;
    assert_eq!(
        read_until_closed(&mut untrusted).await,
        b"421 4.7.1 SSL certificate error\r\n"
    );

    // optional_no_ca + untrusted certificate: accepted and recorded.
    let mut tolerated = tls_connect(no_ca_port, Some(&pki.untrusted)).await;
    assert_eq!(
        read_available(&mut tolerated).await,
        b"220 mail.example.com ESMTP ready\r\n"
    );
    let _ = smtp_authenticate(&mut tolerated).await;
    let request = wait_for_request(&requests).await;
    assert_eq!(
        request.value("Auth-SSL-Verify"),
        Some("FAILED:unable to verify the first certificate")
    );
    assert_eq!(
        request.value("Auth-SSL-Subject"),
        Some("CN=mallory@untrusted.example.com")
    );
    assert_eq!(
        decode_mail_auth(request.value("Auth-SSL-Cert").expect("certificate header")),
        pki.untrusted.certificate_pem
    );
    requests.lock().expect("lock").clear();

    // optional_no_ca + self-signed certificate: accepted and recorded.
    let mut self_signed = tls_connect(no_ca_port, Some(&pki.self_signed)).await;
    assert_eq!(
        read_available(&mut self_signed).await,
        b"220 mail.example.com ESMTP ready\r\n"
    );
    let _ = smtp_authenticate(&mut self_signed).await;
    let request = wait_for_request(&requests).await;
    assert_eq!(
        request.value("Auth-SSL-Verify"),
        Some("FAILED:self-signed certificate")
    );

    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}

#[tokio::test]
async fn verify_depth_counts_intermediate_ca_certificates() {
    let directory = tempfile::tempdir().expect("tempdir");
    let pki = generate_pki(directory.path());
    let backend_port = start_line_backend("220 backend ESMTP\r\n").await;
    let (auth_port, _requests) = start_auth_http(backend_port).await;
    let depth0_port = available_port();
    let depth1_port = available_port();
    let depth2_port = available_port();
    let trusted_intermediate_port = available_port();

    let depth0 = verify_tls_server_config(&pki, "on", 0);
    let depth1 = verify_tls_server_config(&pki, "on", 1);
    let depth2 = verify_tls_server_config(&pki, "on", 2);
    let mut trusted_intermediate = verify_tls_server_config(&pki, "on", 1);
    trusted_intermediate["trustedCertificateFile"] = json!(pki.intermediate_ca.to_string_lossy());
    let config = mail_config(vec![
        mail_server(
            "depth0",
            depth0_port,
            "smtp",
            json!({ "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth") }, "tls": depth0 }),
        ),
        mail_server(
            "depth1",
            depth1_port,
            "smtp",
            json!({ "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth") }, "tls": depth1 }),
        ),
        mail_server(
            "depth2",
            depth2_port,
            "smtp",
            json!({ "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth") }, "tls": depth2 }),
        ),
        mail_server(
            "trusted-intermediate",
            trusted_intermediate_port,
            "smtp",
            json!({ "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth") }, "tls": trusted_intermediate }),
        ),
    ]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    // depth 0 + direct leaf: accepted.
    let mut direct = tls_connect(depth0_port, Some(&pki.trusted)).await;
    assert_eq!(
        read_available(&mut direct).await,
        b"220 mail.example.com ESMTP ready\r\n"
    );
    let _ = smtp_authenticate(&mut direct).await;

    // depth 1 + presented intermediate chain: accepted (one intermediate).
    let mut chain = tls_connect(depth1_port, Some(&pki.intermediate_chain)).await;
    assert_eq!(
        read_available(&mut chain).await,
        b"220 mail.example.com ESMTP ready\r\n"
    );
    let _ = smtp_authenticate(&mut chain).await;

    // depth 0 + presented intermediate chain: rejected.
    let mut too_long = tls_connect(depth0_port, Some(&pki.intermediate_chain)).await;
    assert_eq!(
        read_until_closed(&mut too_long).await,
        b"421 4.7.1 SSL certificate error\r\n"
    );

    // depth 1 + intermediate supplied by ssl_trusted_certificate: accepted.
    let mut supplied = tls_connect(trusted_intermediate_port, Some(&pki.intermediate_leaf)).await;
    assert_eq!(
        read_available(&mut supplied).await,
        b"220 mail.example.com ESMTP ready\r\n"
    );
    let _ = smtp_authenticate(&mut supplied).await;

    // depth 2 + presented intermediate chain: accepted.
    let mut deep = tls_connect(depth2_port, Some(&pki.intermediate_chain)).await;
    assert_eq!(
        read_available(&mut deep).await,
        b"220 mail.example.com ESMTP ready\r\n"
    );
    let _ = smtp_authenticate(&mut deep).await;

    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}

#[tokio::test]
async fn starttls_checks_the_client_certificate_after_upgrade() {
    let directory = tempfile::tempdir().expect("tempdir");
    let pki = generate_pki(directory.path());
    let backend_port = start_line_backend("220 backend ESMTP\r\n").await;
    let (auth_port, requests) = start_auth_http(backend_port).await;
    let port = available_port();
    let mut tls = tls_server_config(&pki, false);
    tls["verifyClient"] = json!("on");
    tls["clientCertificateFile"] = json!(pki.ca_file.to_string_lossy());
    let config = mail_config(vec![mail_server(
        "smtp-starttls",
        port,
        "smtp",
        json!({
            "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth"), "passClientCert": true },
            "tls": tls,
        }),
    )]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    // A rejected certificate is reported after the STARTTLS handshake.
    let mut raw = connect(port).await;
    assert_eq!(
        read_available(&mut raw).await,
        b"220 mail.example.com ESMTP ready\r\n"
    );
    raw.write_all(b"EHLO client.example\r\n")
        .await
        .expect("ehlo");
    let ehlo = read_available(&mut raw).await;
    assert!(
        String::from_utf8_lossy(&ehlo).contains("STARTTLS"),
        "{ehlo:?}"
    );
    raw.write_all(b"STARTTLS\r\n").await.expect("starttls");
    assert_eq!(read_available(&mut raw).await, b"220 2.0.0 Start TLS\r\n");
    let connector =
        tokio_rustls::TlsConnector::from(Arc::new(client_tls_config(Some(&pki.untrusted))));
    let name = rustls::pki_types::ServerName::try_from("mail.example.com").expect("name");
    let mut rejected = connector.connect(name, raw).await.expect("TLS connect");
    assert_eq!(
        read_until_closed(&mut rejected).await,
        b"421 4.7.1 SSL certificate error\r\n"
    );
    assert!(requests.lock().expect("lock").is_empty());

    // The trusted certificate completes the authorization exchange.
    let mut raw = connect(port).await;
    assert_eq!(
        read_available(&mut raw).await,
        b"220 mail.example.com ESMTP ready\r\n"
    );
    raw.write_all(b"EHLO client.example\r\n")
        .await
        .expect("ehlo");
    let _ = read_available(&mut raw).await;
    raw.write_all(b"STARTTLS\r\n").await.expect("starttls");
    assert_eq!(read_available(&mut raw).await, b"220 2.0.0 Start TLS\r\n");
    let connector =
        tokio_rustls::TlsConnector::from(Arc::new(client_tls_config(Some(&pki.trusted))));
    let name = rustls::pki_types::ServerName::try_from("mail.example.com").expect("name");
    let mut accepted = connector.connect(name, raw).await.expect("TLS connect");
    accepted
        .write_all(b"AUTH PLAIN AGFsaWNlAHNlY3JldA==\r\n")
        .await
        .expect("auth");
    let _ = read_available(&mut accepted).await;
    let request = wait_for_request(&requests).await;
    assert_eq!(request.value("Auth-SSL-Verify"), Some("SUCCESS"));

    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}

#[tokio::test]
async fn protocol_specific_rejection_lines_match_nginx() {
    let directory = tempfile::tempdir().expect("tempdir");
    let pki = generate_pki(directory.path());
    let imap_port = available_port();
    let pop3_port = available_port();
    let imap_tls = verify_tls_server_config(&pki, "on", 1);
    let pop3_tls = verify_tls_server_config(&pki, "on", 1);
    let backend_port = start_line_backend("220 backend ESMTP\r\n").await;
    let (auth_port, _) = start_auth_http(backend_port).await;
    let config = mail_config(vec![
        mail_server(
            "imap-tls",
            imap_port,
            "imap",
            json!({ "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth") }, "tls": imap_tls }),
        ),
        mail_server(
            "pop3-tls",
            pop3_port,
            "pop3",
            json!({ "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth") }, "tls": pop3_tls }),
        ),
    ]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let mut imap = tls_connect(imap_port, None).await;
    assert_eq!(
        read_until_closed(&mut imap).await,
        b"* BYE No required SSL certificate\r\n"
    );
    let mut pop3 = tls_connect(pop3_port, None).await;
    assert_eq!(
        read_until_closed(&mut pop3).await,
        b"-ERR No required SSL certificate\r\n"
    );

    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}

#[tokio::test]
async fn pass_client_cert_gates_only_the_raw_certificate_header() {
    let directory = tempfile::tempdir().expect("tempdir");
    let pki = generate_pki(directory.path());
    let backend_port = start_line_backend("+OK backend POP3\r\n").await;
    let (auth_port, requests) = start_auth_http(backend_port).await;
    let port = available_port();
    let tls = verify_tls_server_config(&pki, "on", 1);
    let config = mail_config(vec![mail_server(
        "pop3-tls",
        port,
        "pop3",
        json!({
            "authHttp": { "uri": format!("127.0.0.1:{auth_port}/auth") },
            "tls": tls,
        }),
    )]);
    let path = write_config(directory.path(), &config);
    let (shutdown, task) = spawn_data_plane(&path);

    let mut stream = tls_connect(port, Some(&pki.trusted)).await;
    assert_eq!(read_available(&mut stream).await, b"+OK POP3 ready\r\n");
    // A `?` is literal in `NGX_ESCAPE_MAIL_AUTH` (unlike a generic URI escape);
    // a space is percent-encoded.
    stream.write_all(b"USER ali?ce x\r\n").await.expect("user");
    assert_eq!(read_available(&mut stream).await, b"+OK\r\n");
    stream.write_all(b"PASS secret\r\n").await.expect("pass");
    let _ = read_available(&mut stream).await;
    let request = wait_for_request(&requests).await;
    assert_eq!(request.value("Auth-User"), Some("ali?ce%20x"));
    assert_eq!(request.value("Auth-SSL-Verify"), Some("SUCCESS"));
    assert_eq!(
        request.value("Auth-SSL-Subject"),
        Some("CN=alice@mail.example.com")
    );
    assert_eq!(
        request.value("Auth-SSL-Cert"),
        None,
        "auth_http_pass_client_cert off omits only the raw certificate"
    );

    shutdown.send(()).ok();
    task.await.expect("join").expect("data plane");
}
