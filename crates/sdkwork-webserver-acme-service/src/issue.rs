use chrono::{DateTime, Utc};
use rcgen::KeyPair;
use sdkwork_utils_rust::crypto::sha256_hash;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

use crate::account_store::{AcmeAccountStore, MemoryAcmeAccountStore};
use crate::challenge_store::ChallengeStore;
use crate::config::AcmeConfig;
use crate::dns::Dns01Presenter;
use crate::http_client::{AcmeHttpClientFactory, PlatformVerifierClientFactory};
use crate::lets_encrypt::{issue_lets_encrypt, AcmeChallengeMode};
use crate::model::IssuedCertificateMaterial;
use crate::self_signed::{certificate_evidence_from_pem, issue_self_signed};
use crate::{AcmeServiceError, AcmeServiceResult};
use crate::{
    DEFAULT_ACME_OPERATION_TIMEOUT_MS, MAX_ACME_OPERATION_TIMEOUT_MS, MIN_ACME_OPERATION_TIMEOUT_MS,
};

const MAX_CONCURRENT_CERTIFICATE_ISSUANCE: usize = 8;

/// Certificate identifier ceiling.
///
/// Aligned with the deployment contract's `MAX_CERTIFICATE_IDENTIFIERS` and the
/// CA's own per-certificate name limit. The previous value of 8 was a local
/// choice, not a CA constraint, and it silently made a 20-name certificate
/// unrepresentable in the control plane.
const MAX_CERTIFICATE_IDENTIFIERS: usize = 100;

pub struct CertificateIssuer {
    pub(crate) config: AcmeConfig,
    pub(crate) challenge_store: Arc<ChallengeStore>,
    pub(crate) account_store: Arc<dyn AcmeAccountStore>,
    pub(crate) client_factory: Arc<dyn AcmeHttpClientFactory>,
    pub(crate) cert_root: String,
    pub(crate) operation_timeout: Duration,
    pub(crate) admission: Semaphore,
    /// Associated cloud DNS accounts. When present (and covering every
    /// identifier), issuance and renewal use DNS-01 — the only challenge
    /// that renews wildcard certificates unattended.
    pub(crate) dns_accounts: Option<Arc<crate::dns_account::DnsCloudAccountRegistry>>,
}

impl CertificateIssuer {
    pub fn new(config: AcmeConfig, cert_root: impl Into<String>) -> AcmeServiceResult<Self> {
        Self::new_with_operation_timeout_ms(config, cert_root, DEFAULT_ACME_OPERATION_TIMEOUT_MS)
    }

    pub fn new_with_operation_timeout_ms(
        config: AcmeConfig,
        cert_root: impl Into<String>,
        operation_timeout_ms: u64,
    ) -> AcmeServiceResult<Self> {
        Self::new_with_account_store(
            config,
            cert_root,
            operation_timeout_ms,
            Arc::new(MemoryAcmeAccountStore::default()),
        )
    }

    /// Construct with a durable account store so issuance and renewal reuse
    /// one CA account across process restarts. Without a durable store the
    /// process-lifetime in-memory store still prevents per-operation account
    /// creation within one process.
    pub fn new_with_account_store(
        config: AcmeConfig,
        cert_root: impl Into<String>,
        operation_timeout_ms: u64,
        account_store: Arc<dyn AcmeAccountStore>,
    ) -> AcmeServiceResult<Self> {
        Self::new_with_client_factory(
            config,
            cert_root,
            operation_timeout_ms,
            account_store,
            Arc::new(PlatformVerifierClientFactory),
        )
    }

    /// Construct with a custom ACME HTTP client factory (for example extra
    /// private CA trust roots) and a durable account store.
    pub fn new_with_client_factory(
        config: AcmeConfig,
        cert_root: impl Into<String>,
        operation_timeout_ms: u64,
        account_store: Arc<dyn AcmeAccountStore>,
        client_factory: Arc<dyn AcmeHttpClientFactory>,
    ) -> AcmeServiceResult<Self> {
        config.validate()?;
        let cert_root = cert_root.into();
        if cert_root.is_empty()
            || cert_root.len() > 4_096
            || cert_root
                .bytes()
                .any(|byte| byte == 0 || byte.is_ascii_control())
        {
            return Err(AcmeServiceError::config(
                "certificate live root must contain 1..4096 safe path bytes",
            ));
        }
        if !(MIN_ACME_OPERATION_TIMEOUT_MS..=MAX_ACME_OPERATION_TIMEOUT_MS)
            .contains(&operation_timeout_ms)
        {
            return Err(AcmeServiceError::config(format!(
                "ACME operation timeout must be between {MIN_ACME_OPERATION_TIMEOUT_MS} and {MAX_ACME_OPERATION_TIMEOUT_MS} ms"
            )));
        }
        Ok(Self {
            config,
            challenge_store: Arc::new(ChallengeStore::default()),
            account_store,
            client_factory,
            cert_root,
            operation_timeout: Duration::from_millis(operation_timeout_ms),
            admission: Semaphore::new(MAX_CONCURRENT_CERTIFICATE_ISSUANCE),
            dns_accounts: None,
        })
    }

