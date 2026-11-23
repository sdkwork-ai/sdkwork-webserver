//! Startup self-provisioning of the Web Server's own deployment identity.
//!
//! A packaged standalone Web Server must be able to log a browser in without a
//! separate registration step. That needs three facts to exist before the PC/H5
//! shell is served:
//!
//! 1. the **deploy application** (`deploy_app` in sdkwork-deployments) that owns
//!    this deployment's runtime identity,
//! 2. the **IAM installation** bound one-to-one to that deploy application and
//!    `enabled` (`APPLICATION_IDENTITY_SPEC.md` §2/§3, `IAM_APPLICATION_BOOTSTRAP_SPEC.md`
//!    §5.1), and
//! 3. an IAM-signed **credential-entry bootstrap Access-Token** for tenant
//!    `100001` whose `app_id`/`aud` is that deploy application anchor
//!    (`APP_ACCESS_TOKEN_SPEC.md` §3, `IAM_CREDENTIAL_ENTRY_SPEC.md` §4).
//!
//! Historically each step had its own entrypoint: IAM auto-provisioned an
//! installation from `agent.app.config.json` while building the management
//! router, and `issue-credential-entry-bootstrap-token` minted a token *without*
//! building that router. A packaged first boot therefore asked IAM to issue a
//! credential for an anchor that had never been provisioned on that path, and
//! login failed with
//! `deploy application <anchor> is not bound to an enabled installation of tenant 100001`.
//! This module is the single startup sequence that closes that gap.
//!
//! # Contract
//!
//! - **Idempotent.** The deploy application is resolved by tenant + slug and
//!   created with a deterministic `Idempotency-Key`; `uk_deploy_app_tenant_slug`
//!   and `uk_iam_application_installation_deploy` remain the final authority, and
//!   a concurrent creator's row is adopted by re-reading it.
//! - **Unique.** One installation per deploy application anchor. Provisioning
//!   reconciles the row selected by `deploy_application_id`; it never inserts a
//!   second row for the same anchor.
//! - **Concurrency-safe.** A PostgreSQL advisory lock serializes concurrent
//!   starts of the same tenant + slug, and every write is either an idempotent
//!   upsert or an atomic rename. The lock is an optimization; the database
//!   constraints are the guarantee.
//! - **Reuse-first.** An existing token is reused byte-identically unless it is
//!   expired, bound to another tenant/anchor/environment, an unsigned
//!   development fixture outside development, or the operator explicitly asked
//!   for `--regenerate-access-token` (`IAM_APPLICATION_BOOTSTRAP_SPEC.md` §5.4).
//! - **No secrets in static artifacts.** Static files are materialized with
//!   non-secret deployment identity only (tenant, anchor, application code); the
//!   token reaches the browser through the existing request-time injection and
//!   the `0600` secret file, never through a built bundle.

use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{pool::PoolConnection, PgPool, Postgres};
use tracing::{info, warn};

use sdkwork_database_sqlx::process_shared_database_pool;

use sdkwork_deploy_contract::{
    AppKind, AppOwnerType, CreateAppRequest, DeployAppApi, DeployAppRequestContext,
    DeployBackendApi, DeployBackendRequestContext, ListAppsQuery,
};
use sdkwork_iam_web_adapter::{
    enable_tenant_application, load_manifest_from_app_root, manifest_to_ensure_command,
    provision_tenant_application, relink_tenant_application_deploy_application_id,
    resolve_deployment_bootstrap_access_token, resolve_tenant_application_by_deploy_application_id,
    validate_enabled_tenant_runtime_app, EmbeddedApplicationBootstrapOptions,
    EmbeddedApplicationRuntimeBinding, SdkworkAppManifest, TenantApplicationProvisionCommand,
    DEPLOY_APPLICATION_ID_ENV,
};

use crate::bootstrap::run_database_migrate_only;
use crate::packaged_runtime::configure_packaged_runtime_roots_from_env;

/// Secret file shared with the request-time credential-entry injection path.
pub const CREDENTIAL_ENTRY_BOOTSTRAP_TOKEN_PATH: &str =
    "/etc/sdkwork/webserver/secrets/credential-entry-bootstrap-access-token";

/// Operator override for the token secret file used by startup provisioning.
pub const CREDENTIAL_ENTRY_BOOTSTRAP_TOKEN_FILE_ENV: &str =
    "SDKWORK_WEBSERVER_CREDENTIAL_ENTRY_BOOTSTRAP_TOKEN_FILE";

/// `1`/`true` regenerates the token even when a reusable one exists.
pub const REGENERATE_ACCESS_TOKEN_ENV: &str = "SDKWORK_WEBSERVER_REGENERATE_ACCESS_TOKEN";

/// Deployment slug override. Defaults to the application manifest `app.key`.
pub const DEPLOY_APPLICATION_SLUG_ENV: &str = "SDKWORK_WEBSERVER_DEPLOY_APPLICATION_SLUG";

/// Bootstrap mode for `serve-management`: `strict`, `best-effort`, or `off`.
pub const DEPLOYMENT_BOOTSTRAP_MODE_ENV: &str = "SDKWORK_WEBSERVER_DEPLOYMENT_BOOTSTRAP";

const DEFAULT_TENANT_ID: &str = "100001";
const DEFAULT_ORGANIZATION_ID: &str = "0";
const SELF_SCOPE: &str = "iam:self";
const BOOTSTRAP_LOCK_POLL: Duration = Duration::from_millis(100);
const BOOTSTRAP_LOCK_ATTEMPTS: usize = 50;

