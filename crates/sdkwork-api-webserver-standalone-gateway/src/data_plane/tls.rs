use std::sync::Arc;

use axum_server::tls_rustls::RustlsConfig;
use sdkwork_utils_rust::datetime;
use sdkwork_webserver_core::{ClientAuthConfig, CompiledWebServerApp, ListenerConfig, TlsVersion};

use super::{
    drive_certificates::{self, DriveCertificateStore},
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
/// These form the configured layer of a listener: a configured file wins for a
/// server name while it is usable and no usable Drive bundle covers the name,
/// and the assigned certificate set covers every name the files do not.
pub(crate) struct CompiledPolicyTlsMaterial {
    pub(crate) layers: CertificateSourceIndex,
    pub(crate) minimum_version: TlsVersion,
    pub(crate) maximum_version: TlsVersion,
    pub(crate) alpn: Vec<String>,
    pub(crate) client_auth: Option<ClientAuthConfig>,
}

pub(crate) fn build_tls_config(
    app: &CompiledWebServerApp,
    listener: &ListenerConfig,
) -> Result<Option<RustlsConfig>, DataPlaneError> {
    build_tls_config_with_drive(app, listener, drive_certificates::shared())
}

pub(crate) fn build_tls_config_with_drive(
    app: &CompiledWebServerApp,
    listener: &ListenerConfig,
    drive: Option<Arc<DriveCertificateStore>>,
) -> Result<Option<RustlsConfig>, DataPlaneError> {
    let Some(material) = compile_policy_tls_material(app, listener, false, drive.as_deref())?
    else {
        return Ok(None);
    };
    let policy_id = listener.tls_policy_ref.clone().unwrap_or_default();
    let server_config = build_layered_server_config(
        Arc::new(ResolverLayers::policy_only(material.layers).with_drive(drive)),
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
/// present and usable" contract.
///
/// A listener with no assigned lower layer passes `false`, with one exception:
/// when a usable Drive bundle covers *every* name the unusable file declared,
/// the file is dropped and the Drive authority serves those names. Otherwise a
/// broken local file would block the listener before Drive could ever answer,
/// which would defeat the Drive-first contract. When Drive does not cover all
/// declared names the original error is returned — the listener still fails
/// closed rather than silently serving a subset.
pub(crate) fn compile_policy_tls_material(
    app: &CompiledWebServerApp,
    listener: &ListenerConfig,
    tolerate_unavailable_material: bool,
    drive: Option<&DriveCertificateStore>,
) -> Result<Option<CompiledPolicyTlsMaterial>, DataPlaneError> {
    let Some(policy_id) = &listener.tls_policy_ref else {
        return Ok(None);
    };
    let policy = app
        .tls_policy(policy_id)
        .ok_or_else(|| DataPlaneError::MissingTlsPolicy {
            listener_id: listener.id.clone(),
            policy_id: policy_id.clone(),
        })?;

    install_crypto_provider()?;
    let provider = rustls::crypto::CryptoProvider::get_default()
        .expect("the Rustls crypto provider was installed before certificate parsing");
    let now_unix_seconds = datetime::now().timestamp();
    let mut certificates = Vec::new();
    for certificate_ref in policy.certificate_refs() {
        let certificate =
            app.certificate(certificate_ref)
                .ok_or_else(|| DataPlaneError::MissingCertificate {
                    policy_id: policy.id.clone(),
                    certificate_id: certificate_ref.to_owned(),
                })?;
        let (certificate_file, private_key_file) = app
            .certificate_paths(&certificate.id)
            .ok_or_else(|| DataPlaneError::MissingCertificateFiles {
                certificate_id: certificate.id.clone(),
            })?;
        let loaded = match load_certified_key(
            certificate_file,
            private_key_file,
            app.certificate_chain_file(&certificate.id),
            app.certificate_ocsp_stapling_file(&certificate.id),
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
            Err(error)
                if certificate.server_name_regexes.is_empty()
                    && drive.is_some_and(|drive| {
                        drive.covers_all_declared_names_at(
                            &certificate.server_names,
                            now_unix_seconds,
                        )
                    }) =>
            {
                tracing::warn!(
                    certificate_id = %certificate.id,
                    certificate_file = %certificate_file.display(),
                    error = %error,
                    "configured TLS certificate file is unusable; a usable SDKWork Drive bundle covers every declared server name and takes priority"
                );
                continue;
            }
            Err(error) => return Err(error),
        };
        certificates.push((
            certificate.server_names.clone(),
            certificate.server_name_regexes.clone(),
            loaded,
        ));
    }
    // Relative `clientAuth.caCertificateFiles` are chrooted to the config
    // directory by the compiler; substitute the resolved paths.
    let client_auth = policy.client_auth.as_ref().map(|auth| {
        let resolved = app
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rcgen::{date_time_ymd, CertificateParams, KeyPair};
    use rustls::pki_types::{pem::PemObject, CertificateDer, ServerName};
    use sdkwork_webserver_core::config::{ConfigFormat, ConfigLoadOptions, WebServerConfigLoader};
    use tempfile::TempDir;

    use super::*;
    use crate::data_plane::drive_certificates::{DriveCertificateConfig, DriveCertificateStore};

    fn write_drive_bundle(root: &Path, bundle_id: &str, server_name: &str) {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let mut params =
            CertificateParams::new(vec![server_name.to_owned()]).expect("certificate parameters");
        params.not_before = date_time_ymd(2020, 1, 1);
        params.not_after = date_time_ymd(2040, 1, 1);
        let key = KeyPair::generate().expect("generate key");
        let certificate = params.self_signed(&key).expect("self-signed certificate");
        let directory = root.join(bundle_id);
        std::fs::create_dir_all(&directory).expect("create bundle directory");
        std::fs::write(directory.join("fullchain.pem"), certificate.pem())
            .expect("write certificate");
        std::fs::write(directory.join("privkey.pem"), key.serialize_pem())
            .expect("write private key");
    }

    fn drive_store(directory: &Path) -> Arc<DriveCertificateStore> {
        let config = DriveCertificateConfig {
            root: directory.join("drive"),
            cache_root: directory.join("drive-cache"),
            refresh_interval: std::time::Duration::from_secs(10),
            max_bundles: 16,
        };
        std::fs::create_dir_all(&config.root).expect("create Drive root");
        DriveCertificateStore::load(config).expect("load Drive certificate store")
    }

    fn compile_nginx(directory: &Path) -> sdkwork_webserver_core::CompiledWebServerApp {
        compile_nginx_with_server_name(directory, "site.example.test")
    }

    fn compile_nginx_with_server_name(
        directory: &Path,
        server_name: &str,
    ) -> sdkwork_webserver_core::CompiledWebServerApp {
        let bad_certificate = directory.join("bad.pem");
        let bad_private_key = directory.join("bad.key");
        std::fs::write(&bad_certificate, b"not a certificate").expect("write corrupt certificate");
        std::fs::write(&bad_private_key, b"not a private key").expect("write corrupt key");
        let path = directory.join("nginx.conf");
        let config = format!(
            "server {{\n    listen 127.0.0.1:18443 ssl;\n    server_name {server_name};\n    ssl_certificate \"{}\";\n    ssl_certificate_key \"{}\";\n    location / {{ return 200 \"ok\"; }}\n}}\n",
            bad_certificate.display(),
            bad_private_key.display()
        );
        std::fs::write(&path, config).expect("write nginx config");
        WebServerConfigLoader::new()
            .load_and_compile(
                &path,
                &ConfigLoadOptions::with_format(ConfigFormat::NginxConf),
            )
            .expect("compile nginx configuration")
    }

    fn listener(app: &sdkwork_webserver_core::CompiledWebServerApp) -> &ListenerConfig {
        app.listeners().next().expect("one listener")
    }

    async fn handshake_leaf(
        server_config: Arc<rustls::ServerConfig>,
        certificate_path: &Path,
    ) -> Vec<u8> {
        let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test listener");
        let address = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept connection");
            acceptor
                .accept(stream)
                .await
                .expect("complete TLS handshake");
        });

        let pem = std::fs::read(certificate_path).expect("read served certificate");
        let mut roots = rustls::RootCertStore::empty();
        for certificate in CertificateDer::pem_slice_iter(&pem) {
            roots
                .add(certificate.expect("certificate DER"))
                .expect("trust the Drive leaf");
        }
        let mut client_config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        client_config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));
        let stream = tokio::net::TcpStream::connect(address)
            .await
            .expect("connect to test listener");
        let server_name = ServerName::try_from("site.example.test")
            .expect("server name")
            .to_owned();
        let client = connector
            .connect(server_name, stream)
            .await
            .expect("client TLS handshake");
        let leaf = client
            .get_ref()
            .1
            .peer_certificates()
            .expect("peer certificates")[0]
            .as_ref()
            .to_vec();
        server.await.expect("server task");
        leaf
    }

    #[tokio::test]
    async fn a_covering_drive_bundle_lets_an_unusable_local_certificate_start() {
        let directory = TempDir::new().expect("tempdir");
        write_drive_bundle(&directory.path().join("drive"), "site", "site.example.test");
        let app = compile_nginx(directory.path());
        let listener = listener(&app);

        // Without Drive the corrupt local material still fails closed.
        assert!(build_tls_config_with_drive(&app, listener, None).is_err());

        // With Drive covering every declared name the listener builds; the
        // local file is dropped and Drive serves the handshake.
        let store = drive_store(directory.path());
        let config = build_tls_config_with_drive(&app, listener, Some(store))
            .expect("a covering Drive bundle must let the listener start")
            .expect("TLS configuration");
        let served = handshake_leaf(
            config.get_inner(),
            &directory.path().join("drive/site/fullchain.pem"),
        )
        .await;
        let expected = std::fs::read(directory.path().join("drive/site/fullchain.pem"))
            .expect("read Drive certificate");
        let expected = CertificateDer::pem_slice_iter(&expected)
            .next()
            .expect("Drive certificate PEM")
            .expect("Drive certificate DER");
        assert_eq!(
            served,
            expected.as_ref(),
            "the handshake must serve the Drive leaf"
        );
    }

    #[test]
    fn a_drive_bundle_that_does_not_cover_all_names_keeps_the_local_failure() {
        let directory = TempDir::new().expect("tempdir");
        write_drive_bundle(
            &directory.path().join("drive"),
            "other",
            "other.example.test",
        );
        let app = compile_nginx(directory.path());
        let store = drive_store(directory.path());

        assert!(
            build_tls_config_with_drive(&app, listener(&app), Some(store)).is_err(),
            "a Drive bundle for another name must not hide the broken local certificate"
        );
    }

    #[test]
    fn a_regex_server_name_is_not_dropped_for_drive() {
        let directory = TempDir::new().expect("tempdir");
        write_drive_bundle(&directory.path().join("drive"), "site", "site.example.test");
        // The certificate also claims a regex SNI name. Drive certificates only
        // carry SAN names, so dropping this file would silently lose the regex
        // coverage; the load path must keep failing closed instead.
        let app = compile_nginx_with_server_name(directory.path(), "site.example.test ~^api\\.");
        let store = drive_store(directory.path());
        assert!(
            build_tls_config_with_drive(&app, listener(&app), Some(store)).is_err(),
            "Drive cannot replace a certificate that also declares regex server names"
        );
    }
}
