//! Same-origin dependency API modules composed by the standalone gateway.
//!
//! API_ASSEMBLY_SPEC §6.1: the standalone gateway is the only application-plane
//! HTTP listener, and it composes every dependency it declares as same-origin by
//! calling that dependency's **own** assembly entrypoint. The same section
//! forbids the host from reclassifying dependency ownership, so a dependency's
//! surfaces are never merged into the web server's own contribution: each
//! dependency arrives here as one [`WebModule`] whose owner stays the dependency
//! (API_ASSEMBLY_SPEC §4.1.1).
//!
//! Every module is built on the gateway's process-shared PostgreSQL pool — the
//! same pool handed to IAM and the web store — so this edge keeps exactly one
//! canonical server pool and one schema bootstrap per dependency. A dependency
//! that cannot initialize fails startup with the typed 50301 error instead of
//! leaving a partial route surface that returns 404 for a dependency the
//! component contract declares as served (§6.1).

use sdkwork_database_sqlx::{process_shared_database_pool, DatabasePool};
use sdkwork_web_bootstrap::WebModule;

use crate::profile::StandaloneProfileError;

/// Owner ids reported when a dependency module cannot be composed. They are the
/// dependency's api-assembly owner, never the hosting application.
const IAM_OWNER: &str = "sdkwork-iam";
const DRIVE_OWNER: &str = "sdkwork-drive";
const SKILLS_OWNER: &str = "sdkwork-skills";
const MCP_OWNER: &str = "sdkwork-mcp";
const DEPLOYMENTS_OWNER: &str = "sdkwork-deployments";

/// Every same-origin dependency module this edge serves — one module per owner.
///
/// Order is the composition order of the composed manifest and OpenAPI
/// inventory. Each module is complete: it carries all of its owner's surfaces,
/// its own route manifest, OpenAPI document, permission catalog, domain context
/// injectors, and readiness check.
pub(crate) async fn same_origin_dependency_modules(
) -> Result<Vec<WebModule>, StandaloneProfileError> {
    Ok(vec![
        iam_module().await?,
        drive_module().await?,
        skills_module().await?,
        mcp_module().await?,
        deployments_module().await?,
    ])
}

/// IAM App API (`/app/v3/api/iam/*`), the Backend API cloud account center
/// (`/backend/v3/api/iam/provider_accounts`, `/iam/provider_credentials/*`), and
/// the IAM Open API surface — every IAM-owned surface, as one module.
///
/// IAM ships its surfaces as one already-composed contribution for the same
/// reason drive does. Fetching the App API surface and the Backend API surface
/// from two IAM entrypoints would either fail composition (two contributions,
/// one owner) or — through `ApiModuleRegistry`'s duplicate-owner tolerance —
/// drop the second surface. That failure mode is silent at startup and reaches
/// the operator as a 404 on a surface this gateway's component contract
/// declares as served (§6.1); here it would strand the admin console's cloud
/// account center, because the account center lives on the backend surface.
///
/// `federated_iam_module_manifest_paths` is resolved here rather than by the
/// caller so the module is complete on its own: the entrypoint materializes the
/// consumer-owned IAM catalog (web, skills, mcp) before it builds the routes.
async fn iam_module() -> Result<WebModule, StandaloneProfileError> {
    let pool = shared_pool(IAM_OWNER)?;
    let manifest_paths = crate::iam_module_bootstrap::federated_iam_module_manifest_paths()
        .map_err(|detail| unavailable(IAM_OWNER, detail))?;
    let contribution =
        sdkwork_api_iam_assembly::assemble_owner_api_surfaces_with_pool_and_module_manifests(
            pool,
            &manifest_paths,
        )
        .await
        .map_err(|detail| unavailable(IAM_OWNER, detail))?;
    Ok(WebModule::from_contribution(contribution))
}

