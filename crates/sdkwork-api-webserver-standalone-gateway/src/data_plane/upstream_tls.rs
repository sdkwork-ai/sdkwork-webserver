use std::sync::{Arc, Mutex, OnceLock};

use hashlink::LinkedHashMap;
use rustls::{
    pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer},
    version::{TLS12, TLS13},
    ClientConfig, RootCertStore,
};
use sdkwork_webserver_core::{
    CompiledWebServerApp, TlsVersion, UpstreamConfig, UpstreamTlsTrustMode,
};
use sha2::{Digest, Sha256};

use super::{runtime::read_bounded_tls_material, DataPlaneError};

const MAX_CUSTOM_ROOT_CERTIFICATES: usize = 64;
/// Upper bound on distinct cached upstream TLS configurations. Each entry
/// embeds a crypto provider plus the webpki root store (hundreds of KB), so
/// the cache stays small; entries are keyed by the full TLS policy identity
/// (trust mode, protocol versions, CA bundle and client-identity contents).
const MAXIMUM_SHARED_TLS_CONFIGS: usize = 16;

pub(crate) fn build_upstream_tls_config(
    app: &CompiledWebServerApp,
    upstream: &UpstreamConfig,
) -> Result<ClientConfig, DataPlaneError> {
    let policy_key = upstream_tls_policy_key(app, upstream)?;
    let cached = with_shared_tls_configs(|cache| cache.cached(&policy_key));
    if let Some(shared) = cached {
        // `ClientConfig: Clone` copies its internal `Arc`s plus the root
        // anchor vector — far cheaper than re-reading the webpki store,
        // building a fresh crypto provider, and re-parsing every CA bundle.
        return Ok((*shared).clone());
    }
    let config = build_upstream_tls_config_uncached(app, upstream)?;
    with_shared_tls_configs(|cache| cache.remember(policy_key, Arc::new(config.clone())));
    Ok(config)
}

/// Process-level store of upstream TLS configurations shared by policy
/// identity, LRU-bounded (`MAXIMUM_SHARED_TLS_CONFIGS`).
#[derive(Default)]
struct SharedTlsConfigs {
    entries: LinkedHashMap<[u8; 32], Arc<ClientConfig>>,
}

impl SharedTlsConfigs {
    fn cached(&mut self, key: &[u8; 32]) -> Option<Arc<ClientConfig>> {
        let config = self.entries.remove(key)?;
        // LRU touch: reinsertion moves the entry to the most-recently-used
        // end in O(1).
        self.entries.insert(*key, config.clone());
        Some(config)
    }

    fn remember(&mut self, key: [u8; 32], config: Arc<ClientConfig>) {
        if self.entries.len() >= MAXIMUM_SHARED_TLS_CONFIGS && !self.entries.contains_key(&key) {
            self.entries.pop_front();
        }
        self.entries.insert(key, config);
    }
}

