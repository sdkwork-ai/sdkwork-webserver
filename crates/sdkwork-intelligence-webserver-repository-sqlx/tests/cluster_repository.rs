//! PostgreSQL integration coverage for the distributed cluster plane.
//!
//! Runs against a disposable, empty PostgreSQL database addressed through
//! `SDKWORK_DATABASE_TEST_POSTGRES_URL` (same contract as
//! `repository_parity.rs`): the harness migrates the schema from the baseline
//! and exercises the full cluster repository surface — registration
//! idempotency (one host, many processes; many hosts), token authentication,
//! heartbeats, the peer directory, the mailbox handoff, the liveness sweep,
//! events, and admin pagination — so every statement in `cluster.rs` executes
//! against a real engine.

use std::sync::Arc;

use sdkwork_database_config::{DatabaseConfig, DatabaseEngine};
use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_database_sqlx::create_pool_from_config;
use sdkwork_intelligence_webserver_service::{
    ClusterEventWrite, ClusterHeartbeatWrite, ClusterHostUpsert, ClusterInstanceUpsert,
    ClusterPeerMessageEnqueue, WebRepositoryPort,
};
use sdkwork_utils_rust::crypto::sha256_hash;
use sdkwork_webserver_contract::{WebServiceError, UpdateClusterRequest};
use sdkwork_webserver_database_host::bootstrap_web_database;
use sqlx::Row;

const POSTGRES_TEST_URL_ENV: &str = "SDKWORK_DATABASE_TEST_POSTGRES_URL";

struct EnvironmentVariableGuard {
    key: &'static str,
    previous_value: Option<std::ffi::OsString>,
}

impl EnvironmentVariableGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous_value = std::env::var_os(key);
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

struct TestContext {
    pool: sqlx::PgPool,
    repository: Arc<dyn WebRepositoryPort>,
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL database"]
async fn postgres_cluster_registration_heartbeat_peers_mailbox_and_sweep_are_bounded() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("sdkwork_intelligence_webserver_repository_sqlx=debug")
        .with_test_writer()
        .try_init();
    let url = std::env::var(POSTGRES_TEST_URL_ENV).unwrap_or_else(|_| {
        panic!("set {POSTGRES_TEST_URL_ENV} to a disposable empty PostgreSQL database")
    });
    assert!(
        url.starts_with("postgres://") || url.starts_with("postgresql://"),
        "{POSTGRES_TEST_URL_ENV} must be a PostgreSQL URL"
    );
    let context = prepare_database(DatabaseConfig {
        engine: DatabaseEngine::Postgres,
        url,
        max_connections: 4,
        ..Default::default()
    })
    .await;

    verify_registration_and_authentication(&context).await;
    verify_heartbeat_peers_and_mailbox(&context).await;
    verify_liveness_sweep_events_and_pagination(&context).await;
    context.pool.close().await;
}

async fn prepare_database(config: DatabaseConfig) -> TestContext {
    let lifecycle_pool = create_pool_from_config(config.clone())
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
        "refusing to run cluster integration against a non-empty PostgreSQL schema"
    );
    let _auto_migrate = EnvironmentVariableGuard::set("SDKWORK_DATABASE_AUTO_MIGRATE", "true");
    bootstrap_web_database(lifecycle_pool)
        .await
        .expect("initialize PostgreSQL Web database lifecycle");

    let id_generator = SnowflakeIdGenerator::new(911).expect("create test Snowflake generator");
    let repository = Arc::new(sdkwork_intelligence_webserver_repository_sqlx::PostgresWebRepository::new(
        pool.clone(),
        id_generator,
        [0x5b; 32],
    )) as Arc<dyn WebRepositoryPort>;
    TestContext { pool, repository }
}

