use std::{collections::HashMap, io, path::Path, sync::Arc};

use rustls::{
    pki_types::{
        pem::{PemObject, SectionKind},
        CertificateDer, PrivateKeyDer,
    },
    server::{ClientHello, ResolvesServerCert, WebPkiClientVerifier},
    sign::CertifiedKey,
    RootCertStore, ServerConfig,
};
use sdkwork_webserver_core::{
    normalize_server_name, server_name_covers, ClientAuthConfig, ClientAuthMode, TlsVersion,
};
use sha2::{Digest, Sha256};
use x509_parser::prelude::{FromDer, GeneralName, X509Certificate};

use super::{runtime::read_bounded_tls_material, DataPlaneError};

pub(crate) struct LoadedCertifiedKey {
    pub certified_key: CertifiedKey,
    pub leaf_fingerprint_sha256: String,
    pub not_before_unix_seconds: i64,
    pub not_after_unix_seconds: i64,
}

#[derive(Debug)]
struct SniCertificateResolver {
    exact: HashMap<String, Arc<CertifiedKey>>,
    wildcards: Vec<(String, Arc<CertifiedKey>)>,
}

impl ResolvesServerCert for SniCertificateResolver {
    fn resolve(&self, client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        // Fail closed: a missing or unknown SNI gets no certificate, so the
        // handshake is rejected instead of serving a default certificate to
        // a host the operator never authorized.
        let Some(raw_name) = client_hello.server_name() else {
            return None;
        };
        let Some(server_name) = normalize_server_name(raw_name) else {
            return None;
        };
        if let Some(certified_key) = self.exact.get(&server_name) {
            return Some(certified_key.clone());
        }
        self.wildcards
            .iter()
            .find(|(suffix, _)| wildcard_matches(suffix, &server_name))
            .map(|(_, certified_key)| certified_key.clone())
    }
}

pub(crate) fn build_sni_server_config(
    certificates: Vec<(Vec<String>, CertifiedKey)>,
    minimum_version: TlsVersion,
    maximum_version: TlsVersion,
    alpn: &[String],
    client_auth: Option<&ClientAuthConfig>,
) -> Result<Arc<ServerConfig>, String> {
    let mut exact = HashMap::new();
    let mut wildcards = Vec::new();
    for (server_names, certified_key) in certificates {
        let certified_key = Arc::new(certified_key);
        for server_name in server_names {
            let normalized = normalize_server_name(&server_name)
                .ok_or_else(|| format!("invalid TLS server name {server_name}"))?;
            if let Some(suffix) = normalized.strip_prefix("*.") {
                if wildcards.iter().any(|(existing, _)| existing == suffix) {
                    return Err(normalized);
                }
                wildcards.push((suffix.to_owned(), certified_key.clone()));
            } else if exact
                .insert(normalized.clone(), certified_key.clone())
                .is_some()
            {
                return Err(normalized);
            }
        }
    }
    wildcards.sort_unstable_by(|left, right| {
        right
            .0
            .len()
            .cmp(&left.0.len())
            .then_with(|| left.0.cmp(&right.0))
    });
    let protocol_versions = tls_protocol_versions(minimum_version, maximum_version);
    let builder = ServerConfig::builder_with_protocol_versions(&protocol_versions);
    let resolver = Arc::new(SniCertificateResolver { exact, wildcards });
    let mut server_config = match client_auth {
        Some(auth) if auth.mode != ClientAuthMode::Off => {
            let verifier = build_client_cert_verifier(auth)?;
            builder
                .with_client_cert_verifier(verifier)
                .with_cert_resolver(resolver)
        }
        _ => builder.with_no_client_auth().with_cert_resolver(resolver),
    };
    server_config.alpn_protocols = alpn
        .iter()
        .map(|protocol| protocol.as_bytes().to_vec())
        .collect();
    Ok(Arc::new(server_config))
}

fn build_client_cert_verifier(
    auth: &ClientAuthConfig,
) -> Result<Arc<dyn rustls::server::danger::ClientCertVerifier>, String> {
    if auth.ca_certificate_files.is_empty() {
        return Err("client auth requires at least one CA certificate file".to_owned());
    }
    let mut roots = RootCertStore::empty();
    for path in &auth.ca_certificate_files {
        let bytes = std::fs::read(path)
            .map_err(|error| format!("failed to read client CA `{path}`: {error}"))?;
        let mut loaded = 0usize;
        for item in CertificateDer::pem_slice_iter(&bytes) {
            let cert =
                item.map_err(|error| format!("invalid client CA PEM in `{path}`: {error}"))?;
            roots
                .add(cert)
                .map_err(|error| format!("invalid client CA in `{path}`: {error}"))?;
            loaded = loaded.saturating_add(1);
        }
        if loaded == 0 {
            return Err(format!("client CA `{path}` contained no certificates"));
        }
    }
    let builder = WebPkiClientVerifier::builder(Arc::new(roots));
    let builder = match auth.mode {
        ClientAuthMode::Required => builder,
        ClientAuthMode::Optional => builder.allow_unauthenticated(),
        ClientAuthMode::Off => {
            return Err("client auth mode off does not build a verifier".to_owned());
        }
    };
    builder
        .build()
        .map_err(|error| format!("failed to build client certificate verifier: {error}"))
}

