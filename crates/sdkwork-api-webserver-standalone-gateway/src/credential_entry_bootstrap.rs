//! IAM-signed credential-entry bootstrap Access-Token issuance for packaged
//! standalone deployments (Docker/deb). Unsigned dev fixtures are rejected in
//! production IAM posture; this path persists a tenant-bound session instead.

use std::path::Path;

use sdkwork_iam_web_adapter::resolve_deployment_bootstrap_access_token;
use tracing::info;

use crate::bootstrap::run_database_migrate_only;
use crate::packaged_runtime::configure_packaged_runtime_roots_from_env;

/// Issues a signed credential-entry bootstrap Access-Token and writes it to
/// `output` with mode `0600`.
pub async fn issue_credential_entry_bootstrap_token_to_file(
    output: &Path,
) -> Result<(), String> {
    configure_packaged_runtime_roots_from_env()?;
    run_database_migrate_only().await?;
    align_bootstrap_environment_from_webserver_profile();

    // Never reuse a preconfigured operator token when (re)provisioning the
    // browser bootstrap secret file.
    std::env::remove_var("SDKWORK_ACCESS_TOKEN");

    let tenant_id = read_env_trimmed("SDKWORK_WEB_FRAMEWORK_JWT_BOOTSTRAP_TENANT_ID");
    let app_id = read_env_trimmed("SDKWORK_WEB_FRAMEWORK_JWT_BOOTSTRAP_APP_ID");
    let token = resolve_deployment_bootstrap_access_token(
        tenant_id.as_deref(),
        app_id.as_deref(),
    )
    .await?
    .ok_or_else(|| {
        "IAM database is unavailable; cannot issue credential-entry bootstrap Access-Token"
            .to_owned()
    })?;

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "create credential-entry bootstrap token directory {} failed: {error}",
                parent.display()
            )
        })?;
    }
    std::fs::write(output, format!("{token}\n")).map_err(|error| {
        format!(
            "write credential-entry bootstrap token {} failed: {error}",
            output.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(output, std::fs::Permissions::from_mode(0o600)).map_err(
            |error| {
                format!(
                    "chmod credential-entry bootstrap token {} failed: {error}",
                    output.display()
                )
            },
        )?;
    }

    info!(
        path = %output.display(),
        tenant_id = tenant_id.as_deref().unwrap_or("100001"),
        app_id = app_id.as_deref().unwrap_or("sdkwork-web"),
        "issued IAM-signed credential-entry bootstrap Access-Token"
    );
    Ok(())
}

fn read_env_trimmed(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// `issue_standalone_bootstrap_access_credential` reads Cloud Router env keys;
/// map the webserver lifecycle profile when operators only set
/// `SDKWORK_WEBSERVER_ENVIRONMENT`.
fn align_bootstrap_environment_from_webserver_profile() {
    if std::env::var("SDKWORK_CLOUDROUTER_ROUTER_ENVIRONMENT")
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return;
    }
    if std::env::var("SDKWORK_CLOUDROUTER_ENVIRONMENT")
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return;
    }
    if let Ok(environment) = std::env::var("SDKWORK_WEBSERVER_ENVIRONMENT") {
        let environment = environment.trim();
        if !environment.is_empty() {
            std::env::set_var("SDKWORK_CLOUDROUTER_ROUTER_ENVIRONMENT", environment);
        }
    }
}
