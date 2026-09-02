//! API assembly for sdkwork-webserver.
//! Application bootstrap lives in `bootstrap.rs`; route inventory is in `assembly-manifest.json`.

mod bootstrap;
mod framework_observability;
mod generated;

pub use bootstrap::{assemble_api_router, ApiAssembly, ApiAssemblyContext, assemble_business_routes, migrate_database_from_env, seed_space_repository, web_module, web_module_with_context};

// Expose the Deploy and WebServer repository surfaces that standalone gateway
// hosting consumes through this owner API assembly integration point instead
// of depending on repository crates directly.
pub use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
pub use sdkwork_intelligence_webserver_repository_sqlx::resolution_cache_from_shared_pool;

pub fn assembly_route_count() -> usize {
    generated::ROUTE_CRATE_COUNT
}