    pub fn challenge_store(&self) -> Arc<ChallengeStore> {
        self.challenge_store.clone()
    }

    pub fn cert_root(&self) -> &str {
        &self.cert_root
    }

    pub fn renew_before_days(&self) -> u32 {
        self.config.renew_before_days
    }

    /// Attaches the cloud-account registry. Renewals and new issuance then
    /// use DNS-01 with per-identifier zone resolution; wildcards renew
    /// unattended.
    pub fn attach_dns_accounts(
        &mut self,
        accounts: Arc<crate::dns_account::DnsCloudAccountRegistry>,
    ) {
        self.dns_accounts = Some(accounts);
    }

    /// True when the registry covers every identifier: DNS-01 is available.
    pub fn dns_accounts_cover(&self, hostnames: &[String]) -> bool {
        self.dns_accounts
            .as_ref()
            .is_some_and(|registry| registry.covers_all(hostnames.iter().map(String::as_str)))
    }

    /// Builds the DNS-01 challenge context from the associated cloud
    /// accounts (dispatching presenter + per-identifier zone resolution).
    pub fn dns01_context(&self, hostnames: &[String]) -> Option<IssuerDns01Context> {
        let registry = Arc::clone(self.dns_accounts.as_ref()?);
        if !registry.covers_all(hostnames.iter().map(String::as_str)) {
            return None;
        }
        Some(IssuerDns01Context {
            presenter: registry.dispatch_presenter(),
            registry,
        })
    }

    pub async fn issue(
        &self,
        cert_type: i32,
        hostnames: &[String],
        cert_name: &str,
        key_algorithm: &str,
    ) -> AcmeServiceResult<IssuedCertificateMaterial> {
        self.issue_with_challenge(cert_type, hostnames, cert_name, key_algorithm, None)
            .await
    }

    /// Issue with an explicit challenge strategy.
    ///
    /// `dns01` selects DNS-01 validation and is required for any wildcard
    /// identifier. Passing `None` keeps the historical HTTP-01 behaviour, which
    /// is exact-identifier only; a wildcard request on that path fails with a
    /// validation error before an order is created.
    pub async fn issue_with_challenge(
        &self,
        cert_type: i32,
        hostnames: &[String],
        cert_name: &str,
        key_algorithm: &str,
        dns01: Option<AcmeDns01Context<'_>>,
    ) -> AcmeServiceResult<IssuedCertificateMaterial> {
        if hostnames.is_empty() || hostnames.len() > MAX_CERTIFICATE_IDENTIFIERS {
            return Err(AcmeServiceError::validation(format!(
                "certificate identifiers must contain 1..{MAX_CERTIFICATE_IDENTIFIERS} hostnames"
            )));
        }
        let mut unique_hostnames = BTreeSet::new();
        for hostname in hostnames {
            validate_hostname(hostname)?;
            if !unique_hostnames.insert(hostname.to_ascii_lowercase()) {
                return Err(AcmeServiceError::validation(
                    "certificate identifiers must be unique ignoring ASCII case",
                ));
            }
        }
        if !matches!(key_algorithm, "ECDSA" | "RSA") {
            return Err(AcmeServiceError::validation(
                "keyAlgorithm must be ECDSA or RSA",
            ));
        }
        validate_certificate_name(cert_name)?;
        let _permit = self.admission.try_acquire().map_err(|_| {
            AcmeServiceError::provider(format!(
                "certificate issuance capacity exhausted; maximum concurrent operations: {MAX_CONCURRENT_CERTIFICATE_ISSUANCE}"
            ))
        })?;
        let mode = match dns01 {
            Some(context) => AcmeChallengeMode::Dns01 {
                presenter: context.presenter,
                zones: context.zones,
            },
            None => AcmeChallengeMode::Http01,
        };
        let material = match cert_type {
            1 => {
                issue_lets_encrypt(
                    &self.config,
                    self.challenge_store.as_ref(),
                    self.account_store.as_ref(),
                    self.client_factory.as_ref(),
                    crate::lets_encrypt::AcmeIssueParams {
                        hostnames,
                        cert_name,
                        cert_root: &self.cert_root,
                        key_algorithm,
                        mode,
                    },
                    self.operation_timeout,
                )
                .await
            }
            3 => issue_self_signed(hostnames, cert_name, &self.cert_root, key_algorithm),
            other => Err(AcmeServiceError::validation(format!(
                "unsupported certType {other}; supported: 1 (Let's Encrypt), 3 (self-signed)"
            ))),
        }?;
        validate_issued_material(material, cert_type, hostnames, cert_name, key_algorithm)
    }
}

