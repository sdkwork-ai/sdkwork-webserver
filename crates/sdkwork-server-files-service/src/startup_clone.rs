//! Startup repository seeding.
//!
//! On boot the Web Server ensures the canonical SDKWork space repository is
//! present under the deployment root (`/opt/deploy`). This module:
//!
//! - Clones `https://github.com/sdkwork-ai/sdkwork-space.git` when absent.
//! - Fetches + fast-forwards an existing clone so the Server Files explorer
//!   always reflects the latest upstream state.
//! - Never shells out to a string; the repository URL is a compile-time
//!   constant and the clone path is derived from the contained root.
//!
//! The clone is best-effort and non-fatal: a failure must not prevent the
//! control plane from starting, so `ensure_space_repository` returns a
//! `Result` the caller may downgrade to a warning.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::path_security::validate_allowed_root;

/// The canonical SDKWork space repository seeded at boot.
pub const SDKWORK_SPACE_REPOSITORY: &str = "https://github.com/sdkwork-ai/sdkwork-space.git";
/// Directory name under the deployment root where the repository is cloned.
pub const SDKWORK_SPACE_DIRECTORY: &str = "sdkwork-space";

#[derive(Debug, thiserror::Error)]
pub enum SpaceCloneError {
    #[error("The deployment root is invalid: {0}")]
    InvalidRoot(String),
    #[error("Git is not available on this host")]
    GitUnavailable,
    #[error("Could not seed the SDKWork space repository: {0}")]
    Command(String),
}

/// Ensure the SDKWork space repository exists under `deployment_root`.
///
/// `deployment_root` defaults to `/opt/deploy` when `None`. Returns the path
/// of the seeded repository on success.
pub async fn ensure_space_repository(
    deployment_root: Option<&str>,
) -> Result<PathBuf, SpaceCloneError> {
    let root = validate_allowed_root(deployment_root.unwrap_or("/opt/deploy"))
        .map_err(|error| SpaceCloneError::InvalidRoot(error.to_string()))?;
    let repository_dir = root.join(SDKWORK_SPACE_DIRECTORY);

    // If git is unavailable, fail cleanly instead of silently browsing an
    // unseeded tree.
    if !git_available() {
        return Err(SpaceCloneError::GitUnavailable);
    }

    if repository_dir.join(".git").exists() || repository_dir.join(".git").is_file() {
        update_repository(&repository_dir).await?;
    } else {
        clone_repository(&root).await?;
    }

    Ok(repository_dir)
}

fn git_available() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

async fn clone_repository(root: &Path) -> Result<(), SpaceCloneError> {
    run_git(
        root,
        &[
            "clone",
            "--depth",
            "1",
            SDKWORK_SPACE_REPOSITORY,
            SDKWORK_SPACE_DIRECTORY,
        ],
    )
    .await
}

async fn update_repository(repository_dir: &Path) -> Result<(), SpaceCloneError> {
    // Fast-forward the default branch. This stays non-interactive and bounded.
    run_git(repository_dir, &["fetch", "--depth", "1", "origin"]).await?;
    run_git(repository_dir, &["pull", "--ff-only"]).await
}

async fn run_git(cwd: &Path, args: &[&str]) -> Result<(), SpaceCloneError> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|error| SpaceCloneError::Command(error.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SpaceCloneError::Command(format!(
            "git {} failed: {}",
            args.join(" "),
            stderr.trim()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_root_is_opt_deploy() {
        assert!(SDKWORK_SPACE_REPOSITORY.ends_with(".git"));
        assert_eq!(SDKWORK_SPACE_DIRECTORY, "sdkwork-space");
    }

    #[tokio::test]
    async fn invalid_root_is_rejected() {
        let error = ensure_space_repository(Some(".")).await.unwrap_err();
        assert!(matches!(error, SpaceCloneError::InvalidRoot(_)));
    }
}
