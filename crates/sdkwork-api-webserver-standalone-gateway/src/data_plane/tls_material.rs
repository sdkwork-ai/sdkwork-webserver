use std::{io, path::Path, sync::Arc};

use rustls::{
    pki_types::{
        pem::{PemObject, SectionKind},
        CertificateDer, PrivateKeyDer,
    },
    server::WebPkiClientVerifier,
    sign::CertifiedKey,
    RootCertStore, ServerConfig,
};
use sdkwork_webserver_core::{server_name_covers, ClientAuthConfig, ClientAuthMode, TlsVersion};
use sha2::{Digest, Sha256};
use x509_parser::prelude::{FromDer, GeneralName, X509Certificate};

use super::{
    runtime::read_bounded_tls_material,
    tls_resolver::{
        CertificateSourceIndex, LayeredCertificateResolver, ResolvableCertificate, ResolverLayers,
    },
    DataPlaneError,
};

pub(crate) struct LoadedCertifiedKey {
    pub certified_key: CertifiedKey,
    pub leaf_fingerprint_sha256: String,
    pub not_before_unix_seconds: i64,
    pub not_after_unix_seconds: i64,
}

impl LoadedCertifiedKey {
    /// Moves the key into the resolver's validity-aware wrapper.
    ///
    /// The leaf validity window travels with the key so the resolver can rank
    /// two sources for the same server name without re-parsing the certificate.
    pub(crate) fn into_resolvable(self) -> Arc<ResolvableCertificate> {
        Arc::new(ResolvableCertificate::new(
            Arc::new(self.certified_key),
            self.leaf_fingerprint_sha256,
            self.not_before_unix_seconds,
            self.not_after_unix_seconds,
        ))
    }
}

/// Builds a server configuration that serves exactly one certificate source.
///
/// Listeners that declare a single source use this; listeners that combine a
/// configured policy with assigned certificates go through
/// [`build_layered_server_config`].
pub(crate) fn build_sni_server_config(
    certificates: Vec<(Vec<String>, LoadedCertifiedKey)>,
    minimum_version: TlsVersion,
    maximum_version: TlsVersion,
    alpn: &[String],
    client_auth: Option<&ClientAuthConfig>,
) -> Result<Arc<ServerConfig>, String> {
    let layers = Arc::new(ResolverLayers::policy_only(build_source_index(
        certificates,
    )?));
    build_layered_server_config(layers, minimum_version, maximum_version, alpn, client_auth)
}

