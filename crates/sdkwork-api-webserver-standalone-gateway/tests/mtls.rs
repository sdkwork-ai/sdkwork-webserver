//! Downstream mTLS execution (`tls.downstream-mtls`): listener-level
//! `clientAuth` required/optional with CA verification, wrong-CA rejection,
//! and stream TLS terminate client auth.

use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use rcgen::{
    BasicConstraints, CertificateParams, DnType, DistinguishedName, ExtendedKeyUsagePurpose,
    IsCa, Issuer, KeyPair, KeyUsagePurpose,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use serde_json::{json, Value};
use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::load_and_compile_webserver_config;
use tokio::{net::TcpStream, sync::oneshot};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

struct TestCa {
    certificate: rcgen::Certificate,
    key: KeyPair,
    params: CertificateParams,
}

fn write_test_ca(directory: &Path, stem: &str) -> TestCa {
    let mut params = CertificateParams::new(Vec::new()).expect("CA parameters");
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, format!("{stem} test CA"));
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
    ];
    let key = KeyPair::generate().expect("generate CA key");
    let certificate = params.self_signed(&key).expect("self-sign test CA");
    fs::write(directory.join(format!("{stem}.pem")), certificate.pem())
        .expect("write test CA certificate");
    TestCa {
        certificate,
        key,
        params,
    }
}

/// Sign a server or client identity; returns (cert_pem, key_pem, cert_der).
fn write_signed_identity(
    directory: &Path,
    stem: &str,
    names: &[&str],
    authority: &TestCa,
    client: bool,
) -> (String, String, CertificateDer<'static>, Vec<u8>) {
    let mut params = CertificateParams::new(
        names
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    )
    .expect("signed certificate parameters");
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, names[0]);
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![if client {
        ExtendedKeyUsagePurpose::ClientAuth
    } else {
        ExtendedKeyUsagePurpose::ServerAuth
    }];
    let key = KeyPair::generate().expect("generate signed identity key");
    let certificate = params
        .signed_by(&key, &Issuer::new(authority.params.clone(), &authority.key))
        .expect("sign test identity");
    fs::write(directory.join(format!("{stem}.pem")), certificate.pem())
        .expect("write signed certificate");
    fs::write(directory.join(format!("{stem}.key")), key.serialize_pem())
        .expect("write signed private key");
    (
        certificate.pem(),
        key.serialize_pem(),
        CertificateDer::from(certificate.der().to_vec()),
        key.serialize_der(),
    )
}

fn write_mtls_config(
    directory: &Path,
    port: u16,
    client_auth_mode: &str,
) -> PathBuf {
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-mtls-test",
        "limits": {
            "requestTimeoutMs": 5000,
            "drainTimeoutMs": 1000,
            "maxConnections": 32
        },
        "listeners": [{
            "id": "https",
            "bind": "127.0.0.1",
            "port": port,
            "protocols": ["http1"],
            "tlsPolicyRef": "tls",
            "defaultVirtualHostRef": "host"
        }],
        "certificates": [{
            "id": "server-cert",
            "serverNames": ["mtls.localhost"],
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
            "alpn": ["http/1.1"],
            "clientAuth": {
                "mode": client_auth_mode,
                "caCertificateFiles": ["client-ca.pem"]
            }
        }],
        "resources": [{
            "id": "ok",
            "type": "respond",
            "status": 200,
            "contentType": "text/plain; charset=utf-8",
            "body": "mtls-ok"
        }],
        "virtualHosts": [{
            "id": "host",
            "listenerRefs": ["https"],
            "serverNames": ["mtls.localhost"],
            "routes": [{
                "id": "ok-route",
                "match": { "pathType": "prefix", "path": "/" },
                "resourceRef": "ok"
            }]
        }]
    });
    let path = directory.join("config.json");
    fs::write(&path, serde_json::to_vec_pretty(&config).expect("serialize")).expect("write");
    path
}

fn identity_pem(cert_pem: &str, key_pem: &str) -> Vec<u8> {
    format!("{cert_pem}{key_pem}").into_bytes()
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

async fn wait_ready(client: &reqwest::Client, url: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match client.get(url).send().await {
            Ok(_) => return,
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => panic!("data plane not ready: {error}"),
        }
    }
}

#[tokio::test]
async fn required_client_auth_accepts_trusted_certificates_and_rejects_missing_or_untrusted() {
    let directory = tempfile::tempdir().expect("temp dir");
    let port = free_port();
    let ca = write_test_ca(directory.path(), "client-ca");
    let (server_cert, server_key, _, _) =
        write_signed_identity(directory.path(), "server", &["mtls.localhost"], &ca, false);
    let (trusted_cert, trusted_key, _, _) =
        write_signed_identity(directory.path(), "trusted-client", &["client-a"], &ca, true);
    // A second CA signs an untrusted client identity.
    let other_ca = write_test_ca(directory.path(), "other-ca");
    let (untrusted_cert, untrusted_key, _, _) =
        write_signed_identity(directory.path(), "untrusted-client", &["client-b"], &other_ca, true);

    let config_path = write_mtls_config(directory.path(), port, "required");
    let (shutdown_tx, task) = spawn_data_plane(&config_path).await;
    let url = format!("https://mtls.localhost:{port}/");

    let trusted = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .identity(
            reqwest::Identity::from_pem(&identity_pem(&trusted_cert, &trusted_key))
                .expect("trusted identity"),
        )
        .resolve("mtls.localhost", ([127, 0, 0, 1], port).into())
        .build()
        .expect("trusted client");
    wait_ready(&trusted, &url).await;
    let response = trusted.get(&url).send().await.expect("trusted request");
    assert_eq!(response.status(), 200);
    assert_eq!(response.text().await.expect("body"), "mtls-ok");

    let anonymous = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .resolve("mtls.localhost", ([127, 0, 0, 1], port).into())
        .build()
        .expect("anonymous client");
    // Without a client certificate the handshake must fail.
    anonymous
        .get(&url)
        .send()
        .await
        .expect_err("required client auth must reject a certificate-less handshake");

    let untrusted = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .identity(
            reqwest::Identity::from_pem(&identity_pem(&untrusted_cert, &untrusted_key))
                .expect("untrusted identity"),
        )
        .resolve("mtls.localhost", ([127, 0, 0, 1], port).into())
        .build()
        .expect("untrusted client");
    untrusted
        .get(&url)
        .send()
        .await
        .expect_err("a certificate from an untrusted CA must be rejected");

    let _ = shutdown_tx.send(());
    let _ = task.await;
}