pub(crate) fn install_crypto_provider() -> Result<(), DataPlaneError> {
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        return Ok(());
    }
    if rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .is_err()
        && rustls::crypto::CryptoProvider::get_default().is_none()
    {
        return Err(DataPlaneError::TlsCryptoProvider);
    }
    Ok(())
}

pub(crate) fn load_certified_key(
    certificate_file: &Path,
    private_key_file: &Path,
    declared_server_names: &[String],
    provider: &rustls::crypto::CryptoProvider,
) -> Result<LoadedCertifiedKey, DataPlaneError> {
    let certificate_pem = read_bounded_tls_material(certificate_file)?;
    let private_key_pem = read_bounded_tls_material(private_key_file)?;
    let certificate_chain = parse_certificate_chain(&certificate_pem)
        .map_err(|source| tls_files_error(certificate_file, private_key_file, source))?;
    let evidence = validate_leaf_certificate(certificate_chain[0].as_ref(), declared_server_names)
        .map_err(|source| tls_files_error(certificate_file, private_key_file, source))?;
    let private_key = parse_single_private_key(&private_key_pem)
        .map_err(|source| tls_files_error(certificate_file, private_key_file, source))?;
    let certified_key =
        CertifiedKey::from_der(certificate_chain, private_key, provider).map_err(|source| {
            tls_files_error(
                certificate_file,
                private_key_file,
                io::Error::new(io::ErrorKind::InvalidData, source),
            )
        })?;
    Ok(LoadedCertifiedKey {
        certified_key,
        leaf_fingerprint_sha256: evidence.fingerprint_sha256,
        not_before_unix_seconds: evidence.not_before_unix_seconds,
        not_after_unix_seconds: evidence.not_after_unix_seconds,
    })
}

pub(crate) fn tls_protocol_versions(
    minimum_version: TlsVersion,
    maximum_version: TlsVersion,
) -> Vec<&'static rustls::SupportedProtocolVersion> {
    match (minimum_version, maximum_version) {
        (TlsVersion::Tls12, TlsVersion::Tls12) => vec![&rustls::version::TLS12],
        (TlsVersion::Tls13, TlsVersion::Tls13) => vec![&rustls::version::TLS13],
        (TlsVersion::Tls12, TlsVersion::Tls13) => {
            vec![&rustls::version::TLS13, &rustls::version::TLS12]
        }
        (TlsVersion::Tls13, TlsVersion::Tls12) => {
            unreachable!("semantic validation rejects an inverted TLS version range")
        }
    }
}

fn parse_certificate_chain(bytes: &[u8]) -> Result<Vec<CertificateDer<'static>>, io::Error> {
    let mut certificates = Vec::new();
    for section in <(SectionKind, Vec<u8>)>::pem_slice_iter(bytes) {
        let section = section.map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        match section {
            (SectionKind::Certificate, certificate) => {
                certificates.push(CertificateDer::from(certificate));
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "certificate file contains a non-certificate PEM item",
                ))
            }
        }
    }
    if certificates.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "certificate chain is empty",
        ));
    }
    Ok(certificates)
}

#[derive(Debug)]
struct LeafCertificateEvidence {
    fingerprint_sha256: String,
    not_before_unix_seconds: i64,
    not_after_unix_seconds: i64,
}

fn validate_leaf_certificate(
    certificate_der: &[u8],
    declared_server_names: &[String],
) -> Result<LeafCertificateEvidence, io::Error> {
    let (remaining, certificate) =
        X509Certificate::from_der(certificate_der).map_err(|source| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("cannot parse leaf X.509 certificate: {source}"),
            )
        })?;
    if !remaining.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "leaf X.509 certificate has trailing DER data",
        ));
    }
    // nginx still starts with an expired leaf; warn and continue so
    // multi-site SNI surfaces remain available during cert rotation lag.
    if !certificate.validity().is_valid() {
        tracing::warn!(
            not_before = %certificate.validity().not_before,
            not_after = %certificate.validity().not_after,
            "leaf X.509 certificate is expired or not yet valid; loading anyway for nginx compatibility"
        );
    }
    let subject_alternative_name = certificate
        .subject_alternative_name()
        .map_err(|source| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid subjectAltName extension: {source}"),
            )
        })?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "leaf X.509 certificate has no subjectAltName extension",
            )
        })?;
    let certificate_dns_names = subject_alternative_name
        .value
        .general_names
        .iter()
        .filter_map(|general_name| match general_name {
            GeneralName::DNSName(name) => Some(*name),
            _ => None,
        })
        .collect::<Vec<_>>();
    for declared_server_name in declared_server_names {
        if !certificate_dns_names
            .iter()
            .any(|certificate_name| server_name_covers(certificate_name, declared_server_name))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "leaf X.509 subjectAltName does not cover declared server name {declared_server_name}"
                ),
            ));
        }
    }
    Ok(LeafCertificateEvidence {
        fingerprint_sha256: format!("{:x}", Sha256::digest(certificate_der)),
        not_before_unix_seconds: certificate.validity().not_before.timestamp(),
        not_after_unix_seconds: certificate.validity().not_after.timestamp(),
    })
}

