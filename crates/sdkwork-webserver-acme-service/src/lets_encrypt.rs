use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use instant_acme::{
    Account, AuthorizationStatus, ChallengeType, Identifier, NewAccount, NewOrder, OrderStatus,
    RetryPolicy,
};
use rcgen::{CertificateParams, DistinguishedName};

use crate::account_store::AcmeAccountStore;
use crate::challenge_store::ChallengeStore;
use crate::dns::{
    Dns01Presenter, Dns01RecordHandle, Dns01RecordRequest, dns01_record_name, dns01_txt_value,
};
use crate::http_client::AcmeHttpClientFactory;
use crate::model::IssuedCertificateMaterial;
use crate::self_signed::{certificate_evidence_from_pem, generate_key_pair};
use crate::{AcmeConfig, AcmeServiceError, AcmeServiceResult};

/// Ceiling on authorizations walked in one order.
///
/// One authorization per identifier, so this is exactly the identifier ceiling
/// ([`crate::MAX_CERTIFICATE_IDENTIFIERS`]) rather than a second, independently
/// chosen number. The value is restated as a literal so the ACME engine keeps no
/// dependency on the deployment contract; a test pins the two together.
/// A wildcard order spends one authorization per identifier, so a wildcard plus
/// its apex costs two.
const MAX_AUTHORIZATIONS_PER_ORDER: usize = crate::MAX_CERTIFICATE_IDENTIFIERS;

/// Process-lifetime fallback (directory URL → serialized credentials) for CA
/// accounts whose durable save failed. The durable store stays the source of
/// truth; this cache only keeps one process from re-creating the same account
/// (and burning the CA account-creation quota) while its disk is failing.
/// Locks are held only for map access, never across awaits.
static FALLBACK_ACCOUNT_CREDENTIALS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, String>>,
> = std::sync::OnceLock::new();

fn fallback_accounts() -> &'static std::sync::Mutex<std::collections::HashMap<String, String>> {
    FALLBACK_ACCOUNT_CREDENTIALS.get_or_init(|| std::sync::Mutex::new(Default::default()))
}

fn cache_fallback_account_credentials(
    directory_url: &str,
    credentials: &instant_acme::AccountCredentials,
) {
    match serde_json::to_string(credentials) {
        Ok(json) => {
            fallback_accounts()
                .lock()
                .expect("ACME fallback account map lock is never poisoned across awaits")
                .insert(directory_url.to_owned(), json);
        }
        Err(error) => {
            tracing::warn!(%error, "cannot serialize ACME credentials for the in-process fallback");
        }
    }
}

fn fallback_account_credentials(directory_url: &str) -> Option<instant_acme::AccountCredentials> {
    let json = fallback_accounts()
        .lock()
        .expect("ACME fallback account map lock is never poisoned across awaits")
        .get(directory_url)
        .cloned()?;
    serde_json::from_str(&json).ok()
}

/// Persists account credentials with short bounded retries; transient local
/// I/O hiccups must not cost a freshly created CA account.
async fn best_effort_save_account(
    account_store: &dyn AcmeAccountStore,
    directory_url: &str,
    credentials: &instant_acme::AccountCredentials,
) -> AcmeServiceResult<()> {
    let mut last_error = None;
    for attempt in 0..3_u32 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt))).await;
        }
        match account_store.save(directory_url, credentials).await {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        AcmeServiceError::provider("ACME account save failed without an error report")
    }))
}

/// How an order proves control of its identifiers.
#[derive(Clone)]
pub(crate) enum AcmeChallengeMode<'a> {
    /// HTTP-01 through the edge webroot. Exact identifiers only.
    Http01,
    /// DNS-01 through a provider presenter or a manual operator flow. The
    /// zone resolver maps each authorization's identifier to its hosted
    /// zone, so certificates spanning multiple zones (and multiple cloud
    /// accounts) renew unattended. The presenter is owned (not borrowed) so
    /// the withdrawal guard can hand surviving challenge records to a
    /// detached task when this future is cancelled mid-flight.
    Dns01 {
        presenter: Arc<dyn Dns01Presenter>,
        zones: &'a dyn crate::dns_zone::DnsZoneResolver,
    },
}

