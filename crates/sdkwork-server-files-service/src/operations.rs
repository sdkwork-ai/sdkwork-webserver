//! Project operation mapping.
//!
//! A classified project root is offered a set of operations (build, package,
//! start, stop, restart). Each operation carries the IAM permission required
//! to invoke it; the caller must be authorized separately. Commands are
//! expressed as `argv` vectors (never shell strings) so no shell
//! interpretation or injection is possible.
//!
//! Every offered operation maps to exactly one [`OperationExecution`]:
//! foreground runs are bounded by a timeout with capped capture, and
//! start/stop/restart manage a single per-project process through the pid
//! file in the run directory (see [`crate::operation_runtime`]). Deployment
//! is intentionally NOT offered here: application deployment is the durable
//! command plane (PRD-FR-026), not a shell operation.

pub use super::project::ProjectClassification;
use super::project::ProjectType;

/// Operation identifiers exposed to the frontend and matched by `kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectOperationKind {
    Build,
    Package,
    Start,
    Stop,
    Restart,
}

/// Canonical pid-file slot for the single managed process of one project.
/// Start, stop, and restart all address this slot so a project root owns at
/// most one managed process at a time.
pub const MANAGED_PROCESS_SLOT: &str = "process";

/// An operation offered for a project root.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectOperation {
    pub id: String,
    pub kind: ProjectOperationKind,
    pub label: String,
    pub permission: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub dangerous: bool,
}

/// Executable form of an operation: raw `argv`, never a shell string.
#[derive(Debug, Clone)]
pub struct ProjectOperationCommand {
    pub program: String,
    pub args: Vec<String>,
    /// Working directory relative to the project root (empty = root).
    pub cwd: String,
}

/// The full operation manifest for a browsed project directory.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProjectOperations {
    pub node_id: String,
    pub path: String,
    pub project_type: ProjectType,
    pub operations: Vec<ProjectOperation>,
}

const PERMISSION_BUILD: &str = "web.servers.files.write";
const PERMISSION_DEPLOY: &str = "web.servers.files.deploy";

fn op(
    id: &str,
    kind: ProjectOperationKind,
    label: &str,
    permission: &str,
    dangerous: bool,
) -> ProjectOperation {
    ProjectOperation {
        id: id.to_string(),
        kind,
        label: label.to_string(),
        permission: permission.to_string(),
        description: None,
        dangerous,
    }
}

/// Build the operation manifest for a classified project directory.
pub fn operations_for(
    node_id: &str,
    path: &str,
    classification: &ProjectClassification,
) -> ServerProjectOperations {
    let operations = match classification.project_type {
        ProjectType::FlutterApp => vec![
            op(
                "build",
                ProjectOperationKind::Build,
                "Build (debug)",
                PERMISSION_BUILD,
                false,
            ),
            op(
                "package",
                ProjectOperationKind::Package,
                "Build release bundle",
                PERMISSION_BUILD,
                false,
            ),
            op(
                "start",
                ProjectOperationKind::Start,
                "Run app",
                PERMISSION_DEPLOY,
                false,
            ),
            op(
                "stop",
                ProjectOperationKind::Stop,
                "Stop app",
                PERMISSION_DEPLOY,
                true,
            ),
        ],
        ProjectType::RustBackend => vec![
            op(
                "build",
                ProjectOperationKind::Build,
                "Cargo build",
                PERMISSION_BUILD,
                false,
            ),
            op(
                "package",
                ProjectOperationKind::Package,
                "Cargo build --release",
                PERMISSION_BUILD,
                false,
            ),
            op(
                "start",
                ProjectOperationKind::Start,
                "Run service",
                PERMISSION_DEPLOY,
                false,
            ),
            op(
                "restart",
                ProjectOperationKind::Restart,
                "Restart service",
                PERMISSION_DEPLOY,
                true,
            ),
            op(
                "stop",
                ProjectOperationKind::Stop,
                "Stop service",
                PERMISSION_DEPLOY,
                true,
            ),
        ],
        ProjectType::NodeBackend => vec![
            op(
                "build",
                ProjectOperationKind::Build,
                "Install + build",
                PERMISSION_BUILD,
                false,
            ),
            op(
                "start",
                ProjectOperationKind::Start,
                "Run server",
                PERMISSION_DEPLOY,
                false,
            ),
            op(
                "restart",
                ProjectOperationKind::Restart,
                "Restart server",
                PERMISSION_DEPLOY,
                true,
            ),
            op(
                "stop",
                ProjectOperationKind::Stop,
                "Stop server",
                PERMISSION_DEPLOY,
                true,
            ),
        ],
        ProjectType::H5App | ProjectType::PcApp => vec![
            op(
                "build",
                ProjectOperationKind::Build,
                "Build bundle",
                PERMISSION_BUILD,
                false,
            ),
            op(
                "package",
                ProjectOperationKind::Package,
                "Package distributable",
                PERMISSION_BUILD,
                false,
            ),
            op(
                "start",
                ProjectOperationKind::Start,
                "Preview",
                PERMISSION_DEPLOY,
                false,
            ),
        ],
        ProjectType::SdkworkWorkspace => vec![
            op(
                "build",
                ProjectOperationKind::Build,
                "Build workspace",
                PERMISSION_BUILD,
                false,
            ),
            op(
                "package",
                ProjectOperationKind::Package,
                "Package workspace",
                PERMISSION_BUILD,
                false,
            ),
        ],
        ProjectType::Generic => Vec::new(),
    };

    ServerProjectOperations {
        node_id: node_id.to_string(),
        path: path.to_string(),
        project_type: classification.project_type,
        operations,
    }
}

