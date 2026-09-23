//! TLS material for the tunnel transport: server and client rustls
//! configurations, self-signed development certificates, and certificate
//! fingerprinting (PRD §24, §25).

use std::sync::Arc;

use sha2::{Digest, Sha256};
use std::io::Read;

use sdkwork_webserver_tunnel_core::{Result, TunnelError, ValidationField};

use crate::{RemoteEndpoint, TUNNEL_ALPN};

/// SHA-256 hex fingerprint of a DER-encoded certificate.
pub fn certificate_sha256_hex(der: &[u8]) -> String {
    let digest = Sha256::digest(der);
    hex_lower(&digest)
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from_digit(u32::from(byte >> 4), 16).expect("hex digit"));
        text.push(char::from_digit(u32::from(byte & 0x0F), 16).expect("hex digit"));
    }
    text
}

/// A self-signed development certificate and its private key, both PEM.
#[derive(Debug, Clone)]
pub struct SelfSignedMaterial {
    /// Certificate chain in PEM form (leaf only).
    pub cert_pem: String,
    /// Private key in PEM form.
    pub key_pem: String,
    /// SHA-256 hex fingerprint of the leaf DER.
    pub sha256: String,
    /// The leaf DER bytes.
    pub der: Vec<u8>,
}

/// Mints a self-signed certificate for development gateways. Production
/// gateways supply real material through `tlsCertPemEnv`/`tlsKeyPemEnv`
/// instead.
pub fn generate_self_signed(subject_alt_names: &[String]) -> Result<SelfSignedMaterial> {
    let params = rcgen::CertificateParams::new(subject_alt_names.to_vec()).map_err(|error| {
        TunnelError::Validation {
            field: ValidationField::Config,
            reason: format!("invalid self-signed certificate names: {error}"),
        }
    })?;
    let key = rcgen::KeyPair::generate().map_err(|error| {
        TunnelError::ConnectionFailed(format!("self-signed key generation failed: {error}"))
    })?;
    let certificate = params.self_signed(&key).map_err(|error| {
        TunnelError::ConnectionFailed(format!(
            "self-signed certificate generation failed: {error}"
        ))
    })?;
    let der = certificate.der().as_ref().to_vec();
    Ok(SelfSignedMaterial {
        cert_pem: certificate.pem(),
        key_pem: key.serialize_pem(),
        sha256: certificate_sha256_hex(&der),
        der,
    })
}

/// Loads a PEM certificate chain and private key from bytes into a rustls
/// server configuration for the tunnel ALPN.
pub fn server_config_from_pem(cert_pem: &[u8], key_pem: &[u8]) -> Result<rustls::ServerConfig> {
    let certs = parse_pem_certificates(cert_pem)?;
    let key = parse_pem_private_key(key_pem)?;
    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|error| {
            TunnelError::ConnectionFailed(format!("tunnel server certificate rejected: {error}"))
        })?;
    config.alpn_protocols = vec![TUNNEL_ALPN.to_vec()];
    Ok(config)
}

/// Client-side verification policy, resolved from
/// [`sdkwork_webserver_tunnel_core::TunnelAgentTlsConfig`].
#[derive(Debug, Clone, Default)]
pub struct ClientTlsOptions {
    /// PEM file with a trusted CA or leaf certificate.
    pub ca_pem_path: Option<String>,
    /// SHA-256 hex fingerprint pin of the gateway leaf certificate.
    pub pinned_server_sha256: Option<String>,
    /// Danger: skip verification entirely.
    pub insecure_skip_verify: bool,
}

/// Builds a client rustls configuration per the agent verification policy.
pub fn client_config(options: &ClientTlsOptions) -> Result<rustls::ClientConfig> {
    let builder = rustls::ClientConfig::builder();
    let mut config = if options.insecure_skip_verify {
        tracing::warn!(
            "tunnel agent TLS verification is DISABLED (insecureSkipVerify); use only for development"
        );
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerifier))
            .with_no_client_auth()
    } else if let Some(pin) = options.pinned_server_sha256.as_deref() {
        let expected = normalize_sha256(pin).ok_or_else(|| TunnelError::Validation {
            field: ValidationField::Config,
            reason: "pinnedServerSha256 must be 64 hex characters".to_owned(),
        })?;
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PinnedServerVerifier {
                expected_sha256: expected,
            }))
            .with_no_client_auth()
    } else if let Some(ca_path) = options.ca_pem_path.as_deref() {
        let roots = load_root_store(ca_path)?;
        builder.with_root_certificates(roots).with_no_client_auth()
    } else {
        return Err(TunnelError::Validation {
            field: ValidationField::Config,
            reason: "agent TLS requires caPemPath, pinnedServerSha256, or insecureSkipVerify"
                .to_owned(),
        });
    };
    config.alpn_protocols = vec![TUNNEL_ALPN.to_vec()];
    Ok(config)
}