/// Options for one startup provisioning run.
#[derive(Clone, Debug)]
pub struct DeploymentBootstrapOptions {
    /// Regenerate the Access-Token even when the current one is reusable.
    pub regenerate_access_token: bool,
    /// Explicit token secret path (defaults to the packaged canonical path).
    pub token_path: Option<PathBuf>,
    /// Explicit tenant (defaults to the manifest `backend.tenantId`, then `100001`).
    pub tenant_id: Option<String>,
    /// Explicit deploy application anchor or legacy slug.
    pub deploy_application_id: Option<String>,
    /// Explicit deploy application slug (defaults to the manifest `app.key`).
    pub app_slug: Option<String>,
    /// Explicit application root (defaults to `SDKWORK_APP_ROOT`, then the source tree).
    pub app_root: Option<PathBuf>,
    /// Run the database lifecycle convergence before provisioning.
    pub migrate: bool,
    /// Persist (issue or reuse) the credential-entry bootstrap Access-Token.
    ///
    /// Explicit operator commands set this; `serve-management` sets it from the
    /// browser-injection policy so a production boot never recreates the secret
    /// the entrypoint just purged.
    pub persist_credential: bool,
}

impl Default for DeploymentBootstrapOptions {
    fn default() -> Self {
        Self {
            regenerate_access_token: false,
            token_path: None,
            tenant_id: None,
            deploy_application_id: None,
            app_slug: None,
            app_root: None,
            migrate: false,
            persist_credential: true,
        }
    }
}

impl DeploymentBootstrapOptions {
    /// Reads the supported environment overrides.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            regenerate_access_token: env_flag(REGENERATE_ACCESS_TOKEN_ENV),
            token_path: env::var_os(CREDENTIAL_ENTRY_BOOTSTRAP_TOKEN_FILE_ENV)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            tenant_id: env_trimmed("SDKWORK_WEB_FRAMEWORK_JWT_BOOTSTRAP_TENANT_ID"),
            deploy_application_id: env_trimmed(DEPLOY_APPLICATION_ID_ENV),
            app_slug: env_trimmed(DEPLOY_APPLICATION_SLUG_ENV),
            app_root: env::var_os("SDKWORK_APP_ROOT")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            migrate: false,
            persist_credential: true,
        }
    }

    /// Parses `--regenerate-access-token`, `--token <path>`, `--tenant <id>`,
    /// `--deploy-application-id <id|slug>`, `--slug <slug>`, and `--migrate`.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown flag or a flag missing its value.
    pub fn from_arguments(arguments: &[String]) -> Result<Self, String> {
        let mut options = Self::from_env();
        let mut index = 0;
        while index < arguments.len() {
            let token = &arguments[index];
            let mut value = || {
                index += 1;
                arguments
                    .get(index)
                    .cloned()
                    .ok_or_else(|| format!("{token} requires a value"))
            };
            match token.as_str() {
                "--regenerate-access-token" | "--refresh" => {
                    options.regenerate_access_token = true;
                }
                "--migrate" => options.migrate = true,
                "--token" => options.token_path = Some(PathBuf::from(value()?)),
                "--tenant" | "--tenant-id" => options.tenant_id = Some(value()?),
                "--deploy-application-id" => options.deploy_application_id = Some(value()?),
                "--slug" => options.app_slug = Some(value()?),
                other => return Err(format!("unknown bootstrap argument: {other}")),
            }
            index += 1;
        }
        Ok(options)
    }
}

/// Result of one bootstrap run.
#[derive(Clone, Debug, Serialize)]
pub struct DeploymentBootstrapOutcome {
    pub tenant_id: String,
    pub app_slug: String,
    pub deploy_application_id: String,
    pub installation_id: String,
    pub token_path: String,
    pub token_reused: bool,
    pub installation_reconciled: bool,
    pub static_runtime_envs_updated: Vec<String>,
}

/// Runs the complete startup provisioning sequence.
///
/// # Errors
///
/// Fails when the application manifest/root cannot be resolved, the deploy
/// application cannot be created or read back, the installation cannot be
/// enabled, or a credential cannot be issued. Every error names the step so a
/// first-boot failure is actionable.
pub async fn bootstrap_deployment(
    options: &DeploymentBootstrapOptions,
) -> Result<DeploymentBootstrapOutcome, String> {
    configure_packaged_runtime_roots_from_env()?;
    if options.migrate {
        run_database_migrate_only().await?;
    }

    let app_root = resolve_app_root(options.app_root.clone())?;
    let manifest = load_manifest_from_app_root(&app_root)?;
    let environment = resolve_environment();
    // IAM's bootstrap issuance resolves its lifecycle from the Cloud Router
    // environment keys. Align them once so the installation environment, the
    // token's `environment` claim, and this process's lifecycle cannot disagree.
    if env_trimmed("SDKWORK_CLOUDROUTER_ROUTER_ENVIRONMENT").is_none()
        && env_trimmed("SDKWORK_CLOUDROUTER_ENVIRONMENT").is_none()
    {
        env::set_var("SDKWORK_CLOUDROUTER_ROUTER_ENVIRONMENT", &environment);
    }
    let tenant_id = resolve_tenant_id(options, &manifest);
    let organization_id = manifest
        .backend
        .organization_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_ORGANIZATION_ID)
        .to_owned();
    let app_slug = resolve_app_slug(options, &manifest);
    let token_path = options
        .token_path
        .clone()
        .unwrap_or_else(default_token_path);

    let pool = process_shared_database_pool().ok_or_else(|| {
        "startup deployment bootstrap requires the process-shared PostgreSQL pool; the database \
         lifecycle must converge before provisioning"
            .to_owned()
    })?;
    let pg = pool
        .as_postgres()
        .ok_or_else(|| "startup deployment bootstrap requires a PostgreSQL pool".to_owned())?
        .clone();

    // Login needs the tenant/bootstrap administrator as much as it needs the
    // installation: a valid token with no tenant row resolves to nothing.
    sdkwork_iam_bootstrap::upsert_postgres_default_subject(&pg)
        .await
        .map_err(|error| format!("ensure IAM default tenant/administrator failed: {error}"))?;

    let lock_key = advisory_lock_key(&tenant_id, &app_slug);
    let mut lock = acquire_bootstrap_lock(&pg, lock_key).await;

    let result = bootstrap_locked(
        &pg,
        &manifest,
        &app_root,
        &tenant_id,
        &organization_id,
        &app_slug,
        &environment,
        &token_path,
        options,
    )
    .await;

    if let Some(connection) = lock.as_mut() {
        release_bootstrap_lock(connection, lock_key).await;
    }
    result
}

