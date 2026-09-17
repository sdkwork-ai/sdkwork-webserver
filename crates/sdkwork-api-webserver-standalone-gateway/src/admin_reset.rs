//! Bootstrap administrator password reset for the standalone gateway.
//!
//! Password recovery cannot travel through an authenticated surface: the
//! operator has lost exactly the credential that surface requires. This module
//! mirrors `sdkwork-cloudrouter`'s `cloudrouterctl reset-admin` and writes the
//! IAM password credential row directly, with the same Argon2id parameters and
//! against the same canonical tenant / administrator subject the IAM bootstrap
//! owns (`sdkwork-iam-bootstrap`).
//!
//! Two invariants keep the recovery path safe:
//!
//! - the new password never appears in the process argument list; it is read
//!   from [`SDKWORK_WEBSERVER_ADMIN_RESET_PASSWORD_ENV`] so `ps` / Task Manager
//!   cannot observe it, and the `pnpm admin:reset:*` driver sets that variable
//!   for the child process;
//! - the command verifies the stored hash after writing it, so a reset that
//!   reports success has already proven that the credential row in the database
//!   accepts the new password.

use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use sdkwork_iam_bootstrap::{
    DEFAULT_BOOTSTRAP_ADMIN_USERNAME, DEFAULT_BOOTSTRAP_ADMIN_USER_ID, DEFAULT_IAM_TENANT_ID,
};
use sqlx::PgPool;

/// Environment variable carrying the replacement password.
///
/// The `pnpm admin:reset:*` scripts set this for the child process so the secret
/// stays out of the command line.
pub const SDKWORK_WEBSERVER_ADMIN_RESET_PASSWORD_ENV: &str =
    "SDKWORK_WEBSERVER_ADMIN_RESET_PASSWORD";

/// Service name used for environment-driven database resolution. It only
/// derives a table prefix, which
/// [`sdkwork_database_sqlx::create_process_shared_pool_from_env`] clears because
/// module migrations own literal table names.
const DATABASE_SERVICE_NAME: &str = "SDKWORK_WEBSERVER_ADMIN_RESET";

/// `iam_credential.credential_type` of the password credential.
const PASSWORD_CREDENTIAL_TYPE: &str = "password";

/// Matches the shared installer floor so every SDKWork product rejects the same
/// weak administrator secrets.
const MIN_PASSWORD_LENGTH: usize = 8;

const USAGE: &str = "usage: reset-admin [--username <username>] [--tenant-id <tenant-id>]; \
                     the new password is read from SDKWORK_WEBSERVER_ADMIN_RESET_PASSWORD";

/// Administrator identity targeted by a reset.
///
/// Every field is optional: the default target is the IAM bootstrap
/// administrator of the platform tenant, which is what an environment that ran
/// `pnpm dev` or a packaged install actually provisioned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdminResetOptions {
    pub username: Option<String>,
    pub tenant_id: Option<String>,
}

impl AdminResetOptions {
    /// Parse the `reset-admin` argument list.
    ///
    /// Only identity selection is accepted here. The password is rejected on
    /// purpose: accepting it as an argument would publish the secret to every
    /// process observer on the host.
    pub fn from_arguments(arguments: &[String]) -> Result<Self, String> {
        let mut options = Self::default();
        let mut index = 0;
        while index < arguments.len() {
            let token = arguments[index].as_str();
            match token {
                "--username" => {
                    options.username = Some(value_after(arguments, index, token)?);
                }
                "--tenant-id" => {
                    options.tenant_id = Some(value_after(arguments, index, token)?);
                }
                other if other.starts_with("--password") => {
                    return Err(format!(
                        "the password must not be passed as an argument; set \
                         {SDKWORK_WEBSERVER_ADMIN_RESET_PASSWORD_ENV} or pass --password to the \
                         pnpm script"
                    ));
                }
                other => return Err(format!("unknown reset-admin option `{other}`; {USAGE}")),
            }
            index += 2;
        }
        Ok(options)
    }