/// Derives the SNI name for a gateway endpoint. IP literals pass through
/// (rustls verifies IP SANs), DNS hosts lowercase, and only an empty host
/// falls back to the protocol name.
pub fn server_name_for(endpoint: &RemoteEndpoint) -> String {
    let host = endpoint.host.trim();
    if host.is_empty() {
        "sdkwork-tunnel-gateway".to_owned()
    } else {
        host.to_ascii_lowercase()
    }
}

fn load_root_store(ca_path: &str) -> Result<rustls::RootCertStore> {
    let pem = read_bounded_pem(ca_path)?;
    let certificates = parse_pem_certificates(&pem)?;
    let mut roots = rustls::RootCertStore::empty();
    let (added, ignored) = roots.add_parsable_certificates(certificates);
    if added == 0 {
        return Err(TunnelError::ConnectionFailed(format!(
            "no usable certificates in tunnel CA file {ca_path}"
        )));
    }
    tracing::debug!(path = %ca_path, added, ignored, "tunnel CA roots loaded");
    Ok(roots)
}

/// Reads a bounded PEM file (certificates and keys stay well under a
/// megabyte). TLS material loads once at composition time, so a bounded
/// synchronous read is appropriate here.
fn read_bounded_pem(path: &str) -> Result<Vec<u8>> {
    const MAX_PEM_BYTES: u64 = 512 * 1024;
    let file = std::fs::File::open(path)
        .map_err(|error| TunnelError::ConnectionFailed(format!("cannot open {path}: {error}")))?;
    let mut buffer = Vec::new();
    file.take(MAX_PEM_BYTES + 1)
        .read_to_end(&mut buffer)
        .map_err(|error| TunnelError::ConnectionFailed(format!("cannot read {path}: {error}")))?;
    if buffer.len() as u64 > MAX_PEM_BYTES {
        return Err(TunnelError::ConnectionFailed(format!(
            "{path} exceeds the {MAX_PEM_BYTES} byte PEM ceiling"
        )));
    }
    Ok(buffer)
}

/// Extracts every `-----BEGIN <label>-----` block from a PEM document.
fn pem_blocks(pem: &[u8], label: &str) -> Vec<String> {
    let text = String::from_utf8_lossy(pem);
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let mut blocks = Vec::new();
    let mut rest = text.as_ref();
    while let Some(start) = rest.find(&begin) {
        let after_begin = &rest[start + begin.len()..];
        let Some(end_offset) = after_begin.find(&end) else {
            break;
        };
        blocks.push(format!("{begin}{}{end}", &after_begin[..end_offset]));
        rest = &after_begin[end_offset + end.len()..];
    }
    blocks
}

fn parse_pem_certificates(pem: &[u8]) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>> {
    use rustls::pki_types::pem::PemObject;
    let blocks = pem_blocks(pem, "CERTIFICATE");
    if blocks.is_empty() {
        return Err(TunnelError::ConnectionFailed(
            "certificate PEM contains no CERTIFICATE blocks".to_owned(),
        ));
    }
    blocks
        .iter()
        .map(|block| {
            rustls::pki_types::CertificateDer::from_pem_slice(block.as_bytes()).map_err(|error| {
                TunnelError::ConnectionFailed(format!("bad certificate PEM block: {error}"))
            })
        })
        .collect()
}

fn parse_pem_private_key(pem: &[u8]) -> Result<rustls::pki_types::PrivateKeyDer<'static>> {
    use rustls::pki_types::pem::PemObject;
    for label in ["PRIVATE KEY", "RSA PRIVATE KEY", "EC PRIVATE KEY"] {
        if let Some(block) = pem_blocks(pem, label).first() {
            return rustls::pki_types::PrivateKeyDer::from_pem_slice(block.as_bytes()).map_err(
                |error| {
                    TunnelError::ConnectionFailed(format!("bad private key PEM block: {error}"))
                },
            );
        }
    }
    Err(TunnelError::ConnectionFailed(
        "key PEM contains no private key block".to_owned(),
    ))
}

/// Normalizes a SHA-256 hex pin (lowercase, `:` removed); `None` when the
/// input is not a 64-hex string.
fn normalize_sha256(raw: &str) -> Option<String> {
    let lowered = raw.trim().to_ascii_lowercase().replace(':', "");
    if lowered.len() != 64 || !lowered.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(lowered)
}

/// Development-only verifier that accepts any server certificate.
#[derive(Debug)]
struct SkipServerVerifier;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::CryptoProvider::get_default()
            .map(|provider| {
                provider
                    .signature_verification_algorithms
                    .supported_schemes()
            })
            .unwrap_or_default()
    }
}

/// Verifier that authenticates the server by SHA-256 certificate pin
/// instead of a CA chain.
#[derive(Debug)]
struct PinnedServerVerifier {
    expected_sha256: String,
}

