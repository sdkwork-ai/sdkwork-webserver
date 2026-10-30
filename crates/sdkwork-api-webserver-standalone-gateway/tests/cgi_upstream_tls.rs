//! Wave 7B integration: uWSGI upstream TLS through the CGI transport seam.
//!
//! The nginx compatibility plane materializes `uwsgi_pass suwsgi://…` plus the
//! `uwsgi_ssl_*` family into the per-upstream TLS policy; this test proves the
//! runtime side of that contract with a real TLS responder:
//!
//! - a trusted CA plus `uwsgi_ssl_name` (server certificate SAN only names the
//!   override) and a required client identity (mTLS);
//! - an untrusted CA fails closed with a 502;
//! - the explicit dangerous accept-any verifier (`uwsgi_ssl_verify off`) can
//!   reach a self-signed responder, while default verification cannot;
//! - SCGI over TLS is reachable through the typed plane (SDKWork extension:
//!   nginx 1.29.6 has no SCGI TLS directives).
//!
//! The typed JSON configuration is used directly because it can express the
//! per-upstream TLS policy without the nginx compatibility syntax.

use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
    time::timeout,
};

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
        .push(DnType::CommonName, format!("{stem} test CA"));
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

/// Returns (certificate PEM path, private key PEM path, DERs) for a signed
/// identity; `client == false` adds `serverAuth` EKU.
fn write_signed_identity(
    directory: &Path,
    stem: &str,
    names: &[&str],
    authority: &TestCa,
    client: bool,
) -> (
    PathBuf,
    PathBuf,
    Vec<CertificateDer<'static>>,
    PrivateKeyDer<'static>,
) {
    let mut params = CertificateParams::new(
        names
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    )
    .expect("identity parameters");
    params.distinguished_name = rcgen::DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, names[0]);
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![if client {
        ExtendedKeyUsagePurpose::ClientAuth
    } else {
        ExtendedKeyUsagePurpose::ServerAuth
    }];
    let key = KeyPair::generate().expect("identity key");
    let certificate = params
        .signed_by(&key, &Issuer::new(authority.params.clone(), &authority.key))
        .expect("sign identity");
    let cert_path = directory.join(format!("{stem}.pem"));
    let key_path = directory.join(format!("{stem}.key"));
    fs::write(&cert_path, certificate.pem()).expect("write cert");
    let key_pem = key.serialize_pem();
    fs::write(&key_path, &key_pem).expect("write key");
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der()));
    (
        cert_path,
        key_path,
        vec![CertificateDer::from(certificate.der().to_vec())],
        key_der,
    )
}

fn server_tls_config(
    certificates: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
    client_ca: Option<&Path>,
) -> rustls::ServerConfig {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let builder = rustls::ServerConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
        .expect("protocol versions");
    let builder = if let Some(client_ca) = client_ca {
        let mut roots = rustls::RootCertStore::empty();
        let pem = fs::read(client_ca).expect("read client CA");
        for certificate in rustls::pki_types::CertificateDer::pem_slice_iter(&pem) {
            roots
                .add(certificate.expect("client CA certificate"))
                .expect("add client CA");
        }
        let verifier =
            rustls::server::WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider)
                .build()
                .expect("client verifier");
        builder.with_client_cert_verifier(verifier)
    } else {
        builder.with_no_client_auth()
    };
    builder
        .with_single_cert(certificates, key)
        .expect("server TLS config")
}

/// A TLS responder for the CGI family. It drains one request (the test only
/// asserts TLS reachability and the authenticated handshake) and answers a
/// small HTTP/1.1 response.
async fn spawn_tls_cgi_responder(config: rustls::ServerConfig) -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind TLS responder");
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
                // Drain whatever request bytes arrive; the CGI clients write
                // their full request then wait for the response.
                let mut buffer = [0u8; 16 * 1024];
                let _ = timeout(Duration::from_millis(500), stream.read(&mut buffer)).await;
                let body = b"cgi-tls-ok";
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes()).await;
                let _ = stream.write_all(body).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    port
}

fn write_cgi_config(
    directory: &Path,
    listen_port: u16,
    target_port: u16,
    protocol: &str,
    tls: serde_json::Value,
) -> PathBuf {
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "cgi-upstream-tls-test",
        "limits": {
            "requestTimeoutMs": 5000,
            "drainTimeoutMs": 1000,
            "maxConnections": 32
        },
        "listeners": [{
            "id": "http",
            "bind": "127.0.0.1",
            "port": listen_port,
            "protocols": ["http1"],
            "defaultVirtualHostRef": "host"
        }],
        "upstreams": [{
            "id": "cgi-target",
            "targets": [{ "url": format!("https://127.0.0.1:{target_port}") }],
            "addressPolicy": { "allowedCidrs": ["127.0.0.1/32"] },
            "tls": tls
        }],
        "resources": [{
            "id": "cgi",
            "type": "cgi-upstream",
            "protocol": protocol,
            "upstreamRef": "cgi-target",
            "params": [],
            "passRequestHeaders": true
        }],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["http"],
            "serverNames": ["cgi-tls.localhost"],
            "routes": [{
                "id": "route",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "cgi"
            }]
        }]
    });
    let path = directory.join("cgi-tls.json");
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

async fn http_get(port: u16) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect data plane");
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: cgi-tls.localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("write request");
    let mut response = Vec::new();
    timeout(Duration::from_secs(10), stream.read_to_end(&mut response))
        .await
        .expect("response timeout")
        .expect("read response");
    let head_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("response head");
    let head = String::from_utf8_lossy(&response[..head_end]).into_owned();
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .expect("status line");
    let body = String::from_utf8_lossy(&response[head_end + 4..]).into_owned();
    (status, body)
}

