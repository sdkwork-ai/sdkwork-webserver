//! Web runtime bootstrap: database lifecycle + repository + service assembly.

use std::sync::Arc;

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_database_sqlx::enable_process_shared_database_pool;
use sdkwork_intelligence_webserver_service::{WebRepositoryPort, WebService};
use sdkwork_utils_rust::derive_aes_256_key;
use sdkwork_webserver_acme_service::{
    AcmeAccountStore, AcmeConfig, CertificateIssuer, EncryptedFileAcmeAccountStore,
    MemoryAcmeAccountStore, DEFAULT_ACME_OPERATION_TIMEOUT_MS,
};
use sdkwork_webserver_contract::{web_environment_name, web_is_production_like_environment};
use sdkwork_webserver_database_host::bootstrap_web_database_from_env;
use sdkwork_webserver_edge_runtime::EdgeRuntime;
use sdkwork_webserver_source_provider::GitDriveSourceImporter;

use crate::{PostgresWebRepository, SecretEncryptionKey};

const ENV_SECRET_KEY_INFO: &[u8] = b"sdkwork-web-env-variable-encryption";

/// Bootstrapped Web application runtime.
pub struct WebRuntime {
    pub service: WebService,
}

fn snowflake_from_env() -> Result<SnowflakeIdGenerator, String> {
    let node_id = match std::env::var("SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID") {
        Ok(value) => value
            .parse::<u16>()
            .map_err(|error| format!("invalid SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID: {error}"))?,
        Err(_) => {
            return Err(
                "SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID is required (multi-instance must set unique node id)"
                    .to_string(),
            );
        }
    };
    SnowflakeIdGenerator::new(node_id).map_err(|error| error.to_string())
}

fn secret_key_from_env() -> Result<SecretEncryptionKey, String> {
    let production_like = web_is_production_like_environment();
    let raw = match std::env::var("SDKWORK_WEBSERVER_SECRET_ENCRYPTION_KEY") {
        Ok(value) => value,
        Err(_) if !production_like => {
            tracing::warn!(
                "SDKWORK_WEBSERVER_SECRET_ENCRYPTION_KEY missing; using development-only derived key"
            );
            "sdkwork-web-development-secret-key".to_string()
        }
        Err(_) => {
            return Err(
                "SDKWORK_WEBSERVER_SECRET_ENCRYPTION_KEY is required in production-like environments"
                    .to_string(),
            );
        }
    };
    Ok(derive_aes_256_key(
        raw.as_bytes(),
        b"sdkwork-web-env",
        ENV_SECRET_KEY_INFO,
    ))
}

