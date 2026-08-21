//! Canonical Web Server config file discovery.
//!
//! Resolution order (per `ENVIRONMENT_SPEC.md` section 8 and
//! `RUNTIME_DIRECTORY_SPEC.md` section 4):
//!
//! 1. Explicit command-line argument.
//! 2. `SDKWORK_WEBSERVER_SERVER_CONFIG_FILE` environment variable.
//! 3. Canonical OS system-scope directory for application code `webserver`
//!    plus `sdkwork.webserver.config.json`: Linux `/etc/sdkwork/webserver`,
//!    macOS `/Library/Application Support/sdkwork/webserver`, Windows
//!    `%ProgramData%\sdkwork\webserver`.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use crate::config_paths::{DATA_PLANE_CONFIG_FILE_ENV, DATA_PLANE_CONFIG_FILE_NAME};

/// Canonical Web Server data-plane config file name inside the application
/// config directory (`specs/sdkwork.webserver.config.schema.json` is the schema
/// authority for this format).
pub const WEBSERVER_CONFIG_FILE_NAME: &str = DATA_PLANE_CONFIG_FILE_NAME;

/// Canonical data-plane config override variable.
pub const WEBSERVER_CONFIG_FILE_ENV: &str = DATA_PLANE_CONFIG_FILE_ENV;

/// Resolve the Web Server config file path for an entry point.
///
/// `argument` is the optional positional config argument from the command
/// line. See the module docs for the resolution order.
pub fn resolve_webserver_config_path(argument: Option<String>) -> Result<PathBuf, String> {
    let default_directory = canonical_webserver_config_directory()?;
    resolve_webserver_config_path_with_default(argument, &default_directory)
}

/// Resolution core with an injected default directory so tests can use
/// disposable directories instead of host system paths.
fn resolve_webserver_config_path_with_default(
    argument: Option<String>,
    default_directory: &Path,
) -> Result<PathBuf, String> {
    if let Some(path) = argument {
        if path.trim().is_empty() {
            return Err("the config argument must not be empty".to_owned());
        }
        return Ok(PathBuf::from(path));
    }
    if let Ok(path) = env::var(WEBSERVER_CONFIG_FILE_ENV) {
        if path.trim().is_empty() {
            return Err(format!("{WEBSERVER_CONFIG_FILE_ENV} must not be empty"));
        }
        return Ok(PathBuf::from(path));
    }
    let default_path = default_directory.join(WEBSERVER_CONFIG_FILE_NAME);
    match fs::metadata(&default_path) {
        Ok(metadata) if metadata.is_file() => Ok(default_path),
        Ok(_) => Err(format!(
            "no Web Server config file at {}; the path is not a regular file",
            default_path.display()
        )),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Err(format!(
            "no Web Server config file found at {}; pass a config argument, set {WEBSERVER_CONFIG_FILE_ENV}, or place the config at the canonical location",
            default_path.display()
        )),
        Err(source) => Err(format!(
            "Web Server config at {} is not accessible: {source}; pass a config argument or set {WEBSERVER_CONFIG_FILE_ENV}",
            default_path.display()
        )),
    }
}

/// Canonical OS system-scope config directory for the Web Server application
/// code, following the host operating system.
pub use crate::config_paths::canonical_webserver_config_directory;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_env::env_test_lock;

    fn with_env(key: &str, value: Option<&str>, test: impl FnOnce()) {
        let previous = std::env::var(key).ok();
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
        test();
        match previous {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }

    fn default_directory() -> PathBuf {
        PathBuf::from("/etc/sdkwork/webserver")
    }

    #[test]
    fn explicit_argument_wins_over_environment() {
        let _guard = env_test_lock();
        with_env(WEBSERVER_CONFIG_FILE_ENV, Some("from-env.json"), || {
            let resolved = resolve_webserver_config_path_with_default(
                Some("from-argument.json".to_owned()),
                &default_directory(),
            )
            .expect("argument must resolve");
            assert_eq!(resolved, PathBuf::from("from-argument.json"));
        });
    }

    #[test]
    fn environment_override_wins_over_canonical_default() {
        let _guard = env_test_lock();
        with_env(
            WEBSERVER_CONFIG_FILE_ENV,
            Some("/custom/sdkwork.webserver.config.json"),
            || {
                let resolved =
                    resolve_webserver_config_path_with_default(None, &default_directory())
                        .expect("environment must resolve");
                assert_eq!(
                    resolved,
                    PathBuf::from("/custom/sdkwork.webserver.config.json")
                );
            },
        );
    }

    #[test]
    fn empty_environment_override_is_rejected() {
        let _guard = env_test_lock();
        with_env(WEBSERVER_CONFIG_FILE_ENV, Some("  "), || {
            let error = resolve_webserver_config_path_with_default(None, &default_directory())
                .expect_err("empty override must fail");
            assert!(error.contains(WEBSERVER_CONFIG_FILE_ENV));
            assert!(error.contains("must not be empty"));
        });
    }

    #[test]
    fn empty_argument_is_rejected() {
        let _guard = env_test_lock();
        let error =
            resolve_webserver_config_path_with_default(Some("  ".to_owned()), &default_directory())
                .expect_err("empty argument must fail");
        assert!(error.contains("argument must not be empty"));
    }

    #[test]
    fn default_directory_present_but_not_a_file_is_rejected() {
        let _guard = env_test_lock();
        let directory = tempfile::tempdir().expect("temp dir");
        // The default path exists as a directory, not a regular file.
        std::fs::create_dir(directory.path().join(WEBSERVER_CONFIG_FILE_NAME))
            .expect("create directory");
        with_env(WEBSERVER_CONFIG_FILE_ENV, None, || {
            let error = resolve_webserver_config_path_with_default(None, directory.path())
                .expect_err("non-file default must fail");
            assert!(error.contains("not a regular file"));
        });
    }

    #[test]
    fn canonical_default_is_used_when_present() {
        let _guard = env_test_lock();
        let directory = tempfile::tempdir().expect("temp dir");
        std::fs::write(directory.path().join(WEBSERVER_CONFIG_FILE_NAME), b"{}")
            .expect("write config");
        with_env(WEBSERVER_CONFIG_FILE_ENV, None, || {
            let resolved = resolve_webserver_config_path_with_default(None, directory.path())
                .expect("canonical default must resolve");
            assert_eq!(resolved, directory.path().join(WEBSERVER_CONFIG_FILE_NAME));
        });
    }

    #[test]
    fn missing_canonical_default_fails_closed_with_path_and_override() {
        let _guard = env_test_lock();
        let directory = tempfile::tempdir().expect("temp dir");
        with_env(WEBSERVER_CONFIG_FILE_ENV, None, || {
            let error = resolve_webserver_config_path_with_default(None, directory.path())
                .expect_err("missing default must fail closed");
            let expected = directory.path().join(WEBSERVER_CONFIG_FILE_NAME);
            assert!(error.contains(&expected.display().to_string()));
            assert!(error.contains(WEBSERVER_CONFIG_FILE_ENV));
        });
    }

    #[test]
    fn canonical_directory_uses_webserver_application_code() {
        let directory = canonical_webserver_config_directory().expect("canonical directory");
        assert_eq!(
            directory
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            Some("webserver".to_owned())
        );
    }
}