/// Issuance parameters grouped to keep the ACME entry points readable.
pub(crate) struct AcmeIssueParams<'a> {
    pub hostnames: &'a [String],
    pub cert_name: &'a str,
    pub cert_root: &'a str,
    pub key_algorithm: &'a str,
    pub mode: AcmeChallengeMode<'a>,
}

pub async fn issue_lets_encrypt(
    config: &AcmeConfig,
    challenge_store: &ChallengeStore,
    account_store: &dyn AcmeAccountStore,
    client_factory: &dyn AcmeHttpClientFactory,
    params: AcmeIssueParams<'_>,
    operation_timeout: Duration,
) -> AcmeServiceResult<IssuedCertificateMaterial> {
    match tokio::time::timeout(
        operation_timeout,
        issue_lets_encrypt_inner(
            config,
            challenge_store,
            account_store,
            client_factory,
            params,
            operation_timeout,
        ),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(AcmeServiceError::provider(format!(
            "ACME issuance timed out after {} ms",
            operation_timeout.as_millis()
        ))),
    }
}

async fn issue_lets_encrypt_inner(
    config: &AcmeConfig,
    challenge_store: &ChallengeStore,
    account_store: &dyn AcmeAccountStore,
    client_factory: &dyn AcmeHttpClientFactory,
    params: AcmeIssueParams<'_>,
    operation_timeout: Duration,
) -> AcmeServiceResult<IssuedCertificateMaterial> {
    let AcmeIssueParams {
        hostnames,
        cert_name,
        cert_root,
        key_algorithm,
        mode,
    } = params;

    // A wildcard SAN does not cover its own apex, and the CA must prove control
    // of every name the wildcard could expand to. Only DNS-01 can do that, so a
    // wildcard request on the HTTP-01 path is rejected before any order is
    // created rather than failing later at challenge selection.
    if hostnames
        .iter()
        .any(|hostname| is_wildcard_identifier(hostname))
        && matches!(mode, AcmeChallengeMode::Http01)
    {
        return Err(AcmeServiceError::validation(
            "wildcard identifiers require the DNS-01 validation method",
        ));
    }

    // HTTP-01 needs the edge webroot; DNS-01 never touches it, so a deployment
    // that only issues wildcard certificates does not need one configured.
    let webroot = match mode {
        AcmeChallengeMode::Http01 => {
            Some(config.webroot.as_deref().map(Path::new).ok_or_else(|| {
                AcmeServiceError::config(
                    "SDKWORK_WEBSERVER_ACME_WEBROOT is required for Let's Encrypt HTTP-01 issuance",
                )
            })?)
        }
        AcmeChallengeMode::Dns01 { .. } => None,
    };

    let contact = format!("mailto:{}", config.contact_email);
    // Restore the durable CA account when one exists; otherwise create one
    // account and persist it. Reusing one account avoids the CA account
    // creation rate limit and preserves account identity across renewals.
    // A credentials set whose durable save keeps failing is kept in the
    // process-lifetime fallback so a broken disk cannot burn the CA's
    // account-creation quota (Let's Encrypt: 50 accounts per IP per 3h).
    let account = if let Some(credentials) = account_store.load(&config.directory_url).await? {
        Account::builder_with_http(client_factory.build()?)
            .from_credentials(credentials)
            .await
            .map_err(|error| AcmeServiceError::provider(format!("restore ACME account: {error}")))?
    } else if let Some(credentials) = fallback_account_credentials(&config.directory_url) {
        tracing::warn!(
            directory = %config.directory_url,
            "durable ACME account store is unreadable or unwritable; reusing the account cached in this process"
        );
        Account::builder_with_http(client_factory.build()?)
            .from_credentials(credentials)
            .await
            .map_err(|error| AcmeServiceError::provider(format!("restore ACME account: {error}")))?
    } else {
        let (account, credentials) = Account::builder_with_http(client_factory.build()?)
            .create(
                &NewAccount {
                    contact: &[&contact],
                    terms_of_service_agreed: true,
                    only_return_existing: false,
                },
                config.directory_url.clone(),
                None,
            )
            .await
            .map_err(|error| AcmeServiceError::provider(error.to_string()))?;
        if let Err(error) =
            best_effort_save_account(account_store, &config.directory_url, &credentials).await
        {
            tracing::warn!(
                directory = %config.directory_url,
                error = %error,
                "durable ACME account save failed after retries; caching the credentials in this process to avoid burning the CA account quota"
            );
            cache_fallback_account_credentials(&config.directory_url, &credentials);
        }
        account
    };

    let identifiers = hostnames
        .iter()
        .cloned()
        .map(Identifier::Dns)
        .collect::<Vec<_>>();

    // DNS-01 presentations are owned by the guard so they are withdrawn on
    // every exit path — including the outer operation timeout and the
    // certificate worker's cycle watchdog cancelling this future mid-flight
    // (see `Dns01PresentationGuard`).
    let mut dns01_guard = match &mode {
        AcmeChallengeMode::Dns01 { presenter, .. } => {
            Some(Dns01PresentationGuard::new(Arc::clone(presenter)))
        }
        AcmeChallengeMode::Http01 => None,
    };
    let outcome: AcmeServiceResult<IssuedCertificateMaterial> = async {
        let mut order = account
            .new_order(&NewOrder::new(&identifiers))
            .await
            .map_err(|error| AcmeServiceError::provider(error.to_string()))?;

        // HTTP-01 leases clean their webroot file when dropped, so they must
        // outlive the CA's validation attempt and no longer.
        let mut challenge_leases = Vec::with_capacity(hostnames.len());
        let mut authorization_count = 0_usize;
        let mut authorizations = order.authorizations();
        while let Some(result) = authorizations.next().await {
            authorization_count += 1;
            if authorization_count > MAX_AUTHORIZATIONS_PER_ORDER {
                return Err(AcmeServiceError::provider(format!(
                    "ACME order exceeds {MAX_AUTHORIZATIONS_PER_ORDER} authorizations"
                )));
            }
            let mut authz =
                result.map_err(|error| AcmeServiceError::provider(error.to_string()))?;
            if authz.status == AuthorizationStatus::Valid {
                // Already authorized: reuse it and present nothing. This is the
                // normal path for a renewal inside the CA's authorization cache
                // window, and it keeps a wildcard order from re-publishing a
                // record the CA still considers valid.
                continue;
            }

            let challenge_type = match mode {
                AcmeChallengeMode::Http01 => ChallengeType::Http01,
                AcmeChallengeMode::Dns01 { .. } => ChallengeType::Dns01,
            };
            // Derive the label from `mode` rather than from `challenge_type`:
            // the latter is moved into `authz.challenge(...)` before the error
            // closure could borrow it.
            let challenge_label = challenge_type_label(&mode);
            let mut challenge = authz.challenge(challenge_type).ok_or_else(|| {
                AcmeServiceError::provider(format!(
                    "the CA offers no {challenge_label} challenge for this authorization"
                ))
            })?;

            match &mode {
                AcmeChallengeMode::Http01 => {
                    let token = challenge.token.clone();
                    let key_auth = challenge.key_authorization().as_str().to_string();
                    let lease = challenge_store
                        .register_scoped(webroot, &token, &key_auth)
                        .await?;
                    challenge_leases.push(lease);
                }
                AcmeChallengeMode::Dns01 { presenter, zones } => {
                    // The identifier is rendered with its wildcard marker, so a
                    // `*.example.com` authorization and its apex authorization
                    // both reduce to `_acme-challenge.example.com` while keeping
                    // distinct TXT values. Both must be published together; the
                    // CA may validate them in either order. The zone resolver
                    // picks the hosted zone per identifier, so certificates
                    // spanning multiple zones/accounts renew unattended.
                    let identifier = challenge.identifier().to_string();
                    let zone_apex = zones.zone_for(&identifier).ok_or_else(|| {
                        AcmeServiceError::validation(format!(
                            "no cloud account is associated with identifier {identifier}; \
                             associate its zone with a DNS provider account to renew"
                        ))
                    })?;
                    let record_name = dns01_record_name(&identifier)?;
                    let digest = challenge.key_authorization().digest();
                    let record_value = dns01_txt_value(digest.as_ref());
                    let request = Dns01RecordRequest::new(zone_apex, record_name, record_value)?;
                    let handle = presenter.publish(&request).await?;
                    if let Some(guard) = dns01_guard.as_mut() {
                        guard.push(handle);
                    }
                }
            }

            challenge
                .set_ready()
                .await
                .map_err(|error| AcmeServiceError::provider(error.to_string()))?;
        }

        let retry_timeout = operation_timeout.min(Duration::from_secs(120));
        let policy = RetryPolicy::default().timeout(retry_timeout);
        let status = order
            .poll_ready(&policy)
            .await
            .map_err(|error| AcmeServiceError::provider(error.to_string()))?;
        if status != OrderStatus::Ready {
            return Err(AcmeServiceError::provider(format!(
                "ACME order not ready: {status:?}"
            )));
        }

        drop(challenge_leases);
        let mut params = CertificateParams::new(hostnames.to_vec())
            .map_err(|error| AcmeServiceError::Internal(error.to_string()))?;
        params.distinguished_name = DistinguishedName::new();
        // RSA-2048 key generation is CPU-bound (100-500 ms); never run it on the
        // async executor.
        let key_algorithm_owned = key_algorithm.to_owned();
        let key_pair = tokio::task::spawn_blocking(move || generate_key_pair(&key_algorithm_owned))
            .await
            .map_err(|error| {
                AcmeServiceError::Internal(format!("join key generation: {error}"))
            })??;
        let csr = params
            .serialize_request(&key_pair)
            .map_err(|error| AcmeServiceError::Internal(error.to_string()))?;
        order
            .finalize_csr(csr.der())
            .await
            .map_err(|error| AcmeServiceError::provider(error.to_string()))?;
        let private_key_pem = key_pair.serialize_pem();
        let cert_chain_pem = order
            .poll_certificate(&policy)
            .await
            .map_err(|error| AcmeServiceError::provider(error.to_string()))?;

        let evidence = certificate_evidence_from_pem(&cert_chain_pem)?;
        let cert_dir = format!("{cert_root}/{cert_name}");
        let cert_path = format!("{cert_dir}/fullchain.pem");
        let key_path = format!("{cert_dir}/privkey.pem");

        let material = IssuedCertificateMaterial {
            cert_name: cert_name.to_string(),
            cert_type: 1,
            issuer: evidence.issuer,
            subject: evidence.subject,
            san_list: evidence.san_list,
            serial_sha256: evidence.serial_sha256,
            fingerprint_sha256: evidence.fingerprint_sha256,
            spki_sha256: evidence.spki_sha256,
            chain_sha256: evidence.chain_sha256,
            key_algorithm: evidence.key_algorithm,
            cert_pem: cert_chain_pem.clone(),
            private_key_pem: private_key_pem.clone(),
            chain_pem: Some(cert_chain_pem),
            not_before: evidence.not_before,
            not_after: evidence.not_after,
            cert_path: cert_path.clone(),
            key_path: key_path.clone(),
            chain_path: None,
        };
        // Make `cert_path`/`key_path` true on disk: single-host edges
        // (deb/rpm) terminate TLS directly from these canonical paths, so a
        // renewal is an in-place atomic replace the running edge picks up on
        // its next reload - no manual material copy. A failed export is
        // logged and does not fail the issuance (the material is durable in
        // the database; the edge keeps serving its previous certificate).
        if let Err(export_error) = crate::material_export::export_material_to_disk(&material).await
        {
            tracing::warn!(
                cert_name = %material.cert_name,
                error = %export_error,
                "issued material disk export failed; the canonical certificate paths are stale"
            );
        }
        Ok(material)
    }
    .await;

    // Withdraw on both success and failure while an async context is still
    // available, so the normal exit never needs the detached path. A leftover
    // TXT record is a stale credential that a third party could present
    // against the same identifier, so it must not outlive the order. A failed
    // withdrawal is logged but must not mask the issuance outcome. When this
    // future is cancelled instead of completed, the guard's `Drop` is what
    // withdraws the surviving records (a `Drop` cannot `.await`, so it hands
    // them to a detached task bounded by the presenter's request timeouts).
    if let Some(guard) = dns01_guard.as_mut() {
        guard.withdraw().await;
    }
    outcome
}