fn certificate_issuer_from_env(
    secret_key: &SecretEncryptionKey,
) -> Result<CertificateIssuer, String> {
    let environment = web_environment_name();
    let environment_production_like = web_is_production_like_environment();
    let use_production = match std::env::var("SDKWORK_WEBSERVER_ACME_PROFILE") {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "production" | "prod" => true,
            "staging" | "stage" | "test" | "demo" => false,
            other => {
                return Err(format!(
                    "invalid SDKWORK_WEBSERVER_ACME_PROFILE {other}; expected production, staging, test, or demo"
                ));
            }
        },
        Err(_) => matches!(environment.as_str(), "production" | "prod"),
    };
    let production_like = environment_production_like || use_production;
    let directory_url =
        std::env::var("SDKWORK_WEBSERVER_ACME_DIRECTORY_URL").unwrap_or_else(|_| {
            if use_production {
                "https://acme-v02.api.letsencrypt.org/directory".to_string()
            } else {
                "https://acme-staging-v02.api.letsencrypt.org/directory".to_string()
            }
        });
    let contact_email = match std::env::var("SDKWORK_WEBSERVER_ACME_CONTACT_EMAIL") {
        Ok(value) => value,
        Err(_) if !production_like => "admin@localhost".to_string(),
        Err(_) => {
            return Err(
                "SDKWORK_WEBSERVER_ACME_CONTACT_EMAIL is required in production-like environments"
                    .to_string(),
            );
        }
    };
    let renew_before_days = parse_env_or("SDKWORK_WEBSERVER_CERT_RENEW_BEFORE_DAYS", 30_u32)?;
    let webroot = std::env::var("SDKWORK_WEBSERVER_ACME_WEBROOT").ok();
    // Cross-platform default: `/etc/sdkwork/certs/letsencrypt` on Linux,
    // `%ProgramData%\sdkwork\certs\letsencrypt` on Windows. A hardcoded Linux
    // literal here put issued material in a tree the rest of the application
    // never looks at on Windows.
    let cert_root = sdkwork_webserver_core::canonical_acme_live_root()
        .map_err(|error| format!("ACME live root resolution failed: {error}"))?
        .to_string_lossy()
        .into_owned();
    let operation_timeout_ms = parse_env_or(
        "SDKWORK_WEBSERVER_ACME_OPERATION_TIMEOUT_MS",
        DEFAULT_ACME_OPERATION_TIMEOUT_MS,
    )?;
    // Durable ACME account credentials: one encrypted file per CA directory
    // URL under the account root, shared by every issuance/renewal process.
    // Reusing one account avoids the CA account-creation rate limit and
    // preserves account identity across restarts. The in-memory fallback
    // exists only for development; production-like environments must persist.
    let account_store: Arc<dyn AcmeAccountStore> = match std::env::var(
        "SDKWORK_WEBSERVER_ACME_ACCOUNT_ROOT",
    ) {
        Ok(root) => {
            if root.is_empty()
                || root.len() > 4_096
                || root
                    .bytes()
                    .any(|byte| byte == 0 || byte.is_ascii_control())
            {
                return Err(
                    "SDKWORK_WEBSERVER_ACME_ACCOUNT_ROOT must contain 1..4096 safe path bytes"
                        .to_string(),
                );
            }
            Arc::new(EncryptedFileAcmeAccountStore::new(
                std::path::PathBuf::from(root),
                secret_key,
            ))
        }
        Err(_) if !production_like => {
            tracing::warn!(
                    "SDKWORK_WEBSERVER_ACME_ACCOUNT_ROOT missing; ACME account credentials are kept only in process memory"
                );
            Arc::new(MemoryAcmeAccountStore::default())
        }
        Err(_) => {
            return Err(
                "SDKWORK_WEBSERVER_ACME_ACCOUNT_ROOT is required in production-like environments"
                    .to_string(),
            );
        }
    };

    let config = AcmeConfig::new(
        directory_url,
        contact_email,
        renew_before_days,
        webroot,
        use_production,
    )
    .map_err(|error| format!("ACME configuration failed: {error}"))?;
    let mut issuer = CertificateIssuer::new_with_account_store(
        config,
        cert_root,
        operation_timeout_ms,
        account_store,
    )
    .map_err(|error| format!("certificate issuer bootstrap failed: {error}"))?;

    // The declared challenge method. `AUTO` (the default, and the only sensible
    // one for a deployment that has not thought about it) resolves to HTTP-01
    // for single-domain certificates and to DNS-01 for wildcards.
    let declared =
        match std::env::var(sdkwork_webserver_core::runtime_env::ACME_CHALLENGE_METHOD_ENV) {
            Ok(value) => sdkwork_webserver_acme_service::DeclaredChallengeMethod::parse(&value)
                .map_err(|error| format!("ACME challenge method is invalid: {error}"))?,
            Err(_) => sdkwork_webserver_acme_service::DeclaredChallengeMethod::default(),
        };
    issuer.set_declared_challenge_method(declared);

    // Attach the cloud DNS accounts. Without this the issuer has no way to
    // publish _acme-challenge TXT records, DNS-01 is unreachable, and **every
    // wildcard certificate fails** — the registry existed but nothing ever
    // constructed one, so `dns_accounts_cover` was permanently false and every
    // order silently took the HTTP-01 path.
    match std::env::var(sdkwork_webserver_core::runtime_env::ACME_DNS_ACCOUNTS_FILE_ENV) {
        Ok(path) if !path.trim().is_empty() => {
            let configs = sdkwork_webserver_acme_service::load_dns_account_configs(
                std::path::Path::new(path.trim()),
            )
            .map_err(|error| format!("ACME DNS accounts cannot be loaded: {error}"))?;
            let registry =
                sdkwork_webserver_acme_service::DnsCloudAccountRegistry::from_configs(&configs)
                    .map_err(|error| format!("ACME DNS accounts are invalid: {error}"))?;
            tracing::info!(
                accounts = registry.len(),
                zones = %registry
                    .zone_apices()
                    .join(","),
                "associated ACME cloud DNS accounts; wildcard certificates can be issued"
            );
            let registry = std::sync::Arc::new(registry);
            issuer.attach_dns_accounts(std::sync::Arc::clone(&registry));
            spawn_dns_account_verification(registry);
        }
        _ => {
            // Not a silent no-op: say exactly what is unavailable and why it
            // matters, and do it at startup rather than at the first wildcard.
            tracing::warn!(
                env = sdkwork_webserver_core::runtime_env::ACME_DNS_ACCOUNTS_FILE_ENV,
                challenge_method = declared.as_str(),
                "no ACME DNS accounts are configured; DNS-01 is unavailable and wildcard \
                 certificates will fail closed until a DNS provider account is associated"
            );
        }
    }
    Ok(issuer)
}

