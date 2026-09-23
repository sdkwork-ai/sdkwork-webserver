//! API assembly for sdkwork-webserver.
//! Application bootstrap lives in `bootstrap.rs`; route inventory is in `assembly-manifest.json`.
// SDKWORK-ASSEMBLY-LIB-CUSTOM: this library root carries integration seams that
// the materializer template cannot express. `renderLibRs()` in
// `sdkwork-specs/tools/materialize-api-assembly.mjs` regenerates `lib.rs` from a
// fixed probe list over `bootstrap.rs` and has no entry for
// `framework_observability`, for `runtime_shutdown` (the process-wide drain
// trigger the self-report loop sets and the gateway binary awaits), for the
// standalone-gateway integration points (`seed_space_repository`, `web_module`,
// `web_module_with_context`), or for the owner-API repository re-exports below.
// This marker keeps the file authored; without it the next
// `pnpm api:assembly:materialize` silently drops every one of them and the crate
// stops compiling, because `bootstrap.rs` still does
// `use crate::framework_observability::…`.

mod bootstrap;
mod cluster_self_report;
mod framework_observability;
mod generated;
mod runtime_shutdown;
// The traffic usage read model is built here because the facts belong to a
// sibling module: the assembly is the crate allowed to construct a repository,
// and the service port it satisfies is injected by `bootstrap.rs`.
mod traffic_usage;

pub use bootstrap::{
    assemble_api_router, assemble_business_routes, ensure_database_lifecycle_from_env,
    migrate_database_from_env, seed_space_repository, web_module, web_module_with_context, ApiAssembly,
    ApiAssemblyContext,
};

// SDKWORK-ASSEMBLY-LIB-CUSTOM: the process-wide graceful-shutdown trigger. The
// cluster registry asks an instance to drain on the heartbeat response, and the
// only thing that can stop a gateway's listeners is the data-plane server the
// gateway binary owns — so the trigger has to cross this boundary.
pub use runtime_shutdown::{request_shutdown, shutdown_reason, wait_for_shutdown, ShutdownReason};

// SDKWORK-ASSEMBLY-LIB-CUSTOM: the Deploy and WebServer repository surfaces that
// standalone gateway hosting consumes through this owner API assembly
// integration point instead of depending on repository crates directly
// (API_ASSEMBLY_SPEC §4/§6).
pub use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
pub use sdkwork_intelligence_webserver_repository_sqlx::resolution_cache_from_shared_pool;

pub fn assembly_route_count() -> usize {
    generated::ROUTE_CRATE_COUNT
}
