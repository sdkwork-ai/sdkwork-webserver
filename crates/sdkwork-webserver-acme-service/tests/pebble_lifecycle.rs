//! End-to-end ACME lifecycle test against a local Pebble CA.
//!
//! Ignored by default: requires the `pebble` and `pebble-challtestsrv`
//! binaries in `$PATH` (or the `PEBBLE` / `CHALLTESTSRV` environment
//! variables). Binary downloads: <https://github.com/letsencrypt/pebble/releases>.
//!
//! The test exercises the full control-plane issuance contract against a real
//! ACME server: durable encrypted account persistence, HTTP-01 challenge
//! material written into the webroot and served by a local HTTP server (the
//! data plane role), issuance evidence validation, and account reuse across
//! a second issuance. Run with:
//!
//! ```text
//! cargo test -p sdkwork-webserver-acme-service --test pebble_lifecycle -- --ignored
//! ```

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use axum::{routing::get, Router};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::CertificateDer;
use sdkwork_webserver_acme_service::dns_zone::SingleZoneResolver;
use sdkwork_webserver_acme_service::{
    AcmeConfig, AcmeDns01Context, AcmeServiceError, AcmeServiceResult, CertificateIssuer,
    Dns01Presenter, Dns01RecordHandle, Dns01RecordRequest, EncryptedFileAcmeAccountStore,
    ExtraRootsClientFactory,
};
use tempfile::TempDir;
use tokio::net::TcpListener;

const PEBBLE_DIRECTORY_URL: &str = "https://127.0.0.1:14000/dir";
const PEBBLE_HTTP_CHALLENGE_PORT: u16 = 5002;
/// The DNS server pebble resolves challenge names through.
const PEBBLE_DNS_PORT: u16 = 8053;
/// pebble-challtestsrv's record API, where DNS-01 presentations are published.
const CHALLTESTSRV_MANAGEMENT_PORT: u16 = 8055;

struct Subprocess(Option<Child>);

impl Subprocess {
    fn spawn(command: &mut Command) -> std::io::Result<Self> {
        Ok(Self(Some(command.spawn()?)))
    }
}

impl Drop for Subprocess {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn resolve_binary(env_name: &str, default: &str) -> PathBuf {
    std::env::var(env_name)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default))
}

fn write_pebble_identity(directory: &Path) -> (String, String) {
    let mut params = CertificateParams::new(Vec::new()).expect("certificate params");
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, "pebble");
    params.subject_alt_names = vec![
        SanType::DnsName("localhost".try_into().expect("localhost dns name")),
        SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
        SanType::IpAddress(std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)),
    ];
    let key = KeyPair::generate().expect("generate pebble key");
    let certificate = params.self_signed(&key).expect("self-sign pebble identity");
    let certificate_path = directory.join("pebble-cert.pem");
    let key_path = directory.join("pebble-key.pem");
    std::fs::write(&certificate_path, certificate.pem()).expect("write pebble certificate");
    std::fs::write(&key_path, key.serialize_pem()).expect("write pebble key");
    (
        certificate_path.to_string_lossy().into_owned(),
        key_path.to_string_lossy().into_owned(),
    )
}

fn write_pebble_config(directory: &Path, certificate: &str, private_key: &str) -> PathBuf {
    let config = serde_json::json!({
        "pebble": {
            "listenAddress": "127.0.0.1:14000",
            "managementListenAddress": "127.0.0.1:15000",
            "certificate": certificate,
            "privateKey": private_key,
            "httpPort": PEBBLE_HTTP_CHALLENGE_PORT,
            "tlsPort": 5001,
            "ocspResponderURL": "",
            "externalAccountBindingRequired": false,
            "externalAccountMACKeys": {},
            "domainBlocklist": [],
            "retryAfter": {"authz": 1, "order": 1, "challenge": 1, "request": 1},
            "profiles": {
                "default": {
                    "description": "pebble test profile",
                    "validityPeriod": 7776000
                }
            }
        }
    });
    let path = directory.join("pebble-config.json");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&config).expect("serialize pebble config"),
    )
    .expect("write pebble config");
    path
}

