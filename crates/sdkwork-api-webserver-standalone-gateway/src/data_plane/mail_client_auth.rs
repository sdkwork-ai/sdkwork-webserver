//! nginx mail client-certificate verification (Wave 14B).
//!
//! nginx's mail SSL module requests a client certificate whenever
//! `ssl_verify_client` is not `off`, but its OpenSSL verify callback always
//! returns success: the handshake completes and the verification result is
//! checked *afterwards* (`ngx_mail_verify_cert`). A failure is therefore
//! reported with the protocol's application-level line (`-ERR`, `* BYE`,
//! `421`) and the connection is closed, not with a TLS alert.
//!
//! This module reproduces that two-phase behaviour on top of rustls:
//!
//! - the handshake verifier requests a certificate and accepts whatever the
//!   client presents, while still validating the TLS `CertificateVerify`
//!   signature (a forged signature must not complete the handshake);
//! - [`MailClientAuth::evaluate_peer`] runs the deferred path verification
//!   against the union of `ssl_client_certificate` and
//!   `ssl_trusted_certificate`, enforces `ssl_verify_depth`, and records the
//!   certificate facts nginx forwards to `auth_http`.
//!
//! The `ssl_client_certificate` bundle is also the CertificateRequest CA hint
//! list; `ssl_trusted_certificate` only contributes to the verification store,
//! exactly like `ngx_ssl_client_certificate` vs `ngx_ssl_trusted_certificate`.
//!
//! Known bounded differences from nginx 1.29.6, documented rather than hidden:
//!
//! - `ssl_verify_depth` is enforced against the intermediate certificates the
//!   client presents. nginx enforces the depth of the path OpenSSL actually
//!   built; the two agree for normal chains (a client sends exactly its
//!   chain), but a client that presents unrelated extra intermediates can be
//!   rejected here where nginx would ignore them.
//! - rustls additionally requires a presented EKU list to include
//!   `clientAuth`; nginx's generic OpenSSL verification does not select a
//!   purpose, so a certificate whose EKU is `serverAuth`-only is rejected here
//!   and accepted by nginx. Certificates without an EKU extension behave the
//!   same in both.
//!
//! - `optional_no_ca` without either bundle has no trust anchor to build a
//!   path against (nginx's empty OpenSSL store reports the same optional
//!   "unable to verify" error), so the runtime still enforces the certificate
//!   validity window and records `FAILED:...`; it never reports `SUCCESS`.
//!
//! nginx 1.29.6 imposes **no** 8 KiB limit on the `Auth-SSL-Cert` header: a
//! probe with a 13.5 KiB PEM produced a successful 14.5 KiB `auth_http`
//! request, so the runtime forwards the whole certificate instead of
//! inventing a truncation the reference build does not have.

use std::sync::Arc;

use base64::Engine as _;
use rustls::{
    pki_types::{pem::PemObject, CertificateDer, UnixTime},
    server::{
        danger::{ClientCertVerified, ClientCertVerifier},
        WebPkiClientVerifier,
    },
    DistinguishedName, RootCertStore, SignatureScheme,
};
use sdkwork_webserver_core::{MailClientVerifyMode, MailTlsConfig};
use sha1::{Digest as _, Sha1};
use x509_parser::prelude::{FromDer, X509Certificate};

use super::DataPlaneError;

/// A client certificate accepted for the session, with the facts nginx
/// forwards to `auth_http`.
#[derive(Debug, Clone)]
pub(crate) struct MailClientCertificate {
    /// `SUCCESS` or `FAILED:<reason>` (`Auth-SSL-Verify`).
    pub(crate) verify: String,
    /// RFC 2253 subject (`Auth-SSL-Subject`).
    pub(crate) subject: String,
    /// RFC 2253 issuer (`Auth-SSL-Issuer`).
    pub(crate) issuer: String,
    /// Uppercase hexadecimal serial (`Auth-SSL-Serial`).
    pub(crate) serial: String,
    /// Lowercase SHA-1 hexadecimal fingerprint (`Auth-SSL-Fingerprint`).
    pub(crate) fingerprint: String,
    /// PEM certificate (`Auth-SSL-Cert`, only sent for
    /// `auth_http_pass_client_cert on`).
    pub(crate) pem: String,
}

