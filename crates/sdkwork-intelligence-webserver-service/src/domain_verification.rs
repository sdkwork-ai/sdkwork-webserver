use std::time::Duration;

use sdkwork_webserver_contract::WebServiceError;

use async_trait::async_trait;
use hickory_resolver::config::ResolverConfig;
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::proto::rr::RData;
use hickory_resolver::TokioResolver;
use sdkwork_utils_rust::crypto::sha256_hash;
use sdkwork_webserver_contract::{DomainVerifyResponse, WebServiceResult};

use crate::{DomainVerificationChallenge, DomainVerificationObservation, WebService};

const DNS_LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_TXT_RECORDS: usize = 128;
const MAX_TXT_RECORD_BYTES: usize = 4_096;

#[async_trait]
pub trait DomainOwnershipVerifier: Send + Sync {
    async fn observe(
        &self,
        challenge: &DomainVerificationChallenge,
    ) -> DomainVerificationObservation;
}

pub struct DnsTxtDomainOwnershipVerifier {
    resolver: TokioResolver,
}

impl DnsTxtDomainOwnershipVerifier {
    pub fn new() -> Result<Self, WebServiceError> {
        let resolver = match TokioResolver::builder_tokio() {
            Ok(builder) => builder.build().map_err(|error| {
                tracing::error!(%error, "system DNS resolver configuration failed to build");
                WebServiceError::Internal(
                    "system DNS resolver configuration failed to build".to_string(),
                )
            })?,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "system DNS resolver configuration unavailable; using bounded default resolver configuration"
                );
                hickory_resolver::Resolver::builder_with_config(
                    ResolverConfig::default(),
                    TokioRuntimeProvider::default(),
                )
                .build()
                .map_err(|error| {
                    tracing::error!(
                        error = %error,
                        "default DNS resolver configuration failed to build"
                    );
                    WebServiceError::Internal(
                        "default DNS resolver configuration failed to build".to_string(),
                    )
                })?
            }
        };
        Ok(Self { resolver })
    }
}

impl Default for DnsTxtDomainOwnershipVerifier {
    /// Panics only if neither the system nor the default resolver
    /// configuration can build, which leaves no verifier to construct; the
    /// composition path uses [`Self::new`] and propagates instead.
    #[allow(clippy::panic)]
    fn default() -> Self {
        match Self::new() {
            Ok(verifier) => verifier,
            Err(error) => panic!("no DNS resolver configuration could be built: {error}"),
        }
    }
}

#[async_trait]
impl DomainOwnershipVerifier for DnsTxtDomainOwnershipVerifier {
    async fn observe(
        &self,
        challenge: &DomainVerificationChallenge,
    ) -> DomainVerificationObservation {
        if challenge.method != "DNS_TXT" {
            return DomainVerificationObservation {
                observed_sha256: None,
                failure_code: Some("UNSUPPORTED_VERIFICATION_METHOD".to_string()),
            };
        }
        let lookup = match tokio::time::timeout(
            DNS_LOOKUP_TIMEOUT,
            self.resolver.txt_lookup(challenge.record_name.as_str()),
        )
        .await
        {
            Ok(Ok(lookup)) => lookup,
            Ok(Err(_)) => {
                return DomainVerificationObservation {
                    observed_sha256: None,
                    failure_code: Some("DNS_LOOKUP_FAILED".to_string()),
                }
            }
            Err(_) => {
                return DomainVerificationObservation {
                    observed_sha256: None,
                    failure_code: Some("DNS_LOOKUP_TIMEOUT".to_string()),
                }
            }
        };

        let payloads = lookup
            .answers()
            .iter()
            .take(MAX_TXT_RECORDS)
            .filter_map(|record| {
                let RData::TXT(txt) = &record.data else {
                    return None;
                };
                let size = txt
                    .txt_data
                    .iter()
                    .try_fold(0usize, |size, chunk| size.checked_add(chunk.len()))?;
                if size == 0 || size > MAX_TXT_RECORD_BYTES {
                    return None;
                }
                let mut payload = Vec::with_capacity(size);
                for chunk in txt.txt_data.iter() {
                    payload.extend_from_slice(chunk);
                }
                Some(payload)
            })
            .collect::<Vec<_>>();
        evaluate_txt_payloads(payloads, &challenge.proof_sha256)
    }
}

impl WebService {
    pub(crate) async fn execute_domain_verification(
        &self,
        tenant_id: i64,
        mut challenge: DomainVerificationChallenge,
    ) -> WebServiceResult<DomainVerifyResponse> {
        if challenge.status == "PENDING" && challenge.ready_for_check {
            let observation = self.domain_ownership_verifier.observe(&challenge).await;
            challenge = self
                .repository
                .record_domain_verification_observation(
                    tenant_id,
                    &challenge.challenge_id,
                    &observation,
                )
                .await?;
        }
        Ok(domain_verification_response(challenge))
    }
}

fn evaluate_txt_payloads(
    payloads: Vec<Vec<u8>>,
    expected_sha256: &str,
) -> DomainVerificationObservation {
    let mut hashes = payloads
        .into_iter()
        .map(|payload| sha256_hash(&payload))
        .collect::<Vec<_>>();
    hashes.sort_unstable();
    hashes.dedup();
    if hashes.iter().any(|hash| hash == expected_sha256) {
        DomainVerificationObservation {
            observed_sha256: Some(expected_sha256.to_string()),
            failure_code: None,
        }
    } else {
        DomainVerificationObservation {
            observed_sha256: hashes.into_iter().next(),
            failure_code: Some("DNS_TXT_RECORD_NOT_OBSERVED".to_string()),
        }
    }
}

fn domain_verification_response(challenge: DomainVerificationChallenge) -> DomainVerifyResponse {
    DomainVerifyResponse {
        verified: challenge.status == "VERIFIED",
        status: challenge.status,
        method: challenge.method,
        record_name: challenge.record_name,
        record_value: format!("sdkwork-domain-verification={}", challenge.challenge_id),
        attempt_count: challenge.attempt_count,
        expires_at: challenge.expires_at,
        next_attempt_at: challenge.next_attempt_at,
        checked_at: challenge.checked_at,
        failure_code: challenge.failure_code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txt_observation_matches_exact_payload_and_redacts_raw_values() {
        let expected = b"sdkwork-domain-verification=challenge";
        let expected_sha256 = sha256_hash(expected);
        let matched = evaluate_txt_payloads(
            vec![b"unrelated".to_vec(), expected.to_vec()],
            &expected_sha256,
        );
        assert_eq!(
            matched.observed_sha256.as_deref(),
            Some(expected_sha256.as_str())
        );
        assert!(matched.failure_code.is_none());

        let mismatched =
            evaluate_txt_payloads(vec![b"private-observed-value".to_vec()], &expected_sha256);
        assert_ne!(
            mismatched.observed_sha256.as_deref(),
            Some("private-observed-value")
        );
        assert_eq!(
            mismatched.failure_code.as_deref(),
            Some("DNS_TXT_RECORD_NOT_OBSERVED")
        );
    }
}
