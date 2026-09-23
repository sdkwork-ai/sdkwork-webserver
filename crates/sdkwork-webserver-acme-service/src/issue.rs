use chrono::{DateTime, Utc};
use rcgen::KeyPair;
use sdkwork_deploy_core::validate_certificate_key_algorithm;
use sdkwork_utils_rust::crypto::sha256_hash;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

use crate::account_store::{AcmeAccountStore, MemoryAcmeAccountStore};
use crate::challenge_policy::{
    resolve_challenge_method, ChallengeAvailability, DeclaredChallengeMethod, ResolvedChallenge,
};
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
/// The authoritative ceiling is the control plane's
/// `MAX_CERTIFICATE_IDENTIFIERS` (8), restated by the DDL constraint
/// `chk_webserver_certificate_identifier_position CHECK (position BETWEEN 0 AND 7)`.
/// The engine repeats the bound instead of importing the deployment contract (the
/// ACME engine must not know about the deployment domain), and it must be the
/// *same* bound: a larger one here would let a request through that the control
/// plane rejects with a 400 and the database rejects with a constraint violation,
/// turning a validation error into an internal failure.
pub const MAX_CERTIFICATE_IDENTIFIERS: usize = 8;

pub struct CertificateIssuer {
    pub(crate) config: AcmeConfig,
    pub(crate) challenge_store: Arc<ChallengeStore>,
    pub(crate) account_store: Arc<dyn AcmeAccountStore>,
    pub(crate) client_factory: Arc<dyn AcmeHttpClientFactory>,
    pub(crate) cert_root: String,
    pub(crate) operation_timeout: Duration,
    pub(crate) admission: Semaphore,
    /// Associated cloud DNS accounts. When present (and covering every
    /// identifier), issuance and renewal can use DNS-01 — the only challenge
    /// that renews wildcard certificates unattended.
    pub(crate) dns_accounts: Option<Arc<crate::dns_account::DnsCloudAccountRegistry>>,
    /// Operator-declared challenge method. `AUTO` (the default) resolves to
    /// HTTP-01 for exact identifiers and DNS-01 for wildcards.
    pub(crate) declared_challenge_method: DeclaredChallengeMethod,
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
            declared_challenge_method: DeclaredChallengeMethod::default(),
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

    /// Sets the operator-declared challenge method (from
    /// `SDKWORK_WEBSERVER_ACME_CHALLENGE_METHOD` or the control plane's
    /// `webserver_tls_policy.challenge_method`).
    pub fn set_declared_challenge_method(&mut self, method: DeclaredChallengeMethod) {
        self.declared_challenge_method = method;
    }

    pub fn declared_challenge_method(&self) -> DeclaredChallengeMethod {
        self.declared_challenge_method
    }

    /// True when the registry covers every identifier: DNS-01 is available.
    pub fn dns_accounts_cover(&self, hostnames: &[String]) -> bool {
        self.dns_accounts
            .as_ref()
            .is_some_and(|registry| registry.covers_all(hostnames.iter().map(String::as_str)))
    }

    /// What this deployment can actually do for `hostnames`.
    ///
    /// Both inputs are computed here rather than at the call site so a caller
    /// cannot disagree with the policy about, for example, whether the webroot
    /// is configured.
    pub fn challenge_availability(&self, hostnames: &[String]) -> ChallengeAvailability {
        ChallengeAvailability {
            http01_webroot_configured: self.config.webroot.is_some(),
            dns01_accounts_cover_all: self.dns_accounts_cover(hostnames),
        }
    }

    /// Resolves the challenge method for one order without building anything.
    ///
    /// Used to record/explain the decision; [`Self::challenge_plan`] is the
    /// variant that also supplies the presenter.
    pub fn resolve_challenge(&self, hostnames: &[String]) -> AcmeServiceResult<ResolvedChallenge> {
        resolve_challenge_method(
            self.declared_challenge_method,
            hostnames,
            self.challenge_availability(hostnames),
        )
    }