/// How an offered operation executes on the node.
#[derive(Debug, Clone)]
pub enum OperationExecution {
    /// Run to completion under a hard timeout with capped output capture.
    Foreground(ProjectOperationCommand),
    /// Spawn a detached managed process recorded in the project run
    /// directory ([`RUN_DIRECTORY_NAME`]).
    StartManaged(ProjectOperationCommand),
    /// Stop the project's managed process (pid-file addressed process tree).
    StopManaged,
    /// Stop the managed process, then start it again with the command.
    RestartManaged(ProjectOperationCommand),
}

/// Resolve the concrete execution for a project type + operation kind.
/// Returns `None` when the type does not support the operation.
pub fn execution_for(
    project_type: ProjectType,
    kind: ProjectOperationKind,
) -> Option<OperationExecution> {
    let execution = match (project_type, kind) {
        (ProjectType::FlutterApp, ProjectOperationKind::Build) => {
            OperationExecution::Foreground(shellish("flutter", &["build", "web"]))
        }
        (ProjectType::FlutterApp, ProjectOperationKind::Package) => {
            OperationExecution::Foreground(shellish("flutter", &["build", "web", "--release"]))
        }
        (ProjectType::FlutterApp, ProjectOperationKind::Start) => {
            OperationExecution::StartManaged(shellish("flutter", &["run", "-d", "web-server"]))
        }
        (ProjectType::FlutterApp, ProjectOperationKind::Stop) => OperationExecution::StopManaged,
        (ProjectType::RustBackend, ProjectOperationKind::Build) => {
            OperationExecution::Foreground(shellish("cargo", &["build"]))
        }
        (ProjectType::RustBackend, ProjectOperationKind::Package) => {
            OperationExecution::Foreground(shellish("cargo", &["build", "--release"]))
        }
        (ProjectType::RustBackend, ProjectOperationKind::Start) => {
            OperationExecution::StartManaged(shellish("cargo", &["run"]))
        }
        (ProjectType::RustBackend, ProjectOperationKind::Restart) => {
            OperationExecution::RestartManaged(shellish("cargo", &["run"]))
        }
        (ProjectType::RustBackend, ProjectOperationKind::Stop) => OperationExecution::StopManaged,
        (ProjectType::NodeBackend, ProjectOperationKind::Build) => {
            OperationExecution::Foreground(shellish("npm", &["install"]))
        }
        (ProjectType::NodeBackend, ProjectOperationKind::Start) => {
            OperationExecution::StartManaged(shellish("npm", &["start"]))
        }
        (ProjectType::NodeBackend, ProjectOperationKind::Restart) => {
            OperationExecution::RestartManaged(shellish("npm", &["start"]))
        }
        (ProjectType::NodeBackend, ProjectOperationKind::Stop) => OperationExecution::StopManaged,
        (ProjectType::H5App | ProjectType::PcApp, ProjectOperationKind::Build) => {
            OperationExecution::Foreground(shellish("pnpm", &["build"]))
        }
        (ProjectType::H5App | ProjectType::PcApp, ProjectOperationKind::Package) => {
            OperationExecution::Foreground(shellish("pnpm", &["build"]))
        }
        (ProjectType::H5App | ProjectType::PcApp, ProjectOperationKind::Start) => {
            OperationExecution::StartManaged(shellish("pnpm", &["dev"]))
        }
        (ProjectType::SdkworkWorkspace, ProjectOperationKind::Build) => {
            OperationExecution::Foreground(shellish("pnpm", &["build"]))
        }
        (ProjectType::SdkworkWorkspace, ProjectOperationKind::Package) => {
            OperationExecution::Foreground(shellish("pnpm", &["build"]))
        }
        _ => return None,
    };
    Some(execution)
}