/// DNS-01 presentation context for one issuance.
///
/// The presenter is supplied by the caller because only the caller knows which
/// DNS provider owns the zone and holds the resolved credential; the engine
/// never reads a secret store.
/// Owned DNS-01 challenge context sourced from the issuer's cloud-account
/// registry; yields the borrowing [`AcmeDns01Context`] for an issuance call.
pub struct IssuerDns01Context {
    presenter: Arc<crate::dns_account::DispatchingDns01Presenter>,
    registry: Arc<crate::dns_account::DnsCloudAccountRegistry>,
}

impl IssuerDns01Context {
    pub fn into_context(&self) -> AcmeDns01Context<'_> {
        AcmeDns01Context {
            presenter: self.presenter.as_ref(),
            zones: self.registry.as_ref(),
        }
    }
}

pub struct AcmeDns01Context<'a> {
    pub presenter: &'a dyn Dns01Presenter,
    /// Zone resolver: maps each authorization identifier to its hosted zone
    /// apex (cloud-account registry or a single-zone adapter).
    pub zones: &'a dyn crate::dns_zone::DnsZoneResolver,
}

fn validate_issued_material(
    material: IssuedCertificateMaterial,
    expected_cert_type: i32,
    expected_hostnames: &[String],
    expected_cert_name: &str,
    expected_key_algorithm: &str,
) -> AcmeServiceResult<IssuedCertificateMaterial> {
    let evidence = certificate_evidence_from_pem(&material.cert_pem)
        .map_err(|_| AcmeServiceError::provider("issued certificate leaf evidence is invalid"))?;
    let expected_sans = normalized_san_set(expected_hostnames)?;
    let actual_sans = normalized_san_set(&evidence.san_list).map_err(|_| {
        AcmeServiceError::provider("issued certificate contains an invalid DNS SAN")
    })?;
    if actual_sans != expected_sans {
        return Err(AcmeServiceError::provider(
            "issued certificate DNS SANs do not match the requested identifiers",
        ));
    }
    if evidence.key_algorithm != expected_key_algorithm {
        return Err(AcmeServiceError::provider(
            "issued certificate key algorithm does not match the request",
        ));
    }

    let key_pair = KeyPair::from_pem(&material.private_key_pem)
        .map_err(|_| AcmeServiceError::provider("issued certificate private key is invalid"))?;
    if sha256_hash(&public_key_spki_der(&key_pair)) != evidence.spki_sha256 {
        return Err(AcmeServiceError::provider(
            "issued certificate private key does not match the leaf certificate",
        ));
    }
    match (expected_cert_type, material.chain_pem.as_deref()) {
        (1, Some(chain)) if chain == material.cert_pem.as_str() => {}
        (3, None) => {}
        _ => {
            return Err(AcmeServiceError::provider(
                "issued certificate chain material is inconsistent",
            ));
        }
    }

    let not_before = parse_certificate_timestamp(&evidence.not_before)?;
    let not_after = parse_certificate_timestamp(&evidence.not_after)?;
    let now = Utc::now();
    if not_before > now || now >= not_after {
        return Err(AcmeServiceError::provider(
            "issued certificate is not currently valid",
        ));
    }

    if material.cert_type != expected_cert_type
        || material.cert_name != expected_cert_name
        || material.issuer != evidence.issuer
        || material.subject != evidence.subject
        || normalized_san_set(&material.san_list).map_err(|_| {
            AcmeServiceError::provider("issued certificate metadata contains an invalid DNS SAN")
        })? != actual_sans
        || material.serial_sha256 != evidence.serial_sha256
        || material.fingerprint_sha256 != evidence.fingerprint_sha256
        || material.spki_sha256 != evidence.spki_sha256
        || material.chain_sha256 != evidence.chain_sha256
        || material.key_algorithm != evidence.key_algorithm
        || material.not_before != evidence.not_before
        || material.not_after != evidence.not_after
    {
        return Err(AcmeServiceError::provider(
            "issued certificate metadata does not match the leaf certificate",
        ));
    }
    Ok(material)
}

