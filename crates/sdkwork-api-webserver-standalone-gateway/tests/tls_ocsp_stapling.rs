//! Static OCSP stapling (nginx `ssl_stapling_file`).
//!
//! The runtime reads the declared DER `OCSPResponse` when it builds a
//! certificate's TLS material and publishes it from
//! `rustls::sign::CertifiedKey::ocsp`; rustls then sends it as the TLS
//! `status_request` response. These tests complete a real TLS handshake with a
//! client that captures the stapled bytes, so the assertion is on the wire, not
//! on configuration state:
//!
//! - a certificate with `ocspStaplingFile` serves those exact bytes;
//! - a certificate without one staples nothing.

use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use rcgen::{CertificateParams, DnType, KeyPair};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
    DigitallySignedStruct, SignatureScheme,
};
use sdkwork_api_webserver_standalone_gateway::run_data_plane_until;
use sdkwork_webserver_core::load_and_compile_webserver_config;
use serde_json::json;
use tokio::{io::AsyncWriteExt, net::TcpStream, sync::oneshot};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

/// Self-signed server identity shared by the two test listeners.
fn write_identity(directory: &Path) -> (PathBuf, PathBuf) {
    let mut params = CertificateParams::new(vec!["a.localhost".to_owned()]).expect("params");
    params.distinguished_name = Default::default();
    params
        .distinguished_name
        .push(DnType::CommonName, "a.localhost");
    let key = KeyPair::generate().expect("key");
    let certificate = params.self_signed(&key).expect("self-signed");
    let certificate_path = directory.join("server.pem");
    let key_path = directory.join("server.key");
    fs::write(&certificate_path, certificate.pem()).expect("write cert");
    fs::write(&key_path, key.serialize_pem()).expect("write key");
    (certificate_path, key_path)
}

fn write_config(
    directory: &Path,
    port_a: u16,
    port_b: u16,
    certificate_path: &Path,
    key_path: &Path,
    ocsp_path: &Path,
) -> PathBuf {
    let config = json!({
        "schemaVersion": 1,
        "kind": "sdkwork.webserver.app",
        "appKey": "sdkwork-ocsp-stapling-test",
        "limits": {"requestTimeoutMs": 5000, "drainTimeoutMs": 1000, "maxConnections": 32},
        "listeners": [
            {
                "id": "tls-a",
                "bind": "127.0.0.1",
                "port": port_a,
                "protocols": ["http1"],
                "tlsPolicyRef": "policy-a",
                "defaultVirtualHostRef": "host-a"
            },
            {
                "id": "tls-b",
                "bind": "127.0.0.1",
                "port": port_b,
                "protocols": ["http1"],
                "tlsPolicyRef": "policy-b",
                "defaultVirtualHostRef": "host-b"
            }
        ],
        "certificates": [
            {
                "id": "cert-a",
                "serverNames": ["a.localhost"],
                "source": {
                    "type": "protected-file",
                    "certificateFile": certificate_path.to_string_lossy(),
                    "privateKeyFile": key_path.to_string_lossy(),
                    "ocspStaplingFile": ocsp_path.to_string_lossy()
                }
            },
            {
                "id": "cert-b",
                "serverNames": ["a.localhost"],
                "source": {
                    "type": "protected-file",
                    "certificateFile": certificate_path.to_string_lossy(),
                    "privateKeyFile": key_path.to_string_lossy()
                }
            }
        ],
        "tlsPolicies": [
            {
                "id": "policy-a",
                "certificateRef": "cert-a",
                "minimumVersion": "tls1.2",
                "maximumVersion": "tls1.3",
                "alpn": ["http/1.1"]
            },
            {
                "id": "policy-b",
                "certificateRef": "cert-b",
                "minimumVersion": "tls1.2",
                "maximumVersion": "tls1.3",
                "alpn": ["http/1.1"]
            }
        ],
        "resources": [{
            "id": "ok",
            "type": "respond",
            "status": 200,
            "contentType": "text/plain; charset=utf-8",
            "body": "stapled"
        }],
        "virtualHosts": [
            {
                "id": "host-a",
                "listenerRefs": ["tls-a"],
                "serverNames": ["a.localhost"],
                "routes": [{"id": "route-a", "match": {"pathType": "prefix", "path": "/"}, "resourceRef": "ok"}]
            },
            {
                "id": "host-b",
                "listenerRefs": ["tls-b"],
                "serverNames": ["a.localhost"],
                "routes": [{"id": "route-b", "match": {"pathType": "prefix", "path": "/"}, "resourceRef": "ok"}]
            }
        ]
    });
    let path = directory.join("config.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&config).expect("serialize"),
    )
    .expect("write");
    path
}

#[derive(Debug)]
struct CapturingVerifier {
    ocsp: Arc<Mutex<Option<Vec<u8>>>>,
}

impl ServerCertVerifier for CapturingVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        *self.ocsp.lock().expect("ocsp mutex") = Some(ocsp_response.to_vec());
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _certificate: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _certificate: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::aws_lc_rs::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
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

async fn fetch(port: u16, ocsp: Arc<Mutex<Option<Vec<u8>>>>) -> Vec<u8> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let verifier = Arc::new(CapturingVerifier { ocsp });
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
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
    let server_name = ServerName::try_from("a.localhost").expect("server name");
    let mut tls = connector
        .connect(server_name, stream)
        .await
        .expect("tls handshake");
    tls.write_all(b"GET / HTTP/1.1\r\nHost: a.localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("write request");
    let mut response = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut tls, &mut response)
        .await
        .expect("read response");
    assert!(
        response.starts_with(b"HTTP/1.1 200"),
        "unexpected response: {}",
        String::from_utf8_lossy(&response[..response.len().min(40)])
    );
    response
}

#[tokio::test]
async fn ssl_stapling_file_is_served_and_absent_without_it() {
    let directory = tempfile::tempdir().expect("temp dir");
    let (certificate_path, key_path) = write_identity(directory.path());
    // A bounded DER SEQUENCE stands in for the operator-supplied OCSP response;
    // rustls sends exactly these bytes as the stapled response.
    let ocsp_bytes = vec![
        0x30, 0x08, 0x0a, 0x01, 0x00, 0x30, 0x06, 0x30, 0x04, 0x0a, 0x02, 0x01,
    ];
    let ocsp_path = directory.path().join("ocsp.der");
    fs::write(&ocsp_path, &ocsp_bytes).expect("write ocsp");

    let port_a = free_port();
    let port_b = free_port();
    let config_path = write_config(
        directory.path(),
        port_a,
        port_b,
        &certificate_path,
        &key_path,
        &ocsp_path,
    );
    let (shutdown_tx, task) = spawn_data_plane(&config_path).await;

    let captured_a = Arc::new(Mutex::new(None));
    fetch(port_a, Arc::clone(&captured_a)).await;
    let stapled = captured_a.lock().expect("mutex").clone();
    assert_eq!(
        stapled.as_deref(),
        Some(ocsp_bytes.as_slice()),
        "the configured ssl_stapling_file bytes must be the stapled response"
    );

    let captured_b = Arc::new(Mutex::new(None));
    fetch(port_b, Arc::clone(&captured_b)).await;
    let absent = captured_b.lock().expect("mutex").clone();
    assert!(
        absent.as_deref().is_none_or(<[u8]>::is_empty),
        "a certificate without ssl_stapling_file staples no response: {absent:?}"
    );

    let _ = shutdown_tx.send(());
    let _ = task.await;
}