    fn tenant_id(&self) -> &str {
        trimmed_or(self.tenant_id.as_deref(), DEFAULT_IAM_TENANT_ID)
    }

    fn username(&self) -> &str {
        trimmed_or(self.username.as_deref(), DEFAULT_BOOTSTRAP_ADMIN_USERNAME)
    }
}

fn value_after(arguments: &[String], index: usize, flag: &str) -> Result<String, String> {
    let value = arguments
        .get(index + 1)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty() && !value.starts_with("--"))
        .ok_or_else(|| format!("{flag} requires a value; {USAGE}"))?;
    Ok(value)
}

fn trimmed_or<'a>(value: Option<&'a str>, fallback: &'a str) -> &'a str {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
}

/// Result of a successful reset, printed as JSON for scripts and operators.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AdminResetOutput {
    pub status: &'static str,
    pub user_id: String,
    pub tenant_id: String,
    pub username: String,
    /// False when the target administrator had no password credential yet (for
    /// example an environment that bootstrapped the subject without a password).
    pub credential_existed: bool,
    /// True when the stored hash was read back and accepted the new password.
    pub verified: bool,
}

/// Read the replacement password from the protected environment variable.
pub fn resolve_reset_password_from_env() -> Result<String, String> {
    let password = std::env::var(SDKWORK_WEBSERVER_ADMIN_RESET_PASSWORD_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "{SDKWORK_WEBSERVER_ADMIN_RESET_PASSWORD_ENV} is required; \
                 pass --password to the pnpm script or export the variable"
            )
        })?;
    validate_password(&password)?;
    Ok(password)
}

/// Reject secrets that would immediately weaken the environment they unlock.
pub fn validate_password(password: &str) -> Result<(), String> {
    if password.trim().len() < MIN_PASSWORD_LENGTH {
        return Err(format!(
            "admin reset password must be at least {MIN_PASSWORD_LENGTH} characters"
        ));
    }
    Ok(())
}

/// Resolve the database from the environment, reset the password, and verify it.
pub async fn reset_admin_account_from_env(
    options: AdminResetOptions,
) -> Result<AdminResetOutput, String> {
    let password = resolve_reset_password_from_env()?;
    let pool = sdkwork_database_sqlx::create_process_shared_pool_from_env(DATABASE_SERVICE_NAME)
        .await
        .map_err(|error| format!("database connection failed: {error}"))?
        .ok_or_else(|| {
            "no server database is configured; set SDKWORK_DATABASE_URL or the \
             SDKWORK_DATABASE_* fields (for example through --dev-env-file)"
                .to_owned()
        })?;
    let postgres = pool.as_postgres().ok_or_else(|| {
        "admin reset requires PostgreSQL: the IAM credentials of a client-local SQLite \
         profile are not authoritative for the server deployment"
            .to_owned()
    })?;
    reset_admin_password(postgres, &options, &password).await
}

/// Write the password credential of the target administrator and verify it.
pub async fn reset_admin_password(
    pool: &PgPool,
    options: &AdminResetOptions,
    password: &str,
) -> Result<AdminResetOutput, String> {
    validate_password(password)?;
    let tenant_id = options.tenant_id();
    let username = options.username();
    let user_id = resolve_admin_user_id(pool, tenant_id, username).await?;
    let credential_existed = password_credential_exists(pool, tenant_id, &user_id).await?;
    let password_hash = hash_password(password)?;

    sqlx::query(
        "INSERT INTO iam_credential (id, tenant_id, user_id, credential_type, credential_hash, \
         failed_attempts, status, created_at, updated_at) \
         VALUES ($1, $2, $3, 'password', $4, 0, 'active', $5, $5) \
         ON CONFLICT (tenant_id, user_id, credential_type) DO UPDATE SET \
           credential_hash = EXCLUDED.credential_hash, \
           failed_attempts = 0, \
           locked_until = NULL, \
           status = 'active', \
           updated_at = EXCLUDED.updated_at",
    )
    .bind(bootstrap_credential_id(&user_id))
    .bind(tenant_id)
    .bind(&user_id)
    .bind(&password_hash)
    .bind(chrono::Utc::now())
    .execute(pool)
    .await
    .map_err(|error| format!("failed to store the new admin credential: {error}"))?;

    // Read the row back and verify it against the requested password: a reset
    // that reports success must already prove the stored hash is usable, so an
    // operator never learns about a broken credential on the next login attempt.
    verify_stored_password(pool, tenant_id, &user_id, password).await?;

    let resolved_username = resolve_admin_username(pool, tenant_id, &user_id, username).await?;
    Ok(AdminResetOutput {
        status: "reset",
        user_id,
        tenant_id: tenant_id.to_owned(),
        username: resolved_username,
        credential_existed,
        verified: true,
    })
}