fn normalized_san_set(hostnames: &[String]) -> AcmeServiceResult<BTreeSet<String>> {
    hostnames
        .iter()
        .map(|hostname| {
            validate_hostname(hostname)?;
            Ok(hostname.to_ascii_lowercase())
        })
        .collect()
}

/// SubjectPublicKeyInfo DER of a key pair. rcgen 0.14 no longer exposes the
/// DER directly, so the PEM form (which carries the SPKI) is decoded.
fn public_key_spki_der(key_pair: &rcgen::KeyPair) -> Vec<u8> {
    use base64::Engine as _;
    key_pair
        .public_key_pem()
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .flat_map(|line| {
            base64::engine::general_purpose::STANDARD
                .decode(line.trim())
                .unwrap_or_default()
        })
        .collect()
}

fn parse_certificate_timestamp(value: &str) -> AcmeServiceResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|_| AcmeServiceError::provider("issued certificate validity is invalid"))
}

fn validate_hostname(hostname: &str) -> AcmeServiceResult<()> {
    let hostname = hostname.strip_prefix("*.").unwrap_or(hostname);
    if hostname.is_empty()
        || hostname.len() > 253
        || hostname.starts_with('.')
        || hostname.ends_with('.')
        || hostname.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err(AcmeServiceError::validation(
            "hostname must be a safe ASCII DNS name",
        ));
    }
    Ok(())
}