#[allow(clippy::too_many_arguments)]
async fn bootstrap_locked(
    pg: &PgPool,
    manifest: &SdkworkAppManifest,
    app_root: &Path,
    tenant_id: &str,
    organization_id: &str,
    app_slug: &str,
    environment: &str,
    token_path: &Path,
    options: &DeploymentBootstrapOptions,
) -> Result<DeploymentBootstrapOutcome, String> {
    let configured_anchor = resolve_configured_anchor(options, manifest);
    let existing_token = read_token_candidate(token_path);
    let existing_claims = existing_token.as_deref().and_then(parse_token_claims);
    let candidate_anchor = configured_anchor
        .as_deref()
        .filter(|anchor| is_decimal_anchor(anchor))
        .map(str::to_owned)
        .or_else(|| {
            existing_claims
                .as_ref()
                .and_then(|claims| claims.app_id.as_deref())
                .filter(|anchor| is_decimal_anchor(anchor))
                .map(str::to_owned)
        });

    // Fast path: a numeric anchor whose installation is already enabled needs
    // no deploy-service work. Token reuse is decided below, byte-identically.
    if !options.regenerate_access_token {
        if let Some(anchor) = candidate_anchor.as_deref() {
            if installation_is_enabled(pg, tenant_id, anchor).await {
                return finish_with_token(
                    pg,
                    tenant_id,
                    app_slug,
                    anchor,
                    installation_id_for_anchor(pg, tenant_id, anchor)
                        .await
                        .unwrap_or_default(),
                    environment,
                    token_path,
                    existing_token,
                    existing_claims,
                    options.persist_credential,
                    options.regenerate_access_token,
                    false,
                )
                .await;
            }
        }
    }

    // Full path: create or adopt the deploy application and reconcile its IAM
    // installation. A legacy non-numeric anchor (`sdkwork-web`) is treated as
    // the deployment slug it names, never as an anchor: the anchor must be the
    // deploy application id (`APPLICATION_IDENTITY_SPEC.md` §2).
    let resolved_anchor = match candidate_anchor {
        Some(anchor) => anchor,
        None => {
            let slug = deployment_slug_for_lookup(app_slug, configured_anchor.as_deref());
            match ensure_deploy_application(
                tenant_id,
                organization_id,
                manifest,
                environment,
                &slug,
            )
            .await
            {
                Ok(anchor) => anchor,
                Err(error) => {
                    // A deployment that was handed an explicit anchor can still
                    // serve login if the Deploy module is not composed; it just
                    // cannot publish. Prefer an honest warning over a dead portal.
                    match configured_anchor.as_deref() {
                        Some(anchor) => {
                            warn!(
                                %error,
                                %anchor,
                                "deploy application provisioning was unavailable; continuing with the configured anchor"
                            );
                            anchor.to_owned()
                        }
                        None => return Err(error),
                    }
                }
            }
        }
    };

    let (installation_id, installation_reconciled) = reconcile_installation(
        pg,
        manifest,
        app_root,
        tenant_id,
        organization_id,
        &resolved_anchor,
        configured_anchor.as_deref(),
        environment,
    )
    .await?;
    validate_enabled_tenant_runtime_app(pg, tenant_id, &resolved_anchor)
        .await
        .map_err(|error| {
            format!(
                "installation {installation_id} for deploy application {resolved_anchor} is not \
                 enabled after reconciliation: {error}"
            )
        })?;

    // Keep every downstream IAM call on the same anchor this run resolved.
    env::set_var(DEPLOY_APPLICATION_ID_ENV, &resolved_anchor);

    finish_with_token(
        pg,
        tenant_id,
        app_slug,
        &resolved_anchor,
        installation_id,
        environment,
        token_path,
        existing_token,
        existing_claims,
        options.persist_credential,
        options.regenerate_access_token,
        installation_reconciled,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn finish_with_token(
    _pg: &PgPool,
    tenant_id: &str,
    app_slug: &str,
    anchor: &str,
    installation_id: String,
    environment: &str,
    token_path: &Path,
    file_token: Option<String>,
    file_claims: Option<TokenClaims>,
    persist_credential: bool,
    regenerate: bool,
    installation_reconciled: bool,
) -> Result<DeploymentBootstrapOutcome, String> {
    // Keep every downstream IAM call on the anchor this run resolved, including
    // the fast path where the anchor came from the reusable token's claim.
    env::set_var(DEPLOY_APPLICATION_ID_ENV, anchor);
    if !persist_credential {
        let updated = match materialize_static_runtime_identity(tenant_id, anchor) {
            Ok(updated) => updated,
            Err(error) => {
                warn!(%error, "static runtime identity materialization was skipped");
                Vec::new()
            }
        };
        info!(
            tenant_id,
            app_slug,
            deploy_application_id = anchor,
            installation_id = installation_id.as_str(),
            "deployment identity is ready; credential persistence is disabled by the browser-injection policy"
        );
        return Ok(DeploymentBootstrapOutcome {
            tenant_id: tenant_id.to_owned(),
            app_slug: app_slug.to_owned(),
            deploy_application_id: anchor.to_owned(),
            installation_id,
            token_path: String::new(),
            token_reused: false,
            installation_reconciled,
            static_runtime_envs_updated: updated,
        });
    }
    let env_token = env_trimmed("SDKWORK_ACCESS_TOKEN");
    let env_claims = env_token.as_deref().and_then(parse_token_claims);

    let (_token, reused) = if regenerate {
        let token = issue_token(tenant_id, anchor).await?;
        write_token_file(token_path, &token)?;
        (token, false)
    } else if let Some(token) = env_token.filter(|token| {
        token_is_reusable(token, env_claims.as_ref(), tenant_id, anchor, environment)
    }) {
        // An explicitly configured token wins unchanged; mirror it to the file
        // the browser injection path reads, without re-signing it.
        if file_token.as_deref() != Some(token.as_str()) {
            write_token_file(token_path, &token)?;
        }
        (token, true)
    } else if let Some(token) = file_token.filter(|token| {
        token_is_reusable(token, file_claims.as_ref(), tenant_id, anchor, environment)
    }) {
        (token, true)
    } else {
        let token = issue_token(tenant_id, anchor).await?;
        write_token_file(token_path, &token)?;
        (token, false)
    };

    let updated = match materialize_static_runtime_identity(tenant_id, anchor) {
        Ok(updated) => updated,
        Err(error) => {
            // Static identity is a presentation convenience; a read-only or
            // module-owned static root must not stop the credential from landing.
            warn!(%error, "static runtime identity materialization was skipped");
            Vec::new()
        }
    };
    info!(
        tenant_id,
        app_slug,
        deploy_application_id = anchor,
        installation_id = installation_id.as_str(),
        token_path = %token_path.display(),
        token_reused = reused,
        "deployment identity and credential-entry bootstrap Access-Token are ready"
    );

    Ok(DeploymentBootstrapOutcome {
        tenant_id: tenant_id.to_owned(),
        app_slug: app_slug.to_owned(),
        deploy_application_id: anchor.to_owned(),
        installation_id,
        token_path: token_path.display().to_string(),
        token_reused: reused,
        installation_reconciled,
        static_runtime_envs_updated: updated,
    })
}

/// Creates or adopts one deploy application and returns its numeric anchor.
async fn ensure_deploy_application(
    tenant_id: &str,
    organization_id: &str,
    manifest: &SdkworkAppManifest,
    environment: &str,
    slug: &str,
) -> Result<String, String> {
    let tenant = tenant_id
        .parse::<i64>()
        .map_err(|_| format!("tenant id {tenant_id} is not a decimal int64"))?;
    let organization = organization_id
        .parse::<i64>()
        .map_err(|_| format!("organization id {organization_id} is not a decimal int64"))?;
    // The Deploy owner assembly is the only composition entrypoint (API_ASSEMBLY_SPEC
    // §6.1). It resolves the owner's own pool and service; this gateway does not
    // reach into the Deploy service crate or repository.
    let assembly = sdkwork_api_deployments_assembly::assemble_business_routes()
        .await
        .map_err(|error| {
            format!("compose the Deploy service for app provisioning failed: {error}")
        })?;
    let service = assembly.service;
    let context = DeployAppRequestContext {
        tenant_id: tenant,
        actor_id: None,
        organization_id: Some(organization),
        ..DeployAppRequestContext::default()
    };
    let query = ListAppsQuery {
        page: 1,
        page_size: 50,
        keyword: Some(slug.to_owned()),
        ..ListAppsQuery::default()
    };
    let page = DeployAppApi::list_apps(service.as_ref(), &context, &query)
        .await
        .map_err(|error| format!("list deploy applications by slug {slug} failed: {error}"))?;
    let existing = page
        .items
        .into_iter()
        .find(|app| app.slug.eq_ignore_ascii_case(slug));
    let app = match existing {
        Some(app) => app,
        None => {
            let idempotency_key = bootstrap_idempotency_key(tenant_id, slug);
            let request = CreateAppRequest {
                name: manifest.app.name.trim().to_owned(),
                slug: Some(slug.to_owned()),
                app_kind: AppKind::ApiService,
                owner_type: Some(AppOwnerType::Tenant),
                description: None,
                app_type: None,
                runtime_config: None,
                metadata: Some(serde_json::json!({
                    "source": "sdkwork-webserver-startup-bootstrap",
                })),
                default_environment: Some(environment.to_owned()),
                app_domain_label: None,
                app_domain_suffixes: None,
                source_specs: None,
                idempotency_key: Some(idempotency_key.clone()),
            };
            match DeployAppApi::create_app(
                service.as_ref(),
                &context,
                Some(idempotency_key.as_str()),
                &request,
            )
            .await
            {
                Ok(app) => app,
                Err(error) => {
                    // A concurrent first boot may have won the unique slug. Adopt
                    // the winner instead of failing this process's login.
                    let page = DeployAppApi::list_apps(service.as_ref(), &context, &query)
                        .await
                        .map_err(|list_error| {
                            format!(
                                "create deploy application {slug} failed ({error}); re-reading the \
                                 concurrent winner also failed: {list_error}"
                            )
                        })?;
                    page.items
                        .into_iter()
                        .find(|app| app.slug.eq_ignore_ascii_case(slug))
                        .ok_or_else(|| {
                            format!("create deploy application {slug} failed: {error}")
                        })?
                }
            }
        }
    };

    // The application row is the authority; identity provisioning is replayed
    // idempotently and its failure is recorded on the row by the Deploy service.
    // The installation is still reconciled explicitly below, because login must
    // not depend on a cross-module step that may have been degraded.
    let backend_context = DeployBackendRequestContext {
        operator_id: None,
        tenant_id: Some(tenant),
    };
    if let Err(error) = DeployBackendApi::retry_app_identity_provisioning(
        service.as_ref(),
        &backend_context,
        &app.id,
    )
    .await
    {
        warn!(
            app = %app.id,
            %error,
            "Deploy identity provisioning replay failed; the installation is reconciled directly"
        );
    }

    if app.deploy_application_id <= 0 {
        return Err(format!(
            "deploy application {slug} has no positive deploy_application_id; an installation \
             cannot be anchored"
        ));
    }
    Ok(app.deploy_application_id.to_string())
}

/// Provisions, reconciles, and enables the installation bound to `anchor`.
async fn reconcile_installation(
    pg: &PgPool,
    manifest: &SdkworkAppManifest,
    _app_root: &Path,
    tenant_id: &str,
    organization_id: &str,
    anchor: &str,
    configured_anchor: Option<&str>,
    environment: &str,
) -> Result<(String, bool), String> {
    let options = EmbeddedApplicationBootstrapOptions {
        environment: environment.to_owned(),
        primary_domain_override: deployment_primary_domain(),
        ..EmbeddedApplicationBootstrapOptions::default()
    };
    let runtime_app_id = manifest
        .backend
        .app_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(manifest.app.key.as_str())
        .to_owned();
    let binding = EmbeddedApplicationRuntimeBinding {
        runtime_app_id: runtime_app_id.clone(),
        display_name: Some(manifest.app.name.clone()),
        app_key_override: None,
        instance_key_override: None,
    };
    // Existing deployments may carry an installation anchored on the legacy
    // manifest runtime id or on the configured slug (`sdkwork-web`). Relink that
    // row to the deploy application anchor before provisioning instead of
    // inserting a second row for the same app_id/instance_key.
    ensure_installation_anchor(pg, tenant_id, anchor, &runtime_app_id, configured_anchor).await?;
    let ensure = manifest_to_ensure_command(manifest, &options, Some(&binding))?;

    let mut permissions = ensure.default_access_permissions.clone();
    if !permissions
        .iter()
        .any(|permission| permission == SELF_SCOPE)
    {
        permissions.push(SELF_SCOPE.to_owned());
    }
    if permissions.is_empty() {
        return Err(
            "the application manifest declares no access-token permission scope; an installation \
             cannot be enabled without access permissions"
                .to_owned(),
        );
    }

    let mut command = TenantApplicationProvisionCommand {
        tenant_id: tenant_id.to_owned(),
        organization_id: organization_id.to_owned(),
        app_id: Some(runtime_app_id),
        instance_key: ensure.instance_key,
        display_name: ensure.display_name,
        environment: ensure.environment,
        application_type: Some(ensure.app_type),
        primary_domain: Some(ensure.primary_domain),
        access_permissions: permissions,
        runtime_config: serde_json::json!({}),
        deploy_application_id: Some(anchor.to_owned()),
        ownership_scope: Some("TENANT".to_owned()),
        owner_user_id: None,
        provisioning_idempotency_key: Some(format!("deploy:{anchor}")),
    };

    let application = match provision_tenant_application(pg, &command).await {
        Ok(application) => application,
        Err(error) if error.contains("immutable") && error.contains("appId") => {
            // The deploy plane may have created the installation with an IAM
            // assigned runtime app id. Keep it; only the deploy anchor is ours
            // to reconcile.
            command.app_id = None;
            provision_tenant_application(pg, &command)
                .await
                .map_err(|retry| {
                    format!("reconcile IAM installation for anchor {anchor} failed: {retry}")
                })?
        }
        Err(error) => {
            return Err(format!(
                "reconcile IAM installation for anchor {anchor} failed: {error}"
            ))
        }
    };

    let application = if application.status == "enabled" {
        application
    } else {
        enable_tenant_application(pg, tenant_id, &application.id)
            .await
            .map_err(|error| {
                format!("enable IAM installation {} failed: {error}", application.id)
            })?
    };

    let reconciled = application.status == "enabled";
    Ok((application.id, reconciled))
}

/// Points an existing installation at the deploy application anchor.
///
/// A packaged deployment upgraded from an older release can already carry a row
/// anchored on the manifest runtime id (`sdkwork-webserver`) or on the legacy
/// configured slug (`sdkwork-web`). Provisioning by the new anchor would try to
/// insert a second row with the same app id/instance key and hit the unique
/// index. Relinking the existing row is the forward-only repair
/// (`APPLICATION_IDENTITY_SPEC.md` §2): no row is deleted and no installation is
/// duplicated.
async fn ensure_installation_anchor(
    pg: &PgPool,
    tenant_id: &str,
    anchor: &str,
    runtime_app_id: &str,
    configured_anchor: Option<&str>,
) -> Result<(), String> {
    if resolve_tenant_application_by_deploy_application_id(pg, tenant_id, anchor)
        .await?
        .is_some()
    {
        return Ok(());
    }
    let mut candidates = Vec::with_capacity(2);
    if let Some(legacy) = configured_anchor
        .map(str::trim)
        .filter(|legacy| !legacy.is_empty() && !is_decimal_anchor(legacy) && *legacy != anchor)
    {
        candidates.push(legacy);
    }
    if runtime_app_id != anchor {
        candidates.push(runtime_app_id);
    }
    for legacy in candidates {
        if resolve_tenant_application_by_deploy_application_id(pg, tenant_id, legacy)
            .await?
            .is_none()
        {
            continue;
        }
        relink_tenant_application_deploy_application_id(pg, tenant_id, legacy, anchor)
            .await
            .map_err(|error| {
                format!(
                    "relink IAM installation from legacy anchor {legacy} to deploy application \
                     {anchor} failed: {error}"
                )
            })?;
        info!(
            tenant_id,
            legacy_anchor = legacy,
            deploy_application_id = anchor,
            "relinked an existing IAM installation to the deploy application anchor"
        );
        return Ok(());
    }
    Ok(())
}

async fn installation_is_enabled(pg: &PgPool, tenant_id: &str, anchor: &str) -> bool {
    validate_enabled_tenant_runtime_app(pg, tenant_id, anchor)
        .await
        .is_ok()
}

async fn installation_id_for_anchor(
    pg: &PgPool,
    tenant_id: &str,
    anchor: &str,
) -> Result<String, String> {
    let application =
        resolve_tenant_application_by_deploy_application_id(pg, tenant_id, anchor).await?;
    Ok(application
        .map(|application| application.id)
        .unwrap_or_default())
}

/// Whether the shared browser-injection policy permits a bootstrap token here.
///
/// Mirrors `IAM_CREDENTIAL_ENTRY_SPEC.md` §4/§5 and the container predicate:
/// development, or test behind the documented isolated-runner opt-in. Staging,
/// demo, and production never receive the token in browser artifacts, so an
/// automatic startup must not recreate the secret file the entrypoint purges.
#[must_use]
pub fn browser_injection_allowed_from_env() -> bool {
    match resolve_environment().as_str() {
        "development" => true,
        "test" => env_flag("SDKWORK_WEBSERVER_CREDENTIAL_ENTRY_BOOTSTRAP_ALLOW_TEST"),
        _ => false,
    }
}

async fn issue_token(tenant_id: &str, anchor: &str) -> Result<String, String> {
    env::remove_var("SDKWORK_ACCESS_TOKEN");
    resolve_deployment_bootstrap_access_token(Some(tenant_id), Some(anchor))
        .await?
        .ok_or_else(|| {
            "IAM database is unavailable; cannot issue the credential-entry bootstrap Access-Token"
                .to_owned()
        })
}

/// Materializes tenant/anchor/application identity into each installed SPA root.
///
/// Only non-secret fields are written; the token stays in the `0600` secret file.
fn materialize_static_runtime_identity(
    tenant_id: &str,
    anchor: &str,
) -> Result<Vec<String>, String> {
    let mut updated = Vec::new();
    for key in [
        "SDKWORK_WEBSERVER_PC_STATIC_ROOT",
        "SDKWORK_WEBSERVER_H5_STATIC_ROOT",
    ] {
        let Some(root) = env::var_os(key)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
        else {
            continue;
        };
        let path = root.join("runtime-env.json");
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            continue;
        }
        let raw = fs::read_to_string(&path)
            .map_err(|error| format!("read {} failed: {error}", path.display()))?;
        let mut value: Value = serde_json::from_str(&raw)
            .map_err(|error| format!("parse {} failed: {error}", path.display()))?;
        if !apply_runtime_identity(&mut value, tenant_id, anchor)? {
            continue;
        }
        let rendered = format!(
            "{}\n",
            serde_json::to_string_pretty(&value)
                .map_err(|error| format!("render {} failed: {error}", path.display()))?
        );
        write_file_atomically(&path, rendered.as_bytes(), 0o644)?;
        updated.push(path.display().to_string());
    }
    Ok(updated)
}

/// Adds the non-secret deployment identity to one browser runtime document.
///
/// Returns `false` when every field already matches, so a repeated startup does
/// not rewrite the file. The token is deliberately absent: it must never reach a
/// built artifact.
fn apply_runtime_identity(
    value: &mut Value,
    tenant_id: &str,
    anchor: &str,
) -> Result<bool, String> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| "runtime-env.json must contain a JSON object".to_owned())?;
    let before = object.clone();
    object.insert("tenantId".to_owned(), Value::String(tenant_id.to_owned()));
    object.insert(
        "deployApplicationId".to_owned(),
        Value::String(anchor.to_owned()),
    );
    object.insert(
        "applicationCode".to_owned(),
        Value::String("webserver".to_owned()),
    );
    Ok(*object != before)
}