/// Why the session must be rejected before the frontend dialog starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MailClientCertificateRejection {
    /// `ssl_verify_client on` with no client certificate.
    NoCertificate,
    /// A certificate was presented but did not verify (or exceeded the depth).
    CertificateError,
}

/// Resolved client-certificate policy of one mail TLS listener.
pub(crate) struct MailClientAuth {
    mode: MailClientVerifyMode,
    depth: u32,
    /// Deferred verifier over the union of both bundles. `None` when the only
    /// configured mode is `optional_no_ca` without a CA bundle; that mode then
    /// tolerates the resulting "unknown issuer" for well-formed certificates.
    verifier: Option<Arc<dyn ClientCertVerifier>>,
    hint_subjects: Vec<DistinguishedName>,
}

impl std::fmt::Debug for MailClientAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MailClientAuth")
            .field("mode", &self.mode)
            .field("depth", &self.depth)
            .field("has_verifier", &self.verifier.is_some())
            .field("hint_subjects", &self.hint_subjects.len())
            .finish()
    }
}

impl MailClientAuth {
    /// Resolve the policy from the typed mail TLS configuration.
    pub(crate) fn from_config(
        server_id: &str,
        tls: &MailTlsConfig,
    ) -> Result<Option<Self>, DataPlaneError> {
        if tls.verify_client == MailClientVerifyMode::Off {
            return Ok(None);
        }
        let mut roots = RootCertStore::empty();
        let mut hint_subjects: Vec<DistinguishedName> = Vec::new();
        let mut hint_der: Vec<Vec<u8>> = Vec::new();
        if let Some(path) = &tls.client_certificate_file {
            let certificates = load_bundle(server_id, path)?;
            for certificate in &certificates {
                roots.add(certificate.clone()).map_err(|error| {
                    DataPlaneError::MailTlsMaterial {
                        mail_server_id: server_id.to_owned(),
                        detail: format!("invalid client CA in `{path}`: {error}"),
                    }
                })?;
                if let Some((der, subject)) = subject_hint(certificate) {
                    if !hint_der.contains(&der) {
                        hint_der.push(der);
                        hint_subjects.push(subject);
                    }
                }
            }
        }
        if let Some(path) = &tls.trusted_certificate_file {
            for certificate in load_bundle(server_id, path)? {
                roots
                    .add(certificate)
                    .map_err(|error| DataPlaneError::MailTlsMaterial {
                        mail_server_id: server_id.to_owned(),
                        detail: format!("invalid trusted certificate in `{path}`: {error}"),
                    })?;
            }
        }
        let verifier = if roots.is_empty() {
            None
        } else {
            let builder = WebPkiClientVerifier::builder(Arc::new(roots))
                .allow_unauthenticated()
                .clear_root_hint_subjects();
            let builder = if hint_subjects.is_empty() {
                builder
            } else {
                builder.add_root_hint_subjects(hint_subjects.clone())
            };
            Some(
                builder
                    .build()
                    .map_err(|error| DataPlaneError::MailTlsMaterial {
                        mail_server_id: server_id.to_owned(),
                        detail: format!(
                            "cannot build the mail client-certificate verifier: {error}"
                        ),
                    })?,
            )
        };
        Ok(Some(Self {
            mode: tls.verify_client,
            depth: tls.verify_depth,
            verifier,
            hint_subjects,
        }))
    }

    /// The rustls handshake verifier for this policy.
    ///
    /// It requests a certificate but accepts it unconditionally: the nginx
    /// check runs after the handshake so the failure is an application-level
    /// reply instead of a TLS alert.
    pub(crate) fn handshake_verifier(self: &Arc<Self>) -> Arc<dyn ClientCertVerifier> {
        let algorithms = rustls::crypto::CryptoProvider::get_default()
            .map(|provider| provider.signature_verification_algorithms)
            .unwrap_or_else(|| {
                rustls::crypto::aws_lc_rs::default_provider().signature_verification_algorithms
            });
        Arc::new(MailDeferredClientVerifier {
            profile: Arc::clone(self),
            algorithms,
        })
    }

