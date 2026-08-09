use std::path::{Path, PathBuf};

const WEB_IAM_MODULE_MANIFEST: &str = "specs/iam.module.manifest.json";

pub(crate) fn web_iam_module_manifest_path() -> Result<PathBuf, String> {
    let app_root = std::env::var_os("SDKWORK_APP_ROOT")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(source_web_app_root);
    let manifest_path = app_root.join(WEB_IAM_MODULE_MANIFEST);
    if !manifest_path.is_file() {
        return Err(format!(
            "Web IAM module manifest is missing: {}",
            manifest_path.display()
        ));
    }
    Ok(manifest_path)
}

fn source_web_app_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_web_manifest_grants_every_console_permission_to_app_user() {
        let manifest_path = source_web_app_root().join(WEB_IAM_MODULE_MANIFEST);
        let manifest = std::fs::read_to_string(&manifest_path).expect("read Web IMF manifest");
        let manifest: serde_json::Value =
            serde_json::from_str(&manifest).expect("parse Web IMF manifest");
        let app_user_patterns = manifest["roles"]["roleGrantExtensions"]
            .as_array()
            .and_then(|extensions| {
                extensions
                    .iter()
                    .find(|extension| extension["roleCode"] == "app_user")
            })
            .and_then(|extension| extension["patterns"].as_array())
            .expect("app_user role grant extension");

        for expected in ["web.applications.*", "web.certificates.*"] {
            assert!(
                app_user_patterns.iter().any(|pattern| pattern == expected),
                "app_user must receive {expected}"
            );
        }
        for templates in [
            "organizationTemplates",
            "departmentTemplates",
            "positionTemplates",
            "membershipTemplates",
        ] {
            assert!(
                manifest["directory"][templates].is_array(),
                "Web IMF directory.{templates} must be present"
            );
        }
        assert!(
            manifest["dependencies"]["requiresModules"]
                .as_array()
                .is_some_and(|modules| modules.iter().any(|module| module == "iam-kernel")),
            "Web IMF must depend on iam-kernel"
        );
        assert!(
            app_user_patterns.iter().all(|pattern| {
                pattern.as_str().is_some_and(|pattern| {
                    !pattern.starts_with("web.nginx")
                        && !pattern.starts_with("web.servers")
                        && !pattern.starts_with("web.auditLogs")
                })
            }),
            "app_user must not receive backend-admin permissions"
        );
    }
}
