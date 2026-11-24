//! Canonical SDKWork Drive certificate-root resolution.
//!
//! The data plane's Drive certificate source and the certificate distributor
//! must agree on one root. This module owns that decision so the two processes
//! cannot drift apart:
//!
//! 1. An explicit `SDKWORK_WEBSERVER_DRIVE_CERTIFICATE_ROOT` wins.
//! 2. Otherwise the canonical shared Drive mount
//!    (`/opt/deploy/drive/certificates`) is used when the mount exists.
//! 3. `SDKWORK_WEBSERVER_DRIVE_CERTIFICATE_ENABLED=false` disables the source
//!    in both processes.
//!
//! Auto-detection makes the Drive authority the preferred certificate source in
//! a standard SDKWork deployment without requiring the operator to repeat the
//! path in both the gateway and the certificate worker.

use std::path::{Component, Path, PathBuf};

/// Explicit opt-in/opt-out for the Drive certificate source.
pub const DRIVE_CERTIFICATE_ENABLED_ENV: &str = "SDKWORK_WEBSERVER_DRIVE_CERTIFICATE_ENABLED";
/// Explicit Drive certificate root override.
pub const DRIVE_CERTIFICATE_ROOT_ENV: &str = "SDKWORK_WEBSERVER_DRIVE_CERTIFICATE_ROOT";
/// Host-shared SDKWork Drive mount (`SDKWORK_WEBSERVER_SPEC.md` §17.5).
pub const SHARED_DRIVE_ROOT: &str = "/opt/deploy/drive";
/// Certificate bundle subtree inside the shared Drive mount.
pub const DRIVE_CERTIFICATE_SUBDIRECTORY: &str = "certificates";

/// Whether the deployment explicitly disabled the Drive certificate source.
pub fn drive_certificate_source_disabled() -> bool {
    std::env::var(DRIVE_CERTIFICATE_ENABLED_ENV)
        .ok()
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
}

/// The Drive certificate root for this process, if the source is enabled.
///
/// Returns `None` when the source is disabled or neither an explicit root nor
/// the canonical shared mount exists.
pub fn resolve_drive_certificate_root() -> Option<PathBuf> {
    resolve_drive_certificate_root_with(
        Path::new(SHARED_DRIVE_ROOT),
        std::env::var(DRIVE_CERTIFICATE_ROOT_ENV).ok().as_deref(),
        drive_certificate_source_disabled(),
    )
}

/// Testable form of [`resolve_drive_certificate_root`].
pub fn resolve_drive_certificate_root_with(
    shared_drive_root: &Path,
    configured_root: Option<&str>,
    disabled: bool,
) -> Option<PathBuf> {
    if disabled {
        return None;
    }
    if let Some(configured) = configured_root
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(PathBuf::from(configured));
    }
    is_real_directory(shared_drive_root)
        .then(|| shared_drive_root.join(DRIVE_CERTIFICATE_SUBDIRECTORY))
}

/// Validate a configured Drive certificate path (root or cache).
///
/// The value may be relative (development profiles use `./target/...`), but it
/// must stay a bounded, control-free path and must never contain a parent
/// component. A `..` root would make the scanner or the cache writer operate on
/// a directory outside the configured authority.
pub fn validate_drive_certificate_path(key: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 4_096
        || value
            .bytes()
            .any(|byte| byte == 0 || byte.is_ascii_control())
    {
        return Err(format!("{key} must contain 1..4096 safe path bytes"));
    }
    let path = Path::new(value);
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(format!("{key} must not contain `..` path components"));
    }
    if path.has_root() && path.parent().is_none() {
        return Err(format!("{key} must not be a filesystem root"));
    }
    Ok(())
}

fn is_real_directory(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_root_wins_over_the_canonical_mount() {
        let directory = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            resolve_drive_certificate_root_with(
                directory.path(),
                Some("/srv/custom-drive-certificates"),
                false,
            ),
            Some(PathBuf::from("/srv/custom-drive-certificates"))
        );
    }

    #[test]
    fn the_canonical_mount_supplies_the_root_only_when_it_exists() {
        let directory = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            resolve_drive_certificate_root_with(directory.path(), None, false),
            Some(directory.path().join("certificates"))
        );

        let missing = directory.path().join("missing");
        assert_eq!(
            resolve_drive_certificate_root_with(&missing, None, false),
            None
        );
    }

    #[test]
    fn an_explicit_disable_wins_over_every_root() {
        let directory = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            resolve_drive_certificate_root_with(
                directory.path(),
                Some("/srv/custom-drive-certificates"),
                true,
            ),
            None
        );
    }

    #[test]
    fn configured_paths_reject_parent_traversal_and_unsafe_bytes() {
        assert!(validate_drive_certificate_path("ROOT", "/opt/deploy/drive/certificates").is_ok());
        assert!(validate_drive_certificate_path("ROOT", "./target/drive-certificates").is_ok());
        for value in [
            "/opt/deploy/drive/../escape",
            "../escape",
            "..",
            "",
            "/opt/deploy/drive/certificates\n",
            "/",
        ] {
            assert!(
                validate_drive_certificate_path("ROOT", value).is_err(),
                "{value:?} must be rejected"
            );
        }
    }
}
