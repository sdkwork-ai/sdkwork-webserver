use std::sync::Arc;

use rustls::{
    pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer},
    version::{TLS12, TLS13},
    ClientConfig, RootCertStore,
};
use sdkwork_webserver_core::{
    CompiledWebServerApp, TlsVersion, UpstreamConfig, UpstreamTlsTrustMode,
};

use super::{runtime::read_bounded_tls_material, DataPlaneError};

const MAX_CUSTOM_ROOT_CERTIFICATES: usize = 64;

pub(crate) fn build_upstream_tls_config(
    app: &CompiledWebServerApp,
    upstream: &UpstreamConfig,
) -> Result<ClientConfig, DataPlaneError> {
    let policy = upstream.tls.as_ref();
    let mut roots = RootCertStore::empty();
    if policy.is_none_or(|policy| {
        matches!(
            policy.trust_mode,
            UpstreamTlsTrustMode::System | UpstreamTlsTrustMode::SystemAndCustom
        )
    }) {
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    }
    add_custom_roots(&mut roots, app, upstream)?;

    let versions = match policy.map(|policy| (policy.minimum_version, policy.maximum_version)) {
        Some((TlsVersion::Tls12, TlsVersion::Tls12)) => vec![&TLS12],
        Some((TlsVersion::Tls13, TlsVersion::Tls13)) => vec![&TLS13],
        _ => vec![&TLS13, &TLS12],
    };
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let builder = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&versions)
        .map_err(|source| DataPlaneError::UpstreamClient {
            upstream_id: upstream.id.clone(),
            source: Box::new(source),
        })?
        .with_root_certificates(roots);
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
    config.enable_sni = true;
    Ok(config)
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
