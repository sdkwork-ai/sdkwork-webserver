use std::sync::Arc;

use axum_server::tls_rustls::RustlsConfig;
use sdkwork_webserver_core::ListenerConfig;

use super::{
    runtime::RuntimeGeneration,
    tls_material::{build_sni_server_config, install_crypto_provider, load_certified_key},
    DataPlaneError,
};

pub(crate) fn build_tls_config(
    generation: &Arc<RuntimeGeneration>,
    listener: &ListenerConfig,
) -> Result<Option<RustlsConfig>, DataPlaneError> {
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
        let loaded = load_certified_key(
            certificate_file,
            private_key_file,
            &certificate.server_names,
            provider,
        )?;
        certificates.push((certificate.server_names.clone(), loaded.certified_key));
    }
    let server_config = build_sni_server_config(
        certificates,
        policy.minimum_version,
        policy.maximum_version,
        &policy.alpn,
        policy.client_auth.as_ref(),
    )
    .map_err(|server_name| DataPlaneError::AmbiguousTlsServerName {
        policy_id: policy.id.clone(),
        server_name,
    })?;
    Ok(Some(RustlsConfig::from_config(server_config)))
}
