//! Canonical installed configuration paths for the Web Server application.
//!
//! Authority: `APPLICATION_DEPLOY_LAYOUT_SPEC.md`, `RUNTIME_DIRECTORY_SPEC.md` section 4.
//!
//! ## Layout
//!
//! Every installed deployment uses one application config root
//! (`<config-root>`) containing:
//!
//! | File | Purpose |
//! | --- | --- |
//! | `config.toml` | Process runtime: profile, ingress, database, secrets refs, module imports |
//! | `sdkwork.webserver.config.json` | Data-plane listeners, routes, upstreams, TLS policy |
//! | `secrets/*` | Protected secret files referenced from TOML/JSON (never embedded) |
//!
//! ## Platform defaults
//!
//! | Platform | `<config-root>` |
//! | --- | --- |
//! | Linux (Ubuntu, Debian, RHEL, container) | `/etc/sdkwork/webserver` |
//! | macOS (launchd / service) | `/Library/Application Support/sdkwork/webserver` |
//! | Windows (service) | `%ProgramData%\sdkwork\webserver` |
//! | Development (no install) | Env vars or repo `etc/` examples; canonical file optional |

use std::path::{Path, PathBuf};

/// Runtime directory application code (`specs/topology.spec.json`).
pub const APPLICATION_CODE: &str = "webserver";

/// Process runtime TOML file name inside `<config-root>`.
pub const RUNTIME_CONFIG_FILE_NAME: &str = "config.toml";

/// Override for the runtime TOML path (`RUNTIME_DIRECTORY_SPEC.md` section 4.1).
pub const RUNTIME_CONFIG_FILE_ENV: &str = "SDKWORK_WEBSERVER_CONFIG_FILE";

/// Deprecated name retained only so fail-closed diagnostics can name the retired key.
pub const RUNTIME_CONFIG_FILE_ENV_LEGACY: &str = "SDKWORK_WEBSERVER_RUNTIME_CONFIG_FILE";

/// Data-plane JSON file name inside `<config-root>`.
pub const DATA_PLANE_CONFIG_FILE_NAME: &str = "sdkwork.webserver.config.json";

/// Override for the data-plane JSON path (`ENVIRONMENT_SPEC.md` section 8).
pub const DATA_PLANE_CONFIG_FILE_ENV: &str = "SDKWORK_WEBSERVER_SERVER_CONFIG_FILE";

/// Secret files subdirectory under `<config-root>`.
pub const SECRETS_SUBDIR: &str = "secrets";

/// System-scope certificate inventory root (`/etc/sdkwork/certs`).
/// Each domain owns a directory: `/etc/sdkwork/certs/<domain>/` containing
/// `cert.pem` and `key.pem` (plus optional `chain.pem`). The ACME worker
/// and operators share this layout; configs reference entries with the
/// `certs://<domain>/` URI form.
pub const CERTIFICATES_SUBDIR: &str = "certs";

/// Leaf certificate file inside a domain certificate directory.
pub const CERTIFICATE_FILE_NAME: &str = "cert.pem";

/// Private key file inside a domain certificate directory.
pub const PRIVATE_KEY_FILE_NAME: &str = "key.pem";

/// Optional issuer chain file inside a domain certificate directory.
pub const CERTIFICATE_CHAIN_FILE_NAME: &str = "chain.pem";

/// URI scheme for `certs://<domain>/…` certificate references.
pub const CERTS_URI_SCHEME: &str = "certs://";

/// Linux system-scope config root (documented operator default).
pub const LINUX_CONFIG_ROOT: &str = "/etc/sdkwork/webserver";

/// Canonical OS system-scope SDKWork base directory (`sdkwork/<application-code>`).
pub fn canonical_os_system_scope_sdkwork_base() -> Result<PathBuf, String> {
    #[cfg(target_os = "linux")]
    {
        Ok(PathBuf::from("/etc/sdkwork"))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(PathBuf::from("/Library/Application Support/sdkwork"))
    }
    #[cfg(target_os = "windows")]
    {
        let program_data =
            std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".to_owned());
        Ok(PathBuf::from(program_data).join("sdkwork"))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err(format!(
            "Web Server config discovery is not supported on this operating system; set {RUNTIME_CONFIG_FILE_ENV} or {DATA_PLANE_CONFIG_FILE_ENV}"
        ))
    }
}