async fn verify_registration_and_authentication(context: &TestContext) {
    let repository = &context.repository;

    // The default cluster is provisioned lazily exactly once.
    let default_first = repository.resolve_cluster_identity(0, None).await.expect("default cluster");
    let default_again = repository.resolve_cluster_identity(0, None).await.expect("default cluster again");
    assert_eq!(default_first.cluster_id, default_again.cluster_id);
    assert_eq!(default_first.code, "default");

    // Host upsert keys on (tenant, machine code); the second registration
    // updates instead of inserting.
    let host_a = host_write(0, default_first.cluster_id, "edge-a", "mc-edge-a");
    let host_first = repository.upsert_cluster_host(host_a.clone()).await.expect("host insert");
    assert!(host_first.created);
    let host_again = repository.upsert_cluster_host(host_a).await.expect("host re-registration");
    assert!(!host_again.created);
    assert_eq!(host_first.uuid, host_again.uuid);

    // One host runs many instances: distinct PIDs are distinct instances.
    let token_one = "winst_instance-one-token";
    let instance_one = repository
        .upsert_cluster_instance(instance_write(
            0,
            host_first.id,
            default_first.cluster_id,
            4_100,
            hash(token_one),
        ))
        .await
        .expect("instance insert");
    assert!(instance_one.created);
    let token_two = "winst_instance-one-rotated";
    let instance_re_register = repository
        .upsert_cluster_instance(instance_write(
            0,
            host_first.id,
            default_first.cluster_id,
            4_100,
            hash(token_two),
        ))
        .await
        .expect("instance re-registration");
    assert!(!instance_re_register.created);
    assert_eq!(instance_one.uuid, instance_re_register.uuid);

    let instance_two = repository
        .upsert_cluster_instance(instance_write(
            0,
            host_first.id,
            default_first.cluster_id,
            4_200,
            hash("winst_instance-two-token"),
        ))
        .await
        .expect("second instance on the same host");
    assert!(instance_two.created);
    assert_ne!(instance_one.uuid, instance_two.uuid);

    // A second host with the same PID is a distinct member.
    let host_b = repository
        .upsert_cluster_host(host_write(0, default_first.cluster_id, "edge-b", "mc-edge-b"))
        .await
        .expect("host b insert");
    assert!(host_b.created);
    let instance_b = repository
        .upsert_cluster_instance(instance_write(
            0,
            host_b.id,
            default_first.cluster_id,
            4_100,
            hash("winst_instance-b-token"),
        ))
        .await
        .expect("instance on host b");
    assert_ne!(instance_one.uuid, instance_b.uuid);

    // The heartbeat token authenticates back to the instance identity; the
    // port takes the RAW token (it hashes internally), the rotated token
    // wins, and the stale one is gone.
    let credentials = repository
        .authenticate_cluster_instance_token(token_two)
        .await
        .expect("rotated token resolves");
    assert_eq!(credentials.instance_uuid, instance_one.uuid);
    assert_eq!(credentials.host_uuid, host_first.uuid);
    assert_eq!(credentials.cluster_uuid, default_first.cluster_uuid);
    // Unknown tokens fail closed with NotFound at the port; the service maps
    // that to a rejected machine credential.
    let stale = repository.authenticate_cluster_instance_token(token_one).await;
    assert!(matches!(stale, Err(WebServiceError::NotFound(_))), "the previous registration token must stop resolving");
    let unknown = repository
        .authenticate_cluster_instance_token("winst_never-issued")
        .await;
    assert!(matches!(unknown, Err(WebServiceError::NotFound(_))));
}

