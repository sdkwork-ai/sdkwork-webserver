use std::sync::Arc;

use axum_server::tls_rustls::RustlsConfig;
use sdkwork_webserver_core::{ClientAuthConfig, ListenerConfig, TlsVersion};

use super::{
    runtime::RuntimeGeneration,
    tls_material::{
        build_layered_server_config, build_source_index, install_crypto_provider,
        load_certified_key,
    },
    tls_resolver::{CertificateSourceIndex, ResolverLayers},
    DataPlaneError,
};

/// The certificate files a listener's TLS policy references, together with the
/// protocol parameters that policy owns.
///
/// These form the upper layer of a `policy-first` listener: a configured file
/// wins for a server name while it is usable, and the assigned certificate set
/// covers every name the files do not.
pub(crate) struct CompiledPolicyTlsMaterial {
    pub(crate) layers: CertificateSourceIndex,
    pub(crate) minimum_version: TlsVersion,
    pub(crate) maximum_version: TlsVersion,
    pub(crate) alpn: Vec<String>,
    pub(crate) client_auth: Option<ClientAuthConfig>,
}

pub(crate) fn build_tls_config(
    generation: &Arc<RuntimeGeneration>,
    listener: &ListenerConfig,
) -> Result<Option<RustlsConfig>, DataPlaneError> {
    let Some(material) = compile_policy_tls_material(generation, listener, false)? else {
        return Ok(None);
    };
    let policy_id = listener.tls_policy_ref.clone().unwrap_or_default();
    let server_config = build_layered_server_config(
        Arc::new(ResolverLayers::policy_only(material.layers)),
        material.minimum_version,
        material.maximum_version,
        &material.alpn,
        material.client_auth.as_ref(),
    )
    .map_err(|server_name| DataPlaneError::AmbiguousTlsServerName {
        policy_id,
        server_name,
    })?;
    Ok(Some(RustlsConfig::from_config(server_config)))
}

/// Resolves a listener's TLS policy into loadable certificate material.
///
/// Returns `None` when the listener references no policy. Separated from
/// [`build_tls_config`] because a `policy-first` listener feeds these layers
/// into a shared resolver instead of building its own `ServerConfig`.
///
/// `tolerate_unavailable_material` decides what an unreadable, malformed or
/// expired referenced file does. A `policy-first` listener passes `true`: the
/// file drops out of the upper layer and the assigned certificate set covers
/// its server names, which is precisely the "a configured file wins when it is
/// present and usable" contract. Every other listener passes `false`, because
/// with no lower layer a dropped file would leave the listener serving nothing
/// — a silent outage rather than a fallback.
pub(crate) fn compile_policy_tls_material(
    generation: &Arc<RuntimeGeneration>,
    listener: &ListenerConfig,
    tolerate_unavailable_material: bool,
) -> Result<Option<CompiledPolicyTlsMaterial>, DataPlaneError> {
    let Some(policy_id) = &listener.tls_policy_ref else {
        return Ok(None);
    };
    let policy =
        generation
            .app
            .tls_policy(policy_id)
            .ok_or_else(|| DataPlaneError::MissingTlsPolicy {
                listener_id: listener.id.clone(),
                policy_id: policy_id.clone(),
            })?;

    install_crypto_provider()?;
    let provider = rustls::crypto::CryptoProvider::get_default()
        .expect("the Rustls crypto provider was installed before certificate parsing");
    let mut certificates = Vec::new();
    for certificate_ref in policy.certificate_refs() {
        let certificate = generation.app.certificate(certificate_ref).ok_or_else(|| {
            DataPlaneError::MissingCertificate {
                policy_id: policy.id.clone(),
                certificate_id: certificate_ref.to_owned(),
            }
        })?;
        let (certificate_file, private_key_file) = generation
            .app
            .certificate_paths(&certificate.id)
            .ok_or_else(|| DataPlaneError::MissingCertificateFiles {
                certificate_id: certificate.id.clone(),
            })?;
        let loaded = match load_certified_key(
            certificate_file,
            private_key_file,
            &certificate.server_names,
            provider,
        ) {
            Ok(loaded) => loaded,
            Err(error) if tolerate_unavailable_material => {
                tracing::warn!(
                    certificate_id = %certificate.id,
                    error = %error,
                    "configured TLS certificate file is unavailable; the assigned certificate set covers its server names"
                );
                continue;
            }
            Err(error) => return Err(error),
        };
        certificates.push((certificate.server_names.clone(), loaded));
    }
    // Relative `clientAuth.caCertificateFiles` are chrooted to the config
    // directory by the compiler; substitute the resolved paths.
    let client_auth = policy.client_auth.as_ref().map(|auth| {
        let resolved = generation
            .app
            .client_auth_ca_paths(&policy.id)
            .map(|paths| {
                paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| auth.ca_certificate_files.clone());
        ClientAuthConfig {
            mode: auth.mode,
            ca_certificate_files: resolved,
        }
    });
    let layers = build_source_index(certificates).map_err(|server_name| {
        DataPlaneError::AmbiguousTlsServerName {
            policy_id: policy.id.clone(),
            server_name,
        }
    })?;
    Ok(Some(CompiledPolicyTlsMaterial {
        layers,
        minimum_version: policy.minimum_version,
        maximum_version: policy.maximum_version,
        alpn: policy.alpn.clone(),
        client_auth,
    }))
}
