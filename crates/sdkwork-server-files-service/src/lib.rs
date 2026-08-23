//! Secure server filesystem browsing for deployment nodes.
//!
//! This crate implements the domain service behind the Web Server
//! "Server Files" admin surface. It provides:
//!
//! - **Path containment security**: every requested path is canonicalized and
//!   must remain strictly inside the node's authorized filesystem root (for
//!   example `/opt/deploy`). Directory traversal and symlink escapes are
//!   rejected before any file-system access.
//! - **Directory listing** with a lightweight project-type classifier
//!   (`h5-app`, `pc-app`, `flutter-app`, `rust-backend`, `node-backend`,
//!   `sdkwork-workspace`, `generic`).
//! - **File reading** bounded by a configured maximum content size.
//! - **Project operation mapping** that turns a classified project root into
//!   an executable command (build / package / start / deploy / stop /
//!   restart), gated by IAM permission metadata.
//!
//! The service is intentionally transport-agnostic: HTTP route handlers in
//! `sdkwork-routes-*` and the frontend `ServerFilesClient` share these types.

mod operations;
mod path_security;
mod project;
mod service;
mod startup_clone;

pub use operations::{
    command_for, operations_for, ProjectOperation, ProjectOperationCommand, ProjectOperationKind,
    ServerProjectOperations,
};
pub use path_security::{
    PathContainmentError, resolve_contained_path, validate_allowed_root,
};
pub use project::{
    ProjectClassification, ProjectType, classify_directory, classify_entry_names,
};
pub use service::{
    BrowseDirectoryError, ReadFileError, ServerFilesService, ServerFilesServiceConfig,
};
pub use startup_clone::{
    ensure_space_repository, SpaceCloneError, SDKWORK_SPACE_DIRECTORY, SDKWORK_SPACE_REPOSITORY,
};