// ---------------------------------------------------------------------------
// Identity resolution and token reuse (pure where possible)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Default)]
struct TokenClaims {
    #[serde(default)]
    tenant_id: Option<String>,
    #[serde(default)]
    app_id: Option<String>,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    exp: Option<i64>,
}

fn parse_token_claims(token: &str) -> Option<TokenClaims> {
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let payload = parts.next()?;
    let _signature = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()?;
    serde_json::from_slice(&decoded).ok()
}

fn token_is_reusable(
    token: &str,
    claims: Option<&TokenClaims>,
    tenant_id: &str,
    anchor: &str,
    environment: &str,
) -> bool {
    let token = token.trim();
    if token.is_empty() {
        return false;
    }
    // A non-JWT value is an opaque operator-supplied credential: reuse it and
    // let the runtime parser reject it if the backend needs a JWT.
    let mut parts = token.split('.');
    let (Some(_header), Some(_payload), Some(signature)) =
        (parts.next(), parts.next(), parts.next())
    else {
        return true;
    };
    if parts.next().is_some() {
        return true;
    }
    if signature.eq_ignore_ascii_case("signature")
        && normalize_environment(environment) != "development"
    {
        // An unsigned development fixture must never be reused outside
        // development (`IAM_APPLICATION_BOOTSTRAP_SPEC.md` §5.4.2).
        return false;
    }
    let Some(claims) = claims else {
        return false;
    };
    let now = chrono::Utc::now().timestamp();
    if let Some(exp) = claims.exp {
        if exp != 0 && exp <= now + 60 {
            return false;
        }
    }
    if let Some(value) = claims.tenant_id.as_deref() {
        if value.trim() != tenant_id {
            return false;
        }
    }
    if let Some(value) = claims.app_id.as_deref() {
        if value.trim() != anchor {
            return false;
        }
    }
    if let Some(value) = claims.environment.as_deref() {
        if normalize_environment(value) != normalize_environment(environment) {
            return false;
        }
    }
    true
}

