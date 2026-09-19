use std::path::PathBuf;
use std::sync::Arc;

use sdkwork_database_config::DatabaseConfig;
use sdkwork_database_drift::DriftEngine;
use sdkwork_database_lifecycle::{lifecycle_options_from_env, LifecycleOrchestrator};
use sdkwork_database_spi::{DatabaseAssetProvider, DatabaseManifest, DefaultDatabaseModule};
use sdkwork_database_sqlx::{create_pool_from_config, DatabasePool};

pub struct WebDatabaseHost {
    pool: DatabasePool,
    module: Arc<DefaultDatabaseModule>,
}

impl WebDatabaseHost {
    pub fn pool(&self) -> &DatabasePool {
        &self.pool
    }

    pub fn module(&self) -> Arc<DefaultDatabaseModule> {
        self.module.clone()
    }
}

/// Application-scoped advisory-lock key for the bootstrap sequence
/// (init + migrate + drift). Concurrent gateway processes and the
/// in-process double bootstrap serialize on it so migrations and the drift
/// check never race.
const BOOTSTRAP_LOCK_KEY: i64 = 0x5344_4B57_4253_5450;

pub async fn bootstrap_web_database(pool: DatabasePool) -> Result<WebDatabaseHost, String> {
    // The advisory lock must live on one dedicated connection for the whole
    // bootstrap; the orchestrator runs on its own pool connections.
    let pg = pool
        .as_postgres()
        .ok_or("Web database bootstrap requires the PostgreSQL engine")?;
    let mut lock_tx = pg
        .begin()
        .await
        .map_err(|error| format!("begin Web database bootstrap lock: {error}"))?;
    // Allow a generous bounded wait instead of inheriting the pool's 10 s
    // lock_timeout: a sibling process running a long migration must not
    // fail this startup.
    sqlx::query("SET LOCAL lock_timeout = '10min'")
        .execute(&mut *lock_tx)
        .await
        .map_err(|error| format!("configure Web database bootstrap lock: {error}"))?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(BOOTSTRAP_LOCK_KEY)
        .execute(&mut *lock_tx)
        .await
        .map_err(|error| format!("acquire Web database bootstrap lock: {error}"))?;

    let result = run_web_database_bootstrap(pool.clone()).await;

    match result {
        Ok(host) => {
            lock_tx
                .commit()
                .await
                .map_err(|error| format!("release Web database bootstrap lock: {error}"))?;
            Ok(host)
        }
        Err(error) => {
            // Dropping the transaction releases the advisory lock.
            let _ = lock_tx.rollback().await;
            Err(error)
        }
    }
}

async fn run_web_database_bootstrap(pool: DatabasePool) -> Result<WebDatabaseHost, String> {
    let app_root = resolve_app_root();
    let module = Arc::new(
        DefaultDatabaseModule::from_app_root(&app_root)
            .map_err(|error| format!("load Web database module failed: {error}"))?,
    );
    let manifest = DatabaseManifest::from_file(module.manifest_path())
        .map_err(|error| format!("read Web database manifest failed: {error}"))?;
    let options = lifecycle_options_from_env("Web", &manifest);
    let orchestrator = LifecycleOrchestrator::new(pool.clone(), module.clone())
        .with_applied_by("sdkwork-webserver");

    orchestrator
        .init()
        .await
        .map_err(|error| format!("Web database init failed: {error}"))?;

    if options.auto_migrate {
        orchestrator
            .migrate()
            .await
            .map_err(|error| format!("Web database migrate failed: {error}"))?;
    }

    let drift = DriftEngine::new(pool.clone(), module.clone())
        .analyze()
        .await
        .map_err(|error| format!("Web database drift check failed: {error}"))?;
    if drift.summary.error > 0 {
        let details = drift
            .diffs
            .iter()
            .filter(|diff| diff.severity == "error")
            .take(5)
            .map(|diff| diff.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!(
            "Web database schema drift detected ({} error(s)): {details}. Run `pnpm db:migrate` and then `pnpm db:drift:check`",
            drift.summary.error
        ));
    }

    Ok(WebDatabaseHost { pool, module })
}

pub async fn bootstrap_web_database_from_env() -> Result<WebDatabaseHost, String> {
    let _ = dotenvy::dotenv();
    let config = DatabaseConfig::from_env("Web")
        .map_err(|error| format!("read Web database config failed: {error}"))?;
    let pool = create_pool_from_config(config)
        .await
        .map_err(|error| format!("create Web database pool failed: {error}"))?;
    bootstrap_web_database(pool).await
}

fn resolve_app_root() -> PathBuf {
    std::env::var("SDKWORK_WEBSERVER_APP_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
        })
}