    /// The complete challenge decision for one order: the method **and**, for
    /// DNS-01, the presenter and zone resolver.
    ///
    /// This is the single entry point callers must use. Choosing the method in
    /// one place and the presenter in another is how the two drifted apart
    /// before: the method was derived from `dns_accounts_cover` while the
    /// presenter came from a registry that nothing ever attached, so wildcards
    /// were routed to HTTP-01 and rejected by the engine.
    pub fn challenge_plan(&self, hostnames: &[String]) -> AcmeServiceResult<ChallengePlan> {
        match self.resolve_challenge(hostnames)? {
            ResolvedChallenge::Http01 => Ok(ChallengePlan::Http01),
            ResolvedChallenge::Dns01 => {
                let context = self.dns01_context(hostnames).ok_or_else(|| {
                    // Unreachable while the policy resolved DNS-01 from the same
                    // registry, but fail closed rather than silently downgrade.
                    AcmeServiceError::config(
                        "DNS-01 was selected but no cloud account covers every identifier",
                    )
                })?;
                Ok(ChallengePlan::Dns01(context))
            }
        }
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
        // Checked against the shared vocabulary, not a local list: the same set
        // guards the DDL constraints and the control plane's own validation, and the
        // key generation below is keyed by the same constants.
        if let Err(reason) = validate_certificate_key_algorithm(key_algorithm) {
            return Err(AcmeServiceError::validation(format!(
                "keyAlgorithm {reason}"
            )));
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

/// The resolved challenge for one order, ready to hand to
/// [`CertificateIssuer::issue_with_challenge`].
pub enum ChallengePlan {
    /// HTTP-01 through the edge webroot.
    Http01,
    /// DNS-01 through the issuer's cloud-account registry.
    Dns01(IssuerDns01Context),
}

impl ChallengePlan {
    /// The DDL/wire spelling of the selected method, for logging and for
    /// asserting what was actually used.
    pub fn method(&self) -> &'static str {
        match self {
            Self::Http01 => ResolvedChallenge::Http01.as_str(),
            Self::Dns01(_) => ResolvedChallenge::Dns01.as_str(),
        }
    }
}

/// Manual `Debug`: the DNS-01 variant holds the presenter and registry, which
/// carry credentials and are deliberately not `Debug`. Reporting the selected
/// method is all a log or an assertion message needs.
impl std::fmt::Debug for ChallengePlan {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("ChallengePlan")
            .field(&self.method())
            .finish()
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

    fn issuer_with_webroot(webroot: Option<&str>) -> CertificateIssuer {
        let config = AcmeConfig::new(
            "https://acme-staging-v02.api.letsencrypt.org/directory".to_string(),
            "admin@example.com".to_string(),
            30,
            webroot.map(str::to_string),
            false,
        )
        .expect("config");
        CertificateIssuer::new(config, "/tmp/certs/live").expect("issuer")
    }

    fn registry_covering(zone_apex: &str) -> Arc<crate::dns_account::DnsCloudAccountRegistry> {
        let mut registry = crate::dns_account::DnsCloudAccountRegistry::empty();
        registry
            .register(crate::dns_account::DnsCloudAccount {
                account_id: "test-account".to_string(),
                zone_apex: zone_apex.to_string(),
                provider: crate::dns::DnsProviderKind::Cloudflare,
                presenter: Arc::new(crate::dns::InMemoryDns01Presenter::default()),
            })
            .expect("register account");
        Arc::new(registry)
    }

    /// The product default on the real issuer, not only on the pure function:
    /// an exact-only order goes HTTP-01 even when DNS accounts are attached.
    #[test]
    fn challenge_plan_prefers_http01_for_exact_identifiers() {
        let mut issuer = issuer_with_webroot(Some("/var/www/acme"));
        issuer.attach_dns_accounts(registry_covering("example.com"));
        assert_eq!(
            issuer.declared_challenge_method(),
            DeclaredChallengeMethod::Auto
        );
        let plan = issuer
            .challenge_plan(&["www.example.com".to_string()])
            .expect("plan");
        assert_eq!(plan.method(), "HTTP_01");
    }

    /// The other half of the default: a wildcard goes DNS-01 as soon as an
    /// account covers its zone.
    #[test]
    fn challenge_plan_selects_dns01_for_wildcards() {
        let mut issuer = issuer_with_webroot(Some("/var/www/acme"));
        issuer.attach_dns_accounts(registry_covering("example.com"));
        let plan = issuer
            .challenge_plan(&["*.example.com".to_string(), "example.com".to_string()])
            .expect("plan");
        assert_eq!(plan.method(), "DNS_01");
    }

    /// Without a registry a wildcard is a *configuration* failure, reported at
    /// the boundary instead of by the CA deep inside the order.
    #[test]
    fn challenge_plan_fails_closed_for_wildcards_without_dns_accounts() {
        let issuer = issuer_with_webroot(Some("/var/www/acme"));
        let error = issuer
            .challenge_plan(&["*.example.com".to_string()])
            .expect_err("wildcard without accounts must fail closed");
        assert!(
            matches!(error, AcmeServiceError::Config(_)),
            "a missing DNS account is configuration, not a bad request: {error:?}"
        );
        assert!(error.to_string().contains("DNS-01"));
    }

    /// A registry that covers some identifiers but not all must not be treated
    /// as "DNS-01 is available": a partially covered order would publish the
    /// covered records and then fail the uncovered authorization.
    #[test]
    fn challenge_plan_requires_every_identifier_to_be_covered() {
        let mut issuer = issuer_with_webroot(Some("/var/www/acme"));
        issuer.attach_dns_accounts(registry_covering("example.com"));
        assert!(issuer.dns_accounts_cover(&["www.example.com".to_string()]));
        assert!(!issuer.dns_accounts_cover(&["www.other.org".to_string()]));
        let error = issuer
            .challenge_plan(&["*.example.com".to_string(), "*.other.org".to_string()])
            .expect_err("a partially covered order must fail closed");
        assert!(error.to_string().contains("DNS-01"));
    }

    #[test]
    fn declared_challenge_method_overrides_the_auto_default() {
        let mut issuer = issuer_with_webroot(Some("/var/www/acme"));
        issuer.attach_dns_accounts(registry_covering("example.com"));
        issuer.set_declared_challenge_method(DeclaredChallengeMethod::Dns01);
        let plan = issuer
            .challenge_plan(&["www.example.com".to_string()])
            .expect("plan");
        assert_eq!(plan.method(), "DNS_01");

        issuer.set_declared_challenge_method(DeclaredChallengeMethod::Http01);
        let error = issuer
            .challenge_plan(&["*.example.com".to_string()])
            .expect_err("HTTP-01 cannot prove a wildcard");
        assert!(matches!(error, AcmeServiceError::Validation(_)));
    }

    /// Without a webroot and without DNS accounts there is no way to prove
    /// control; the failure names both remedies.
    #[test]
    fn challenge_plan_reports_both_remedies_when_nothing_is_configured() {
        let issuer = issuer_with_webroot(None);
        let error = issuer
            .challenge_plan(&["www.example.com".to_string()])
            .expect_err("no challenge method is available");
        let message = error.to_string();
        assert!(
            message.contains("SDKWORK_WEBSERVER_ACME_WEBROOT"),
            "{message}"
        );
        assert!(message.contains("DNS provider account"), "{message}");
    }

    /// Engine-level invariant kept behind the policy: even if a caller
    /// hand-builds the HTTP-01 mode, a wildcard cannot reach the CA.
    #[tokio::test]
    async fn wildcard_still_cannot_reach_the_ca_through_the_http01_path() {
        let issuer = issuer_with_webroot(Some("/var/www/acme"));
        let error = issuer
            .issue(1, &["*.example.com".to_string()], "wildcard", "ECDSA")
            .await
            .expect_err("the engine must reject a wildcard on the HTTP-01 path");
        assert!(
            error.to_string().contains("wildcard identifiers require"),
            "{error}"
        );
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
