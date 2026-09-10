// ACME Renewal Information (ARI, RFC 9773) lookup.
//
// The CA publishes a suggested renewal window per certificate; the control
// plane records it on the certificate aggregate so the due-renewal scheduler
// prefers the CA window over the fixed `renew_before_days` fallback.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use instant_acme::{CertificateIdentifier, Error as InstantAcmeError};
use x509_parser::extensions::ParsedExtension;
use x509_parser::pem::parse_x509_pem;

use crate::http_client::BoundedAcmeHttpClient;
use crate::{AcmeServiceError, AcmeServiceResult, CertificateIssuer};

/// CA-suggested renewal window (RFC 9773 §4.2), serialized RFC 3339.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AriRenewalWindow {
    pub window_start: String,
    pub window_end: String,
}

impl CertificateIssuer {
    /// Queries the CA-suggested renewal window for the leaf of `cert_pem`.
    ///
    /// Returns `None` when the CA does not support ARI (`Error::Unsupported`),
    /// so the fixed renewal window remains the fallback. Any other failure is
    /// reported to the caller.
    pub async fn renewal_info(
        &self,
        cert_pem: &str,
    ) -> AcmeServiceResult<Option<AriRenewalWindow>> {
        // Self-bounded like issuance and revocation: the renewal-cycle
        // watchdog drops hung cycles, but the lookup itself must fail within
        // the issuer's operation timeout so a stalled CA never pins a worker
        // batch past its lease.
        match tokio::time::timeout(self.operation_timeout, self.renewal_info_inner(cert_pem)).await
        {
            Ok(result) => result,
            Err(_) => Err(AcmeServiceError::provider(format!(
                "ACME ARI lookup timed out after {} ms",
                self.operation_timeout.as_millis()
            ))),
        }
    }

    async fn renewal_info_inner(
        &self,
        cert_pem: &str,
    ) -> AcmeServiceResult<Option<AriRenewalWindow>> {
        let (_, pem) = parse_x509_pem(cert_pem.as_bytes()).map_err(|error| {
            AcmeServiceError::Internal(format!("parse certificate for ARI lookup: {error}"))
        })?;
        if pem.label != "CERTIFICATE" {
            return Err(AcmeServiceError::validation(
                "certificate for ARI lookup must be a PEM certificate",
            ));
        }
        let cert = pem.parse_x509().map_err(|error| {
            AcmeServiceError::Internal(format!("parse leaf certificate: {error}"))
        })?;
        let authority_key_identifier = cert
            .extensions()
            .iter()
            .find_map(|extension| match extension.parsed_extension() {
                ParsedExtension::AuthorityKeyIdentifier(identifier) => {
                    identifier.key_identifier.clone()
                }
                _ => None,
            })
            .ok_or_else(|| {
                AcmeServiceError::Internal(
                    "certificate has no authority key identifier extension".to_string(),
                )
            })?;
        let credentials = self
            .account_store
            .load(&self.config.directory_url)
            .await?
            .ok_or_else(|| {
                AcmeServiceError::provider(
                    "no ACME account exists for the configured CA; cannot query renewal information",
                )
            })?;
        let account =
            instant_acme::Account::builder_with_http(Box::new(BoundedAcmeHttpClient::new()?))
                .from_credentials(credentials)
                .await
                .map_err(|error| {
                    AcmeServiceError::provider(format!(
                        "restore ACME account for ARI lookup: {error}"
                    ))
                })?;
        let certificate_id = CertificateIdentifier {
            authority_key_identifier: std::borrow::Cow::Owned(
                URL_SAFE_NO_PAD.encode(authority_key_identifier.0),
            ),
            serial: std::borrow::Cow::Owned(URL_SAFE_NO_PAD.encode(cert.raw_serial())),
        };
        let (renewal_info, _) = match account.renewal_info(&certificate_id).await {
            Ok(renewal_info) => renewal_info,
            Err(InstantAcmeError::Unsupported(_)) => return Ok(None),
            Err(error) => {
                return Err(AcmeServiceError::provider(format!(
                    "ACME renewal information lookup failed: {error}"
                )));
            }
        };
        Ok(Some(AriRenewalWindow {
            window_start: renewal_info.suggested_window.start.to_string(),
            window_end: renewal_info.suggested_window.end.to_string(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};

    fn sample_certificate_pem() -> String {
        let mut params =
            CertificateParams::new(vec!["dev.localhost".to_string()]).expect("certificate params");
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, "dev.localhost");
        params.use_authority_key_identifier_extension = true;
        let key = KeyPair::generate().expect("generate key");
        params.self_signed(&key).expect("self-sign").pem()
    }

    #[test]
    fn ari_identifier_derivation_matches_certificate_evidence() {
        let cert_pem = sample_certificate_pem();
        let (_, pem) = parse_x509_pem(cert_pem.as_bytes()).expect("parse PEM");
        let cert = pem.parse_x509().expect("parse certificate");
        let authority_key_identifier = cert
            .extensions()
            .iter()
            .find_map(|extension| match extension.parsed_extension() {
                ParsedExtension::AuthorityKeyIdentifier(identifier) => {
                    identifier.key_identifier.clone()
                }
                _ => None,
            })
            .expect("self-signed certificate carries an AKI");
        assert!(!authority_key_identifier.0.is_empty());
        let serial = cert.raw_serial();
        assert!(!serial.is_empty());
        // The encoded identifiers are bounded and URL-safe (no padding).
        let encoded = URL_SAFE_NO_PAD.encode(authority_key_identifier.0);
        assert!(encoded.len() <= 128);
        assert!(!encoded.contains('='));
    }
}
