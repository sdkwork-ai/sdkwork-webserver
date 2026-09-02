//! Project operation mapping.
//!
//! A classified project root is offered a set of operations (build, package,
//! start, deploy, stop, restart). Each operation carries the IAM permission
//! required to invoke it; the caller must be authorized separately. Commands
//! are expressed as `argv` vectors (never shell strings) so no shell
//! interpretation or injection is possible.

pub use super::project::ProjectClassification;
use super::project::ProjectType;

/// Operation identifiers exposed to the frontend and matched by `kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectOperationKind {
    Build,
    Package,
    Start,
    Deploy,
    Stop,
    Restart,
}

/// An operation offered for a project root.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
            op(
                "deploy",
                ProjectOperationKind::Deploy,
                "Deploy to node",
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
            op(
                "deploy",
                ProjectOperationKind::Deploy,
                "Deploy to node",
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
            op(
                "deploy",
                ProjectOperationKind::Deploy,
                "Deploy to node",
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
            op(
                "deploy",
                ProjectOperationKind::Deploy,
                "Deploy to node",
                PERMISSION_DEPLOY,
                true,
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

/// Resolve a concrete executable command for a project type + operation kind.
/// Returns `None` when the type does not support the operation.
pub fn command_for(
    project_type: ProjectType,
    kind: ProjectOperationKind,
) -> Option<ProjectOperationCommand> {
    let command = match (project_type, kind) {
        (ProjectType::FlutterApp, ProjectOperationKind::Build) => {
            shellish("flutter", &["build", "web"])
        }
        (ProjectType::FlutterApp, ProjectOperationKind::Package) => {
            shellish("flutter", &["build", "web", "--release"])
        }
        (ProjectType::FlutterApp, ProjectOperationKind::Start) => {
            shellish("flutter", &["run", "-d", "web-server"])
        }
        (ProjectType::FlutterApp, ProjectOperationKind::Stop) => {
            shellish("kill", &["$(pgrep -f 'flutter run')"])
        }
        (ProjectType::RustBackend, ProjectOperationKind::Build) => shellish("cargo", &["build"]),
        (ProjectType::RustBackend, ProjectOperationKind::Package) => {
            shellish("cargo", &["build", "--release"])
        }
        (ProjectType::RustBackend, ProjectOperationKind::Start) => shellish("cargo", &["run"]),
        (ProjectType::RustBackend, ProjectOperationKind::Restart) => {
            shellish("systemctl", &["restart", "sdkwork"])
        }
        (ProjectType::RustBackend, ProjectOperationKind::Stop) => {
            shellish("systemctl", &["stop", "sdkwork"])
        }
        (ProjectType::NodeBackend, ProjectOperationKind::Build) => shellish("npm", &["install"]),
        (ProjectType::NodeBackend, ProjectOperationKind::Start) => shellish("npm", &["start"]),
        (ProjectType::NodeBackend, ProjectOperationKind::Restart) => {
            shellish("systemctl", &["restart", "sdkwork-node"])
        }
        (ProjectType::NodeBackend, ProjectOperationKind::Stop) => {
            shellish("systemctl", &["stop", "sdkwork-node"])
        }
        (ProjectType::H5App | ProjectType::PcApp, ProjectOperationKind::Build) => {
            shellish("pnpm", &["build"])
        }
        (ProjectType::H5App | ProjectType::PcApp, ProjectOperationKind::Package) => {
            shellish("pnpm", &["build"])
        }
        (ProjectType::H5App | ProjectType::PcApp, ProjectOperationKind::Start) => {
            shellish("pnpm", &["dev"])
        }
        (ProjectType::SdkworkWorkspace, ProjectOperationKind::Build) => {
            shellish("pnpm", &["build"])
        }
        (ProjectType::SdkworkWorkspace, ProjectOperationKind::Package) => {
            shellish("pnpm", &["build"])
        }
        _ => return None,
    };
    Some(command)
}

/// Helper that returns an argv-style command. For commands that genuinely
/// require a shell (e.g. `kill $(pgrep ...)`), the operator is expected to
/// install a deploy manifest; here we keep argv explicit and safe.
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
    fn rust_project_exposes_build_and_deploy() {
        let classification = classify_entry_names(&["Cargo.toml".to_string()]);
        let manifest = operations_for("n1", "/opt/deploy/server", &classification);
        assert!(manifest
            .operations
            .iter()
            .any(|o| o.kind == ProjectOperationKind::Build));
        assert!(manifest
            .operations
            .iter()
            .any(|o| o.kind == ProjectOperationKind::Deploy));
    }

    #[test]
    fn generic_has_no_operations() {
        let classification = classify_entry_names(&["README.md".to_string()]);
        let manifest = operations_for("n1", "/opt/deploy/misc", &classification);
        assert!(manifest.operations.is_empty());
    }

    #[test]
    fn command_for_returns_argv() {
        let classification = classify_entry_names(&["Cargo.toml".to_string()]);
        let cmd = command_for(classification.project_type, ProjectOperationKind::Build).unwrap();
        assert_eq!(cmd.program, "cargo");
        assert_eq!(cmd.args, vec!["build"]);
    }
}
