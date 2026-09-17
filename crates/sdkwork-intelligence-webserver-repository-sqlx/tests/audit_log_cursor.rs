//! Regression gate for the audit-log keyset cursor statement.
//!
//! `web_audit_log` is only listed through the cursor (keyset) mode, so a defect
//! in the assembled statement takes the whole endpoint down instead of one
//! page. The repository parity suite covers this path too, but only as one step
//! of a long contract walk that also needs the application/deployment graph.
//!
//! This test pins the statement itself against a real PostgreSQL server:
//! placeholder numbering, the row-value comparison against `TIMESTAMPTZ`, and a
//! two-row-per-page walk that must neither repeat nor skip a row.

use std::sync::Arc;

use sdkwork_database_config::{DatabaseConfig, DatabaseEngine};
use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_database_sqlx::create_pool_from_config;
use sdkwork_intelligence_webserver_repository_sqlx::PostgresWebRepository;
use sdkwork_intelligence_webserver_service::{AuditLogWrite, WebRepositoryPort};
use sdkwork_webserver_contract::ListAuditLogsQuery;
use sdkwork_webserver_database_host::bootstrap_web_database;

const POSTGRES_TEST_URL_ENV: &str = "SDKWORK_DATABASE_TEST_POSTGRES_URL";
const TENANT: i64 = 410_777;
const ACTION: &str = "audit.cursor.parity";
const TARGET_TYPE: &str = "web_application";
const ROWS: usize = 5;
const PAGE_SIZE: i32 = 2;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL database"]
async fn audit_log_keyset_cursor_pages_without_gaps_or_duplicates() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("sdkwork_intelligence_webserver_repository_sqlx=error")
        .with_test_writer()
        .try_init();
    let url = std::env::var(POSTGRES_TEST_URL_ENV).unwrap_or_else(|_| {
        panic!("set {POSTGRES_TEST_URL_ENV} to a disposable empty PostgreSQL database")
    });
    assert!(
        url.starts_with("postgres://") || url.starts_with("postgresql://"),
        "{POSTGRES_TEST_URL_ENV} must be a PostgreSQL URL"
    );
    let lifecycle_pool = create_pool_from_config(DatabaseConfig {
        engine: DatabaseEngine::Postgres,
        url,
        max_connections: 4,
        ..Default::default()
    })
    .await
    .expect("create lifecycle pool");
    let pool = lifecycle_pool
        .as_postgres()
        .expect("PostgreSQL lifecycle pool")
        .clone();
    let existing_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.tables \
         WHERE table_schema = current_schema() AND table_type = 'BASE TABLE'",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect disposable PostgreSQL schema");
    assert_eq!(
        existing_tables, 0,
        "refusing to run the audit cursor gate against a non-empty PostgreSQL schema"
    );
    let _auto_migrate = EnvironmentVariableGuard::set("SDKWORK_DATABASE_AUTO_MIGRATE", "true");
    bootstrap_web_database(lifecycle_pool)
        .await
        .expect("initialize PostgreSQL Web database lifecycle");

    let repository = Arc::new(PostgresWebRepository::new(
        pool.clone(),
        SnowflakeIdGenerator::new(733).expect("create test Snowflake generator"),
        [0x5b; 32],
    )) as Arc<dyn WebRepositoryPort>;

    for index in 0..ROWS {
        repository
            .insert_audit_log(AuditLogWrite {
                tenant_id: TENANT,
                organization_id: 0,
                operator_id: 91,
                operator_type: "USER",
                action: ACTION,
                target_type: TARGET_TYPE,
                target_id: Some(index as i64),
                target_uuid: None,
                request_id: None,
                metadata_json: "{}",
            })
            .await
            .expect("insert web_audit_log row");
    }

    // 1) Walk the whole log two rows at a time through the opaque cursor. Every
    //    row must be seen exactly once: a keyset statement whose ordering and
    //    comparison disagree shows up here as a repeated or a skipped row.
    let mut seen_ids: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0_usize;
    loop {
        let page = repository
            .list_audit_logs(
                Some(TENANT),
                &ListAuditLogsQuery {
                    page_size: Some(PAGE_SIZE),
                    cursor: cursor.clone(),
                    target_type: Some(TARGET_TYPE.to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect("list web_audit_log cursor page");
        assert!(
            page.items.len() <= PAGE_SIZE as usize,
            "cursor page returned {} rows for page_size {PAGE_SIZE}",
            page.items.len()
        );
        for item in &page.items {
            assert!(
                !seen_ids.contains(&item.id),
                "cursor paging returned audit row {} twice",
                item.id
            );
            seen_ids.push(item.id.clone());
        }
        pages += 1;
        match page.next_cursor {
            Some(next) => {
                assert_eq!(page.has_more, Some(true), "next_cursor requires has_more");
                cursor = Some(next);
            }
            None => {
                assert_eq!(page.has_more, Some(false), "last page must not signal more");
                break;
            }
        }
        assert!(pages <= ROWS, "cursor paging did not terminate");
    }
    assert_eq!(
        seen_ids.len(),
        ROWS,
        "cursor paging covered {} of {ROWS} audit rows",
        seen_ids.len()
    );
    assert_eq!(pages, ROWS.div_ceil(PAGE_SIZE as usize));

    // 2) The RFC 3339 date window goes through the same TIMESTAMPTZ comparison
    //    path, bound as text exactly like the cursor instant.
    let windowed = repository
        .list_audit_logs(
            Some(TENANT),
            &ListAuditLogsQuery {
                page_size: Some(50),
                start_date: Some("2000-01-01T00:00:00Z".to_string()),
                end_date: Some("2100-01-01T00:00:00Z".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("list web_audit_log with date window");
    assert_eq!(windowed.items.len(), ROWS);

    let empty_window = repository
        .list_audit_logs(
            Some(TENANT),
            &ListAuditLogsQuery {
                page_size: Some(50),
                start_date: Some("2100-01-01T00:00:00Z".to_string()),
                end_date: Some("2101-01-01T00:00:00Z".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("list web_audit_log outside the row window");
    assert!(empty_window.items.is_empty());

    pool.close().await;
}

struct EnvironmentVariableGuard {
    key: &'static str,
    previous_value: Option<String>,
}

impl EnvironmentVariableGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous_value = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self {
            key,
            previous_value,
        }
    }
}

impl Drop for EnvironmentVariableGuard {
    fn drop(&mut self) {
        match &self.previous_value {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}
