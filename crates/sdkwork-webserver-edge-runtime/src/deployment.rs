use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};

use tempfile::{Builder, NamedTempFile, TempDir};

use crate::config::EdgeRuntimeConfig;
use crate::nginx;
use crate::paths;
use crate::{CertificateBundleMaterial, EdgeRuntimeError, EdgeRuntimeResult};

const MAX_DEPLOYMENT_ITEMS: usize = 2_048;
const MAX_DEPLOYMENT_BYTES: usize = 64 * 1024 * 1024;
const MAX_ROLLBACK_DIAGNOSTICS: usize = 8;

#[derive(Clone, Debug)]
pub struct NginxSiteConfigMaterial {
    pub domain: String,
    pub config_content: String,
}

#[derive(Clone, Copy, Debug)]
enum TargetKind {
    File,
    Directory,
}

struct StagedNginxConfig {
    target: PathBuf,
    staged: NamedTempFile,
}

struct StagedCertificateBundle {
    target: PathBuf,
    staged: TempDir,
}

struct ActivatedTarget {
    target: PathBuf,
    kind: TargetKind,
    backup: Option<(TempDir, PathBuf)>,
}

pub(crate) struct EdgeDeploymentActivation {
    nginx_root: PathBuf,
    certificate_root: PathBuf,
    activated: Vec<ActivatedTarget>,
    _nginx_lock: Option<File>,
    _certificate_lock: Option<File>,
    decided: bool,
}

impl EdgeDeploymentActivation {
    pub(crate) fn commit(mut self) -> EdgeRuntimeResult<()> {
        self.decided = true;
        for target in &mut self.activated {
            if let Some((backup, _)) = target.backup.take() {
                if let Err(error) = backup.close() {
                    tracing::warn!(
                        target = ?target.target.file_name(),
                        error_kind = ?error.kind(),
                        "failed to remove committed edge deployment backup"
                    );
                }
            }
        }
        Ok(())
    }

    pub(crate) fn rollback(mut self) -> EdgeRuntimeResult<()> {
        let result = self.rollback_inner();
        self.decided = true;
        result
    }

