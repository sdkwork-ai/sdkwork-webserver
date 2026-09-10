// Certificate revocation against the ACME CA.

use instant_acme::{RevocationReason, RevocationRequest};
use x509_parser::pem::parse_x509_pem;

use crate::http_client::BoundedAcmeHttpClient;
use crate::{AcmeServiceError, AcmeServiceResult, CertificateIssuer};

/// Revocation reasons accepted by the control plane (RFC 5280 §5.3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CertificateRevocationReason {
    KeyCompromise,
    AffiliationChanged,
    Superseded,
    CessationOfOperation,
    PrivilegeWithdrawn,
}

impl CertificateRevocationReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::KeyCompromise => "keyCompromise",
            Self::AffiliationChanged => "affiliationChanged",
            Self::Superseded => "superseded",
            Self::CessationOfOperation => "cessationOfOperation",
            Self::PrivilegeWithdrawn => "privilegeWithdrawn",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "keyCompromise" => Some(Self::KeyCompromise),
            "affiliationChanged" => Some(Self::AffiliationChanged),
            "superseded" => Some(Self::Superseded),
            "cessationOfOperation" => Some(Self::CessationOfOperation),
            "privilegeWithdrawn" => Some(Self::PrivilegeWithdrawn),
            _ => None,
        }
    }
}

impl From<CertificateRevocationReason> for RevocationReason {
    fn from(reason: CertificateRevocationReason) -> Self {
        match reason {
            CertificateRevocationReason::KeyCompromise => Self::KeyCompromise,
            CertificateRevocationReason::AffiliationChanged => Self::AffiliationChanged,
            CertificateRevocationReason::Superseded => Self::Superseded,
            CertificateRevocationReason::CessationOfOperation => Self::CessationOfOperation,
            CertificateRevocationReason::PrivilegeWithdrawn => Self::PrivilegeWithdrawn,
        }
    }
}

impl CertificateIssuer {
    /// Revokes the leaf certificate of `cert_pem` with the CA that issued it.
    ///
    /// The issuer account for the configured directory URL must exist (the
    /// certificate was issued by this control plane); the account credentials
    /// are restored from the durable store and reused for the revocation
    /// request. Returns an error when the CA rejects the revocation, so the
    /// caller never marks the certificate revoked locally without the CA
    /// acknowledging it.
    pub async fn revoke_certificate(
        &self,
        cert_pem: &str,
        reason: CertificateRevocationReason,
    ) -> AcmeServiceResult<()> {
        // Every external CA call self-bounds with the issuer's operation
        // timeout: a CA that accepts the connection and never responds fails
        // the management request instead of pinning admission and database
        // resources indefinitely.
        match tokio::time::timeout(
            self.operation_timeout,
            self.revoke_certificate_inner(cert_pem, reason),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(AcmeServiceError::provider(format!(
                "ACME revocation timed out after {} ms",
                self.operation_timeout.as_millis()
            ))),
        }
    }

    async fn revoke_certificate_inner(
        &self,
        cert_pem: &str,
        reason: CertificateRevocationReason,
    ) -> AcmeServiceResult<()> {
        let (_, pem) = parse_x509_pem(cert_pem.as_bytes()).map_err(|error| {
            AcmeServiceError::Internal(format!("parse certificate for revocation: {error}"))
        })?;
        if pem.label != "CERTIFICATE" {
            return Err(AcmeServiceError::validation(
                "certificate for revocation must be a PEM certificate",
            ));
        }
        let credentials = self
            .account_store
            .load(&self.config.directory_url)
            .await?
            .ok_or_else(|| {
                AcmeServiceError::provider(
                    "no ACME account exists for the configured CA; cannot revoke",
                )
            })?;
        let account =
            instant_acme::Account::builder_with_http(Box::new(BoundedAcmeHttpClient::new()?))
                .from_credentials(credentials)
                .await
                .map_err(|error| {
                    AcmeServiceError::provider(format!(
                        "restore ACME account for revocation: {error}"
                    ))
                })?;
        let certificate_der = rustls_pki_types::CertificateDer::from(pem.contents);
        account
            .revoke(&RevocationRequest {
                certificate: &certificate_der,
                reason: Some(reason.into()),
            })
            .await
            .map_err(|error| {
                AcmeServiceError::provider(format!("ACME certificate revocation failed: {error}"))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revocation_reason_round_trips_stable_tokens() {
        for reason in [
            CertificateRevocationReason::KeyCompromise,
            CertificateRevocationReason::AffiliationChanged,
            CertificateRevocationReason::Superseded,
            CertificateRevocationReason::CessationOfOperation,
            CertificateRevocationReason::PrivilegeWithdrawn,
        ] {
            assert_eq!(
                CertificateRevocationReason::parse(reason.as_str()),
                Some(reason)
            );
        }
        assert_eq!(CertificateRevocationReason::parse("unspecified"), None);
        assert_eq!(CertificateRevocationReason::parse(""), None);
    }
}