async fn verify_heartbeat_peers_and_mailbox(context: &TestContext) {
    let repository = &context.repository;

    let default_cluster = repository.resolve_cluster_identity(0, None).await.expect("default cluster");
    let host = repository
        .upsert_cluster_host(host_write(0, default_cluster.cluster_id, "hb-host", "mc-hb-host"))
        .await
        .expect("heartbeat host");
    let instance = repository
        .upsert_cluster_instance(instance_write(
            0,
            host.id,
            default_cluster.cluster_id,
            5_100,
            hash("winst_hb-token"),
        ))
        .await
        .expect("heartbeat instance");
    let peer_host = repository
        .upsert_cluster_host(host_write(0, default_cluster.cluster_id, "hb-peer", "mc-hb-peer"))
        .await
        .expect("peer host");
    let peer = repository
        .upsert_cluster_instance(instance_write(
            0,
            peer_host.id,
            default_cluster.cluster_id,
            5_100,
            hash("winst_peer-token"),
        ))
        .await
        .expect("peer instance");

    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    let transition = repository
        .record_cluster_heartbeat(ClusterHeartbeatWrite {
            tenant_id: 0,
            instance_id: instance.id,
            host_id: host.id,
            status: 1,
            health_state: "HEALTHY".to_string(),
            uptime_seconds: 120,
            build_version: Some("1.1.0".to_string()),
            metrics_json: r#"{"rssMb":128}"#.to_string(),
            reported_at: now.clone(),
        })
        .await
        .expect("first heartbeat");
    // Registration places the process online already (status 1), so the first
    // heartbeat observes no status transition.
    assert_eq!(transition.previous_status, 1);

    let registered = repository
        .retrieve_cluster_instance(&instance.uuid)
        .await
        .expect("reload heartbeating instance");
    assert_eq!(registered.status, 1);
    assert_eq!(registered.uptime_seconds, 120);
    assert_eq!(registered.metrics["rssMb"], 128);

    let host_row = repository
        .retrieve_cluster_host(&host.uuid)
        .await
        .expect("reload heartbeat host");
    assert_eq!(host_row.status, 1, "host liveness follows its members");
    assert_eq!(host_row.instance_count, 1, "the heartbeat host carries exactly its own instance");

    // Peer directory excludes the requester and carries reachability fields.
    // The registry is shared across the whole test, so the expected size is
    // computed from the rows on disk instead of a hardcoded count.
    let peers = repository
        .list_cluster_peers(&instance.uuid, 200)
        .await
        .expect("peer directory");
    let expected_peers: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM webserver_cluster_instance
         WHERE tenant_id = 0 AND deleted_at IS NULL AND uuid <> $1",
    )
    .bind(&instance.uuid)
    .fetch_one(&context.pool)
    .await
    .expect("count peers");
    assert_eq!(peers.len() as i64, expected_peers);
    assert!(peers.iter().all(|peer| peer.instance_id != instance.uuid));
    let peer_entry = peers
        .iter()
        .find(|candidate| candidate.instance_id == peer.uuid)
        .expect("peer entry");
    assert_eq!(peer_entry.host_name, "hb-peer");

    // Mailbox: a direct message and a broadcast are enqueued through the
    // repository; a broadcast materializes one copy per online member so
    // every member claims its own (at-most-once per member).
    let enqueue_now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    let expires = (chrono::Utc::now() + chrono::Duration::hours(1))
        .to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    let direct_enqueued = repository
        .enqueue_cluster_peer_messages(ClusterPeerMessageEnqueue {
            tenant_id: 0,
            cluster_uuid: &default_cluster.cluster_uuid,
            from_instance_uuid: None,
            to_instance_uuid: Some(&instance.uuid),
            message_type: "cluster.reload",
            payload_json: r#"{"origin":"test"}"#,
            deliver_at: &enqueue_now,
            expires_at: &expires,
        })
        .await
        .expect("enqueue direct message");
    assert_eq!(direct_enqueued, 1);
    let broadcast_enqueued = repository
        .enqueue_cluster_peer_messages(ClusterPeerMessageEnqueue {
            tenant_id: 0,
            cluster_uuid: &default_cluster.cluster_uuid,
            from_instance_uuid: None,
            to_instance_uuid: None,
            message_type: "cluster.announce",
            payload_json: r#"{"origin":"test"}"#,
            deliver_at: &enqueue_now,
            expires_at: &expires,
        })
        .await
        .expect("enqueue broadcast");
    let online_members: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM webserver_cluster_instance
         WHERE tenant_id = 0 AND deleted_at IS NULL AND status = 1",
    )
    .fetch_one(&context.pool)
    .await
    .expect("count online members");
    assert_eq!(broadcast_enqueued as i64, online_members);

    // Messages become deliverable at their insert instant, so the claim uses
    // a fresh watermark captured after the inserts.
    let deliver_watermark = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    let messages = repository
        .claim_cluster_peer_messages(instance.id, 32, &deliver_watermark)
        .await
        .expect("claim messages");
    assert_eq!(messages.len(), 2, "direct copy + broadcast copy");
    assert!(messages.iter().all(|message| message.payload["origin"] == "test"));
    let replay = repository
        .claim_cluster_peer_messages(instance.id, 32, &deliver_watermark)
        .await
        .expect("claim again");
    assert!(replay.is_empty(), "delivered messages must not be re-claimed");

    // The peer instance receives exactly its own broadcast copy; the direct
    // message stays addressed to the other instance.
    let peer_messages = repository
        .claim_cluster_peer_messages(peer.id, 32, &deliver_watermark)
        .await
        .expect("claim for peer");
    assert_eq!(peer_messages.len(), 1);
    assert_eq!(peer_messages[0].message_type, "cluster.announce");

    // Neither claimer has leftover pending rows addressed to it (broadcast
    // copies for instances that never claim stay pending by design until the
    // sweep expires them).
    let leftover: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM webserver_cluster_peer_message
         WHERE tenant_id = 0 AND state = 'PENDING'
           AND to_instance_id IN ($1, $2)",
    )
    .bind(instance.id)
    .bind(peer.id)
    .fetch_one(&context.pool)
    .await
    .expect("count leftover pending rows");
    assert_eq!(leftover, 0, "claimed instances must have no pending copies");
}

