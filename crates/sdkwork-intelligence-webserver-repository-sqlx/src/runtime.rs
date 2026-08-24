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
            "staging" | "stage" | "test" => false,
            other => {
                return Err(format!(
                    "invalid SDKWORK_WEBSERVER_ACME_PROFILE {other}; expected production or staging"
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
    let cert_root = std::env::var("SDKWORK_WEBSERVER_CERT_LIVE_ROOT")
        .unwrap_or_else(|_| "/etc/sdkwork/certs/letsencrypt".to_string());
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
    CertificateIssuer::new_with_account_store(
        config,
        cert_root,
        operation_timeout_ms,
        account_store,
    )
    .map_err(|error| format!("certificate issuer bootstrap failed: {error}"))
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
        ),
    })
}