/// Probes the associated DNS accounts, in the background, and reports what each
/// provider said.
///
/// Two deliberate properties:
///
/// * **Spawning rather than awaiting.** A third-party API must not be able to
///   delay the process's boot, and a provider that happens to be down when the
///   server starts is not a reason to refuse to start. The DNS accounts file is
///   still validated synchronously above, because a *malformed* file is a
///   mistake this process can settle on its own; whether a credential is
///   *accepted* is the provider's answer, and it arrives when it arrives.
/// * **Warning, never failing.** A rejection is an operator's configuration
///   mistake, and the log line carries the provider's own words ("Authentication
///   error (10000)", "InvalidAccessKeyId.NotFound") so it names the fix. This is
///   the earliest point that answer can reach anyone: without the probe the same
///   fact only surfaces a renewal window later, as an expired certificate.
fn spawn_dns_account_verification(
    registry: std::sync::Arc<sdkwork_webserver_acme_service::DnsCloudAccountRegistry>,
) {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        tracing::debug!(
            "no async runtime is available; the ACME DNS account probe was not scheduled"
        );
        return;
    };
    handle.spawn(async move {
        let reports = registry.verify_accounts().await;
        let rejected = reports.iter().filter(|report| report.is_rejected()).count();
        for report in &reports {
            if report.is_rejected() {
                // The provider refused the account: `describe()` renders its own
                // diagnostic, which is what makes this actionable.
                tracing::warn!(account = %report.account_id, zone = %report.zone_apex,
                    provider = %report.provider, "{}", report.describe());
            } else {
                tracing::info!(account = %report.account_id, zone = %report.zone_apex,
                    provider = %report.provider, "{}", report.describe());
            }
        }
        if rejected > 0 {
            tracing::warn!(
                accounts = reports.len(),
                rejected,
                "{} of {} ACME cloud DNS accounts were refused by their provider; certificates \
                 for those zones will fail to issue until the credentials are corrected",
                rejected,
                reports.len()
            );
        }
    });
}

fn parse_env_or<T>(key: &str, default: T) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    std::env::var(key)
        .map(|value| {
            value
                .parse::<T>()
                .map_err(|error| format!("invalid {key}: {error}"))
        })
        .unwrap_or(Ok(default))
}

/// Bootstrap database lifecycle, repository, and service from environment variables.
pub async fn bootstrap_web_runtime_from_env() -> Result<WebRuntime, String> {
    enable_process_shared_database_pool();
    let lifecycle_host = bootstrap_web_database_from_env().await?;
    let id_generator = snowflake_from_env()?;
    let secret_key = secret_key_from_env()?;
    let pool = lifecycle_host
        .pool()
        .as_postgres()
        .ok_or_else(|| "web runtime requires a PostgreSQL database pool".to_string())?;
    let repository = Arc::new(PostgresWebRepository::new(
        pool.clone(),
        id_generator,
        secret_key,
    )) as Arc<dyn WebRepositoryPort>;

    let certificate_issuer = Arc::new(certificate_issuer_from_env(&secret_key)?);
    let edge_runtime = Arc::new(
        EdgeRuntime::from_env()
            .map_err(|error| format!("edge runtime bootstrap failed: {error}"))?,
    );
    let source_importer = Arc::new(GitDriveSourceImporter::from_env().await?);

    Ok(WebRuntime {
        service: WebService::new_with_source_importer(
            repository,
            certificate_issuer,
            edge_runtime,
            source_importer,
        )
        .map_err(|error| format!("web service bootstrap failed: {error}"))?,
    })
}