    /// Run nginx's post-handshake verification over the peer chain.
    ///
    /// Returns `Ok(None)` when verification is disabled, `Ok(Some(facts))` when
    /// a certificate is accepted (including the tolerated `optional_no_ca`
    /// failures), and a rejection when the session must be closed with the
    /// protocol's certificate error line.
    pub(crate) fn evaluate_peer(
        &self,
        peer_certificates: Option<&[CertificateDer<'static>]>,
    ) -> Result<Option<MailClientCertificate>, MailClientCertificateRejection> {
        let Some(chain) = peer_certificates.filter(|chain| !chain.is_empty()) else {
            return match self.mode {
                MailClientVerifyMode::On => Err(MailClientCertificateRejection::NoCertificate),
                MailClientVerifyMode::Optional | MailClientVerifyMode::OptionalNoCa => {
                    Ok(Some(MailClientCertificate::absent()))
                }
                MailClientVerifyMode::Off => Ok(None),
            };
        };
        let Some((leaf, intermediates)) = chain.split_first() else {
            return Err(MailClientCertificateRejection::CertificateError);
        };
        if u64::try_from(intermediates.len()).unwrap_or(u64::MAX) > u64::from(self.depth) {
            // OpenSSL `X509_V_ERR_CERT_CHAIN_TOO_LONG` is not an nginx
            // "optional" failure, so this rejects in every mode.
            return Err(MailClientCertificateRejection::CertificateError);
        }
        let parsed = X509Certificate::from_der(leaf.as_ref())
            .map(|(_, certificate)| certificate)
            .map_err(|_| MailClientCertificateRejection::CertificateError)?;
        let now = UnixTime::now();
        let verified = match &self.verifier {
            Some(verifier) => verifier.verify_client_cert(leaf, intermediates, now),
            None => {
                // `optional_no_ca` without a bundle: nginx's empty store gives
                // every certificate the "unable to verify the first
                // certificate" error; the certificate itself must still be
                // inside its validity window for that error to be optional.
                if !certificate_in_validity_window(&parsed, now) {
                    return Err(MailClientCertificateRejection::CertificateError);
                }
                Err(rustls::Error::InvalidCertificate(
                    rustls::CertificateError::UnknownIssuer,
                ))
            }
        };
        let facts = match verified {
            Ok(_) => MailClientCertificate::from_certificate(&parsed, leaf.as_ref(), "SUCCESS"),
            Err(rustls::Error::InvalidCertificate(rustls::CertificateError::UnknownIssuer))
                if self.mode == MailClientVerifyMode::OptionalNoCa =>
            {
                let reason = optional_no_ca_reason(&parsed, intermediates);
                MailClientCertificate::from_certificate(
                    &parsed,
                    leaf.as_ref(),
                    &format!("FAILED:{reason}"),
                )
            }
            Err(_) => return Err(MailClientCertificateRejection::CertificateError),
        };
        Ok(Some(facts))
    }
}

impl MailClientCertificate {
    fn absent() -> Self {
        Self {
            verify: "NONE".to_owned(),
            subject: String::new(),
            issuer: String::new(),
            serial: String::new(),
            fingerprint: String::new(),
            pem: String::new(),
        }
    }