/// Owns the DNS-01 records published for one order until they are withdrawn.
///
/// The inline [`Dns01PresentationGuard::withdraw`] covers the normal exit.
/// Cancellation — the outer operation timeout in [`issue_lets_encrypt`], or
/// the certificate worker's cycle watchdog dropping the issuance future
/// mid-flight — cannot await, so [`Drop for Dns01PresentationGuard`] hands the
/// surviving records to a detached withdrawal task. Without it, a cancelled
/// order leaks live challenge TXT records for the identifier: a stale
/// credential a third party could present against the same identifier.
struct Dns01PresentationGuard {
    presenter: Arc<dyn Dns01Presenter>,
    published: Vec<Dns01RecordHandle>,
}

impl Dns01PresentationGuard {
    fn new(presenter: Arc<dyn Dns01Presenter>) -> Self {
        Self {
            presenter,
            published: Vec::new(),
        }
    }

    fn push(&mut self, handle: Dns01RecordHandle) {
        self.published.push(handle);
    }

    async fn withdraw(&mut self) {
        withdraw_dns01_presentations(self.presenter.as_ref(), &self.published).await;
        self.published.clear();
    }
}

impl Drop for Dns01PresentationGuard {
    fn drop(&mut self) {
        if self.published.is_empty() {
            return;
        }
        let published = std::mem::take(&mut self.published);
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                let presenter = Arc::clone(&self.presenter);
                runtime.spawn(async move {
                    withdraw_dns01_presentations(presenter.as_ref(), &published).await;
                });
            }
            Err(_) => tracing::error!(
                record_count = published.len(),
                "no async runtime is available to withdraw published DNS-01 challenge records"
            ),
        }
    }
}

