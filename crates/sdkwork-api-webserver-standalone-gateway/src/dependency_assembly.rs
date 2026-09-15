use std::path::{Path, PathBuf};

use sdkwork_database_sqlx::process_shared_database_pool;
use sdkwork_web_bootstrap::ApiAssemblyContribution;

use crate::profile::StandaloneProfileError;

/// Owner id reported when the drive admin storage surface cannot be composed.
const DRIVE_ADMIN_STORAGE_OWNER: &str = "sdkwork-drive";

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
/// validation, so this hook contributes neither.
///
/// The drive **admin storage** backend surface is the one dependency that does
/// belong here:
///
/// - `sdkwork-api-drive-assembly` sits outside the webserver assembly crate
///   graph, so the webserver assembly cannot mount it;
/// - it owns a route inventory disjoint from the drive App API that
///   `profile.rs` already composes — storage serves
///   `/backend/v3/api/drive/storage/*` while the App API serves
///   `/app/v3/api/drive/*` — so composing both cannot collide;
/// - its routes are dual-token with no explicit permission of their own, so it
///   contributes nothing to this edge's permission catalog and cannot drift
///   against the Web module's IAM catalog.
///
/// It is bootstrapped on the gateway's process-shared PostgreSQL pool, the same
/// pool handed to IAM and the web store, so this edge keeps exactly one
/// canonical server pool and one drive schema bootstrap.
pub(crate) async fn optional_same_origin_dependency_contributions(
) -> Result<Vec<ApiAssemblyContribution>, StandaloneProfileError> {
    let _ = (
        sibling_application_available("sdkwork-skills"),
        sibling_application_available("sdkwork-mcp"),
    );
    Ok(vec![assemble_drive_admin_storage_contribution().await?])
}

/// Composes the drive admin storage surface on the process-shared pool.
///
/// Uses the host-framework variant of the drive router so the request context
/// comes from this gateway's single Web Framework layer instead of a
/// drive-owned domain injector (API_ASSEMBLY_SPEC §3/§6.1).
async fn assemble_drive_admin_storage_contribution(
) -> Result<ApiAssemblyContribution, StandaloneProfileError> {
    let pool = process_shared_database_pool().ok_or_else(missing_process_pool_error)?;
    sdkwork_api_drive_assembly::assemble_backend_admin_storage_contribution_with_pool(&pool)
        .await
        .map_err(|detail| {
            StandaloneProfileError::assembly_unavailable(DRIVE_ADMIN_STORAGE_OWNER, detail)
        })
}

/// Fails closed when the database lifecycle has not installed the pool yet.
///
/// Serving the Storage Center without its route inventory would otherwise show
/// up as an opaque 404 on an authenticated operator action, so the missing
/// prerequisite is reported as the typed 50301 dependency-unavailable error.
fn missing_process_pool_error() -> StandaloneProfileError {
    StandaloneProfileError::assembly_unavailable(
        DRIVE_ADMIN_STORAGE_OWNER,
        "the process-shared PostgreSQL pool is unavailable; the standalone gateway must bootstrap its database lifecycle before composing the drive admin storage surface",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sibling_application_availability_checks_app_manifest() {
        assert!(sibling_application_available("sdkwork-skills"));
        assert!(sibling_application_available("sdkwork-mcp"));
    }

    #[test]
    fn missing_process_pool_fails_closed_as_dependency_unavailable() {
        match missing_process_pool_error() {
            StandaloneProfileError::AssemblyUnavailable { owner, code, detail } => {
                assert_eq!(owner, "sdkwork-drive");
                assert_eq!(code, 50301);
                assert!(
                    detail.contains("process-shared PostgreSQL pool"),
                    "operator must learn which prerequisite is missing: {detail}"
                );
            }
            other => panic!("drive admin storage composition must fail closed; got {other:?}"),
        }
    }
}
