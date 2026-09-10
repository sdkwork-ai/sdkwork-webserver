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
use std::time::Duration;

use crate::path_security::validate_allowed_root;

/// Wall-clock budget for one git subprocess (clone/fetch/pull). A network
/// hang must not stall the boot sequence indefinitely; the child is killed
/// when the budget elapses.
const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

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
    if !git_available().await {
        return Err(SpaceCloneError::GitUnavailable);
    }

    if repository_dir.join(".git").exists() || repository_dir.join(".git").is_file() {
        update_repository(&repository_dir).await?;
    } else {
        clone_repository(&root).await?;
    }

    Ok(repository_dir)
}

async fn git_available() -> bool {
    matches!(
        tokio::time::timeout(
            GIT_COMMAND_TIMEOUT,
            tokio::process::Command::new("git")
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status(),
        )
        .await,
        Ok(Ok(status)) if status.success()
    )
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
    let mut child = tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        // Stderr is captured only to describe the failure, and only a bounded
        // prefix is read back so a chatty child cannot grow memory.
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| SpaceCloneError::Command(error.to_string()))?;

    let status = match tokio::time::timeout(GIT_COMMAND_TIMEOUT, child.wait()).await {
        Ok(status) => status.map_err(|error| SpaceCloneError::Command(error.to_string()))?,
        Err(_) => {
            let _ = child.kill().await;
            return Err(SpaceCloneError::Command(format!(
                "git {} timed out after {:?}",
                args.join(" "),
                GIT_COMMAND_TIMEOUT
            )));
        }
    };

    if !status.success() {
        return Err(SpaceCloneError::Command(format!(
            "git {} failed with {}",
            args.join(" "),
            status
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