fn resolve_tenant_id(
    options: &DeploymentBootstrapOptions,
    manifest: &SdkworkAppManifest,
) -> String {
    options
        .tenant_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            manifest
                .backend
                .tenant_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or(DEFAULT_TENANT_ID)
        .to_owned()
}

fn resolve_app_slug(options: &DeploymentBootstrapOptions, manifest: &SdkworkAppManifest) -> String {
    options
        .app_slug
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| env_trimmed(DEPLOY_APPLICATION_SLUG_ENV))
        .unwrap_or_else(|| manifest.app.key.trim().to_owned())
}

fn resolve_configured_anchor(
    options: &DeploymentBootstrapOptions,
    manifest: &SdkworkAppManifest,
) -> Option<String> {
    options
        .deploy_application_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| env_trimmed(DEPLOY_APPLICATION_ID_ENV))
        .or_else(|| {
            manifest
                .backend
                .deploy_application_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
}

/// A legacy anchor that names a slug (`sdkwork-web`) is resolved as a slug.
fn deployment_slug_for_lookup(app_slug: &str, configured_anchor: Option<&str>) -> String {
    configured_anchor
        .filter(|anchor| !is_decimal_anchor(anchor))
        .map(str::trim)
        .filter(|anchor| !anchor.is_empty())
        .unwrap_or(app_slug)
        .to_owned()
}

fn is_decimal_anchor(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) && value != "0"
}