#[tokio::test]
async fn optional_client_auth_allows_certificate_less_handshakes() {
    let directory = tempfile::tempdir().expect("temp dir");
    let port = free_port();
    let ca = write_test_ca(directory.path(), "client-ca");
    let (server_cert, server_key, _, _) =
        write_signed_identity(directory.path(), "server", &["mtls.localhost"], &ca, false);

    let config_path = write_mtls_config(directory.path(), port, "optional");
    let (shutdown_tx, task) = spawn_data_plane(&config_path).await;
    let url = format!("https://mtls.localhost:{port}/");

    let anonymous = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .resolve("mtls.localhost", ([127, 0, 0, 1], port).into())
        .build()
        .expect("anonymous client");
    wait_ready(&anonymous, &url).await;
    let response = anonymous.get(&url).send().await.expect("anonymous request");
    assert_eq!(response.status(), 200);

    let _ = shutdown_tx.send(());
    let _ = task.await;
}

#[tokio::test]
async fn stream_tls_terminate_requires_client_certificates_when_configured() {
    let directory = tempfile::tempdir().expect("temp dir");
    let port = free_port();
    let upstream_port = free_port();
    let ca = write_test_ca(directory.path(), "stream-ca");
    let (_server_cert, _server_key, server_der, _) =
        write_signed_identity(directory.path(), "stream-server", &["localhost"], &ca, false);
    let (_client_cert, _client_key, client_der, client_key_der) =
        write_signed_identity(directory.path(), "stream-client", &["client"], &ca, true);

    // Minimal TCP echo upstream.
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", upstream_port))
            .await
            .expect("echo bind");
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buffer = vec![0_u8; 1024];
                if let Ok(read) = socket.read(&mut buffer).await {
                    let _ = socket.write_all(&buffer[..read]).await;
                }
            });
        }
    });

    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-stream-mtls-test",
        "limits": { "maxConnections": 32 },
        "listeners": [],
        "resources": [],
        "virtualHosts": [],
        "certificates": [{
            "id": "stream-cert",
            "serverNames": ["localhost"],
            "source": {
                "type": "protected-file",
                "certificateFile": "stream-server.pem",
                "privateKeyFile": "stream-server.key"
            }
        }],
        "streams": [{
            "id": "secure-stream",
            "bind": "127.0.0.1",
            "port": port,
            "target": { "type": "literal", "host": "127.0.0.1", "port": upstream_port },
            "tls": {
                "mode": "terminate",
                "certificateRef": "stream-cert",
                "clientAuth": {
                    "mode": "required",
                    "caCertificateFiles": ["stream-ca.pem"]
                }
            }
        }]
    });
    let config_path = directory.path().join("config.json");
    fs::write(&config_path, serde_json::to_vec_pretty(&config).expect("serialize"))
        .expect("write");
    let (shutdown_tx, task) = spawn_data_plane(&config_path).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    // Trust the CA that signed the stream server identity.
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(CertificateDer::from(ca.certificate.der().to_vec()))
        .expect("trust CA");

    // Without a client certificate the stream handshake fails.
    let anonymous_config = rustls::ClientConfig::builder()
        .with_root_certificates(roots.clone())
        .with_no_client_auth();
    let anonymous = tokio_rustls::TlsConnector::from(Arc::new(anonymous_config));
    let tcp = TcpStream::connect(("127.0.0.1", port)).await.expect("connect");
    let result = anonymous
        .connect(
            ServerName::try_from("localhost".to_owned()).expect("name"),
            tcp,
        )
        .await;
    eprintln!("[mtls] anonymous handshake result: {result:?}");
    assert!(
        result.is_err(),
        "stream mTLS must reject a certificate-less handshake; got {:?}",
        result.as_ref().err()
    );

    // With the trusted client certificate the stream echoes.
    let identity = client_der.clone();
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(client_key_der));
    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_client_auth_cert(vec![identity], key)
        .expect("client auth config");
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));
    let tcp = TcpStream::connect(("127.0.0.1", port)).await.expect("connect");
    let mut stream = connector
        .connect(
            ServerName::try_from("localhost".to_owned()).expect("name"),
            tcp,
        )
        .await
        .expect("mTLS handshake");
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    stream.write_all(b"ping").await.expect("write");
    let mut echoed = vec![0_u8; 4];
    stream.read_exact(&mut echoed).await.expect("echo");
    assert_eq!(&echoed, b"ping");

    let _ = shutdown_tx.send(());
    let _ = task.await;
}
