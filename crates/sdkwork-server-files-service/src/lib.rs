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
//!   an executable operation (build / package / start / stop / restart),
//!   gated by IAM permission metadata. Foreground operations run under a
//!   hard timeout with capped output capture; start/stop/restart manage a
//!   single per-project process through a pid file. Deployment is not a
//!   shell operation (PRD-FR-026 command plane).
//!
//! The service is intentionally transport-agnostic: HTTP route handlers in
//! `sdkwork-routes-*` and the frontend `ServerFilesClient` share these types.

mod operation_runtime;
mod operations;
mod path_security;
mod project;
mod service;
mod startup_clone;

pub use operation_runtime::{
    run_foreground, start_managed, stop_managed, ForegroundRunOutcome, ManagedStartOutcome,
    ManagedStopOutcome, OperationRunError, MANAGED_STOP_TIMEOUT, MAXIMUM_CAPTURED_OUTPUT_BYTES,
    RUN_DIRECTORY_NAME,
};
pub use operations::{
    execution_for, operations_for, OperationExecution, ProjectOperation, ProjectOperationCommand,
    ProjectOperationKind, ServerProjectOperations, MANAGED_PROCESS_SLOT,
};
pub use path_security::{
    display_path, is_sensitive_file_name, resolve_contained_path, validate_allowed_root,
    PathContainmentError,
};
pub use project::{classify_directory, classify_entry_names, ProjectClassification, ProjectType};
pub use service::{
    BrowseDirectoryError, DirectoryListing, EntryKind, FileContent, ReadFileError, ServerEntry,
    ServerFilesService, ServerFilesServiceConfig,
};
pub use startup_clone::{
    ensure_space_repository, SpaceCloneError, SDKWORK_SPACE_DIRECTORY, SDKWORK_SPACE_REPOSITORY,
};
