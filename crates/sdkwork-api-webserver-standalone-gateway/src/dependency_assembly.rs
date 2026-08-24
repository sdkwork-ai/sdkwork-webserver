use std::path::{Path, PathBuf};

use sdkwork_web_bootstrap::ApiAssemblyContribution;

use crate::profile::StandaloneProfileError;

fn source_web_app_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sibling_app_root(repo_name: &str) -> PathBuf {
    std::env::var_os("SDKWORK_APP_ROOT")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(source_web_app_root)
        .join("..")
        .join(repo_name)
}

/// Returns true when a sibling SDKWork application root is present on disk.
pub(crate) fn sibling_application_available(repo_name: &str) -> bool {
    sibling_app_root(repo_name)
        .join("sdkwork.app.config.json")
        .is_file()
}

/// Optional same-origin dependency contributions that are **not** already
/// merged into `sdkwork-api-webserver-assembly`.
///
/// Skills and MCP app/backend surfaces are owned by the webserver assembly
/// (`assemble_api_router` → `merge_same_origin_dependency_contribution`).
/// Re-adding them here when sibling checkouts exist would duplicate route
/// paths and fail `ComposedApiAssembly::try_compose` / OpenAPI inventory
/// validation. Keep this hook for future topology-declared dependencies that
/// remain outside the webserver assembly crate graph.
pub(crate) async fn optional_same_origin_dependency_contributions(
) -> Result<Vec<ApiAssemblyContribution>, StandaloneProfileError> {
    let _ = (
        sibling_application_available("sdkwork-skills"),
        sibling_application_available("sdkwork-mcp"),
    );
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sibling_application_availability_checks_app_manifest() {
        assert!(sibling_application_available("sdkwork-skills"));
        assert!(sibling_application_available("sdkwork-mcp"));
    }

    #[tokio::test]
    async fn does_not_duplicate_skills_or_mcp_already_owned_by_webserver_assembly() {
        let contributions = optional_same_origin_dependency_contributions()
            .await
            .expect("optional contributions");
        assert!(
            contributions.is_empty(),
            "skills/mcp must stay assembly-owned; got {} extra contribution(s)",
            contributions.len()
        );
    }
}
