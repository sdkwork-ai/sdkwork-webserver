//! Managed Web Server configuration-file service.
//!
//! This crate is the domain service behind the Web Server admin
//! "Server Config" surface. It provides a safe, bounded, and auditable
//! editing plane over the configuration files of the *current* server
//! deployment:
//!
//! - **Catalog** ([`WebserverConfigService::catalog`]): enumerates the
//!   well-known configuration groups of a deployment —
//!   `default-config` (top-level runtime config such as `config.toml`),
//!   `import-config` (the `imports.d/` module import plane), and
//!   `module-config` (sibling-module sidecar configs under
//!   `sdkwork-space/<module>/deployments/webserver/`, read-only by design
//!   because each sidecar is owned by its module checkout).
//! - **Read** ([`WebserverConfigService::read`]): bounded text read with a
//!   SHA-256 content digest used for optimistic concurrency.
//! - **Write** ([`WebserverConfigService::write`]): validated, atomic,
//!   backup-protected overwrite. Every entry is addressed by a stable
//!   content-independent `id` (SHA-256 of its catalog identity), so only
//!   catalog-listed files are ever addressable — there is no arbitrary
//!   path surface.
//!
//! Security model:
//!
//! 1. Only files enumerated by [`WebserverConfigService::catalog`] can be
//!    read or written; ids resolve through a fresh catalog lookup.
//! 2. Every resolved path still passes the shared path-containment layer
//!    (`sdkwork-server-files-service`), so traversal and symlink escapes
//!    are impossible regardless.
//! 3. `module-config` entries are read-only: sibling-module sidecars are
//!    owned by their module checkouts and are never rewritten through the
//!    Web Server control plane.
//! 4. Writes are size-bounded, NUL-free, syntax-validated where a parser
//!    exists (JSON/TOML), and land atomically (temp file + rename) after a
//!    timestamped backup of the previous content is created.

mod language;
mod service;
mod wire;

pub use language::config_language_for;
pub use service::{
    WebserverConfigCatalog, WebserverConfigEntry, WebserverConfigError, WebserverConfigFile,
    WebserverConfigKind, WebserverConfigService, WebserverConfigServiceConfig,
    WebserverConfigWriteResult, IMPORTS_CONFIG_DIRECTORY, MODULE_CONFIG_RELATIVE_DIRECTORY,
    SDKWORK_SPACE_DIRECTORY,
};
