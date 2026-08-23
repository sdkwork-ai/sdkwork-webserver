//! API assembly for sdkwork-webserver.
//! Application bootstrap lives in `bootstrap.rs`; route inventory is in `assembly-manifest.json`.

mod bootstrap;
mod framework_observability;
mod generated;

pub use bootstrap::{
    assemble_api_router, migrate_database_from_env, seed_space_repository, ApiAssembly,
    ApiAssemblyContext,
};

pub fn assembly_route_count() -> usize {
    generated::ROUTE_CRATE_COUNT
}