/// Resolve the administrator row a reset targets.
///
/// The canonical bootstrap subject is tried first, then the configured
/// username, then the tenant's active owner membership. Environments where IAM
/// bootstrap assigned a non-canonical id or renamed the subject therefore stay
/// recoverable instead of failing with "user not found".
async fn resolve_admin_user_id(
    pool: &PgPool,
    tenant_id: &str,
    username: &str,
) -> Result<String, String> {
    if let Some(user_id) = select_user_id_by_canonical_id(pool, tenant_id).await? {
        return Ok(user_id);
    }
    if let Some(user_id) = select_user_id_by_username(pool, tenant_id, username).await? {
        return Ok(user_id);
    }
    if let Some(user_id) = select_tenant_owner_user_id(pool, tenant_id).await? {
        return Ok(user_id);
    }
    Err(format!(
        "no administrator found in tenant {tenant_id} (tried id={DEFAULT_BOOTSTRAP_ADMIN_USER_ID}, \
         username={username}, and the active owner membership); run `pnpm dev` or the installed \
         bootstrap once so IAM provisions the administrator, then retry"
    ))
}

async fn select_user_id_by_canonical_id(
    pool: &PgPool,
    tenant_id: &str,
) -> Result<Option<String>, String> {
    select_optional_string(
        "SELECT id FROM iam_user WHERE tenant_id = $1 AND id = $2 AND status = 'active' \
         AND is_deleted = 0 LIMIT 1",
        pool,
        tenant_id,
        DEFAULT_BOOTSTRAP_ADMIN_USER_ID,
        "the administrator account",
    )
    .await
}

async fn select_user_id_by_username(
    pool: &PgPool,
    tenant_id: &str,
    username: &str,
) -> Result<Option<String>, String> {
    select_optional_string(
        "SELECT id FROM iam_user WHERE tenant_id = $1 AND username = $2 AND status = 'active' \
         AND is_deleted = 0 LIMIT 1",
        pool,
        tenant_id,
        username,
        "the administrator account",
    )
    .await
}

async fn select_tenant_owner_user_id(
    pool: &PgPool,
    tenant_id: &str,
) -> Result<Option<String>, String> {
    sqlx::query_scalar::<_, String>(
        "SELECT u.id FROM iam_user u \
         JOIN iam_organization_membership m \
           ON m.tenant_id = u.tenant_id AND m.user_id = u.id \
         WHERE u.tenant_id = $1 AND u.status = 'active' AND u.is_deleted = 0 \
           AND m.status = 'active' AND m.membership_kind = 'owner' \
         ORDER BY m.sort_order ASC, u.created_at ASC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to resolve the tenant owner account: {error}"))
}

async fn resolve_admin_username(
    pool: &PgPool,
    tenant_id: &str,
    user_id: &str,
    fallback: &str,
) -> Result<String, String> {
    let username = select_optional_string(
        "SELECT username FROM iam_user WHERE tenant_id = $1 AND id = $2 AND is_deleted = 0",
        pool,
        tenant_id,
        user_id,
        "the administrator username",
    )
    .await?;
    Ok(username.unwrap_or_else(|| fallback.to_owned()))
}

async fn password_credential_exists(
    pool: &PgPool,
    tenant_id: &str,
    user_id: &str,
) -> Result<bool, String> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM iam_credential \
         WHERE tenant_id = $1 AND user_id = $2 AND credential_type = 'password')",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("failed to inspect the admin credential: {error}"))
}

