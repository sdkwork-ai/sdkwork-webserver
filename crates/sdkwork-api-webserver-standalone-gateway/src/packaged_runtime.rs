use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Component, Path, PathBuf},
};

const DEPLOYMENT_PROFILE_ENV: &str = "SDKWORK_DEPLOYMENT_PROFILE";
const WEB_DEPLOYMENT_PROFILE_ENV: &str = "SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE";
const PACKAGED_ROOTS: [(&str, &str); 5] = [
    ("SDKWORK_APP_ROOT", "."),
    ("SDKWORK_WEBSERVER_APP_ROOT", "."),
    ("SDKWORK_WEBSERVER_SERVER_APP_ROOT", "."),
    ("SDKWORK_IAM_APP_ROOT", "share/sdkwork/iam"),
    ("SDKWORK_DRIVE_APP_ROOT", "share/sdkwork/drive"),
];

pub fn configure_packaged_runtime_roots_from_env() -> Result<(), String> {
    let deployment_profile = env::var(WEB_DEPLOYMENT_PROFILE_ENV)
        .or_else(|_| env::var(DEPLOYMENT_PROFILE_ENV))
        .unwrap_or_else(|_| "standalone".to_owned());
    if !deployment_profile.trim().eq_ignore_ascii_case("standalone") {
        return Ok(());
    }

    let executable = env::current_exe()
        .map_err(|error| format!("could not resolve the standalone executable: {error}"))?;
    let Some(package_root) = package_root_from_executable(&executable)? else {
        return Ok(());
    };

    for (key, default_relative) in PACKAGED_ROOTS {
        let configured = env::var_os(key).unwrap_or_else(|| OsString::from(default_relative));
        let resolved = resolve_packaged_root(&package_root, &configured, key)?;
        validate_packaged_root(&resolved, key)?;
        env::set_var(key, resolved);
    }
    Ok(())
}

fn package_root_from_executable(executable: &Path) -> Result<Option<PathBuf>, String> {
    let binary_directory = executable
        .parent()
        .ok_or_else(|| "standalone executable has no parent directory".to_owned())?;
    if binary_directory.file_name() != Some(OsStr::new("bin")) {
        return Ok(None);
    }
    binary_directory
        .parent()
        .map(|root| Some(root.to_owned()))
        .ok_or_else(|| "packaged standalone bin directory has no package root".to_owned())
}

fn resolve_packaged_root(
    package_root: &Path,
    configured: &OsStr,
    key: &str,
) -> Result<PathBuf, String> {
    if configured.is_empty() {
        return Err(format!("{key} must not be empty"));
    }
    let configured = Path::new(configured);
    if configured.is_absolute() {
        return Ok(configured.to_owned());
    }
    if configured
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(format!(
            "relative {key} must stay inside the standalone package root"
        ));
    }
    Ok(package_root.join(configured))
}

fn validate_packaged_root(root: &Path, key: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| format!("{key} {} is not available: {error}", root.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "{key} {} must be a non-symlink directory",
            root.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_roots_resolve_from_the_parent_of_bin() {
        let executable = Path::new("/opt/sdkwork-web/bin/sdkwork-gateway");
        let package_root = package_root_from_executable(executable).unwrap().unwrap();
        assert_eq!(package_root, Path::new("/opt/sdkwork-web"));
        assert_eq!(
            resolve_packaged_root(
                &package_root,
                OsStr::new("share/sdkwork/iam"),
                "SDKWORK_IAM_APP_ROOT"
            )
            .unwrap(),
            Path::new("/opt/sdkwork-web/share/sdkwork/iam")
        );
    }

    #[test]
    fn source_tree_executables_do_not_infer_an_install_root() {
        assert_eq!(
            package_root_from_executable(Path::new(
                "/workspace/sdkwork-web/target/debug/sdkwork-gateway"
            ))
            .unwrap(),
            None
        );
    }

    #[test]
    fn packaged_roots_reject_parent_directory_escape() {
        let error = resolve_packaged_root(
            Path::new("/opt/sdkwork-web"),
            OsStr::new("../sdkwork-iam"),
            "SDKWORK_IAM_APP_ROOT",
        )
        .unwrap_err();
        assert!(error.contains("must stay inside"));
    }
}