    fn from_certificate(certificate: &X509Certificate<'_>, der: &[u8], verify: &str) -> Self {
        Self {
            verify: verify.to_owned(),
            subject: rfc2253_name(certificate.subject()),
            issuer: rfc2253_name(certificate.issuer()),
            serial: serial_hex(certificate.raw_serial()),
            fingerprint: lowercase_hex(&Sha1::digest(der)),
            pem: certificate_pem(der),
        }
    }
}

/// Deferred nginx-style verifier: request a certificate, accept it at the TLS
/// layer, and check it after the handshake.
#[derive(Debug)]
struct MailDeferredClientVerifier {
    profile: Arc<MailClientAuth>,
    algorithms: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl ClientCertVerifier for MailDeferredClientVerifier {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        // nginx never sets `SSL_VERIFY_FAIL_IF_NO_PEER_CERT`; a missing
        // certificate is handled after the handshake (`on` rejects with the
        // protocol's "No required SSL certificate" line).
        false
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &self.profile.hint_subjects
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, certificate, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, certificate, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// Read one PEM bundle, rejecting an empty or malformed file.
fn load_bundle(
    server_id: &str,
    path: &str,
) -> Result<Vec<CertificateDer<'static>>, DataPlaneError> {
    let pem = std::fs::read(path).map_err(|source| DataPlaneError::MailTlsMaterial {
        mail_server_id: server_id.to_owned(),
        detail: format!("cannot read certificate bundle `{path}`: {source}"),
    })?;
    let mut certificates = Vec::new();
    for item in CertificateDer::pem_slice_iter(&pem) {
        certificates.push(item.map_err(|source| DataPlaneError::MailTlsMaterial {
            mail_server_id: server_id.to_owned(),
            detail: format!("invalid certificate PEM in `{path}`: {source}"),
        })?);
    }
    if certificates.is_empty() {
        return Err(DataPlaneError::MailTlsMaterial {
            mail_server_id: server_id.to_owned(),
            detail: format!("certificate bundle `{path}` contains no certificates"),
        });
    }
    Ok(certificates)
}

/// The CertificateRequest CA hint for one bundle certificate.
fn subject_hint(certificate: &CertificateDer<'_>) -> Option<(Vec<u8>, DistinguishedName)> {
    let (_, parsed) = X509Certificate::from_der(certificate.as_ref()).ok()?;
    let raw = parsed.subject().as_raw().to_vec();
    let content = der_sequence_content(&raw)?.to_vec();
    Some((raw, DistinguishedName::in_sequence(&content)))
}

/// The content bytes of a DER `SEQUENCE` (`0x30`).
fn der_sequence_content(raw: &[u8]) -> Option<&[u8]> {
    let length_byte = *raw.get(1)?;
    let (header, length) = if length_byte & 0x80 == 0 {
        (2usize, usize::from(length_byte))
    } else {
        let count = usize::from(length_byte & 0x7f);
        if count == 0 || count > 4 {
            return None;
        }
        let mut length = 0usize;
        for byte in raw.get(2..2 + count)? {
            length = (length << 8) | usize::from(*byte);
        }
        (2 + count, length)
    };
    raw.get(header..header + length)
}

fn certificate_in_validity_window(certificate: &X509Certificate<'_>, now: UnixTime) -> bool {
    let not_before = certificate.validity().not_before.timestamp();
    let not_after = certificate.validity().not_after.timestamp();
    let now = i64::try_from(now.as_secs()).unwrap_or(i64::MAX);
    not_before <= now && now <= not_after
}

/// nginx's `X509_verify_cert_error_string` for the "optional" failures
/// `optional_no_ca` tolerates, selected from the shape of the presented chain.
fn optional_no_ca_reason(
    leaf: &X509Certificate<'_>,
    intermediates: &[CertificateDer<'static>],
) -> &'static str {
    if leaf.subject().as_raw() == leaf.issuer().as_raw() {
        return "self-signed certificate";
    }
    let Some(last) = intermediates.last() else {
        return "unable to verify the first certificate";
    };
    match X509Certificate::from_der(last.as_ref()) {
        Ok((_, last)) if last.subject().as_raw() == last.issuer().as_raw() => {
            "self-signed certificate in certificate chain"
        }
        _ => "unable to get local issuer certificate",
    }
}

/// RFC 2253 (OpenSSL `XN_FLAG_RFC2253`) rendering of a distinguished name:
/// attributes are most-specific-first, separated by `,` with no spaces, and the
/// RFC 2253 special characters are backslash-escaped.
fn rfc2253_name(name: &x509_parser::x509::X509Name<'_>) -> String {
    let mut rdns = Vec::new();
    let mut rdn_seq = name.iter().collect::<Vec<_>>();
    rdn_seq.reverse();
    for rdn in rdn_seq {
        let mut attributes = Vec::new();
        let mut entries = rdn.iter().collect::<Vec<_>>();
        entries.reverse();
        for attribute in entries {
            let key = oid_short_name(attribute.attr_type());
            let value = attribute
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|_| uppercase_hex(attribute.as_slice()));
            attributes.push(format!("{key}={}", escape_rfc2253(&value)));
        }
        rdns.push(attributes.join("+"));
    }
    rdns.join(",")
}

/// OpenSSL's short attribute names for the OIDs that appear in practice; any
/// other OID is rendered in dotted form, like `XN_FLAG_RFC2253`.
fn oid_short_name(oid: &x509_parser::asn1_rs::Oid<'_>) -> String {
    match oid.to_id_string().as_str() {
        "2.5.4.3" => "CN".to_owned(),
        "2.5.4.6" => "C".to_owned(),
        "2.5.4.7" => "L".to_owned(),
        "2.5.4.8" => "ST".to_owned(),
        "2.5.4.9" => "STREET".to_owned(),
        "2.5.4.10" => "O".to_owned(),
        "2.5.4.11" => "OU".to_owned(),
        "2.5.4.4" => "SN".to_owned(),
        "2.5.4.42" => "GN".to_owned(),
        "1.2.840.113549.1.9.1" => "emailAddress".to_owned(),
        "0.9.2342.19200300.100.1.1" => "UID".to_owned(),
        "0.9.2342.19200300.100.1.25" => "DC".to_owned(),
        other => other.to_owned(),
    }
}

fn escape_rfc2253(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    let last_index = value.chars().count().saturating_sub(1);
    for (index, ch) in value.chars().enumerate() {
        let last = index == last_index;
        match ch {
            ',' | '+' | '"' | '\\' | '<' | '>' | ';' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            '#' if index == 0 => {
                escaped.push('\\');
                escaped.push('#');
            }
            ' ' if index == 0 || last => {
                escaped.push('\\');
                escaped.push(' ');
            }
            ch if (ch as u32) < 0x20 || ch as u32 == 0x7f => {
                escaped.push_str(&format!("\\{:02X}", ch as u32));
            }
            ch => escaped.push(ch),
        }
    }
    escaped
}

fn serial_hex(serial: &[u8]) -> String {
    let serial = match serial {
        [0, rest @ ..] if !rest.is_empty() => rest,
        other => other,
    };
    uppercase_hex(serial)
}

fn uppercase_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

fn lowercase_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// OpenSSL-compatible PEM encoding of one certificate (64-column base64, LF).
fn certificate_pem(der: &[u8]) -> String {
    let base64 = base64::engine::general_purpose::STANDARD.encode(der);
    let mut pem = String::with_capacity(base64.len() + base64.len() / 64 + 64);
    pem.push_str("-----BEGIN CERTIFICATE-----\n");
    for chunk in base64.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(chunk).unwrap_or_default());
        pem.push('\n');
    }
    pem.push_str("-----END CERTIFICATE-----\n");
    pem
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_strips_the_der_positive_sign_byte() {
        assert_eq!(serial_hex(&[0x00, 0xC0, 0xFF, 0xEE]), "C0FFEE");
        assert_eq!(serial_hex(&[0x26, 0x12]), "2612");
        assert_eq!(serial_hex(&[0x00]), "00");
    }

    #[test]
    fn rfc2253_escaping_matches_openssl() {
        assert_eq!(escape_rfc2253("ACME, Inc+Co"), "ACME\\, Inc\\+Co");
        assert_eq!(escape_rfc2253("we,ird+name"), "we\\,ird\\+name");
        assert_eq!(escape_rfc2253("#hash"), "\\#hash");
        assert_eq!(escape_rfc2253("trail "), "trail\\ ");
    }

    #[test]
    fn certificate_pem_wraps_base64_at_64_columns() {
        let der = vec![0x30, 0x03, 0x02, 0x01, 0x01];
        let pem = certificate_pem(&der);
        assert!(pem.starts_with("-----BEGIN CERTIFICATE-----\n"));
        assert!(pem.ends_with("-----END CERTIFICATE-----\n"));
        for line in pem.lines().filter(|line| !line.starts_with("-----")) {
            assert!(line.len() <= 64, "{line:?}");
        }
    }
}