/// Builds a server configuration from a full layer set.
///
/// The resulting `ServerConfig` is immutable: a rotation builds a new one with
/// a new resolver and publishes it atomically, which is what lets a rotation
/// carry its own protocol parameters instead of being limited to a certificate
/// swap.
pub(crate) fn build_layered_server_config(
    layers: Arc<ResolverLayers>,
    minimum_version: TlsVersion,
    maximum_version: TlsVersion,
    alpn: &[String],
    client_auth: Option<&ClientAuthConfig>,
) -> Result<Arc<ServerConfig>, String> {
    let protocol_versions = tls_protocol_versions(minimum_version, maximum_version);
    let builder = ServerConfig::builder_with_protocol_versions(&protocol_versions);
    let resolver = Arc::new(LayeredCertificateResolver::new(layers));
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

/// Indexes loaded material by server name, rejecting ambiguous claims.
pub(crate) fn build_source_index(
    certificates: Vec<(Vec<String>, LoadedCertifiedKey)>,
) -> Result<CertificateSourceIndex, String> {
    let mut index = CertificateSourceIndex::default();
    for (server_names, loaded) in certificates {
        index.insert(&server_names, loaded.into_resolvable())?;
    }
    Ok(index)
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
    chain_file: Option<&Path>,
    ocsp_stapling_file: Option<&Path>,
    declared_server_names: &[String],
    provider: &rustls::crypto::CryptoProvider,
) -> Result<LoadedCertifiedKey, DataPlaneError> {
    let certificate_pem = read_bounded_tls_material(certificate_file)?;
    let private_key_pem = read_bounded_tls_material(private_key_file)?;
    let mut certificate_chain = parse_certificate_chain(&certificate_pem)
        .map_err(|source| tls_files_error(certificate_file, private_key_file, source))?;
    // nginx `ssl_trusted_certificate` / typed-TOML `chainFile`: the declared
    // PEM chain completes the served certificate chain. Certificates already
    // present in `certFile` (the usual fullchain form) are deduplicated by
    // DER, so a redundant declaration is harmless while a leaf-only `certFile`
    // plus a separate chain still presents the intermediates instead of
    // silently dropping the declaration.
    if let Some(chain_file) = chain_file {
        let chain_pem = read_bounded_tls_material(chain_file)?;
        let mut chain_certificates = 0usize;
        for item in CertificateDer::pem_slice_iter(&chain_pem) {
            chain_certificates = chain_certificates.saturating_add(1);
            let certificate = item.map_err(|error| {
                tls_files_error(
                    chain_file,
                    private_key_file,
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("invalid chainFile PEM: {error}"),
                    ),
                )
            })?;
            if !certificate_chain
                .iter()
                .any(|existing| existing.as_ref() == certificate.as_ref())
            {
                certificate_chain.push(certificate);
            }
        }
        if chain_certificates == 0 {
            return Err(tls_files_error(
                chain_file,
                private_key_file,
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "chainFile contains no PEM certificates",
                ),
            ));
        }
    }
    let evidence = validate_leaf_certificate(certificate_chain[0].as_ref(), declared_server_names)
        .map_err(|source| tls_files_error(certificate_file, private_key_file, source))?;
    let private_key = parse_single_private_key(&private_key_pem)
        .map_err(|source| tls_files_error(certificate_file, private_key_file, source))?;
    let mut certified_key = CertifiedKey::from_der(certificate_chain, private_key, provider)
        .map_err(|source| {
            tls_files_error(
                certificate_file,
                private_key_file,
                io::Error::new(io::ErrorKind::InvalidData, source),
            )
        })?;
    // nginx `ssl_stapling_file`: a static DER OCSP response served as the
    // stapled response. The file is read once when TLS material is built; nginx
    // refreshes it from disk, the runtime requires a config reload (documented
    // limitation). rustls sends `CertifiedKey::ocsp` verbatim.
    if let Some(ocsp_file) = ocsp_stapling_file {
        let response = read_bounded_tls_material(ocsp_file)?;
        if response.is_empty() {
            return Err(tls_files_error(
                ocsp_file,
                private_key_file,
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "ssl_stapling_file contains no OCSP response",
                ),
            ));
        }
        if response.len() > 64 * 1024 {
            return Err(tls_files_error(
                ocsp_file,
                private_key_file,
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "ssl_stapling_file exceeds the 64 KiB OCSP response bound",
                ),
            ));
        }
        // An OCSPResponse is an ASN.1 SEQUENCE; reject obvious non-DER files
        // early instead of stapling arbitrary bytes.
        if response[0] != 0x30 {
            return Err(tls_files_error(
                ocsp_file,
                private_key_file,
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "ssl_stapling_file is not a DER OCSP response (missing SEQUENCE tag)",
                ),
            ));
        }
        certified_key.ocsp = Some(response);
    }
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
    // PRD §8.3/§9: an expired or not-yet-valid leaf must be rejected, not
    // served "just in case". Activating it would publish an invalid certificate
    // to every client while the control plane still reports the site healthy.
    if !certificate.validity().is_valid() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "leaf X.509 certificate is expired or not yet valid (notBefore={}, notAfter={})",
                certificate.validity().not_before,
                certificate.validity().not_after
            ),
        ));
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
        fingerprint_sha256: sdkwork_utils_rust::hex_encode(
            Sha256::digest(certificate_der).as_slice(),
        ),
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