fn resolve_environment() -> String {
    for key in [
        "SDKWORK_WEBSERVER_ENVIRONMENT",
        "SDKWORK_ENVIRONMENT",
        "SDKWORK_CLOUDROUTER_ROUTER_ENVIRONMENT",
        "SDKWORK_CLOUDROUTER_ENVIRONMENT",
    ] {
        if let Some(value) = env_trimmed(key) {
            return normalize_environment(&value);
        }
    }
    "development".to_owned()
}

/// Hostname the installation records as its primary domain.
///
/// The public URL is the value the installer actually exposes; using its host
/// keeps the installation's primary domain from falling back to `localhost` in
/// a real deployment. An explicit `SDKWORK_WEBSERVER_PRIMARY_DOMAIN` wins.
fn deployment_primary_domain() -> Option<String> {
    if let Some(domain) = env_trimmed("SDKWORK_WEBSERVER_PRIMARY_DOMAIN") {
        return Some(domain);
    }
    let url = env_trimmed("SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL")?;
    url::Url::parse(&url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
}

fn normalize_environment(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "dev" | "development" | "local" => "development",
        "test" | "testing" => "test",
        "stage" | "staging" => "staging",
        "demo" => "demo",
        "prod" | "production" => "production",
        other => other,
    }
    .to_owned()
}