fn parse_single_private_key(bytes: &[u8]) -> Result<PrivateKeyDer<'static>, io::Error> {
    let mut keys = <(SectionKind, Vec<u8>)>::pem_slice_iter(bytes)
        .map(|section| {
            section
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
                .and_then(|(kind, key)| match kind {
                    SectionKind::RsaPrivateKey
                    | SectionKind::PrivateKey
                    | SectionKind::EcPrivateKey => Ok(PrivateKeyDer::from_pem(kind, key)),
                    _ => Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "private-key file contains a non-key PEM item",
                    )),
                })
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten();
    let key = keys.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "private-key file contains no key",
        )
    })?;
    if keys.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "private-key file contains more than one key",
        ));
    }
    Ok(key)
}

fn tls_files_error(
    certificate_file: &Path,
    private_key_file: &Path,
    source: io::Error,
) -> DataPlaneError {
    DataPlaneError::TlsFiles {
        certificate_file: certificate_file.to_path_buf(),
        private_key_file: private_key_file.to_path_buf(),
        source,
    }
}

fn wildcard_matches(suffix: &str, server_name: &str) -> bool {
    server_name
        .strip_suffix(suffix)
        .is_some_and(|prefix| prefix.ends_with('.') && !prefix[..prefix.len() - 1].contains('.'))
}

#[cfg(test)]
mod tests {
    use rcgen::{date_time_ymd, CertificateParams, KeyPair};
    use sdkwork_webserver_core::TlsVersion;

    use super::{
        parse_certificate_chain, parse_single_private_key, tls_protocol_versions,
        validate_leaf_certificate, wildcard_matches,
    };

    #[test]
    fn tls_policy_version_range_selects_only_declared_protocols() {
        assert_eq!(
            tls_protocol_versions(TlsVersion::Tls12, TlsVersion::Tls12),
            vec![&rustls::version::TLS12]
        );
        assert_eq!(
            tls_protocol_versions(TlsVersion::Tls13, TlsVersion::Tls13),
            vec![&rustls::version::TLS13]
        );
        assert_eq!(
            tls_protocol_versions(TlsVersion::Tls12, TlsVersion::Tls13),
            vec![&rustls::version::TLS13, &rustls::version::TLS12]
        );
    }

    #[test]
    fn leaf_certificate_must_cover_every_declared_server_name() {
        let params = CertificateParams::new(vec!["alpha.example.test".to_owned()])
            .expect("certificate parameters");
        let key = KeyPair::generate().expect("generate key");
        let certificate = params.self_signed(&key).expect("generate certificate");
        validate_leaf_certificate(
            certificate.der().as_ref(),
            &["alpha.example.test".to_owned()],
        )
        .expect("matching SAN must validate");
        let error = validate_leaf_certificate(
            certificate.der().as_ref(),
            &["beta.example.test".to_owned()],
        )
        .expect_err("mismatched SAN must fail");
        assert!(error.to_string().contains("does not cover"));
    }

    #[test]
    fn expired_leaf_certificate_is_accepted_for_nginx_compat() {
        let mut params = CertificateParams::new(vec!["expired.example.test".to_owned()])
            .expect("certificate parameters");
        params.not_before = date_time_ymd(2000, 1, 1);
        params.not_after = date_time_ymd(2001, 1, 1);
        let key = KeyPair::generate().expect("generate key");
        let certificate = params.self_signed(&key).expect("generate certificate");
        let evidence = validate_leaf_certificate(
            certificate.der().as_ref(),
            &["expired.example.test".to_owned()],
        )
        .expect("expired leaf still loads for nginx compatibility");
        assert!(evidence.not_after_unix_seconds < evidence.not_before_unix_seconds + 400 * 86_400);
    }

    #[test]
    fn empty_or_malformed_pem_material_is_rejected() {
        assert!(parse_certificate_chain(b"not a PEM certificate").is_err());
        assert!(parse_single_private_key(b"not a PEM private key").is_err());
    }

    #[test]
    fn wildcard_matches_exactly_one_left_label() {
        assert!(wildcard_matches("example.test", "www.example.test"));
        assert!(!wildcard_matches("example.test", "example.test"));
        assert!(!wildcard_matches("example.test", "a.b.example.test"));
    }
}