#[cfg(test)]
mod tests {
    use rcgen::{date_time_ymd, CertificateParams, KeyPair};
    use sdkwork_webserver_core::TlsVersion;

    use super::{
        parse_certificate_chain, parse_single_private_key, tls_protocol_versions,
        validate_leaf_certificate,
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
    fn expired_leaf_certificate_is_rejected() {
        let mut params = CertificateParams::new(vec!["expired.example.test".to_owned()])
            .expect("certificate parameters");
        params.not_before = date_time_ymd(2000, 1, 1);
        params.not_after = date_time_ymd(2001, 1, 1);
        let key = KeyPair::generate().expect("generate key");
        let certificate = params.self_signed(&key).expect("generate certificate");
        let error = validate_leaf_certificate(
            certificate.der().as_ref(),
            &["expired.example.test".to_owned()],
        )
        .expect_err("an expired leaf must never be activated");
        assert!(error.to_string().contains("expired or not yet valid"));
    }

    #[test]
    fn chain_file_completes_the_served_chain_without_duplicates() {
        use super::{install_crypto_provider, load_certified_key};

        install_crypto_provider().expect("crypto provider");
        let provider = rustls::crypto::CryptoProvider::get_default().expect("provider");
        let directory = tempfile::tempdir().expect("temp dir");

        let leaf_params =
            CertificateParams::new(vec!["chain.example.test".to_owned()]).expect("leaf parameters");
        let leaf_key = KeyPair::generate().expect("leaf key");
        let leaf = leaf_params
            .self_signed(&leaf_key)
            .expect("leaf certificate");
        let certificate_path = directory.path().join("leaf.pem");
        let private_key_path = directory.path().join("leaf.key");
        std::fs::write(&certificate_path, leaf.pem()).expect("write leaf");
        std::fs::write(&private_key_path, leaf_key.serialize_pem()).expect("write key");

        let chain_params = CertificateParams::new(vec!["issuer.example.test".to_owned()])
            .expect("chain parameters");
        let chain_key = KeyPair::generate().expect("chain key");
        let issuer = chain_params
            .self_signed(&chain_key)
            .expect("issuer certificate");
        let chain_path = directory.path().join("chain.pem");

        // A chain file that repeats the leaf is deduplicated: the served chain
        // stays a single certificate.
        std::fs::write(&chain_path, format!("{}\n{}", leaf.pem(), issuer.pem()))
            .expect("write chain");
        let loaded = load_certified_key(
            &certificate_path,
            &private_key_path,
            Some(&chain_path),
            None,
            &["chain.example.test".to_owned()],
            provider,
        )
        .expect("chain file loads");
        assert_eq!(
            loaded.certified_key.cert.len(),
            2,
            "leaf plus one missing intermediate, with the duplicate leaf deduplicated"
        );

        // Without a chain file the served chain is exactly the certificate file.
        let loaded = load_certified_key(
            &certificate_path,
            &private_key_path,
            None,
            None,
            &["chain.example.test".to_owned()],
            provider,
        )
        .expect("no chain file");
        assert_eq!(loaded.certified_key.cert.len(), 1);

        // A malformed chain file fails closed instead of being ignored.
        std::fs::write(&chain_path, "not a PEM chain").expect("write invalid chain");
        let error = load_certified_key(
            &certificate_path,
            &private_key_path,
            Some(&chain_path),
            None,
            &["chain.example.test".to_owned()],
            provider,
        )
        .err()
        .expect("malformed chain must fail closed");
        assert!(error.to_string().contains("chain.pem"), "{error}");
    }

    #[test]
    fn empty_or_malformed_pem_material_is_rejected() {
        assert!(parse_certificate_chain(b"not a PEM certificate").is_err());
        assert!(parse_single_private_key(b"not a PEM private key").is_err());
    }
}