async fn verify_stored_password(
    pool: &PgPool,
    tenant_id: &str,
    user_id: &str,
    password: &str,
) -> Result<(), String> {
    let stored = select_optional_string(
        "SELECT credential_hash FROM iam_credential \
         WHERE tenant_id = $1 AND user_id = $2 AND credential_type = 'password' \
           AND status = 'active' LIMIT 1",
        pool,
        tenant_id,
        user_id,
        "the stored admin credential",
    )
    .await?
    .ok_or_else(|| {
        format!(
            "the admin credential was written but is not readable back as an active \
             {PASSWORD_CREDENTIAL_TYPE} credential for user {user_id}"
        )
    })?;

    let parsed = PasswordHash::new(&stored)
        .map_err(|error| format!("the stored admin credential is not a usable hash: {error}"))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| "the stored admin credential rejected the new password".to_owned())
}

async fn select_optional_string(
    query: &'static str,
    pool: &PgPool,
    tenant_id: &str,
    value: &str,
    subject: &str,
) -> Result<Option<String>, String> {
    sqlx::query_scalar::<_, String>(query)
        .bind(tenant_id)
        .bind(value)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("failed to load {subject}: {error}"))
}

fn hash_password(password: &str) -> Result<String, String> {
    Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map(|hash| hash.to_string())
        .map_err(|error| format!("failed to hash the admin password: {error}"))
}

/// Credential id used when the administrator had no password credential yet.
/// Matches the IAM bootstrap naming so a later bootstrap reconciles the same row
/// instead of creating a duplicate.
fn bootstrap_credential_id(user_id: &str) -> String {
    format!("iamc_bootstrap_{user_id}")
}

#[cfg(test)]
// WORKSPACE-PATH:allow-fixture-block: password fixtures are local test literals
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn defaults_target_the_canonical_bootstrap_administrator() {
        let options = AdminResetOptions::from_arguments(&[]).expect("no arguments is valid");
        assert_eq!(options.tenant_id(), DEFAULT_IAM_TENANT_ID);
        assert_eq!(options.username(), DEFAULT_BOOTSTRAP_ADMIN_USERNAME);
    }

    #[test]
    fn parses_identity_overrides() {
        let options = AdminResetOptions::from_arguments(&arguments(&[
            "--username",
            "  root  ",
            "--tenant-id",
            "200002",
        ]))
        .expect("identity overrides parse");
        assert_eq!(options.username(), "root");
        assert_eq!(options.tenant_id(), "200002");
    }

    #[test]
    fn rejects_a_password_on_the_argument_list() {
        let error = AdminResetOptions::from_arguments(&arguments(&["--password", "secret"]))
            .expect_err("the password must not be accepted as an argument");
        assert!(
            error.contains(SDKWORK_WEBSERVER_ADMIN_RESET_PASSWORD_ENV),
            "{error}"
        );
    }

    #[test]
    fn rejects_an_option_without_a_value() {
        let error = AdminResetOptions::from_arguments(&arguments(&["--username"]))
            .expect_err("a flag without a value must fail");
        assert!(error.contains("requires a value"), "{error}");
    }

    #[test]
    fn rejects_unknown_options() {
        let error = AdminResetOptions::from_arguments(&arguments(&["--nope"]))
            .expect_err("unknown options must fail");
        assert!(error.contains("unknown reset-admin option"), "{error}");
    }

    #[test]
    fn rejects_short_passwords() {
        assert!(validate_password("short").is_err());
        assert!(validate_password("long-enough").is_ok());
    }

    #[test]
    fn hashed_passwords_verify_with_argon2() {
        let hash = hash_password("Admin-Reset-2026!").expect("hashing succeeds");
        assert!(hash.starts_with("$argon2"), "{hash}");
        let parsed = PasswordHash::new(&hash).expect("the hash parses");
        assert!(Argon2::default()
            .verify_password(b"Admin-Reset-2026!", &parsed)
            .is_ok());
    }
}