async fn wait_for_directory(client: &reqwest::Client, url: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if client.get(url).send().await.is_ok() {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "pebble directory did not become ready at {url}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn challenge_router(webroot: PathBuf) -> Router {
    Router::new().route(
        "/.well-known/acme-challenge/{token}",
        get(
            move |axum::extract::Path(token): axum::extract::Path<String>| async move {
                let token_bytes = token.as_bytes();
                if token_bytes.is_empty()
                    || token_bytes.len() > 256
                    || !token_bytes
                        .iter()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                {
                    return (axum::http::StatusCode::NOT_FOUND, String::new());
                }
                let path = webroot
                    .join(".well-known")
                    .join("acme-challenge")
                    .join(&token);
                match tokio::fs::read_to_string(&path).await {
                    Ok(content) => (axum::http::StatusCode::OK, content),
                    Err(_) => (axum::http::StatusCode::NOT_FOUND, String::new()),
                }
            },
        ),
    )
}

/// A controlled CA plus the DNS server pebble validates through.
///
/// Booted per test rather than shared: both children die with the harness, so a
/// half-finished run can never leave an old CA answering on these ports while a
/// later run's readiness probe passes against it.
struct ControlledCa {
    temp: TempDir,
    /// The CA's own TLS identity, which the ACME client must trust as a root.
    ca_cert_path: PathBuf,
    /// pebble-challtestsrv's record API (`/set-txt`, `/clear-txt`).
    management_url: String,
    _pebble: Subprocess,
    _challtestsrv: Subprocess,
}

impl ControlledCa {
    async fn boot() -> Self {
        let pebble_path = resolve_binary("PEBBLE", "./pebble");
        let challtestsrv_path = resolve_binary("CHALLTESTSRV", "./pebble-challtestsrv");
        if !pebble_path.exists() || !challtestsrv_path.exists() {
            panic!(
                "pebble binaries are required: {} and {} (see https://github.com/letsencrypt/pebble/releases)",
                pebble_path.display(),
                challtestsrv_path.display()
            );
        }

        let temp = TempDir::new().expect("temp dir");
        let (certificate, private_key) = write_pebble_identity(temp.path());
        let pebble_config = write_pebble_config(temp.path(), &certificate, &private_key);

        // The challenge test server provides DNS resolution (all names ->
        // loopback) and a TXT record API, but no challenge services: HTTP-01 has
        // to come from the webroot server each suite runs itself, or the test
        // would only be asserting that challtestsrv's canned answer works.
        let challtestsrv = Subprocess::spawn(
            Command::new(&challtestsrv_path)
                .arg("-management")
                .arg(format!(":{CHALLTESTSRV_MANAGEMENT_PORT}"))
                .arg("-dnsserver")
                .arg(format!(":{PEBBLE_DNS_PORT}"))
                .arg("-http01")
                .arg("")
                .arg("-tlsalpn01")
                .arg("")
                .arg("-https01")
                .arg("")
                .arg("-doh")
                .arg("")
                .stdout(Stdio::null())
                .stderr(Stdio::null()),
        )
        .expect("spawn pebble-challtestsrv");

        let pebble = Subprocess::spawn(
            Command::new(&pebble_path)
                .env("PEBBLE_AUTHZREUSE", "0")
                .arg("-config")
                .arg(&pebble_config)
                .arg("-dnsserver")
                .arg(format!("127.0.0.1:{PEBBLE_DNS_PORT}"))
                .arg("-strict")
                .stdout(Stdio::null())
                .stderr(Stdio::null()),
        )
        .expect("spawn pebble");

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .expect("reqwest client");
        wait_for_directory(&client, PEBBLE_DIRECTORY_URL).await;

        Self {
            ca_cert_path: temp.path().join("pebble-cert.pem"),
            management_url: format!("http://127.0.0.1:{CHALLTESTSRV_MANAGEMENT_PORT}"),
            temp,
            _pebble: pebble,
            _challtestsrv: challtestsrv,
        }
    }

    /// The CA root the ACME client has to trust. Read from disk rather than
    /// checked in: the identity is generated per run, so a stale copy would
    /// silently fail every issuance.
    fn trust_anchor(&self) -> CertificateDer<'static> {
        CertificateDer::from_pem_slice(
            &std::fs::read(&self.ca_cert_path).expect("read pebble certificate"),
        )
        .expect("parse pebble certificate")
    }

    fn issuer(&self, webroot: Option<&Path>, account_root: &Path) -> CertificateIssuer {
        let account_store = std::sync::Arc::new(EncryptedFileAcmeAccountStore::new(
            account_root.to_path_buf(),
            b"test-master-key-00000000000000000000000000",
        ));
        CertificateIssuer::new_with_client_factory(
            AcmeConfig::new(
                PEBBLE_DIRECTORY_URL.to_string(),
                "admin@example.com".to_string(),
                30,
                webroot.map(|path| path.to_string_lossy().into_owned()),
                false,
            )
            .expect("acme config"),
            self.temp.path().join("live").to_string_lossy().into_owned(),
            180_000,
            account_store,
            std::sync::Arc::new(ExtraRootsClientFactory::new(vec![self.trust_anchor()])),
        )
        .expect("issuer")
    }
}