async fn verify_liveness_sweep_events_and_pagination(context: &TestContext) {
    let repository = &context.repository;

    let default_cluster = repository.resolve_cluster_identity(0, None).await.expect("default cluster");
    let host = repository
        .upsert_cluster_host(host_write(0, default_cluster.cluster_id, "sweep-host", "mc-sweep"))
        .await
        .expect("sweep host");
    let instance = repository
        .upsert_cluster_instance(instance_write(
            0,
            host.id,
            default_cluster.cluster_id,
            6_100,
            hash("winst_sweep-token"),
        ))
        .await
        .expect("sweep instance");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    repository
        .record_cluster_heartbeat(ClusterHeartbeatWrite {
            tenant_id: 0,
            instance_id: instance.id,
            host_id: host.id,
            status: 1,
            health_state: "HEALTHY".to_string(),
            uptime_seconds: 10,
            build_version: None,
            metrics_json: "{}".to_string(),
            reported_at: now.clone(),
        })
        .await
        .expect("sweep heartbeat");

    // Backdate beyond the default cluster's 60s offline threshold and sweep.
    sqlx::query(
        "UPDATE webserver_cluster_instance SET last_heartbeat_at = now() - INTERVAL '10 minutes'
         WHERE id = $1",
    )
    .bind(instance.id)
    .execute(&context.pool)
    .await
    .expect("backdate instance heartbeat");
    sqlx::query(
        "UPDATE webserver_cluster_host SET last_heartbeat_at = now() - INTERVAL '10 minutes'
         WHERE id = $1",
    )
    .bind(host.id)
    .execute(&context.pool)
    .await
    .expect("backdate host heartbeat");

    let expired_instances = repository
        .expire_stale_cluster_instances(&now, 512)
        .await
        .expect("instance sweep");
    assert!(expired_instances
        .iter()
        .any(|expired| expired.instance_uuid == instance.uuid));
    let expired_instance = expired_instances
        .iter()
        .find(|expired| expired.instance_uuid == instance.uuid)
        .expect("sweep entry");
    assert_eq!(expired_instance.host_uuid, host.uuid);
    assert_eq!(expired_instance.cluster_uuid, default_cluster.cluster_uuid);

    let expired_hosts = repository
        .expire_stale_cluster_hosts(&now, 512)
        .await
        .expect("host sweep");
    assert!(expired_hosts.iter().any(|expired| expired.host_uuid == host.uuid));

    // Lifecycle events back the admin timeline; severity filter + cursor work.
    repository
        .record_cluster_event(ClusterEventWrite {
            tenant_id: 0,
            cluster_uuid: &default_cluster.cluster_uuid,
            host_uuid: Some(&host.uuid),
            instance_uuid: Some(&instance.uuid),
            event_type: "INSTANCE_OFFLINE",
            severity: "WARNING",
            message: "instance stopped heartbeating and is offline",
            detail_json: "{}",
            occurred_at: &now,
        })
        .await
        .expect("event write");
    repository
        .record_cluster_event(ClusterEventWrite {
            tenant_id: 0,
            cluster_uuid: &default_cluster.cluster_uuid,
            host_uuid: None,
            instance_uuid: None,
            event_type: "CLUSTER_CREATED",
            severity: "INFO",
            message: "test cluster event",
            detail_json: "{}",
            occurred_at: &now,
        })
        .await
        .expect("event write");

    let warnings = repository
        .list_cluster_events(None, Some("WARNING"), 200, None)
        .await
        .expect("filtered events");
    assert!(!warnings.items.is_empty());
    assert!(warnings
        .items
        .iter()
        .all(|event| event.severity == "WARNING" && event.detail.is_object()));

    // Walk the event cursor to exhaustion with a tiny page.
    let mut cursor = None;
    let mut visited = 0_usize;
    loop {
        let page = repository
            .list_cluster_events(None, None, 2, cursor.as_deref())
            .await
            .expect("event page");
        visited += page.items.len();
        match (page.has_more, page.next_cursor) {
            (Some(true), Some(next)) => cursor = Some(next),
            _ => break,
        }
    }
    let total_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM webserver_cluster_event WHERE tenant_id = 0",
    )
    .fetch_one(&context.pool)
    .await
    .expect("count events");
    assert_eq!(visited, total_events as usize, "cursor walk must visit every event exactly once");

    // Instance listing filters by health state and paginates by cursor.
    let unhealthy_page = repository
        .list_cluster_instances(None, None, None, Some("UNHEALTHY"), 200, None)
        .await
        .expect("unhealthy instances");
    assert!(unhealthy_page.items.iter().all(|item| item.health_state == "UNHEALTHY"));

    let mut instance_cursor = None;
    let mut instance_visited = 0_usize;
    loop {
        let page = repository
            .list_cluster_instances(None, None, None, None, 2, instance_cursor.as_deref())
            .await
            .expect("instance page");
        instance_visited += page.items.len();
        assert!(page
            .items
            .iter()
            .all(|item| item.host_name.as_deref().is_some_and(|name| !name.is_empty())));
        match (page.has_more, page.next_cursor) {
            (Some(true), Some(next)) => instance_cursor = Some(next),
            _ => break,
        }
    }
    let total_instances: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM webserver_cluster_instance WHERE tenant_id = 0 AND deleted_at IS NULL",
    )
    .fetch_one(&context.pool)
    .await
    .expect("count instances");
    assert_eq!(instance_visited, total_instances as usize);

    // Host cursor walk sees every host exactly once.
    let mut host_cursor = None;
    let mut host_visited = 0_usize;
    loop {
        let page = repository
            .list_cluster_hosts(None, None, 2, host_cursor.as_deref())
            .await
            .expect("host page");
        host_visited += page.items.len();
        match (page.has_more, page.next_cursor) {
            (Some(true), Some(next)) => host_cursor = Some(next),
            _ => break,
        }
    }
    let total_hosts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM webserver_cluster_host WHERE tenant_id = 0 AND deleted_at IS NULL",
    )
    .fetch_one(&context.pool)
    .await
    .expect("count hosts");
    assert_eq!(host_visited, total_hosts as usize);

    // Cluster CRUD: partial update keeps untouched columns, delete soft-removes.
    let created = repository
        .create_cluster(&sdkwork_webserver_contract::CreateClusterRequest {
            name: "Regression Cluster".to_string(),
            code: "regression".to_string(),
            description: None,
            heartbeat_interval_seconds: None,
            offline_threshold_seconds: None,
        })
        .await
        .expect("create cluster");
    assert_eq!(created.host_count, 0);
    let updated = repository
        .update_cluster(
            &created.id,
            &UpdateClusterRequest {
                name: Some("Regression Cluster v2".to_string()),
                description: None,
                status: None,
                heartbeat_interval_seconds: Some(30),
                offline_threshold_seconds: None,
            },
        )
        .await
        .expect("update cluster");
    assert_eq!(updated.name, "Regression Cluster v2");
    assert_eq!(updated.heartbeat_interval_seconds, 30);
    assert_eq!(updated.offline_threshold_seconds, 60, "absent fields keep their value");
    repository.delete_cluster(&created.id).await.expect("delete cluster");
    let missing = repository.retrieve_cluster(&created.id).await;
    assert!(matches!(missing, Err(WebServiceError::NotFound(_))));

    // Overview aggregates remain consistent with the rows on disk.
    let overview = repository.cluster_overview().await.expect("overview");
    assert!(overview.total_hosts >= 4);
    assert!(overview.total_instances >= 5);
    assert!(overview.pending_peer_messages >= 0);

    // Heartbeat sample drill-down: the sweep instance recorded exactly one
    // heartbeat sample, returned newest-first with an exhausted cursor.
    let sample_page = repository
        .list_cluster_heartbeats(&instance.uuid, 5, None)
        .await
        .expect("sample page");
    assert_eq!(sample_page.items.len(), 1);
    assert_eq!(sample_page.has_more, Some(false));
    assert!(sample_page.next_cursor.is_none());
    assert_eq!(sample_page.items[0].status, 1);
    assert_eq!(sample_page.items[0].metrics, serde_json::json!({}));

    // Heartbeat sample purge removes only what aged past the watermark.
    let purged = repository
        .purge_cluster_heartbeats(
            &(chrono::Utc::now() + chrono::Duration::hours(1))
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
            512,
        )
        .await
        .expect("purge samples");
    assert!(purged >= 2, "both heartbeat samples are older than the watermark");
}

