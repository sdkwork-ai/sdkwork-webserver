//! Edge node runtime: nginx site paths, certificate bundle materialization, reload.

mod certificate_material;
mod config;
mod deployment;
mod error;
mod nginx;
mod paths;
mod tls_probe;

pub use certificate_material::CertificateBundleMaterial;
pub use config::EdgeRuntimeConfig;
pub use deployment::NginxSiteConfigMaterial;
pub use error::{EdgeRuntimeError, EdgeRuntimeResult};
pub use nginx::{
    deploy_nginx_config, reload_nginx, validate_active_nginx_config, validate_nginx_config,
    verify_served_config,
};
pub use paths::{cert_bundle_paths, nginx_site_path};
pub use tls_probe::verify_served_certificate;

use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub struct PendingCertificateBundleActivation {
    activation: paths::CertificateBundleActivation,
    permit: OwnedSemaphorePermit,
}

pub struct PendingEdgeDeployment {
    activation: deployment::EdgeDeploymentActivation,
    permit: OwnedSemaphorePermit,
}

impl PendingEdgeDeployment {
    pub async fn commit(self) -> Result<(), EdgeRuntimeError> {
        let Self { activation, permit } = self;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            activation.commit()
        })
        .await
        .map_err(|error| {
            EdgeRuntimeError::Filesystem(format!("edge deployment commit task failed: {error}"))
        })?
    }

    pub async fn rollback(self) -> Result<(), EdgeRuntimeError> {
        let Self { activation, permit } = self;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            activation.rollback()
        })
        .await
        .map_err(|error| {
            EdgeRuntimeError::Filesystem(format!("edge deployment rollback task failed: {error}"))
        })?
    }
}

impl PendingCertificateBundleActivation {
    pub async fn commit(self) -> Result<(), EdgeRuntimeError> {
        let Self { activation, permit } = self;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            activation.commit()
        })
        .await
        .map_err(|error| {
            EdgeRuntimeError::Filesystem(format!("certificate bundle commit task failed: {error}"))
        })?
    }

    pub async fn rollback(self) -> Result<(), EdgeRuntimeError> {
        let Self { activation, permit } = self;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            activation.rollback()
        })
        .await
        .map_err(|error| {
            EdgeRuntimeError::Filesystem(format!(
                "certificate bundle rollback task failed: {error}"
            ))
        })?
    }
}

pub struct EdgeRuntime {
    config: EdgeRuntimeConfig,
    filesystem_activation_admission: Arc<Semaphore>,
}

impl EdgeRuntime {
    pub fn new(config: EdgeRuntimeConfig) -> Self {
        Self {
            config,
            filesystem_activation_admission: Arc::new(Semaphore::new(1)),
        }
    }

    pub fn from_env() -> Result<Self, EdgeRuntimeError> {
        Ok(Self::new(EdgeRuntimeConfig::from_env()?))
    }

    pub fn config(&self) -> &EdgeRuntimeConfig {
        &self.config
    }

    pub fn write_certificate_bundle(
        &self,
        material: &CertificateBundleMaterial,
    ) -> Result<(), EdgeRuntimeError> {
        paths::write_certificate_bundle(&self.config.cert_live_root, material)
    }

    pub async fn write_certificate_bundle_async(
        &self,
        material: &CertificateBundleMaterial,
    ) -> Result<(), EdgeRuntimeError> {
        self.activate_certificate_bundle_async(material)
            .await?
            .commit()
            .await
    }