impl PinnedServerVerifier {
    fn verify_pin(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let actual = certificate_sha256_hex(end_entity.as_ref());
        if sdkwork_utils_rust::crypto::secure_compare(&actual, &self.expected_sha256) {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            tracing::warn!(
                actual = %actual,
                expected = %self.expected_sha256,
                "tunnel gateway certificate pin mismatch"
            );
            Err(rustls::Error::General(
                "tunnel gateway certificate does not match pinnedServerSha256".to_owned(),
            ))
        }
    }

    fn signature_accepted(
        &self,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::CryptoProvider::get_default()
            .map(|provider| {
                provider
                    .signature_verification_algorithms
                    .supported_schemes()
            })
            .unwrap_or_default()
    }
}

impl rustls::client::danger::ServerCertVerifier for PinnedServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        self.verify_pin(end_entity)
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.signature_accepted()
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.signature_accepted()
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install_crypto_provider() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }

    #[test]
    fn self_signed_material_has_fingerprint() {
        install_crypto_provider();
        let material = generate_self_signed(&["localhost".to_owned(), "127.0.0.1".to_owned()])
            .expect("self-signed material");
        assert_eq!(material.sha256.len(), 64);
        assert!(material.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(material.key_pem.contains("PRIVATE KEY"));
        let parsed = parse_pem_certificates(material.cert_pem.as_bytes()).expect("parse");
        assert_eq!(certificate_sha256_hex(parsed[0].as_ref()), material.sha256);
    }

    #[test]
    fn server_config_accepts_self_signed_material() {
        install_crypto_provider();
        let material = generate_self_signed(&["gateway.test".to_owned()]).expect("material");
        server_config_from_pem(material.cert_pem.as_bytes(), material.key_pem.as_bytes())
            .expect("server config");
    }

    #[test]
    fn server_config_rejects_mismatched_key() {
        install_crypto_provider();
        let first = generate_self_signed(&["gateway.test".to_owned()]).expect("material");
        let second = generate_self_signed(&["gateway.test".to_owned()]).expect("material");
        let error = server_config_from_pem(first.cert_pem.as_bytes(), second.key_pem.as_bytes())
            .expect_err("mismatched key must fail");
        assert!(error.to_string().contains("rejected"));
    }

    #[test]
    fn server_name_prefers_dns_hosts_and_ips() {
        assert_eq!(
            server_name_for(&RemoteEndpoint::new("tunnel.example.com", 8443)),
            "tunnel.example.com"
        );
        assert_eq!(
            server_name_for(&RemoteEndpoint::new("127.0.0.1", 8443)),
            "127.0.0.1"
        );
        assert_eq!(
            server_name_for(&RemoteEndpoint::new("", 8443)),
            "sdkwork-tunnel-gateway"
        );
    }

    #[test]
    fn sha256_pin_normalization() {
        // Pins must be exactly 64 hex characters; colons are tolerated and
        // case is normalized.
        let spaced = {
            let hex = "ab".repeat(32);
            let mut text = String::new();
            for (index, chunk) in hex.as_bytes().chunks(2).enumerate() {
                if index > 0 {
                    text.push(':');
                }
                text.push_str(std::str::from_utf8(chunk).expect("utf8"));
            }
            text
        };
        assert_eq!(
            normalize_sha256(&spaced).as_deref(),
            Some("ab".repeat(32).as_str())
        );
        assert_eq!(normalize_sha256("AB:CD"), None);
        assert_eq!(normalize_sha256("nothex"), None);
        assert_eq!(
            normalize_sha256(&"a".repeat(64)).as_deref(),
            Some("a".repeat(64).as_str())
        );
    }

    #[test]
    fn client_config_requires_an_explicit_policy() {
        install_crypto_provider();
        let outcome = client_config(&ClientTlsOptions::default());
        let error = match outcome {
            Ok(_) => panic!("implicit verification must not be configurable"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("caPemPath"));
    }

    #[test]
    fn client_config_with_pin_builds() {
        install_crypto_provider();
        client_config(&ClientTlsOptions {
            pinned_server_sha256: Some("a".repeat(64)),
            ..ClientTlsOptions::default()
        })
        .expect("pinned verifier config");
    }

    #[test]
    fn client_config_with_ca_file_builds() {
        install_crypto_provider();
        let material = generate_self_signed(&["gateway.test".to_owned()]).expect("material");
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("gateway.pem");
        std::fs::write(&path, &material.cert_pem).expect("write pem");
        client_config(&ClientTlsOptions {
            ca_pem_path: Some(path.to_string_lossy().into_owned()),
            ..ClientTlsOptions::default()
        })
        .expect("ca config");
    }

    #[test]
    fn pem_blocks_extract_every_section() {
        let document = "noise\n-----BEGIN CERTIFICATE-----\nAAA\n-----END CERTIFICATE-----\nmid\n-----BEGIN CERTIFICATE-----\nBBB\n-----END CERTIFICATE-----\ntail";
        let blocks = pem_blocks(document.as_bytes(), "CERTIFICATE");
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].contains("AAA"));
        assert!(blocks[1].contains("BBB"));
        assert!(pem_blocks(document.as_bytes(), "PRIVATE KEY").is_empty());
    }
}
