use std::path::{Path, PathBuf};

const WEB_IAM_MODULE_MANIFEST: &str = "specs/iam.module.manifest.json";
const SKILLS_IAM_MODULE_MANIFEST: &str = "specs/iam.module.manifest.json";
const MCP_IAM_MODULE_MANIFEST: &str = "specs/iam.module.manifest.json";

/// Resolves consumer IAM module manifests that the standalone gateway must
/// materialize into the shared IAM catalog.
///
/// Web always federates from `specs/iam.module.manifest.json` (not an IAM
/// registry enabled module). Skills/MCP are federated only when a consumer app
/// root exposes its own `specs/iam.module.manifest.json` *and* that module is
/// not already listed in the packaged IAM registry `enabledModules`. Falling
/// back to `iam/modules/{skills,mcp}` duplicates `moduleId` and crashes
/// bootstrap with `additional module manifest duplicates moduleId skills`.
pub(crate) fn federated_iam_module_manifest_paths() -> Result<Vec<PathBuf>, String> {
    let enabled = iam_registry_enabled_modules();
    let mut manifests = Vec::with_capacity(3);
    manifests.push(web_iam_module_manifest_path()?);
    if !enabled.contains("skills") {
        if let Some(path) = optional_module_manifest_path(
            "skills",
            &[
                env_app_root("SDKWORK_SKILLS_APP_ROOT"),
                Some(sibling_app_root("sdkwork-skills")),
            ],
            SKILLS_IAM_MODULE_MANIFEST,
        ) {
            manifests.push(path);
        }
    }
    if !enabled.contains("mcp") {
        if let Some(path) = optional_module_manifest_path(
            "mcp",
            &[
                env_app_root("SDKWORK_MCP_APP_ROOT"),
                Some(sibling_app_root("sdkwork-mcp")),
            ],
            MCP_IAM_MODULE_MANIFEST,
        ) {
            manifests.push(path);
        }
    }
    Ok(manifests)
}

fn iam_registry_enabled_modules() -> std::collections::BTreeSet<String> {
    let mut enabled = std::collections::BTreeSet::new();
    let candidates = [
        env_app_root("SDKWORK_IAM_APP_ROOT").map(|root| root.join("iam/registry/iam-registry.config.json")),
        Some(PathBuf::from("/app/share/sdkwork/iam/iam/registry/iam-registry.config.json")),
        Some(PathBuf::from("/usr/share/sdkwork/iam/iam/registry/iam-registry.config.json")),
        Some(sibling_app_root("sdkwork-iam").join("iam/registry/iam-registry.config.json")),
    ];
    for path in candidates.into_iter().flatten() {
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        if let Some(items) = value.get("enabledModules").and_then(|entry| entry.as_array()) {
            for item in items {
                if let Some(module_id) = item.as_str() {
                    enabled.insert(module_id.to_string());
                }
            }
            break;
        }
    }
    enabled
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
    fn federated_manifests_skip_skills_and_mcp_when_registry_enables_them() {
        let manifests = federated_iam_module_manifest_paths().expect("resolve federated manifests");
        assert!(
            !manifests.is_empty(),
            "at least the web IAM manifest must resolve"
        );
        assert!(
            manifests[0].ends_with("iam.module.manifest.json"),
            "web manifest must be first: {manifests:?}"
        );
        let enabled = iam_registry_enabled_modules();
        if enabled.contains("skills") {
            let skills = sibling_app_root("sdkwork-skills").join(SKILLS_IAM_MODULE_MANIFEST);
            assert!(
                manifests.iter().all(|path| path != &skills),
                "skills must not be federated when already enabled in IAM registry: {manifests:?}"
            );
        }
        if enabled.contains("mcp") {
            let mcp = sibling_app_root("sdkwork-mcp").join(MCP_IAM_MODULE_MANIFEST);
            assert!(
                manifests.iter().all(|path| path != &mcp),
                "mcp must not be federated when already enabled in IAM registry: {manifests:?}"
            );
        }
    }
}