/// Presents DNS-01 TXT records through pebble-challtestsrv, so the CA really
/// queries them over DNS instead of reading a test double's memory.
struct ChalltestsrvDns01Presenter {
    client: reqwest::Client,
    management_url: String,
}

impl ChalltestsrvDns01Presenter {
    fn new(management_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            management_url: management_url.into(),
        }
    }

    async fn post(&self, route: &str, record_name: &str, value: &str) -> AcmeServiceResult<()> {
        // challtestsrv matches records by the fully-qualified owner name, which
        // is what the CA asks its resolver for.
        let owner = format!("{}.", record_name.trim_end_matches('.'));
        let response = self
            .client
            .post(format!("{}/{route}", self.management_url))
            .json(&serde_json::json!({ "host": owner, "value": value }))
            .send()
            .await
            .map_err(|error| {
                AcmeServiceError::provider(format!("challtestsrv {route}: {error}"))
            })?;
        if !response.status().is_success() {
            return Err(AcmeServiceError::provider(format!(
                "challtestsrv {route} for {owner} answered {}",
                response.status()
            )));
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Dns01Presenter for ChalltestsrvDns01Presenter {
    async fn publish(&self, request: &Dns01RecordRequest) -> AcmeServiceResult<Dns01RecordHandle> {
        self.post("set-txt", &request.record_name, &request.record_value)
            .await?;
        Ok(Dns01RecordHandle::from(request))
    }

    async fn withdraw(&self, handle: &Dns01RecordHandle) -> AcmeServiceResult<()> {
        // Clear by owner: challtestsrv keeps the record's value list and the CA
        // is finished with every value it published.
        self.post("clear-txt", &handle.record_name, "").await
    }
}

#[tokio::test]
#[ignore = "requires pebble and pebble-challtestsrv binaries"]
async fn full_issuance_lifecycle_against_pebble() {
    let ca = ControlledCa::boot().await;
    let webroot = ca.temp.path().join("webroot");
    std::fs::create_dir_all(webroot.join(".well-known").join("acme-challenge"))
        .expect("create webroot");

    // pebble resolves every challenge name through pebble-challtestsrv, which
    // answers both `127.0.0.1` (A) and `::1` (AAAA), and pebble fetches the AAAA
    // address first. An IPv4-only bind therefore refuses the CA's request —
    // `dial tcp [::1]:5002: connectex: No connection could be made because the
    // target machine actively refused it` — and the ACME error blames the
    // challenge (urn:ietf:params:acme:error:connection) when the real cause is a
    // listener that is not reachable on the family the CA chose. A wildcard IPv6
    // bind accepts both families on Windows and Linux; the IPv4 bind is the
    // fallback for a host without a usable IPv6 stack.
    let listener = match TcpListener::bind(("::", PEBBLE_HTTP_CHALLENGE_PORT)).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!(
                "IPv6 challenge bind on port {PEBBLE_HTTP_CHALLENGE_PORT} failed ({error}); \
                 serving HTTP-01 challenges over IPv4 only"
            );
            TcpListener::bind(("0.0.0.0", PEBBLE_HTTP_CHALLENGE_PORT))
                .await
                .expect("bind HTTP-01 challenge port")
        }
    };
    let challenge_webroot = webroot.clone();
    let challenge_server = tokio::spawn(async move {
        axum::serve(listener, challenge_router(challenge_webroot))
            .await
            .expect("serve HTTP-01 challenges");
    });

    let account_root = ca.temp.path().join("accounts");
    let issuer = ca.issuer(Some(&webroot), &account_root);

    let first = issuer
        .issue(
            1,
            &["http01.example.com".to_string()],
            "http01-example",
            "ECDSA",
        )
        .await
        .expect("first issuance through pebble");
    assert_eq!(first.cert_type, 1);
    assert!(first.cert_pem.contains("BEGIN CERTIFICATE"));
    assert!(first.private_key_pem.contains("PRIVATE KEY"));
    let account_files_after_first = std::fs::read_dir(&account_root)
        .expect("read account root")
        .count();
    assert!(
        account_files_after_first > 0,
        "durable account credentials must be persisted"
    );

    // A second issuance reuses the persisted account instead of creating a
    // new one: the account file count must stay unchanged.
    let second = issuer
        .issue(
            1,
            &["second.example.com".to_string()],
            "second-example",
            "ECDSA",
        )
        .await
        .expect("second issuance through pebble");
    assert!(second.cert_pem.contains("BEGIN CERTIFICATE"));
    let account_files_after_second = std::fs::read_dir(&account_root)
        .expect("read account root")
        .count();
    assert_eq!(
        account_files_after_second, account_files_after_first,
        "second issuance must reuse the persisted ACME account"
    );

    drop(ca);
    challenge_server.abort();
}