fn with_shared_tls_configs<R>(apply: impl FnOnce(&mut SharedTlsConfigs) -> R) -> R {
    static CACHE: OnceLock<Mutex<SharedTlsConfigs>> = OnceLock::new();
    let mut guard = CACHE
        .get_or_init(|| Mutex::new(SharedTlsConfigs::default()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    apply(&mut guard)
}

/// Hash every input that changes the built `ClientConfig`: trust mode,
/// protocol versions, custom CA bundle contents (in declared order), and
/// client-identity contents. Files are re-read bounded on cache misses so
/// rotated material produces a new key instead of a stale shared config.
fn upstream_tls_policy_key(
    app: &CompiledWebServerApp,
    upstream: &UpstreamConfig,
) -> Result<[u8; 32], DataPlaneError> {
    let mut hasher = Sha256::new();
    let policy = upstream.tls.as_ref();
    let trust_mode = match policy.map(|policy| policy.trust_mode) {
        None | Some(UpstreamTlsTrustMode::System) => 0_u8,
        Some(UpstreamTlsTrustMode::SystemAndCustom) => 1_u8,
        Some(UpstreamTlsTrustMode::Custom) => 2_u8,
    };
    hasher.update([trust_mode]);
    let versions_key = match policy.map(|policy| (policy.minimum_version, policy.maximum_version)) {
        Some((TlsVersion::Tls12, TlsVersion::Tls12)) => 0_u8,
        Some((TlsVersion::Tls13, TlsVersion::Tls13)) => 1_u8,
        _ => 2_u8,
    };
    hasher.update([versions_key]);
    hasher.update([
        u8::from(policy.is_some_and(|policy| policy.send_sni)),
        u8::from(policy.is_some_and(|policy| policy.dangerous_accept_any_verify)),
    ]);
    for path in app
        .upstream_tls_ca_certificate_paths(&upstream.id)
        .unwrap_or_default()
    {
        let pem = read_bounded_tls_material(path)?;
        hasher.update((pem.len() as u64).to_le_bytes());
        hasher.update(&pem);
    }
    match app.upstream_tls_client_identity_paths(&upstream.id) {
        Some((certificate_path, private_key_path)) => {
            let certificate = read_bounded_tls_material(certificate_path)?;
            let private_key = read_bounded_tls_material(private_key_path)?;
            hasher.update([1_u8]);
            hasher.update((certificate.len() as u64).to_le_bytes());
            hasher.update(&certificate);
            hasher.update((private_key.len() as u64).to_le_bytes());
            hasher.update(&private_key);
        }
        None => hasher.update([0_u8]),
    }
    Ok(hasher.finalize().into())
}

fn build_upstream_tls_config_uncached(
    app: &CompiledWebServerApp,
    upstream: &UpstreamConfig,
) -> Result<ClientConfig, DataPlaneError> {
    let policy = upstream.tls.as_ref();
    // nginx `uwsgi_ssl_verify off` is an explicit dangerous accept-any mode:
    // the runtime never weakens verification implicitly, so this branch is only
    // reached through the documented flag (the loader emits a warning for it).
    let dangerous_accept_any = policy.is_some_and(|policy| policy.dangerous_accept_any_verify);
    let mut roots = RootCertStore::empty();
    if !dangerous_accept_any
        && policy.is_none_or(|policy| {
            matches!(
                policy.trust_mode,
                UpstreamTlsTrustMode::System | UpstreamTlsTrustMode::SystemAndCustom
            )
        })
    {
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    }
    if !dangerous_accept_any {
        add_custom_roots(&mut roots, app, upstream)?;
    } else if policy.is_some_and(|policy| !policy.ca_certificate_files.is_empty()) {
        return Err(DataPlaneError::UpstreamTls {
            upstream_id: upstream.id.clone(),
            material: "dangerous accept-any policy",
            source: "custom CA files cannot be combined with dangerousAcceptAnyVerify".into(),
        });
    }

    let versions = match policy.map(|policy| (policy.minimum_version, policy.maximum_version)) {
        Some((TlsVersion::Tls12, TlsVersion::Tls12)) => vec![&TLS12],
        Some((TlsVersion::Tls13, TlsVersion::Tls13)) => vec![&TLS13],
        _ => vec![&TLS13, &TLS12],
    };
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let builder = ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&versions)
        .map_err(|source| DataPlaneError::UpstreamClient {
            upstream_id: upstream.id.clone(),
            source: Box::new(source),
        })?;
    let builder = if dangerous_accept_any {
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(DangerousAcceptAnyVerifier::new(provider)?))
    } else {
        builder.with_root_certificates(roots)
    };
    let mut config = if let Some((certificate_path, private_key_path)) =
        app.upstream_tls_client_identity_paths(&upstream.id)
    {
        let certificates = read_certificate_chain(certificate_path).map_err(|source| {
            DataPlaneError::UpstreamTls {
                upstream_id: upstream.id.clone(),
                material: "client certificate",
                source,
            }
        })?;
        let private_key =
            read_private_key(private_key_path).map_err(|source| DataPlaneError::UpstreamTls {
                upstream_id: upstream.id.clone(),
                material: "client private key",
                source,
            })?;
        builder
            .with_client_auth_cert(certificates, private_key)
            .map_err(|source| DataPlaneError::UpstreamClient {
                upstream_id: upstream.id.clone(),
                source: Box::new(source),
            })?
    } else {
        builder.with_no_client_auth()
    };
    // nginx `uwsgi_ssl_server_name off` disables the SNI extension while the
    // certificate is still verified against the configured/target name.
    config.enable_sni = policy.is_none_or(|policy| policy.send_sni);
    config.alpn_protocols.clear();
    Ok(config)
}