/// Helper that returns an argv-style command. Operations are argv-only by
/// contract: process lifecycle is addressed through the pid-file slot, never
/// through shell interpolation.
fn shellish(program: &str, args: &[&str]) -> ProjectOperationCommand {
    ProjectOperationCommand {
        program: program.to_string(),
        args: args.iter().map(|s| s.to_string()).collect(),
        cwd: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::classify_entry_names;

    #[test]
    fn rust_project_exposes_lifecycle_operations_without_fake_deploy() {
        let classification = classify_entry_names(&["Cargo.toml".to_string()]);
        let manifest = operations_for("n1", "/opt/deploy/server", &classification);
        assert!(manifest
            .operations
            .iter()
            .any(|o| o.kind == ProjectOperationKind::Build));
        assert!(manifest
            .operations
            .iter()
            .any(|o| o.kind == ProjectOperationKind::Stop));
        // Deployment is the durable command plane, never a shell operation.
        assert!(manifest.operations.iter().all(|o| o.id != "deploy"),);
    }

    #[test]
    fn generic_has_no_operations() {
        let classification = classify_entry_names(&["README.md".to_string()]);
        let manifest = operations_for("n1", "/opt/deploy/misc", &classification);
        assert!(manifest.operations.is_empty());
    }

    #[test]
    fn execution_for_maps_foreground_and_managed_ops() {
        let classification = classify_entry_names(&["Cargo.toml".to_string()]);
        match execution_for(classification.project_type, ProjectOperationKind::Build) {
            Some(OperationExecution::Foreground(command)) => {
                assert_eq!(command.program, "cargo");
                assert_eq!(command.args, vec!["build"]);
            }
            other => panic!("build must be foreground, got {other:?}"),
        }
        match execution_for(classification.project_type, ProjectOperationKind::Start) {
            Some(OperationExecution::StartManaged(command)) => {
                assert_eq!(command.program, "cargo");
            }
            other => panic!("start must be managed, got {other:?}"),
        }
        assert!(matches!(
            execution_for(classification.project_type, ProjectOperationKind::Stop),
            Some(OperationExecution::StopManaged)
        ));
        // No argv command may embed shell interpolation ever again.
        let commands = [
            execution_for(ProjectType::FlutterApp, ProjectOperationKind::Start),
            execution_for(ProjectType::NodeBackend, ProjectOperationKind::Start),
            execution_for(ProjectType::H5App, ProjectOperationKind::Start),
        ];
        for command in commands.into_iter().flatten() {
            let OperationExecution::StartManaged(command) = command else {
                continue;
            };
            for arg in &command.args {
                assert!(!arg.contains("$("), "shell interpolation in {arg:?}");
            }
        }
    }
}