/// Drive App API (`/app/v3/api/drive/*`) **and** Admin Storage backend surface
/// (`/backend/v3/api/drive/storage/*`) as one module.
///
/// API_ASSEMBLY_SPEC §4.1.1 owns one contribution per served owner, and
/// `ComposedApiAssembly::try_compose` enforces it: an owner selected twice fails
/// composition outright. Both drive surfaces therefore arrive as one
/// already-composed contribution from the drive assembly. Fetching them from two
/// drive entrypoints and installing them here would either fail composition (two
/// contributions, one owner) or — through `ApiModuleRegistry`'s duplicate-owner
/// tolerance — drop the second surface and serve a partial route surface that
/// returns 404 for a dependency the gateway component contract declares as served
/// (§6.1).
async fn drive_module() -> Result<WebModule, StandaloneProfileError> {
    let pool = shared_pool(DRIVE_OWNER)?;
    let contribution =
        sdkwork_api_drive_assembly::assemble_same_origin_contribution_with_pool(&pool)
            .await
            .map_err(|detail| unavailable(DRIVE_OWNER, detail))?;
    Ok(WebModule::from_contribution(contribution))
}

/// Skills App API (`/app/v3/api/skills/*`) and Backend API
/// (`/backend/v3/api/skills/*`).
///
/// The skills assembly already hands both surfaces over as one contribution
/// (`assemble_api_router_with_pool`), and its router is host-neutral: the
/// assembly installs no Web Framework layer, so this edge's single framework
/// layer authenticates skills requests like every other composed surface.
async fn skills_module() -> Result<WebModule, StandaloneProfileError> {
    let pool = shared_pool(SKILLS_OWNER)?;
    sdkwork_api_skills_assembly::web_module_with_pool(pool)
        .await
        .map_err(|detail| unavailable(SKILLS_OWNER, detail))
}

/// MCP App API (`/app/v3/api/mcp/*`) and Backend API
/// (`/backend/v3/api/mcp/*`), also handed over as one host-neutral contribution.
async fn mcp_module() -> Result<WebModule, StandaloneProfileError> {
    let pool = shared_pool(MCP_OWNER)?;
    sdkwork_api_mcp_assembly::web_module_with_pool(pool)
        .await
        .map_err(|detail| unavailable(MCP_OWNER, detail))
}

/// Deployments domain and certificate management, the standalone control plane.
///
/// This uses the assembly's same-origin entrypoint rather than
/// `web_module_with_pool`: the deployments [`WebModule`] is the **Deploy**
/// application surface (app + backend routes for deploy itself) and its routers
/// already carry their own Web Framework layer. The standalone edge needs the
/// un-wrapped domain/certificate blocks so they authenticate through this
/// gateway's single framework layer, together with every other composed surface.
async fn deployments_module() -> Result<WebModule, StandaloneProfileError> {
    let pool = shared_pool(DEPLOYMENTS_OWNER)?;
    let contribution =
        sdkwork_api_deployments_assembly::assemble_same_origin_contribution_with_pool(pool)
            .await
            .map_err(|detail| unavailable(DEPLOYMENTS_OWNER, detail))?;
    Ok(WebModule::from_contribution(contribution))
}

fn unavailable(owner: &'static str, detail: impl Into<String>) -> StandaloneProfileError {
    StandaloneProfileError::assembly_unavailable(owner, detail)
}

/// Returns the canonical process pool or fails closed.
///
/// Serving a declared same-origin dependency without its route inventory would
/// otherwise show up as an opaque 404 on an authenticated operator action, so a
/// missing prerequisite is reported as the typed 50301 dependency-unavailable
/// error naming the dependency that could not be composed.
fn shared_pool(owner: &'static str) -> Result<DatabasePool, StandaloneProfileError> {
    process_shared_database_pool().ok_or_else(|| {
        unavailable(
            owner,
            "the process-shared PostgreSQL pool is unavailable; the standalone gateway must \
             bootstrap its database lifecycle before composing its same-origin dependency \
             modules (API_ASSEMBLY_SPEC §6.1)",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_process_pool_fails_closed_as_dependency_unavailable() {
        // The pool is not installed in a unit test process, so every dependency
        // must report the typed 50301 error under its own owner.
        for owner in [
            IAM_OWNER,
            DRIVE_OWNER,
            SKILLS_OWNER,
            MCP_OWNER,
            DEPLOYMENTS_OWNER,
        ] {
            match shared_pool(owner) {
                Err(StandaloneProfileError::AssemblyUnavailable {
                    owner: reported,
                    code,
                    detail,
                }) => {
                    assert_eq!(reported, owner);
                    assert_eq!(code, 50301);
                    assert!(
                        detail.contains("process-shared PostgreSQL pool"),
                        "operator must learn which prerequisite is missing: {detail}"
                    );
                }
                other => panic!("{owner} composition must fail closed; got {other:?}"),
            }
        }
    }
}