/// Explicit dangerous accept-any certificate verifier.
///
/// Only reachable through `dangerousAcceptAnyVerify` (nginx explicit
/// `uwsgi_ssl_verify off`); the handshake-signature checks still run so a
/// non-TLS peer cannot claim a handshake proof.
#[derive(Debug)]
struct DangerousAcceptAnyVerifier {
    algorithms: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl DangerousAcceptAnyVerifier {
    fn new(provider: Arc<rustls::crypto::CryptoProvider>) -> Result<Self, DataPlaneError> {
        Ok(Self {
            algorithms: provider.signature_verification_algorithms,
        })
    }
}

impl rustls::client::danger::ServerCertVerifier for DangerousAcceptAnyVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
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

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

fn add_custom_roots(
    roots: &mut RootCertStore,
    app: &CompiledWebServerApp,
    upstream: &UpstreamConfig,
) -> Result<(), DataPlaneError> {
    let ca_paths = app
        .upstream_tls_ca_certificate_paths(&upstream.id)
        .unwrap_or_default();
    let mut root_count = 0usize;
    for path in ca_paths {
        let pem = read_bounded_tls_material(path)?;
        let certificates = CertificateDer::pem_slice_iter(&pem)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| DataPlaneError::InvalidUpstreamCaBundle {
                upstream_id: upstream.id.clone(),
                path: path.clone(),
            })?;
        if certificates.is_empty() {
            return Err(if pem.iter().all(u8::is_ascii_whitespace) {
                DataPlaneError::EmptyUpstreamCaBundle {
                    upstream_id: upstream.id.clone(),
                    path: path.clone(),
                }
            } else {
                DataPlaneError::InvalidUpstreamCaBundle {
                    upstream_id: upstream.id.clone(),
                    path: path.clone(),
                }
            });
        }
        root_count = root_count.saturating_add(certificates.len());
        if root_count > MAX_CUSTOM_ROOT_CERTIFICATES {
            return Err(DataPlaneError::TooManyUpstreamRootCertificates {
                upstream_id: upstream.id.clone(),
                actual: root_count,
                maximum: MAX_CUSTOM_ROOT_CERTIFICATES,
            });
        }
        for certificate in certificates {
            roots
                .add(certificate)
                .map_err(|_| DataPlaneError::InvalidUpstreamCaBundle {
                    upstream_id: upstream.id.clone(),
                    path: path.clone(),
                })?;
        }
    }
    Ok(())
}

fn read_certificate_chain(
    path: &std::path::Path,
) -> Result<Vec<CertificateDer<'static>>, Box<dyn std::error::Error + Send + Sync>> {
    let pem = read_bounded_tls_material(path)?;
    let certificates = CertificateDer::pem_slice_iter(&pem).collect::<Result<Vec<_>, _>>()?;
    if certificates.is_empty() {
        return Err("client certificate file contains no certificates".into());
    }
    Ok(certificates)
}

fn read_private_key(
    path: &std::path::Path,
) -> Result<PrivateKeyDer<'static>, Box<dyn std::error::Error + Send + Sync>> {
    let pem = read_bounded_tls_material(path)?;
    PrivateKeyDer::from_pem_slice(&pem).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> Arc<ClientConfig> {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        Arc::new(
            ClientConfig::builder_with_provider(provider)
                .with_protocol_versions(&[&TLS13, &TLS12])
                .expect("protocol versions are valid")
                .with_root_certificates(RootCertStore::empty())
                .with_no_client_auth(),
        )
    }

    #[test]
    fn shared_tls_config_cache_round_trips_and_stays_bounded() {
        let mut cache = SharedTlsConfigs::default();
        let key_a = [1_u8; 32];
        let key_b = [2_u8; 32];
        let config_a = sample_config();
        cache.remember(key_a, Arc::clone(&config_a));
        assert!(Arc::ptr_eq(
            &cache.cached(&key_a).expect("cached entry"),
            &config_a
        ));
        assert!(cache.cached(&key_b).is_none());

        // Fill to capacity with keys disjoint from key_a/key_b.
        for index in 0_u8..(MAXIMUM_SHARED_TLS_CONFIGS as u8) {
            cache.remember([u8::MAX - index; 32], sample_config());
        }
        assert_eq!(cache.entries.len(), MAXIMUM_SHARED_TLS_CONFIGS);
        cache.remember(key_b, sample_config());
        assert_eq!(cache.entries.len(), MAXIMUM_SHARED_TLS_CONFIGS);
        // key_a (untouched since the fill began) is the least recently used
        // live entry, so the newest insert must have evicted it.
        assert!(!cache.entries.contains_key(&key_a));
        assert!(cache.entries.contains_key(&key_b));
    }

    #[test]
    fn shared_tls_config_cache_touch_refreshes_lru_order() {
        let mut cache = SharedTlsConfigs::default();
        for index in 0_u8..(MAXIMUM_SHARED_TLS_CONFIGS as u8) {
            cache.remember([index; 32], sample_config());
        }
        // Touch key 0 so key 1 becomes the least recently used entry.
        assert!(cache.cached(&[0_u8; 32]).is_some());
        cache.remember([u8::MAX; 32], sample_config());
        assert!(
            cache.entries.contains_key(&[0_u8; 32]),
            "touched entry must survive"
        );
        assert!(
            !cache.entries.contains_key(&[1_u8; 32]),
            "untouched entry must be evicted"
        );
    }
}
