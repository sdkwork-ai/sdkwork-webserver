//! Credential-entry bootstrap Access-Token entrypoint for packaged standalone
//! deployments (Docker/deb).
//!
//! The actual startup sequence lives in [`crate::deployment_bootstrap`]: it
//! ensures the deploy application, reconciles and enables the IAM installation
//! bound to it, reuses a reusable token, and only then issues a new one. This
//! module keeps the historical `issue-credential-entry-bootstrap-token`
//! entrypoint as a thin adapter so the container entrypoint and the Debian/RPM
//! post-install scripts keep one command name.

use std::path::Path;

use tracing::info;

use crate::deployment_bootstrap::{bootstrap_deployment, DeploymentBootstrapOptions};

/// Ensures the deployment identity and writes the credential-entry bootstrap
/// Access-Token to `output` with mode `0600`.
///
/// Issuance is reuse-first: an existing token for the same tenant, deploy
/// application anchor, and environment is left byte-identical. Pass
/// `--regenerate-access-token` (or set `SDKWORK_WEBSERVER_REGENERATE_ACCESS_TOKEN=1`)
/// when a fresh credential is explicitly required.
pub async fn issue_credential_entry_bootstrap_token_to_file(output: &Path) -> Result<(), String> {
    let mut options = DeploymentBootstrapOptions::from_env();
    options.token_path = Some(output.to_path_buf());
    // This historical entrypoint always converged the schema before issuing;
    // keep that contract for the installer's first boot.
    options.migrate = true;
    let outcome = bootstrap_deployment(&options).await?;
    info!(
        path = %output.display(),
        tenant_id = %outcome.tenant_id,
        deploy_application_id = %outcome.deploy_application_id,
        installation_id = %outcome.installation_id,
        token_reused = outcome.token_reused,
        "issued or reused the IAM credential-entry bootstrap Access-Token"
    );
    Ok(())
}