    fn rollback_inner(&mut self) -> EdgeRuntimeResult<()> {
        let mut failures = Vec::new();
        for target in self.activated.iter_mut().rev() {
            if let Err(error) = restore_target(target) {
                if failures.len() < MAX_ROLLBACK_DIAGNOSTICS {
                    failures.push(error.to_string());
                }
            }
        }
        if self._nginx_lock.is_some() {
            if let Err(error) = nginx::sync_directory(&self.nginx_root) {
                if failures.len() < MAX_ROLLBACK_DIAGNOSTICS {
                    failures.push(error.to_string());
                }
            }
        }
        if self._certificate_lock.is_some() {
            if let Err(error) = paths::sync_directory(&self.certificate_root) {
                if failures.len() < MAX_ROLLBACK_DIAGNOSTICS {
                    failures.push(error.to_string());
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(EdgeRuntimeError::Filesystem(format!(
                "edge deployment rollback failed: {}",
                failures.join("; ")
            )))
        }
    }
}

impl Drop for EdgeDeploymentActivation {
    fn drop(&mut self) {
        if !self.decided {
            if let Err(error) = self.rollback_inner() {
                tracing::error!(error = %error, "failed to roll back undecided edge deployment");
            }
        }
    }
}

pub(crate) fn activate_edge_deployment(
    config: &EdgeRuntimeConfig,
    nginx_configs: &[NginxSiteConfigMaterial],
    certificates: &[CertificateBundleMaterial],
) -> EdgeRuntimeResult<EdgeDeploymentActivation> {
    validate_deployment_bounds(nginx_configs, certificates)?;

    // Nginx-less nodes neither stage Nginx configuration nor take the Nginx
    // deployment lock: the Rust data plane owns request-path serving, so the
    // lock would only create unused directories and serialize against work
    // that is not happening. Certificate staging is unaffected because the
    // node daemon still verifies and reports certificate delivery.
    let nginx_lock = if nginx_configs.is_empty() || !config.nginx_enabled {
        None
    } else {
        Some(nginx::acquire_nginx_deployment_lock(config)?)
    };
    let certificate_lock = if certificates.is_empty() {
        None
    } else {
        std::fs::create_dir_all(&config.cert_live_root).map_err(|error| {
            EdgeRuntimeError::Filesystem(format!("create certificate deployment root: {error}"))
        })?;
        Some(paths::acquire_certificate_activation_lock(
            &config.cert_live_root,
        )?)
    };

    let staged_nginx = stage_nginx_configs(config, nginx_configs)?;
    let staged_certificates = stage_certificates(config, certificates)?;
    let mut activation = EdgeDeploymentActivation {
        nginx_root: config.nginx_sites_root.clone(),
        certificate_root: config.cert_live_root.clone(),
        activated: Vec::with_capacity(staged_nginx.len() + staged_certificates.len()),
        _nginx_lock: nginx_lock,
        _certificate_lock: certificate_lock,
        decided: false,
    };

    for staged in staged_nginx {
        activation.activate_staged_file(staged)?;
    }
    for staged in staged_certificates {
        activation.activate_staged_directory(staged)?;
    }
    if activation._nginx_lock.is_some() {
        nginx::sync_directory(&activation.nginx_root)?;
    }
    if activation._certificate_lock.is_some() {
        paths::sync_directory(&activation.certificate_root)?;
    }
    Ok(activation)
}

impl EdgeDeploymentActivation {
    fn activate_staged_file(&mut self, staged: StagedNginxConfig) -> EdgeRuntimeResult<()> {
        let target = staged.target;
        self.activated
            .push(backup_target(&target, TargetKind::File, ".nginx-backup-")?);
        staged.staged.persist(&target).map_err(|error| {
            EdgeRuntimeError::Filesystem(format!(
                "activate staged Nginx configuration: {}",
                error.error
            ))
        })?;
        Ok(())
    }

    fn activate_staged_directory(
        &mut self,
        staged: StagedCertificateBundle,
    ) -> EdgeRuntimeResult<()> {
        let target = staged.target;
        self.activated.push(backup_target(
            &target,
            TargetKind::Directory,
            ".certificate-backup-",
        )?);
        let staged_path = staged.staged.keep();
        std::fs::rename(&staged_path, &target).map_err(|error| {
            let _ = std::fs::remove_dir_all(&staged_path);
            EdgeRuntimeError::Filesystem(format!("activate staged certificate bundle: {error}"))
        })?;
        Ok(())
    }
}

fn validate_deployment_bounds(
    nginx_configs: &[NginxSiteConfigMaterial],
    certificates: &[CertificateBundleMaterial],
) -> EdgeRuntimeResult<()> {
    if nginx_configs.len() > MAX_DEPLOYMENT_ITEMS || certificates.len() > MAX_DEPLOYMENT_ITEMS {
        return Err(EdgeRuntimeError::Config(format!(
            "edge deployment exceeds {MAX_DEPLOYMENT_ITEMS} items per material class"
        )));
    }
    let mut total_bytes = 0usize;
    let mut domains = HashSet::with_capacity(nginx_configs.len());
    for material in nginx_configs {
        if !domains.insert(material.domain.to_ascii_lowercase()) {
            return Err(EdgeRuntimeError::Config(
                "edge deployment contains a duplicate Nginx domain".to_string(),
            ));
        }
        total_bytes = total_bytes
            .checked_add(material.domain.len())
            .and_then(|size| size.checked_add(material.config_content.len()))
            .ok_or_else(|| EdgeRuntimeError::Config("edge deployment size overflow".to_string()))?;
    }
    let mut certificate_names = HashSet::with_capacity(certificates.len());
    for material in certificates {
        if !certificate_names.insert(material.bundle_name.to_ascii_lowercase()) {
            return Err(EdgeRuntimeError::Config(
                "edge deployment contains a duplicate certificate name".to_string(),
            ));
        }
        total_bytes = total_bytes
            .checked_add(material.bundle_name.len())
            .and_then(|size| size.checked_add(material.fullchain_pem.len()))
            .and_then(|size| size.checked_add(material.private_key_pem.len()))
            .ok_or_else(|| EdgeRuntimeError::Config("edge deployment size overflow".to_string()))?;
    }
    if total_bytes > MAX_DEPLOYMENT_BYTES {
        return Err(EdgeRuntimeError::Config(format!(
            "edge deployment exceeds {MAX_DEPLOYMENT_BYTES} material bytes"
        )));
    }
    Ok(())
}

fn stage_nginx_configs(
    config: &EdgeRuntimeConfig,
    materials: &[NginxSiteConfigMaterial],
) -> EdgeRuntimeResult<Vec<StagedNginxConfig>> {
    if materials.is_empty() || nginx::nginx_disabled(config, "stage_nginx_configs") {
        return Ok(Vec::new());
    }
    materials
        .iter()
        .map(|material| {
            Ok(StagedNginxConfig {
                target: paths::nginx_site_path(config, &material.domain),
                staged: nginx::stage_nginx_config(
                    config,
                    &material.domain,
                    &material.config_content,
                )?,
            })
        })
        .collect()
}

fn stage_certificates(
    config: &EdgeRuntimeConfig,
    materials: &[CertificateBundleMaterial],
) -> EdgeRuntimeResult<Vec<StagedCertificateBundle>> {
    materials
        .iter()
        .map(|material| {
            paths::validate_certificate_name(&material.bundle_name)?;
            paths::validate_certificate_material(
                &material.fullchain_pem,
                &material.private_key_pem,
            )?;
            let staged = Builder::new()
                .prefix(".cert-stage-")
                .tempdir_in(&config.cert_live_root)
                .map_err(|error| {
                    EdgeRuntimeError::Filesystem(format!(
                        "stage certificate deployment bundle: {error}"
                    ))
                })?;
            paths::write_staged_bundle(&staged, material)?;
            paths::sync_directory(staged.path())?;
            Ok(StagedCertificateBundle {
                target: config.cert_live_root.join(&material.bundle_name),
                staged,
            })
        })
        .collect()
}

fn backup_target(
    target: &Path,
    kind: TargetKind,
    backup_prefix: &str,
) -> EdgeRuntimeResult<ActivatedTarget> {
    let backup = match std::fs::symlink_metadata(target) {
        Ok(metadata) => {
            let valid_type = match kind {
                TargetKind::File => metadata.is_file(),
                TargetKind::Directory => metadata.is_dir(),
            };
            if metadata.file_type().is_symlink() || !valid_type {
                return Err(EdgeRuntimeError::Filesystem(
                    "edge deployment target has an unsafe filesystem type".to_string(),
                ));
            }
            let parent = target.parent().ok_or_else(|| {
                EdgeRuntimeError::Filesystem(
                    "edge deployment target has no parent directory".to_string(),
                )
            })?;
            let holder = Builder::new()
                .prefix(backup_prefix)
                .tempdir_in(parent)
                .map_err(|error| {
                    EdgeRuntimeError::Filesystem(format!("create edge deployment backup: {error}"))
                })?;
            let previous = holder.path().join("previous");
            std::fs::rename(target, &previous).map_err(|error| {
                EdgeRuntimeError::Filesystem(format!("backup edge deployment target: {error}"))
            })?;
            Some((holder, previous))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(EdgeRuntimeError::Filesystem(format!(
                "inspect edge deployment target: {error}"
            )))
        }
    };
    Ok(ActivatedTarget {
        target: target.to_path_buf(),
        kind,
        backup,
    })
}

fn restore_target(target: &mut ActivatedTarget) -> EdgeRuntimeResult<()> {
    match std::fs::symlink_metadata(&target.target) {
        Ok(metadata) => {
            let valid_type = match target.kind {
                TargetKind::File => metadata.is_file(),
                TargetKind::Directory => metadata.is_dir(),
            };
            if metadata.file_type().is_symlink() || !valid_type {
                preserve_backup(target);
                return Err(EdgeRuntimeError::Filesystem(
                    "refuse to remove unsafe edge deployment target during rollback".to_string(),
                ));
            }
            let result = match target.kind {
                TargetKind::File => std::fs::remove_file(&target.target),
                TargetKind::Directory => std::fs::remove_dir_all(&target.target),
            };
            if let Err(error) = result {
                preserve_backup(target);
                return Err(EdgeRuntimeError::Filesystem(format!(
                    "remove uncommitted edge deployment target: {error}"
                )));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            preserve_backup(target);
            return Err(EdgeRuntimeError::Filesystem(format!(
                "inspect uncommitted edge deployment target: {error}"
            )));
        }
    }

    if let Some((holder, previous)) = target.backup.take() {
        if let Err(error) = std::fs::rename(&previous, &target.target) {
            let _retained = holder.keep();
            return Err(EdgeRuntimeError::Filesystem(format!(
                "restore previous edge deployment target: {error}"
            )));
        }
    }
    Ok(())
}

fn preserve_backup(target: &mut ActivatedTarget) {
    if let Some((holder, _)) = target.backup.take() {
        let _retained = holder.keep();
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use rcgen::{CertificateParams, DistinguishedName, KeyPair};

    use super::*;

    fn runtime_config(root: &Path) -> EdgeRuntimeConfig {
        EdgeRuntimeConfig {
            nginx_enabled: false,
            nginx_binary: "nginx".to_string(),
            nginx_main_config: root.join("nginx.conf"),
            nginx_sites_root: root.join("sites"),
            cert_live_root: root.join("certs"),
            site_family: "sdkwork".to_string(),
            nginx_command_timeout_ms: 10_000,
            tls_verify_address: "127.0.0.1:443".parse().unwrap(),
            tls_verify_timeout_ms: 5_000,
        }
    }

    fn certificate(name: &str, generation: &str) -> CertificateBundleMaterial {
        let mut params = CertificateParams::new(vec![format!("{generation}.localhost")]).unwrap();
        params.distinguished_name = DistinguishedName::new();
        let key = KeyPair::generate().unwrap();
        let certificate = params.self_signed(&key).unwrap();
        CertificateBundleMaterial {
            bundle_name: name.to_string(),
            fullchain_pem: certificate.pem(),
            private_key_pem: key.serialize_pem(),
        }
    }

    #[test]
    fn certificate_batch_rolls_back_every_target() {
        let root = TempDir::new().unwrap();
        let config = runtime_config(root.path());
        let original = certificate("primary", "original");
        paths::write_certificate_bundle(&config.cert_live_root, &original).unwrap();

        let replacement = certificate("primary", "replacement");
        let additional = certificate("additional", "additional");
        let pending =
            activate_edge_deployment(&config, &[], &[replacement.clone(), additional.clone()])
                .unwrap();
        assert_eq!(
            std::fs::read_to_string(config.cert_live_root.join("primary").join("fullchain.pem"))
                .unwrap(),
            replacement.fullchain_pem
        );
        pending.rollback().unwrap();

        assert_eq!(
            std::fs::read_to_string(config.cert_live_root.join("primary").join("fullchain.pem"))
                .unwrap(),
            original.fullchain_pem
        );
        assert!(!config.cert_live_root.join("additional").exists());
    }

    #[test]
    fn mixed_file_and_certificate_activation_rolls_back_as_one_unit() {
        let root = TempDir::new().unwrap();
        let config = runtime_config(root.path());
        std::fs::create_dir_all(&config.nginx_sites_root).unwrap();
        std::fs::create_dir_all(&config.cert_live_root).unwrap();
        let site_target = config.nginx_sites_root.join("example.com.conf");
        std::fs::write(&site_target, "old configuration").unwrap();
        let original = certificate("primary", "original");
        paths::write_certificate_bundle(&config.cert_live_root, &original).unwrap();

        let mut staged_site = NamedTempFile::new_in(&config.nginx_sites_root).unwrap();
        staged_site.write_all(b"new configuration").unwrap();
        staged_site.as_file().sync_all().unwrap();
        let replacement = certificate("primary", "replacement");
        let mut staged_certificates = stage_certificates(&config, &[replacement]).unwrap();
        let mut pending = EdgeDeploymentActivation {
            nginx_root: config.nginx_sites_root.clone(),
            certificate_root: config.cert_live_root.clone(),
            activated: Vec::new(),
            _nginx_lock: Some(nginx::acquire_nginx_deployment_lock(&config).unwrap()),
            _certificate_lock: Some(
                paths::acquire_certificate_activation_lock(&config.cert_live_root).unwrap(),
            ),
            decided: false,
        };
        pending
            .activate_staged_file(StagedNginxConfig {
                target: site_target.clone(),
                staged: staged_site,
            })
            .unwrap();
        pending
            .activate_staged_directory(staged_certificates.pop().unwrap())
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(&site_target).unwrap(),
            "new configuration"
        );

        pending.rollback().unwrap();
        assert_eq!(
            std::fs::read_to_string(&site_target).unwrap(),
            "old configuration"
        );
        assert_eq!(
            std::fs::read_to_string(config.cert_live_root.join("primary").join("fullchain.pem"))
                .unwrap(),
            original.fullchain_pem
        );
    }

    #[test]
    fn certificate_batch_commit_keeps_every_target_and_rejects_duplicates() {
        let root = TempDir::new().unwrap();
        let config = runtime_config(root.path());
        let primary = certificate("primary", "primary");
        let secondary = certificate("secondary", "secondary");
        activate_edge_deployment(&config, &[], &[primary.clone(), secondary.clone()])
            .unwrap()
            .commit()
            .unwrap();
        assert!(config.cert_live_root.join("primary").is_dir());
        assert!(config.cert_live_root.join("secondary").is_dir());

        let duplicate = CertificateBundleMaterial {
            bundle_name: "PRIMARY".to_string(),
            ..primary.clone()
        };
        assert!(activate_edge_deployment(&config, &[], &[primary, duplicate]).is_err());
    }
}
