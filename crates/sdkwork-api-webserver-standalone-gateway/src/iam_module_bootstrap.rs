use std::path::{Path, PathBuf};

const WEB_IAM_MODULE_MANIFEST: &str = "specs/iam.module.manifest.json";
const SKILLS_IAM_MODULE_MANIFEST: &str = "specs/iam.module.manifest.json";
const MCP_IAM_MODULE_MANIFEST: &str = "specs/iam.module.manifest.json";

/// Resolves consumer IAM module manifests that the standalone gateway must
/// materialize into the shared IAM catalog (web + skills + mcp).
///
/// Skills/MCP catalogs own `app_user` roleGrantExtensions for self-service
/// CRUD; without federation, dual-token console calls pass surface checks but
/// fail authorization (or appear broken when permissions are missing).
pub(crate) fn federated_iam_module_manifest_paths() -> Result<Vec<PathBuf>, String> {
    let mut manifests = Vec::with_capacity(3);
    manifests.push(web_iam_module_manifest_path()?);
    if let Some(path) = optional_module_manifest_path(
        "skills",
        &[
            env_app_root("SDKWORK_SKILLS_APP_ROOT"),
            Some(sibling_app_root("sdkwork-skills")),
            installed_iam_module_root("skills"),
        ],
        SKILLS_IAM_MODULE_MANIFEST,
    ) {
        manifests.push(path);
    }
    if let Some(path) = optional_module_manifest_path(
        "mcp",
        &[
            env_app_root("SDKWORK_MCP_APP_ROOT"),
            Some(sibling_app_root("sdkwork-mcp")),
            installed_iam_module_root("mcp"),
        ],
        MCP_IAM_MODULE_MANIFEST,
    ) {
        manifests.push(path);
    }
    Ok(manifests)
}

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

fn optional_module_manifest_path(
    module_id: &str,
    candidate_roots: &[Option<PathBuf>],
    relative_manifest: &str,
) -> Option<PathBuf> {
    for root in candidate_roots.iter().flatten() {
        let candidate = root.join(relative_manifest);
        if candidate.is_file() {
            return Some(candidate);
        }
        // Installed IAM catalog layout: .../iam/modules/<moduleId>/iam.module.manifest.json
        let installed = root.join("iam.module.manifest.json");
        if installed.is_file() {
            return Some(installed);
        }
    }
    tracing::warn!(
        module_id,
        "optional IAM module manifest was not found; console self-service permissions for this module will not be federated"
    );
    None
}

fn env_app_root(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn sibling_app_root(repo_name: &str) -> PathBuf {
    source_web_app_root().join("..").join(repo_name)
}

fn installed_iam_module_root(module_id: &str) -> Option<PathBuf> {
    // Prefer process share roots used by standalone/Docker layouts.
    let share_candidates = [
        std::env::var_os("SDKWORK_SHARE_ROOT").map(PathBuf::from),
        Some(PathBuf::from("/app/share/sdkwork")),
        Some(PathBuf::from("/usr/share/sdkwork")),
    ];
    for share in share_candidates.into_iter().flatten() {
        let module_dir = share
            .join("iam")
            .join("iam")
            .join("modules")
            .join(module_id);
        if module_dir.join("iam.module.manifest.json").is_file() {
            return Some(module_dir);
        }
    }
    None
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

    #[test]
    fn federated_manifests_include_skills_and_mcp_when_siblings_exist() {
        let manifests = federated_iam_module_manifest_paths().expect("resolve federated manifests");
        assert!(
            manifests
                .iter()
                .any(|path| path.ends_with("sdkwork-webserver/specs/iam.module.manifest.json")
                    || path.ends_with("specs/iam.module.manifest.json")),
            "web manifest must be present: {manifests:?}"
        );
        let skills = sibling_app_root("sdkwork-skills").join(SKILLS_IAM_MODULE_MANIFEST);
        let mcp = sibling_app_root("sdkwork-mcp").join(MCP_IAM_MODULE_MANIFEST);
        if skills.is_file() {
            assert!(
                manifests.iter().any(|path| path == &skills),
                "skills sibling manifest must be federated: {manifests:?}"
            );
        }
        if mcp.is_file() {
            assert!(
                manifests.iter().any(|path| path == &mcp),
                "mcp sibling manifest must be federated: {manifests:?}"
            );
        }
    }
}
