use std::{path::PathBuf, sync::Arc};

use sdkwork_database_config::{DatabaseConfig, DatabaseEngine};
use sdkwork_database_drift::DriftEngine;
use sdkwork_database_lifecycle::LifecycleOrchestrator;
use sdkwork_database_spi::{DefaultDatabaseModule, LocaleTag, SeedProfile};
use sdkwork_database_sqlx::create_pool_from_config;

const POSTGRES_TEST_URL_ENV: &str = "SDKWORK_DATABASE_TEST_POSTGRES_URL";

fn application_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve application root")
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL database"]
async fn postgres_baseline_seed_and_drift_are_clean() {
    let url = std::env::var(POSTGRES_TEST_URL_ENV).unwrap_or_else(|_| {
        panic!("set {POSTGRES_TEST_URL_ENV} to a disposable empty PostgreSQL database")
    });
    assert!(
        url.starts_with("postgres://") || url.starts_with("postgresql://"),
        "{POSTGRES_TEST_URL_ENV} must be a PostgreSQL URL"
    );

    let module = Arc::new(
        DefaultDatabaseModule::from_app_root(application_root()).expect("load Web database module"),
    );
    let config = DatabaseConfig {
        engine: DatabaseEngine::Postgres,
        url,
        max_connections: 2,
        ..Default::default()
    };
    let pool = create_pool_from_config(config)
        .await
        .expect("create PostgreSQL pool");
    let postgres = pool.as_postgres().expect("PostgreSQL pool");
    let existing_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.tables \
         WHERE table_schema = current_schema() AND table_type = 'BASE TABLE'",
    )
    .fetch_one(postgres)
    .await
    .expect("inspect disposable PostgreSQL schema");
    assert_eq!(
        existing_tables, 0,
        "refusing to run against a non-empty PostgreSQL schema"
    );

    let orchestrator = LifecycleOrchestrator::new(pool.clone(), module.clone())
        .with_applied_by("sdkwork-webserver-test");
    orchestrator
        .init()
        .await
        .expect("initialize PostgreSQL baseline");

    let pending = orchestrator
        .plan_migrations()
        .await
        .expect("plan PostgreSQL migrations");
    assert!(pending.is_empty(), "pre-launch baseline must be current");

    let migrated = orchestrator
        .migrate()
        .await
        .expect("apply PostgreSQL migrations");
    assert_eq!(migrated, 0);

    let application_type_columns: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns \
         WHERE table_schema = current_schema() \
           AND table_name = 'webserver_site' AND column_name = 'application_type'",
    )
    .fetch_one(postgres)
    .await
    .expect("inspect application_type migration result");
    assert_eq!(application_type_columns, 1);

    let runtime_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.tables \
         WHERE table_schema = current_schema() \
           AND table_name IN ('webserver_runtime_assignment', 'webserver_runtime_observation')",
    )
    .fetch_one(postgres)
    .await
    .expect("inspect runtime control-plane migration result");
    assert_eq!(runtime_tables, 2);

    let applied = orchestrator
        .seed(&LocaleTag::zh_cn(), &SeedProfile::standard())
        .await
        .expect("seed PostgreSQL database");
    assert_eq!(applied, 1);

    let reapplied = orchestrator
        .seed(&LocaleTag::zh_cn(), &SeedProfile::standard())
        .await
        .expect("re-run idempotent PostgreSQL seed");
    assert_eq!(reapplied, 0);

    let report = DriftEngine::new(pool.clone(), module)
        .analyze()
        .await
        .expect("analyze PostgreSQL drift");
    assert_eq!(report.status, "clean", "{:#?}", report.diffs);
    assert_eq!(report.summary.error, 0, "{:#?}", report.diffs);
    assert!(report.pending_migrations.is_empty());

    pool.close().await;
}