async fn wait_ready(port: u16, config_path: &Path) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "data plane never listened; config {}",
            config_path.display()
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn uwsgi_tls_trusted_ca_name_override_and_mtls_execute() {
    let directory = tempfile::tempdir().expect("temp dir");
    let ca = write_test_ca(directory.path(), "upstream-ca");
    let (server_cert, server_key, server_chain, server_key_der) = write_signed_identity(
        directory.path(),
        "uwsgi-server",
        &["uwsgi-tls.test"],
        &ca,
        false,
    );
    let (_client_cert, _client_key, client_chain, client_key_der) =
        write_signed_identity(directory.path(), "uwsgi-client", &["cgi-client"], &ca, true);
    // The responder requires the CA-signed client identity (mTLS).
    let responder = spawn_tls_cgi_responder(server_tls_config(
        server_chain,
        server_key_der,
        Some(&ca_cert_path(directory.path(), "upstream-ca")),
    ))
    .await;

    let listen = free_port();
    let config_path = write_cgi_config(
        directory.path(),
        listen,
        responder,
        "uwsgi",
        json!({
            "trustMode": "custom",
            "caCertificateFiles": ["upstream-ca.pem"],
            "clientCertificateFile": "uwsgi-client.pem",
            "clientPrivateKeyFile": "uwsgi-client.key",
            "serverName": "uwsgi-tls.test",
            "minimumVersion": "tls1.2",
            "maximumVersion": "tls1.3"
        }),
    );
    let (_shutdown, _task) = spawn_data_plane(&config_path).await;
    wait_ready(listen, &config_path).await;
    let (status, body) = http_get(listen).await;
    assert_eq!(status, 200, "mTLS + name override should succeed: {body}");
    assert!(body.contains("cgi-tls-ok"), "{body}");
    // Silence unused bindings while documenting the fixture material.
    let _ = (&server_cert, &server_key, &client_chain, &client_key_der);
}

fn ca_cert_path(directory: &Path, stem: &str) -> PathBuf {
    directory.join(format!("{stem}.pem"))
}

#[tokio::test]
async fn uwsgi_tls_untrusted_ca_fails_closed_with_502() {
    let directory = tempfile::tempdir().expect("temp dir");
    let trusted_ca = write_test_ca(directory.path(), "trusted-ca");
    let other_ca = write_test_ca(directory.path(), "other-ca");
    let (_server_cert, _server_key, server_chain, server_key_der) = write_signed_identity(
        directory.path(),
        "uwsgi-server",
        &["uwsgi-tls.test"],
        &trusted_ca,
        false,
    );
    let _ = &other_ca;
    let responder =
        spawn_tls_cgi_responder(server_tls_config(server_chain, server_key_der, None)).await;
    let listen = free_port();
    let config_path = write_cgi_config(
        directory.path(),
        listen,
        responder,
        "uwsgi",
        json!({
            "trustMode": "custom",
            "caCertificateFiles": ["other-ca.pem"],
            "serverName": "uwsgi-tls.test"
        }),
    );
    let (_shutdown, _task) = spawn_data_plane(&config_path).await;
    wait_ready(listen, &config_path).await;
    let (status, body) = http_get(listen).await;
    assert_eq!(
        status, 502,
        "an untrusted upstream chain must fail closed: {body}"
    );
}

#[tokio::test]
async fn explicit_dangerous_verify_reaches_a_self_signed_responder() {
    let directory = tempfile::tempdir().expect("temp dir");
    let ca = write_test_ca(directory.path(), "self-signed");
    let (_cert, _key, chain, key_der) = write_signed_identity(
        directory.path(),
        "self-signed-server",
        &["self-signed.test"],
        &ca,
        false,
    );
    // The identity is signed by a CA the runtime never trusts: only the
    // explicit dangerous accept-any policy can complete this handshake.
    let responder = spawn_tls_cgi_responder(server_tls_config(chain, key_der, None)).await;

    let listen = free_port();
    let config_path = write_cgi_config(
        directory.path(),
        listen,
        responder,
        "uwsgi",
        json!({
            "dangerousAcceptAnyVerify": true,
            "serverName": "self-signed.test"
        }),
    );
    let (_shutdown, _task) = spawn_data_plane(&config_path).await;
    wait_ready(listen, &config_path).await;
    let (status, body) = http_get(listen).await;
    assert_eq!(
        status, 200,
        "the explicit dangerous verifier must accept the chain: {body}"
    );
}

#[tokio::test]
async fn scgi_over_tls_executes_through_the_typed_plane() {
    let directory = tempfile::tempdir().expect("temp dir");
    let ca = write_test_ca(directory.path(), "scgi-ca");
    let (_cert, _key, chain, key_der) = write_signed_identity(
        directory.path(),
        "scgi-server",
        &["scgi-tls.test"],
        &ca,
        false,
    );
    let responder = spawn_tls_cgi_responder(server_tls_config(chain, key_der, None)).await;
    let listen = free_port();
    let config_path = write_cgi_config(
        directory.path(),
        listen,
        responder,
        "scgi",
        json!({
            "trustMode": "custom",
            "caCertificateFiles": ["scgi-ca.pem"],
            "serverName": "scgi-tls.test"
        }),
    );
    let (_shutdown, _task) = spawn_data_plane(&config_path).await;
    wait_ready(listen, &config_path).await;
    let (status, body) = http_get(listen).await;
    // SCGI over TLS is a typed-plane SDKWork extension: nginx 1.29.6 has no
    // scgi_ssl_* directives, so there is no compatibility-plane form to test.
    assert_eq!(status, 200, "typed-plane SCGI TLS should execute: {body}");
}