fn bootstrap_idempotency_key(tenant_id: &str, slug: &str) -> String {
    format!("sdkwork-webserver-bootstrap:{tenant_id}:{slug}")
}

fn resolve_app_root(configured: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(root) = configured {
        return Ok(root);
    }
    // The packaged roots were materialized above; this fallback keeps the
    // source-tree `cargo run` path working without an explicit SDKWORK_APP_ROOT.
    Ok(
        sdkwork_iam_web_adapter::resolve_application_app_root_with_fallback(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
        ),
    )
}

fn default_token_path() -> PathBuf {
    PathBuf::from(CREDENTIAL_ENTRY_BOOTSTRAP_TOKEN_PATH)
}

fn env_trimmed(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn env_flag(key: &str) -> bool {
    env_trimmed(key).is_some_and(|value| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn read_token_candidate(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn write_token_file(path: &Path, token: &str) -> Result<(), String> {
    write_file_atomically(path, format!("{token}\n").as_bytes(), 0o600)
}

/// Writes a complete file through a same-directory temporary file and rename.
///
/// The rename is atomic on POSIX. On Windows it can fail when the target
/// already exists, so the fallback rewrites the file in place; callers are
/// startup paths where a torn read is not observable by a request handler.
fn write_file_atomically(path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("create {} failed: {error}", parent.display()))?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("sdkwork-bootstrap");
    let temporary = parent.join(format!(".{file_name}.tmp-{}-{nanos}", std::process::id()));
    fs::write(&temporary, bytes)
        .map_err(|error| format!("write {} failed: {error}", temporary.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(mode))
            .map_err(|error| format!("chmod {} failed: {error}", temporary.display()))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(_) => {
            let written = fs::write(path, bytes)
                .map_err(|error| format!("write {} failed: {error}", path.display()));
            let _ = fs::remove_file(&temporary);
            written
        }
    }
}

fn advisory_lock_key(tenant_id: &str, slug: &str) -> i64 {
    let digest =
        Sha256::digest(format!("sdkwork-webserver-bootstrap:{tenant_id}:{slug}").as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    i64::from_be_bytes(bytes)
}

async fn acquire_bootstrap_lock(pg: &PgPool, key: i64) -> Option<PoolConnection<Postgres>> {
    // Never hold the pool's only connection across the provisioning work below:
    // the work itself needs connections, and a single-connection test pool would
    // deadlock. The lock is an optimization; the unique indexes remain the
    // guarantee.
    if pg.options().get_max_connections() <= 1 {
        return None;
    }
    for _ in 0..BOOTSTRAP_LOCK_ATTEMPTS {
        let mut connection = pg.acquire().await.ok()?;
        match sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_lock($1)")
            .bind(key)
            .fetch_one(&mut *connection)
            .await
        {
            Ok(true) => return Some(connection),
            Ok(false) | Err(_) => {
                drop(connection);
                tokio::time::sleep(BOOTSTRAP_LOCK_POLL).await;
            }
        }
    }
    // Contention is not a failure: the writes below are idempotent and the
    // database still enforces one application and one installation.
    warn!("deployment bootstrap advisory lock was not acquired; continuing idempotently");
    None
}

async fn release_bootstrap_lock(connection: &mut PoolConnection<Postgres>, key: i64) {
    let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(key)
        .execute(&mut **connection)
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(payload: Value, signature: &str) -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"alg":"RS256","typ":"JWT"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).unwrap());
        format!("{header}.{payload}.{signature}")
    }

    #[test]
    fn a_matching_enabled_anchor_token_is_reused_byte_identically() {
        let payload = serde_json::json!({
            "tenant_id": "100001",
            "app_id": "1845123456789012345",
            "environment": "production",
            "exp": 0
        });
        let value = token(payload, "signature-ok");
        let claims = parse_token_claims(&value).expect("claims");
        assert!(token_is_reusable(
            &value,
            Some(&claims),
            "100001",
            "1845123456789012345",
            "production"
        ));
    }

    #[test]
    fn a_token_for_another_anchor_or_tenant_is_not_reused() {
        let payload = serde_json::json!({
            "tenant_id": "200002",
            "app_id": "1845123456789012345",
            "environment": "production"
        });
        let value = token(payload, "signature-ok");
        let claims = parse_token_claims(&value).expect("claims");
        assert!(!token_is_reusable(
            &value,
            Some(&claims),
            "100001",
            "1845123456789012345",
            "production"
        ));
        assert!(!token_is_reusable(
            &value,
            Some(&claims),
            "200002",
            "999",
            "production"
        ));
    }

    #[test]
    fn an_expired_token_is_regenerated_and_no_expiry_is_reused() {
        let expired = token(serde_json::json!({"exp": 1}), "signature-ok");
        let claims = parse_token_claims(&expired).expect("claims");
        assert!(!token_is_reusable(
            &expired,
            Some(&claims),
            "100001",
            "1",
            "production"
        ));
        let no_expiry = token(
            serde_json::json!({"tenant_id": "100001", "app_id": "1"}),
            "signature-ok",
        );
        let claims = parse_token_claims(&no_expiry).expect("claims");
        assert!(token_is_reusable(
            &no_expiry,
            Some(&claims),
            "100001",
            "1",
            "production"
        ));
    }

    #[test]
    fn development_fixtures_are_only_reusable_in_development() {
        let fixture = token(
            serde_json::json!({"tenant_id": "100001", "app_id": "1"}),
            "signature",
        );
        let claims = parse_token_claims(&fixture).expect("claims");
        assert!(token_is_reusable(
            &fixture,
            Some(&claims),
            "100001",
            "1",
            "development"
        ));
        for environment in ["test", "staging", "demo", "production"] {
            assert!(
                !token_is_reusable(&fixture, Some(&claims), "100001", "1", environment),
                "fixture must not be reused in {environment}"
            );
        }
    }

    #[test]
    fn an_opaque_operator_token_is_reused_as_is() {
        assert!(token_is_reusable(
            "opaque-operator-token",
            None,
            "100001",
            "1",
            "production"
        ));
    }

    #[test]
    fn legacy_slug_anchors_are_resolved_as_a_slug_not_an_anchor() {
        assert_eq!(
            deployment_slug_for_lookup("sdkwork-webserver", Some("sdkwork-web")),
            "sdkwork-web"
        );
        assert_eq!(
            deployment_slug_for_lookup("sdkwork-webserver", Some("1845123456789012345")),
            "sdkwork-webserver"
        );
        assert_eq!(
            deployment_slug_for_lookup("sdkwork-webserver", None),
            "sdkwork-webserver"
        );
    }

    #[test]
    fn environment_normalization_covers_packaged_aliases() {
        assert_eq!(normalize_environment("dev"), "development");
        assert_eq!(normalize_environment("PROD"), "production");
        assert_eq!(normalize_environment(" stage "), "staging");
        assert_eq!(normalize_environment("demo"), "demo");
        assert_eq!(normalize_environment("test"), "test");
    }

    #[test]
    fn bootstrap_idempotency_key_is_stable_per_tenant_and_slug() {
        assert_eq!(
            bootstrap_idempotency_key("100001", "sdkwork-webserver"),
            bootstrap_idempotency_key("100001", "sdkwork-webserver")
        );
        assert_ne!(
            bootstrap_idempotency_key("100001", "sdkwork-webserver"),
            bootstrap_idempotency_key("100002", "sdkwork-webserver")
        );
    }

    #[test]
    fn decimal_anchor_detection_rejects_slugs_and_zero() {
        assert!(is_decimal_anchor("1845123456789012345"));
        assert!(!is_decimal_anchor("sdkwork-web"));
        assert!(!is_decimal_anchor("0"));
        assert!(!is_decimal_anchor(""));
    }

    #[test]
    fn runtime_identity_materialization_is_idempotent_and_preserves_other_fields() {
        let mut value = serde_json::json!({
            "environment": "production",
            "profileId": "standalone.production",
            "appApiBaseUrl": "/"
        });
        assert!(
            apply_runtime_identity(&mut value, "100001", "1845123456789012345")
                .expect("apply identity")
        );
        assert_eq!(value["deployApplicationId"], "1845123456789012345");
        assert_eq!(value["environment"], "production");
        assert_eq!(value["appApiBaseUrl"], "/");
        assert_eq!(
            apply_runtime_identity(&mut value, "100001", "1845123456789012345")
                .expect("reapply identity"),
            false,
            "a repeated startup must not rewrite the file"
        );
    }

    #[test]
    fn token_file_write_is_complete_and_atomic_in_place() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory
            .path()
            .join("credential-entry-bootstrap-access-token");
        write_token_file(&path, "header.payload.signature").expect("write token");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "header.payload.signature\n"
        );
        let leftovers = fs::read_dir(directory.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count();
        assert_eq!(leftovers, 0, "the temporary file must be renamed away");
    }
}