    pub async fn activate_certificate_bundle_async(
        &self,
        material: &CertificateBundleMaterial,
    ) -> Result<PendingCertificateBundleActivation, EdgeRuntimeError> {
        let permit = self
            .filesystem_activation_admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                EdgeRuntimeError::Filesystem(
                    "certificate bundle activation capacity exhausted".to_string(),
                )
            })?;
        let cert_live_root = self.config.cert_live_root.clone();
        let material = material.clone();
        let activation = tokio::task::spawn_blocking(move || {
            paths::activate_certificate_bundle(&cert_live_root, &material)
        })
        .await
        .map_err(|error| {
            EdgeRuntimeError::Filesystem(format!("certificate bundle task failed: {error}"))
        })??;
        Ok(PendingCertificateBundleActivation { activation, permit })
    }

    pub async fn activate_deployment_async(
        &self,
        nginx_configs: &[NginxSiteConfigMaterial],
        certificates: &[CertificateBundleMaterial],
    ) -> Result<PendingEdgeDeployment, EdgeRuntimeError> {
        let permit = self
            .filesystem_activation_admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                EdgeRuntimeError::Filesystem(
                    "edge deployment activation capacity exhausted".to_string(),
                )
            })?;
        let config = self.config.clone();
        let nginx_configs = nginx_configs.to_vec();
        let certificates = certificates.to_vec();
        let activation = tokio::task::spawn_blocking(move || {
            deployment::activate_edge_deployment(&config, &nginx_configs, &certificates)
        })
        .await
        .map_err(|error| {
            EdgeRuntimeError::Filesystem(format!("edge deployment task failed: {error}"))
        })??;
        Ok(PendingEdgeDeployment { activation, permit })
    }

    pub fn deploy_app_config(
        &self,
        domain: &str,
        config_content: &str,
    ) -> Result<(), EdgeRuntimeError> {
        deploy_nginx_config(&self.config, domain, config_content)
    }

    pub fn validate_config_content(&self, config_content: &str) -> Result<(), EdgeRuntimeError> {
        validate_nginx_config(&self.config, config_content)
    }

    pub fn reload(&self) -> Result<(), EdgeRuntimeError> {
        reload_nginx(&self.config)
    }

    /// Proves the loaded Nginx configuration contains `expected_fragment`
    /// (PRD-FR-020 served-revision evidence for reload convergence).
    pub fn verify_served_config(&self, expected_fragment: &str) -> Result<(), EdgeRuntimeError> {
        verify_served_config(&self.config, expected_fragment)
    }

    pub fn validate_active_config(&self) -> Result<(), EdgeRuntimeError> {
        validate_active_nginx_config(&self.config)
    }

    pub fn verify_served_certificate(
        &self,
        hostname: &str,
        fingerprint_sha256: &str,
    ) -> Result<(), EdgeRuntimeError> {
        verify_served_certificate(&self.config, hostname, fingerprint_sha256)
    }

    /// Async wrapper that runs the blocking TLS probe (up to
    /// `tls_verify_timeout_ms` per hostname) off the async executor so a
    /// slow or unreachable listener never stalls the agent loop.
    pub async fn verify_served_certificate_async(
        &self,
        hostname: &str,
        fingerprint_sha256: &str,
    ) -> Result<(), EdgeRuntimeError> {
        let config = self.config.clone();
        let hostname = hostname.to_owned();
        let fingerprint_sha256 = fingerprint_sha256.to_owned();
        tokio::task::spawn_blocking(move || {
            verify_served_certificate(&config, &hostname, &fingerprint_sha256)
        })
        .await
        .map_err(|error| {
            EdgeRuntimeError::Filesystem(format!("join served TLS verification: {error}"))
        })?
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn async_certificate_activation_has_no_waiter_queue() {
        let root = TempDir::new().expect("tempdir");
        let runtime = EdgeRuntime::new(EdgeRuntimeConfig {
            nginx_enabled: false,
            nginx_binary: "nginx".to_string(),
            nginx_main_config: PathBuf::from("nginx.conf"),
            nginx_sites_root: root.path().join("sites"),
            cert_live_root: root.path().join("certs"),
            site_family: "sdkwork".to_string(),
            nginx_command_timeout_ms: 10_000,
            tls_verify_address: "127.0.0.1:443".parse().unwrap(),
            tls_verify_timeout_ms: 5_000,
        });
        let permit = runtime
            .filesystem_activation_admission
            .clone()
            .try_acquire_owned()
            .expect("permit");
        let invalid = CertificateBundleMaterial {
            bundle_name: "cert-id".to_string(),
            fullchain_pem: "invalid".to_string(),
            private_key_pem: "invalid".to_string(),
        };
        let error = runtime
            .write_certificate_bundle_async(&invalid)
            .await
            .expect_err("capacity must fail closed");
        assert!(error.to_string().contains("capacity exhausted"));
        drop(permit);
        let error = runtime
            .write_certificate_bundle_async(&invalid)
            .await
            .expect_err("material validation must run after capacity recovers");
        assert!(error.to_string().contains("certificate PEM"));
    }
}
