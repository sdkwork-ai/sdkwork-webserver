use std::path::Path;
use std::time::Duration;

use instant_acme::{
    Account, AuthorizationStatus, ChallengeType, Identifier, NewAccount, NewOrder, OrderStatus,
    RetryPolicy,
};
use rcgen::{CertificateParams, DistinguishedName};

use crate::account_store::AcmeAccountStore;
use crate::challenge_store::ChallengeStore;
use crate::dns::{
    dns01_record_name, dns01_txt_value, Dns01Presenter, Dns01RecordHandle, Dns01RecordRequest,
};
use crate::http_client::AcmeHttpClientFactory;
use crate::model::IssuedCertificateMaterial;
use crate::self_signed::{certificate_evidence_from_pem, generate_key_pair};
use crate::{AcmeConfig, AcmeServiceError, AcmeServiceResult};

/// Ceiling on authorizations walked in one order.
///
/// This mirrors the deployment contract's `MAX_CERTIFICATE_IDENTIFIERS`, and is
/// deliberately a literal rather than a dependency on that contract: the ACME
/// engine must not know about the deployment domain. A wildcard order spends
/// one authorization per identifier, so a wildcard plus its apex costs two.
const MAX_AUTHORIZATIONS_PER_ORDER: usize = 100;

/// How an order proves control of its identifiers.
#[derive(Clone, Copy)]
pub(crate) enum AcmeChallengeMode<'a> {
    /// HTTP-01 through the edge webroot. Exact identifiers only.
    Http01,
    /// DNS-01 through a provider presenter or a manual operator flow.
    Dns01 {
        presenter: &'a dyn Dns01Presenter,
        /// Hosted zone apex the presented records belong to.
        zone_apex: &'a str,
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
    let account = if let Some(credentials) = account_store.load(&config.directory_url).await? {
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
        account_store
            .save(&config.directory_url, &credentials)
            .await?;
        account
    };

    let identifiers = hostnames
        .iter()
        .cloned()
        .map(Identifier::Dns)
        .collect::<Vec<_>>();

    // DNS-01 presentations are collected outside the order block so they can be
    // withdrawn on every exit path, including a timeout or a CA refusal.
    let mut presentations: Vec<Dns01RecordHandle> = Vec::new();
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
            let challenge_label = challenge_type_label(mode);
            let mut challenge = authz.challenge(challenge_type).ok_or_else(|| {
                AcmeServiceError::provider(format!(
                    "the CA offers no {challenge_label} challenge for this authorization"
                ))
            })?;

            match mode {
                AcmeChallengeMode::Http01 => {
                    let token = challenge.token.clone();
                    let key_auth = challenge.key_authorization().as_str().to_string();
                    let lease = challenge_store
                        .register_scoped(webroot, &token, &key_auth)
                        .await?;
                    challenge_leases.push(lease);
                }
                AcmeChallengeMode::Dns01 {
                    presenter,
                    zone_apex,
                } => {
                    // The identifier is rendered with its wildcard marker, so a
                    // `*.example.com` authorization and its apex authorization
                    // both reduce to `_acme-challenge.example.com` while keeping
                    // distinct TXT values. Both must be published together; the
                    // CA may validate them in either order.
                    let identifier = challenge.identifier().to_string();
                    let record_name = dns01_record_name(&identifier)?;
                    let digest = challenge.key_authorization().digest();
                    let record_value = dns01_txt_value(digest.as_ref());
                    let request = Dns01RecordRequest::new(zone_apex, record_name, record_value)?;
                    let handle = presenter.publish(&request).await?;
                    presentations.push(handle);
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

        Ok(IssuedCertificateMaterial {
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
            private_key_pem,
            chain_pem: Some(cert_chain_pem),
            not_before: evidence.not_before,
            not_after: evidence.not_after,
            cert_path,
            key_path,
            chain_path: None,
        })
    }
    .await;

    // Withdraw on both success and failure. A leftover TXT record is a stale
    // credential that a third party could present against the same identifier,
    // so it must not outlive the order. A failed withdrawal is logged but must
    // not mask the issuance outcome.
    if let AcmeChallengeMode::Dns01 { presenter, .. } = mode {
        withdraw_dns01_presentations(presenter, &presentations).await;
    }
    outcome
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
    hostname
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .starts_with("*.")
}

fn challenge_type_label(mode: AcmeChallengeMode<'_>) -> &'static str {
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
        assert_eq!(challenge_type_label(AcmeChallengeMode::Http01), "http-01");
        let presenter = crate::dns::InMemoryDns01Presenter::new();
        assert_eq!(
            challenge_type_label(AcmeChallengeMode::Dns01 {
                presenter: &presenter,
                zone_apex: "example.com",
            }),
            "dns-01"
        );
    }

    #[test]
    fn the_authorization_ceiling_matches_the_deployment_contract() {
        // Kept as a literal to avoid coupling the engine to the deployment
        // domain; this test is the reminder that the two must not drift.
        assert_eq!(MAX_AUTHORIZATIONS_PER_ORDER, 100);
    }
}