fn validate_certificate_name(cert_name: &str) -> AcmeServiceResult<()> {
    if cert_name.is_empty()
        || cert_name.len() > 253
        || matches!(cert_name, "." | "..")
        || cert_name.starts_with('.')
        || cert_name.ends_with('.')
        || cert_name.contains("..")
        || !cert_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(AcmeServiceError::validation(
            "certificate name must contain 1..253 safe ASCII name bytes",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::self_signed::generate_key_pair;

    fn self_signed_material() -> IssuedCertificateMaterial {
        issue_self_signed(
            &["dev.localhost".to_string()],
            "dev-localhost",
            "/tmp/certs/live",
            "ECDSA",
        )
        .expect("self-signed material")
    }

    #[tokio::test]
    async fn issues_self_signed_certificate() {
        let config = AcmeConfig::new(
            "https://acme-staging-v02.api.letsencrypt.org/directory".to_string(),
            "admin@example.com".to_string(),
            30,
            None,
            false,
        )
        .expect("config");
        let issuer = CertificateIssuer::new(config, "/tmp/certs/live").expect("issuer");
        let material = issuer
            .issue(3, &["dev.localhost".to_string()], "dev-localhost", "ECDSA")
            .await
            .expect("issue");
        assert_eq!(material.cert_type, 3);
        assert!(material.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(material.private_key_pem.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn validates_issued_certificate_material() {
        validate_issued_material(
            self_signed_material(),
            3,
            &["dev.localhost".to_string()],
            "dev-localhost",
            "ECDSA",
        )
        .expect("valid issued material");
    }

    #[test]
    fn rejects_issued_certificate_with_unrequested_san() {
        let error = validate_issued_material(
            self_signed_material(),
            3,
            &["other.localhost".to_string()],
            "dev-localhost",
            "ECDSA",
        )
        .expect_err("SAN mismatch must fail closed");
        assert!(error.to_string().contains("SANs do not match"));
    }

    #[test]
    fn rejects_issued_certificate_with_unrequested_key_algorithm() {
        let error = validate_issued_material(
            self_signed_material(),
            3,
            &["dev.localhost".to_string()],
            "dev-localhost",
            "RSA",
        )
        .expect_err("key algorithm mismatch must fail closed");
        assert!(error.to_string().contains("algorithm does not match"));
    }

    #[test]
    fn rejects_issued_certificate_with_mismatched_private_key() {
        let mut material = self_signed_material();
        material.private_key_pem = generate_key_pair("ECDSA")
            .expect("replacement key")
            .serialize_pem();
        let error = validate_issued_material(
            material,
            3,
            &["dev.localhost".to_string()],
            "dev-localhost",
            "ECDSA",
        )
        .expect_err("certificate and key mismatch must fail closed");
        assert!(error.to_string().contains("does not match the leaf"));
    }

    /// PLAN-2026-0003 §14 phase 3d requires the validity mismatch case of the
    /// rejection table to be exercised, not only SAN and SPKI. The certificate
    /// below is well-formed and correctly keyed but its window starts in the
    /// future, so `validate_issued_material` must refuse to store it.
    #[test]
    fn rejects_issued_certificate_that_is_not_yet_valid() {
        use rcgen::{CertificateParams, DistinguishedName, DnType};
        use time::OffsetDateTime;

        let key_pair = generate_key_pair("ECDSA").expect("issuance key pair");
        let mut params =
            CertificateParams::new(vec!["dev.localhost".to_string()]).expect("certificate params");
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, "dev.localhost");
        let not_before = OffsetDateTime::now_utc() + time::Duration::days(1);
        params.not_before = not_before;
        params.not_after = not_before + time::Duration::days(90);
        let cert_pem = params
            .self_signed(&key_pair)
            .expect("future-dated self-signed certificate")
            .pem();
        let evidence = certificate_evidence_from_pem(&cert_pem).expect("leaf evidence");

        let material = IssuedCertificateMaterial {
            cert_name: "dev-localhost".to_string(),
            cert_type: 3,
            issuer: evidence.issuer,
            subject: evidence.subject,
            san_list: evidence.san_list,
            serial_sha256: evidence.serial_sha256,
            fingerprint_sha256: evidence.fingerprint_sha256,
            spki_sha256: evidence.spki_sha256,
            chain_sha256: evidence.chain_sha256,
            key_algorithm: evidence.key_algorithm,
            cert_pem,
            private_key_pem: key_pair.serialize_pem(),
            chain_pem: None,
            not_before: evidence.not_before,
            not_after: evidence.not_after,
            cert_path: "/tmp/certs/dev-localhost/fullchain.pem".to_string(),
            key_path: "/tmp/certs/dev-localhost/privkey.pem".to_string(),
            chain_path: None,
        };

        let error = validate_issued_material(
            material,
            3,
            &["dev.localhost".to_string()],
            "dev-localhost",
            "ECDSA",
        )
        .expect_err("a certificate that is not yet valid must fail closed");
        assert!(error.to_string().contains("not currently valid"));
    }

    #[test]
    fn rejects_issued_certificate_with_tampered_metadata() {
        let mut material = self_signed_material();
        material.fingerprint_sha256 = "0".repeat(64);
        let error = validate_issued_material(
            material,
            3,
            &["dev.localhost".to_string()],
            "dev-localhost",
            "ECDSA",
        )
        .expect_err("metadata mismatch must fail closed");
        assert!(error.to_string().contains("metadata does not match"));
    }

    #[test]
    fn rejects_unbounded_operation_timeout() {
        let config = AcmeConfig::new(
            "https://acme-staging-v02.api.letsencrypt.org/directory".to_string(),
            "admin@example.com".to_string(),
            30,
            None,
            false,
        )
        .expect("config");
        assert!(
            CertificateIssuer::new_with_operation_timeout_ms(config, "/tmp/certs", 9_999).is_err()
        );
    }

    #[tokio::test]
    async fn rejects_unsafe_hostname_and_certificate_name() {
        let config = AcmeConfig::new(
            "https://acme-staging-v02.api.letsencrypt.org/directory".to_string(),
            "admin@example.com".to_string(),
            30,
            None,
            false,
        )
        .expect("config");
        let issuer = CertificateIssuer::new(config, "/tmp/certs/live").expect("issuer");
        assert!(issuer
            .issue(3, &["../escape".to_string()], "safe-name", "ECDSA")
            .await
            .is_err());
        assert!(issuer
            .issue(3, &["dev.localhost".to_string()], "../escape", "ECDSA")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn issuance_admission_has_no_waiter_queue() {
        let config = AcmeConfig::new(
            "https://acme-staging-v02.api.letsencrypt.org/directory".to_string(),
            "admin@example.com".to_string(),
            30,
            None,
            false,
        )
        .expect("config");
        let issuer = CertificateIssuer::new(config, "/tmp/certs/live").expect("issuer");
        let permits = (0..MAX_CONCURRENT_CERTIFICATE_ISSUANCE)
            .map(|_| issuer.admission.try_acquire().expect("permit"))
            .collect::<Vec<_>>();
        let error = issuer
            .issue(3, &["dev.localhost".to_string()], "dev-localhost", "ECDSA")
            .await
            .expect_err("capacity must fail closed");
        assert!(error.to_string().contains("capacity exhausted"));
        drop(permits);
        issuer
            .issue(3, &["dev.localhost".to_string()], "dev-localhost", "ECDSA")
            .await
            .expect("capacity recovers");
    }
}