/// A wildcard cannot be proven over HTTP-01 — no CA will fetch
/// `/.well-known/acme-challenge/...` from `*.example.com` — so this suite is the
/// only one that exercises the DNS-01 half of the challenge policy end to end:
/// the engine derives `_acme-challenge.example.com`, the presenter publishes the
/// TXT digest through a real DNS server, the CA queries it, and the issued leaf
/// has to carry both the wildcard and the apex.
///
/// The apex is included deliberately: a wildcard order costs one authorization
/// per identifier, and the two share one record name, so the second presentation
/// must not overwrite the first.
#[tokio::test]
#[ignore = "requires pebble and pebble-challtestsrv binaries"]
async fn wildcard_issuance_over_dns01_against_pebble() {
    let ca = ControlledCa::boot().await;
    let account_root = ca.temp.path().join("accounts");
    let issuer = ca.issuer(None, &account_root);
    let presenter = ChalltestsrvDns01Presenter::new(ca.management_url.clone());
    let zones = SingleZoneResolver {
        zone_apex: "example.com".to_string(),
    };
    let dns01 = AcmeDns01Context {
        presenter: &presenter,
        zones: &zones,
    };

    let material = issuer
        .issue_with_challenge(
            1,
            &["*.example.com".to_string(), "example.com".to_string()],
            "wildcard-example",
            "ECDSA",
            Some(dns01),
        )
        .await
        .expect("wildcard issuance over DNS-01");

    assert!(material.cert_pem.contains("BEGIN CERTIFICATE"));
    assert!(material.private_key_pem.contains("PRIVATE KEY"));
    // A wildcard order is issued as one certificate, not two: the same
    // certificate has to cover the apex as well.
    let evidence = material
        .san_list
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    assert!(evidence.contains(&"*.example.com"), "{evidence:?}");
    assert!(evidence.contains(&"example.com"), "{evidence:?}");
}