async fn withdraw_dns01_presentations(
    presenter: &dyn Dns01Presenter,
    presentations: &[Dns01RecordHandle],
) {
    for handle in presentations {
        if let Err(error) = presenter.withdraw(handle).await {
            tracing::warn!(
                record_name = %handle.record_name,
                error = %error,
                "failed to withdraw a DNS-01 challenge record"
            );
        }
    }
}

fn is_wildcard_identifier(hostname: &str) -> bool {
    crate::challenge_policy::is_wildcard_identifier(hostname)
}

fn challenge_type_label(mode: &AcmeChallengeMode<'_>) -> &'static str {
    match mode {
        AcmeChallengeMode::Http01 => "http-01",
        AcmeChallengeMode::Dns01 { .. } => "dns-01",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_detection_handles_case_and_leading_dots() {
        assert!(is_wildcard_identifier("*.example.com"));
        assert!(is_wildcard_identifier("*.EXAMPLE.COM"));
        // A leading dot is tolerated because some callers normalize a trailing
        // dot into a leading one before reaching the engine.
        assert!(is_wildcard_identifier(".*.example.com"));
        assert!(!is_wildcard_identifier("example.com"));
        // Only a single leading label is a wildcard; a mid-name `*` is not.
        assert!(!is_wildcard_identifier("a.*.example.com"));
        assert!(!is_wildcard_identifier("*"));
    }

    #[test]
    fn challenge_labels_are_the_acme_wire_tokens() {
        assert_eq!(challenge_type_label(&AcmeChallengeMode::Http01), "http-01");
        let presenter = Arc::new(crate::dns::InMemoryDns01Presenter::new());
        let zones = crate::dns_zone::SingleZoneResolver {
            zone_apex: "example.com".to_owned(),
        };
        assert_eq!(
            challenge_type_label(&AcmeChallengeMode::Dns01 {
                presenter,
                zones: &zones,
            }),
            "dns-01"
        );
    }

    #[test]
    fn the_authorization_ceiling_matches_the_identifier_ceiling() {
        // The engine walks at most one authorization per requested identifier,
        // so the two bounds must be the same number. Restating it (instead of
        // importing the deployment contract) is what makes this assertion worth
        // having: it is the reminder that they cannot drift.
        assert_eq!(
            MAX_AUTHORIZATIONS_PER_ORDER,
            crate::MAX_CERTIFICATE_IDENTIFIERS
        );
        assert_eq!(MAX_AUTHORIZATIONS_PER_ORDER, 8);
    }

    #[test]
    fn wildcard_detection_is_shared_with_the_challenge_policy() {
        // One predicate, two consumers: if these ever disagree, a wildcard order
        // can be routed to HTTP-01 and rejected by the CA instead of by policy.
        use crate::challenge_policy::is_wildcard_identifier as policy_wildcard;
        for hostname in [
            "*.example.com",
            "*.EXAMPLE.COM",
            ".*.example.com",
            "a.*.example.com",
            "*",
            "example.com",
        ] {
            assert_eq!(
                is_wildcard_identifier(hostname),
                policy_wildcard(hostname),
                "wildcard predicate disagreed for `{hostname}`"
            );
        }
    }
}