fn hash(token: &str) -> String {
    sha256_hash(token.as_bytes())
}

fn host_write(tenant_id: i64, cluster_id: i64, hostname: &str, machine_code: &str) -> ClusterHostUpsert {
    ClusterHostUpsert {
        tenant_id,
        cluster_id,
        name: hostname.to_string(),
        hostname: hostname.to_string(),
        machine_code: machine_code.to_string(),
        os_name: Some("Linux".to_string()),
        os_version: Some("Debian GNU/Linux 13".to_string()),
        kernel_version: None,
        arch: Some("x86_64".to_string()),
        cpu_model: None,
        cpu_cores: Some(8),
        memory_total_mb: Some(16_384),
        remote_ip: Some("203.0.113.10".to_string()),
        local_ips: vec!["10.0.0.1".to_string(), "10.0.0.2".to_string()],
        mac_addresses: vec!["02:00:00:00:00:01".to_string()],
        daemon_version: Some("1.1.0".to_string()),
    }
}

#[allow(clippy::too_many_arguments)]
fn instance_write(
    tenant_id: i64,
    host_id: i64,
    cluster_id: i64,
    process_pid: i32,
    instance_token_hash: String,
) -> ClusterInstanceUpsert {
    ClusterInstanceUpsert {
        tenant_id,
        host_id,
        cluster_id,
        name: format!("gateway#{process_pid}"),
        role: "GATEWAY".to_string(),
        environment: "test".to_string(),
        process_pid,
        process_started_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        bind_host: Some("0.0.0.0".to_string()),
        bind_port: Some(3800),
        public_endpoint: Some(format!("https://node-{process_pid}.example.test")),
        build_version: Some("1.1.0".to_string()),
        instance_token_hash,
    }
}

fn rand_i64() -> i64 {
    use std::sync::atomic::{AtomicI64, Ordering};
    static COUNTER: AtomicI64 = AtomicI64::new(90_000);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}