/// Canonical OS system-scope certificate inventory root
/// (`/etc/sdkwork/certs` on Linux, equivalent on macOS/Windows).
/// `SDKWORK_CERTS_DIR` overrides the root for containers and tests.
pub fn canonical_certificates_directory() -> Result<PathBuf, String> {
    if let Ok(override_dir) = std::env::var("SDKWORK_CERTS_DIR") {
        let override_dir = override_dir.trim();
        if !override_dir.is_empty() {
            return Ok(PathBuf::from(override_dir));
        }
    }
    Ok(canonical_os_system_scope_sdkwork_base()?.join(CERTIFICATES_SUBDIR))
}

/// Canonical certificate directory for one domain:
/// `/etc/sdkwork/certs/<domain>/`.
pub fn canonical_certificate_domain_directory(domain: &str) -> Result<PathBuf, String> {
    let domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();
    if domain.is_empty()
        || domain.len() > 253
        || domain.chars().any(|character| {
            !(character.is_ascii_alphanumeric() || matches!(character, '-' | '.' | '_'))
        })
    {
        return Err(format!("invalid certificate domain `{domain}`"));
    }
    Ok(canonical_certificates_directory()?.join(domain))
}

/// Canonical leaf certificate path for a domain.
pub fn canonical_certificate_file(domain: &str) -> Result<PathBuf, String> {
    Ok(canonical_certificate_domain_directory(domain)?.join(CERTIFICATE_FILE_NAME))
}

/// Canonical private key path for a domain.
pub fn canonical_certificate_key_file(domain: &str) -> Result<PathBuf, String> {
    Ok(canonical_certificate_domain_directory(domain)?.join(PRIVATE_KEY_FILE_NAME))
}

/// Canonical OS system-scope config directory for application code `webserver`.
pub fn canonical_webserver_config_directory() -> Result<PathBuf, String> {
    Ok(canonical_os_system_scope_sdkwork_base()?.join(APPLICATION_CODE))
}

/// Canonical runtime TOML path for the current host OS.
pub fn canonical_runtime_config_path() -> Result<PathBuf, String> {
    Ok(canonical_webserver_config_directory()?.join(RUNTIME_CONFIG_FILE_NAME))
}

/// Canonical data-plane JSON path for the current host OS.
pub fn canonical_data_plane_config_path() -> Result<PathBuf, String> {
    Ok(canonical_webserver_config_directory()?.join(DATA_PLANE_CONFIG_FILE_NAME))
}

/// Secrets directory under the canonical config root.
pub fn canonical_secrets_directory() -> Result<PathBuf, String> {
    Ok(canonical_webserver_config_directory()?.join(SECRETS_SUBDIR))
}

/// Resolve an explicit runtime config override from the environment.
///
/// Only [`RUNTIME_CONFIG_FILE_ENV`] is accepted. The retired
/// [`RUNTIME_CONFIG_FILE_ENV_LEGACY`] alias fails closed with a migration
/// diagnostic (dual-read is forbidden).
pub fn runtime_config_override_from_env() -> Result<Option<PathBuf>, String> {
    if let Ok(value) = std::env::var(RUNTIME_CONFIG_FILE_ENV_LEGACY) {
        if !value.trim().is_empty() {
            return Err(format!(
                "{RUNTIME_CONFIG_FILE_ENV_LEGACY} is retired; set {RUNTIME_CONFIG_FILE_ENV} instead"
            ));
        }
    }
    if let Ok(value) = std::env::var(RUNTIME_CONFIG_FILE_ENV) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(Some(PathBuf::from(trimmed)));
        }
    }
    Ok(None)
}

/// Join a subpath under an injected config root (for tests and renderers).
pub fn config_root_join(config_root: &Path, file_name: &str) -> PathBuf {
    config_root.join(file_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_config_root_matches_operator_default() {
        let directory = canonical_webserver_config_directory().expect("directory");
        if cfg!(target_os = "linux") {
            assert_eq!(directory, PathBuf::from(LINUX_CONFIG_ROOT));
        }
        assert_eq!(
            directory
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            Some(APPLICATION_CODE.to_owned())
        );
    }

    #[test]
    fn canonical_runtime_config_uses_config_toml() {
        let path = canonical_runtime_config_path().expect("runtime path");
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some(RUNTIME_CONFIG_FILE_NAME)
        );
    }

    #[test]
    fn canonical_data_plane_config_uses_json_schema_name() {
        let path = canonical_data_plane_config_path().expect("data-plane path");
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some(DATA_PLANE_CONFIG_FILE_NAME)
        );
    }
}
