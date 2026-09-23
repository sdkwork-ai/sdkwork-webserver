//! PostgreSQL integration coverage for the distributed cluster plane.
//!
//! Runs against a disposable, empty PostgreSQL database addressed through
//! `SDKWORK_DATABASE_TEST_POSTGRES_URL` (same contract as
//! `repository_parity.rs`): the harness migrates the schema from the baseline
//! and exercises the full cluster repository surface — registration idempotency
//! keyed on the listening slot (one row per slot, stable across process
//! restarts; many slots per host; many hosts), token authentication,
//! heartbeats, the peer directory, the mailbox handoff, the liveness sweep,
//! events, and admin pagination — so every statement in `cluster.rs` executes
//! against a real engine. It also pins the two instance-state ownership rules
//! the repository enforces: an operator's maintenance mark outranks every
//! machine writer (heartbeat, re-registration, prober), and the prober writes
//! its ejection instant once per failure streak.

use std::sync::Arc;

use sdkwork_database_config::{DatabaseConfig, DatabaseEngine};
use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_database_sqlx::create_pool_from_config;
use sdkwork_intelligence_webserver_service::{
    ClusterEventWrite, ClusterHeartbeatWrite, ClusterHostUpsert, ClusterInstanceUpsert,
    ClusterPeerMessageEnqueue, ClusterProbeOutcome, ClusterProbeWrite, WebRepositoryPort,
};
use sdkwork_utils_rust::crypto::sha256_hash;
use sdkwork_webserver_contract::{
    UpdateClusterHostRequest, UpdateClusterInstanceRequest, UpdateClusterRequest, WebServiceError,
};
use sdkwork_webserver_database_host::bootstrap_web_database;

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
    verify_operator_status_ownership_and_probe_latch(&context).await;
    verify_operator_rename_survives_re_registration(&context).await;
    verify_machine_absence_does_not_erase_operator_values(&context).await;
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
    // Coerced through the binding rather than `as`: the workspace denies
    // `trivial_casts`, and `Arc<T> as Arc<dyn Trait>` is exactly that.
    let repository: Arc<dyn WebRepositoryPort> = Arc::new(
        sdkwork_intelligence_webserver_repository_sqlx::PostgresWebRepository::new(
            pool.clone(),
            id_generator,
            [0x5b; 32],
        ),
    );
    TestContext { pool, repository }
}

async fn verify_registration_and_authentication(context: &TestContext) {
    let repository = &context.repository;

    // The default cluster is provisioned lazily exactly once.
    let default_first = repository
        .resolve_cluster_identity(0, None)
        .await
        .expect("default cluster");
    let default_again = repository
        .resolve_cluster_identity(0, None)
        .await
        .expect("default cluster again");
    assert_eq!(default_first.cluster_id, default_again.cluster_id);
    assert_eq!(default_first.code, "default");

    // Host upsert keys on (tenant, machine code); the second registration
    // updates instead of inserting.
    let host_a = host_write(0, default_first.cluster_id, "edge-a", "mc-edge-a");
    let host_first = repository
        .upsert_cluster_host(host_a.clone())
        .await
        .expect("host insert");
    assert!(host_first.created);
    let host_again = repository
        .upsert_cluster_host(host_a)
        .await
        .expect("host re-registration");
    assert!(!host_again.created);
    assert_eq!(host_first.uuid, host_again.uuid);

    // One host runs many instances, and an instance is its listening slot:
    // `edge-a` serving 3800 and `edge-a` serving 3900 are two members.
    let token_one = "winst_instance-one-token";
    // `process_started_at` is pinned instead of generated per call: it is what
    // the registry reads as "this process restarted", so a fixture that
    // re-stamps it on every write could not tell a re-registration apart from a
    // restart.
    const FIRST_START: &str = "2026-09-21T00:00:00Z";
    const RESTART_START: &str = "2026-09-21T06:00:00Z";
    let mut first_write = instance_write(
        0,
        host_first.id,
        default_first.cluster_id,
        3_800,
        4_100,
        hash(token_one),
    );
    first_write.process_started_at = FIRST_START.to_owned();
    let instance_one = repository
        .upsert_cluster_instance(first_write)
        .await
        .expect("instance insert");
    assert!(instance_one.created);

    // The same process re-registering on the same slot (what the heartbeat loop
    // does after a connection reset) updates the row, rotates the token, and is
    // NOT a restart: the process start instant did not move.
    let token_two = "winst_instance-one-rotated";
    // The token the slot carries after the restart. Every registration rotates
    // the credential, so this - not `token_two` - is what must resolve once the
    // restart has landed.
    let restart_token = "winst_instance-one-restarted";
    let mut same_process_write = instance_write(
        0,
        host_first.id,
        default_first.cluster_id,
        3_800,
        4_100,
        hash(token_two),
    );
    same_process_write.process_started_at = FIRST_START.to_owned();
    let instance_re_register = repository
        .upsert_cluster_instance(same_process_write)
        .await
        .expect("instance re-registration");
    assert!(!instance_re_register.created);
    assert_eq!(instance_one.uuid, instance_re_register.uuid);
    assert_eq!(
        restart_count(&context, instance_one.id).await,
        0,
        "a re-registration from the same process is not a restart"
    );

    let instance_two = repository
        .upsert_cluster_instance(instance_write(
            0,
            host_first.id,
            default_first.cluster_id,
            3_900,
            4_200,
            hash("winst_instance-two-token"),
        ))
        .await
        .expect("second slot on the same host");
    assert!(instance_two.created);
    assert_ne!(instance_one.uuid, instance_two.uuid);

    // A restart keeps the slot and takes a new pid. The registration has to
    // land on the row that owns the slot: same instance uuid, one restart
    // counted, and no additional row. Keyed on the pid - the conflict target
    // this replaced - every restart of a long-lived edge appended a brand-new
    // instance, which is how one dev host ended up owning twenty rows for two
    // listening ports.
    let restarted = repository
        .upsert_cluster_instance({
            let mut write = instance_write(
                0,
                host_first.id,
                default_first.cluster_id,
                3_800,
                4_999,
                hash(restart_token),
            );
            write.process_started_at = RESTART_START.to_owned();
            write
        })
        .await
        .expect("instance restart on the same slot");
    assert!(!restarted.created, "a restart must reuse the slot's row");
    assert_eq!(
        instance_one.uuid, restarted.uuid,
        "the instance uuid is stable for the life of the slot"
    );
    assert_eq!(
        restart_count(&context, instance_one.id).await,
        1,
        "the restart is counted, not duplicated"
    );
    let live_on_host: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM webserver_cluster_instance \
         WHERE host_id = $1 AND deleted_at IS NULL",
    )
    .bind(host_first.id)
    .fetch_one(&context.pool)
    .await
    .expect("live instances on host a");
    assert_eq!(
        live_on_host, 2,
        "host edge-a owns exactly its two listening slots, regardless of restarts"
    );

    // A second host on the same slot is a distinct member.
    let host_b = repository
        .upsert_cluster_host(host_write(
            0,
            default_first.cluster_id,
            "edge-b",
            "mc-edge-b",
        ))
        .await
        .expect("host b insert");
    assert!(host_b.created);
    let instance_b = repository
        .upsert_cluster_instance(instance_write(
            0,
            host_b.id,
            default_first.cluster_id,
            3_800,
            4_100,
            hash("winst_instance-b-token"),
        ))
        .await
        .expect("instance on host b");
    assert!(instance_b.created);
    assert_ne!(instance_one.uuid, instance_b.uuid);

    // The heartbeat token authenticates back to the instance identity; the
    // port takes the RAW token (it hashes internally). The slot keeps ONE
    // credential and every registration - re-registration and restart alike -
    // replaces it, so only the newest token resolves.
    let credentials = repository
        .authenticate_cluster_instance_token(restart_token)
        .await
        .expect("the restart's token resolves");
    assert_eq!(credentials.instance_uuid, instance_one.uuid);
    assert_eq!(credentials.host_uuid, host_first.uuid);
    assert_eq!(credentials.cluster_uuid, default_first.cluster_uuid);
    // Unknown tokens fail closed with NotFound at the port; the service maps
    // that to a rejected machine credential.
    for superseded in [token_one, token_two] {
        let stale = repository
            .authenticate_cluster_instance_token(superseded)
            .await;
        assert!(
            matches!(stale, Err(WebServiceError::NotFound(_))),
            "a superseded registration token must stop resolving: {superseded}"
        );
    }
    let unknown = repository
        .authenticate_cluster_instance_token("winst_never-issued")
        .await;
    assert!(matches!(unknown, Err(WebServiceError::NotFound(_))));
}

async fn verify_heartbeat_peers_and_mailbox(context: &TestContext) {
    let repository = &context.repository;

    let default_cluster = repository
        .resolve_cluster_identity(0, None)
        .await
        .expect("default cluster");
    let host = repository
        .upsert_cluster_host(host_write(
            0,
            default_cluster.cluster_id,
            "hb-host",
            "mc-hb-host",
        ))
        .await
        .expect("heartbeat host");
    let instance = repository
        .upsert_cluster_instance(instance_write(
            0,
            host.id,
            default_cluster.cluster_id,
            3_800,
            5_100,
            hash("winst_hb-token"),
        ))
        .await
        .expect("heartbeat instance");
    let peer_host = repository
        .upsert_cluster_host(host_write(
            0,
            default_cluster.cluster_id,
            "hb-peer",
            "mc-hb-peer",
        ))
        .await
        .expect("peer host");
    let peer = repository
        .upsert_cluster_instance(instance_write(
            0,
            peer_host.id,
            default_cluster.cluster_id,
            3_800,
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
            quality_score: None,
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
    assert_eq!(
        host_row.instance_count, 1,
        "the heartbeat host carries exactly its own instance"
    );

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
    assert!(messages
        .iter()
        .all(|message| message.payload["origin"] == "test"));
    let replay = repository
        .claim_cluster_peer_messages(instance.id, 32, &deliver_watermark)
        .await
        .expect("claim again");
    assert!(
        replay.is_empty(),
        "delivered messages must not be re-claimed"
    );

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

/// Operator maintenance is the one instance status a machine writer may not
/// clear, and the prober's eject latch is written once per failure streak.
///
/// Both properties were false before: the node's reporter claims `online` on
/// every heartbeat (`cluster_self_report` sends a fixed `status: 1`) and
/// registration claims it again on every restart, so an operator's maintenance
/// mark — the only thing that takes an instance out of the routing pool short
/// of a cordon — survived at most one heartbeat interval; and `ejected_at` was
/// re-stamped by every further failed probe, which made the ejection age
/// unreadable and the column a duplicate of `updated_at`.
///
/// Assertions read the **table**, not a mapper projection: the question is what
/// storage kept.
async fn verify_operator_status_ownership_and_probe_latch(context: &TestContext) {
    let repository = &context.repository;
    let cluster = repository
        .resolve_cluster_identity(0, None)
        .await
        .expect("default cluster");
    let host = repository
        .upsert_cluster_host(host_write(0, cluster.cluster_id, "owner-host", "mc-owner-host"))
        .await
        .expect("ownership host");
    let instance = repository
        .upsert_cluster_instance(instance_write(
            0,
            host.id,
            cluster.cluster_id,
            3_800,
            5_300,
            hash("winst_owner-token"),
        ))
        .await
        .expect("ownership instance");
    let reported_at = || chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);

    // 1. The operator declares maintenance.
    let marked = repository
        .update_cluster_instance(
            &instance.uuid,
            &UpdateClusterInstanceRequest {
                status: Some(5),
                maintenance_note: Some("kernel upgrade".to_string()),
                ..UpdateClusterInstanceRequest::default()
            },
        )
        .await
        .expect("mark maintenance");
    assert_eq!(marked.status, 5);
    let before = instance_state(context, instance.id).await;
    assert_eq!(before.status, 5);

    // 2. The node heartbeats `online` — the mark holds, and the transition the
    //    service derives from is the recorded status, not the reported one.
    let transition = repository
        .record_cluster_heartbeat(heartbeat_write(instance.id, host.id, 1, &reported_at()))
        .await
        .expect("heartbeat while in maintenance");
    assert_eq!(transition.previous_status, 5);
    assert_eq!(
        transition.status, 5,
        "the heartbeat must acknowledge the recorded status, not the reported one"
    );
    let after_heartbeat = instance_state(context, instance.id).await;
    assert_eq!(
        after_heartbeat.status, 5,
        "a heartbeat must not end an operator maintenance mark"
    );
    assert_eq!(
        after_heartbeat.last_online_at, before.last_online_at,
        "an instance held out of service is not confirmed online by its own heartbeat"
    );
    assert!(
        after_heartbeat.last_heartbeat_at.is_some(),
        "a heartbeat in maintenance still refreshes liveness"
    );

    // 3. A restart re-registers the same slot (new pid) — the mark still holds,
    //    and the restart is still observed.
    repository
        .upsert_cluster_instance(instance_write(
            0,
            host.id,
            cluster.cluster_id,
            3_800,
            5_301,
            hash("winst_owner-token-2"),
        ))
        .await
        .expect("re-register after restart");
    let after_restart = instance_state(context, instance.id).await;
    assert_eq!(
        after_restart.status, 5,
        "re-registration must not end an operator maintenance mark"
    );
    assert_eq!(
        after_restart.restart_count, 1,
        "the slot identity survives the restart and the restart is counted"
    );

    // 4. The operator's write is the only thing that ends the mark.
    let resumed = repository
        .update_cluster_instance(
            &instance.uuid,
            &UpdateClusterInstanceRequest {
                status: Some(1),
                ..UpdateClusterInstanceRequest::default()
            },
        )
        .await
        .expect("end maintenance");
    assert_eq!(resumed.status, 1);
    let after_resume = instance_state(context, instance.id).await;
    assert_eq!(after_resume.status, 1);

    // 5. The first two failed probes count, the third ejects exactly once.
    for expected in 1..=3 {
        let outcome = probe(context, instance.id, false).await;
        assert_eq!(outcome.failures, expected);
        assert_eq!(outcome.eject_transition, expected == 3);
    }
    let ejected = instance_state(context, instance.id).await;
    assert_eq!(ejected.status, 4, "an ejected instance is reported as error");
    assert_eq!(ejected.probe_failures, 3);
    let ejected_at = ejected
        .ejected_at
        .clone()
        .expect("the ejection instant is recorded");

    // 6. A further failure keeps the failure streak but not a new instant: the
    //    ejection age is how long the instance has been out, not how long ago
    //    the prober last looked.
    let further = probe(context, instance.id, false).await;
    assert_eq!(further.failures, 4);
    assert!(
        !further.eject_transition,
        "an already-ejected instance must not re-announce its ejection"
    );
    let still_ejected = instance_state(context, instance.id).await;
    assert_eq!(
        still_ejected.ejected_at.as_deref(),
        Some(ejected_at.as_str()),
        "the ejection instant is written once per failure streak"
    );

    // 7. One healthy probe clears the latch and restores the status.
    let recovered = probe(context, instance.id, true).await;
    assert!(recovered.recovered);
    let healthy = instance_state(context, instance.id).await;
    assert_eq!(healthy.status, 1);
    assert_eq!(healthy.probe_failures, 0);
    assert_eq!(healthy.ejected_at, None);

    // 8. The prober yields to maintenance too: ejecting while the operator's
    //    mark stands must not relabel the instance as an error, and recovering
    //    must not relabel it as online.
    repository
        .update_cluster_instance(
            &instance.uuid,
            &UpdateClusterInstanceRequest {
                status: Some(5),
                ..UpdateClusterInstanceRequest::default()
            },
        )
        .await
        .expect("mark maintenance again");
    for _ in 0..3 {
        probe(context, instance.id, false).await;
    }
    let ejected_in_maintenance = instance_state(context, instance.id).await;
    assert_eq!(
        ejected_in_maintenance.status, 5,
        "the prober must not overwrite an operator maintenance mark with `error`"
    );
    assert!(ejected_in_maintenance.ejected_at.is_some());
    probe(context, instance.id, true).await;
    let recovered_in_maintenance = instance_state(context, instance.id).await;
    assert_eq!(
        recovered_in_maintenance.ejected_at, None,
        "a healthy probe clears the latch even in maintenance"
    );
    assert_eq!(
        recovered_in_maintenance.status, 5,
        "recovery must not put a maintained instance back online"
    );
}

/// A name the operator typed belongs to the operator, on **both** resources.
///
/// `update_cluster_host` / `update_cluster_instance` exist so the console can
/// rename a row, and both registration paths re-derive a default name from the
/// node's own report (`host_display_name` / `instance_display_name`). So a
/// registration that writes `name` back turns the console's rename into a value
/// that silently reverts the next time the node restarts - which is exactly what
/// maintenance work does. The instance table already treats `name` as
/// operator-owned after the first registration; this asserts the host table
/// agrees, because the same edit must not behave differently depending on which
/// resource the operator happened to use.
///
/// The machine's own facts refreshed by the same statement are asserted too, so
/// "stop overwriting" cannot be satisfied by refusing to refresh anything.
async fn verify_operator_rename_survives_re_registration(context: &TestContext) {
    let repository = &context.repository;
    let cluster = repository
        .resolve_cluster_identity(0, None)
        .await
        .expect("default cluster");

    let host = repository
        .upsert_cluster_host(host_write(0, cluster.cluster_id, "rename-host", "mc-rename-host"))
        .await
        .expect("rename host");
    // First registration seeds the name from the machine's own report.
    assert_eq!(
        repository
            .retrieve_cluster_host(&host.uuid)
            .await
            .expect("retrieve host")
            .name,
        "rename-host"
    );

    // 1. The operator renames the host.
    let renamed = repository
        .update_cluster_host(
            &host.uuid,
            &UpdateClusterHostRequest {
                name: Some("rack-a-node-1".to_string()),
                cluster_id: None,
            },
        )
        .await
        .expect("operator renames host");
    assert_eq!(renamed.name, "rack-a-node-1");

    // 2. The node re-registers: same machine code, and it reports its own
    //    default name again.
    repository
        .upsert_cluster_host(host_write(0, cluster.cluster_id, "rename-host", "mc-rename-host"))
        .await
        .expect("re-register host");

    // 3. The operator's name survives; the machine's own facts still refresh.
    let after = repository
        .retrieve_cluster_host(&host.uuid)
        .await
        .expect("retrieve host after restart");
    assert_eq!(
        after.name, "rack-a-node-1",
        "re-registration must not take back an operator host rename"
    );
    assert_eq!(
        after.hostname, "rename-host",
        "the machine's own hostname is a fact and keeps refreshing on the same write"
    );

    // 4. The instance table holds the same rule.
    let instance = repository
        .upsert_cluster_instance(instance_write(
            0,
            host.id,
            cluster.cluster_id,
            3_800,
            6_100,
            hash("winst_rename"),
        ))
        .await
        .expect("rename instance");
    let renamed_instance = repository
        .update_cluster_instance(
            &instance.uuid,
            &UpdateClusterInstanceRequest {
                name: Some("edge-1:3800 (primary)".to_string()),
                ..UpdateClusterInstanceRequest::default()
            },
        )
        .await
        .expect("operator renames instance");
    assert_eq!(renamed_instance.name, "edge-1:3800 (primary)");
    repository
        .upsert_cluster_instance(instance_write(
            0,
            host.id,
            cluster.cluster_id,
            3_800,
            6_101,
            hash("winst_rename"),
        ))
        .await
        .expect("re-register instance");
    assert_eq!(
        repository
            .retrieve_cluster_instance(&instance.uuid)
            .await
            .expect("retrieve instance after restart")
            .name,
        "edge-1:3800 (primary)",
        "re-registration must not take back an operator instance rename"
    );
}

/// A machine that reports nothing must not erase what it never knew.
///
/// `public_endpoint` is operator-writable (`update_cluster_instance_repo` writes
/// `public_endpoint = COALESCE($5, public_endpoint)`, i.e. last non-null wins and
/// an operator cannot clear it either), but the registration path wrote
/// `public_endpoint = EXCLUDED.public_endpoint` outright. A node without
/// `SDKWORK_WEBSERVER_INSTANCE_PUBLIC_ENDPOINT` reports `None`, so every restart
/// - which is exactly what maintenance work does - wiped an endpoint the
/// operator had pinned for an instance behind NAT, and the operator had to
/// re-enter it after each cycle.
///
/// "Absent" and "explicitly none" are different statements: the node that never
/// had the value is not asserting the value is gone.
async fn verify_machine_absence_does_not_erase_operator_values(context: &TestContext) {
    let repository = &context.repository;
    let cluster = repository
        .resolve_cluster_identity(0, None)
        .await
        .expect("default cluster");
    let host = repository
        .upsert_cluster_host(host_write(0, cluster.cluster_id, "absence-host", "mc-absence-host"))
        .await
        .expect("absence host");
    let instance = repository
        .upsert_cluster_instance(instance_write(
            0,
            host.id,
            cluster.cluster_id,
            4_100,
            7_100,
            hash("winst_absence"),
        ))
        .await
        .expect("absence instance");

    // 1. The operator pins the address the instance is reachable at from
    //    outside - an override the registry has no other way to learn.
    let pinned = "https://edge-absence.nat.example.test";
    repository
        .update_cluster_instance(
            &instance.uuid,
            &UpdateClusterInstanceRequest {
                public_endpoint: Some(pinned.to_string()),
                ..UpdateClusterInstanceRequest::default()
            },
        )
        .await
        .expect("operator pins the public endpoint");
    assert_eq!(
        repository
            .retrieve_cluster_instance(&instance.uuid)
            .await
            .expect("retrieve instance")
            .public_endpoint
            .as_deref(),
        Some(pinned)
    );

    // 2. The node restarts without that env var set, so it reports nothing.
    let mut silent = instance_write(
        0,
        host.id,
        cluster.cluster_id,
        4_100,
        7_101,
        hash("winst_absence"),
    );
    silent.public_endpoint = None;
    repository
        .upsert_cluster_instance(silent)
        .await
        .expect("re-register without a public endpoint");

    // 3. The operator's pinned address is still there.
    assert_eq!(
        repository
            .retrieve_cluster_instance(&instance.uuid)
            .await
            .expect("retrieve instance after restart")
            .public_endpoint
            .as_deref(),
        Some(pinned),
        "a node that reports no public endpoint must not clear the operator's"
    );

    // 4. A node that *does* report one still wins: the machine's fresh value is
    //    the point of reporting it at all.
    let mut reporting = instance_write(
        0,
        host.id,
        cluster.cluster_id,
        4_100,
        7_102,
        hash("winst_absence"),
    );
    reporting.public_endpoint = Some("https://edge-absence.tunnel.example.test".to_string());
    repository
        .upsert_cluster_instance(reporting)
        .await
        .expect("re-register reporting a new public endpoint");
    assert_eq!(
        repository
            .retrieve_cluster_instance(&instance.uuid)
            .await
            .expect("retrieve instance after a reporting restart")
            .public_endpoint
            .as_deref(),
        Some("https://edge-absence.tunnel.example.test"),
        "a reported endpoint replaces the stored one"
    );
}

/// Raw instance columns the ownership / latch assertions care about.
struct InstanceState {
    status: i32,
    probe_failures: i32,
    ejected_at: Option<String>,
    last_online_at: Option<String>,
    last_heartbeat_at: Option<String>,
    restart_count: i32,
}

async fn instance_state(context: &TestContext, instance_id: i64) -> InstanceState {
    let row = sqlx::query_as::<_, (
        i32,
        i32,
        Option<String>,
        Option<String>,
        Option<String>,
        i32,
    )>(
        "SELECT status, probe_failures, CAST(ejected_at AS TEXT), CAST(last_online_at AS TEXT),
                CAST(last_heartbeat_at AS TEXT), restart_count
         FROM webserver_cluster_instance WHERE id = $1",
    )
    .bind(instance_id)
    .fetch_one(&context.pool)
    .await
    .expect("read instance state");
    InstanceState {
        status: row.0,
        probe_failures: row.1,
        ejected_at: row.2,
        last_online_at: row.3,
        last_heartbeat_at: row.4,
        restart_count: row.5,
    }
}

fn heartbeat_write(
    instance_id: i64,
    host_id: i64,
    status: i32,
    reported_at: &str,
) -> ClusterHeartbeatWrite {
    ClusterHeartbeatWrite {
        tenant_id: 0,
        instance_id,
        host_id,
        status,
        health_state: "HEALTHY".to_string(),
        uptime_seconds: 60,
        build_version: None,
        metrics_json: "{}".to_string(),
        reported_at: reported_at.to_string(),
        quality_score: None,
    }
}

async fn probe(context: &TestContext, instance_id: i64, healthy: bool) -> ClusterProbeOutcome {
    context
        .repository
        .record_cluster_probe_outcome(ClusterProbeWrite {
            tenant_id: 0,
            instance_id,
            healthy,
        })
        .await
        .expect("record probe outcome")
}

async fn verify_liveness_sweep_events_and_pagination(context: &TestContext) {
    let repository = &context.repository;

    let default_cluster = repository
        .resolve_cluster_identity(0, None)
        .await
        .expect("default cluster");
    let host = repository
        .upsert_cluster_host(host_write(
            0,
            default_cluster.cluster_id,
            "sweep-host",
            "mc-sweep",
        ))
        .await
        .expect("sweep host");
    let instance = repository
        .upsert_cluster_instance(instance_write(
            0,
            host.id,
            default_cluster.cluster_id,
            3_800,
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
            quality_score: None,
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
    assert!(expired_hosts
        .iter()
        .any(|expired| expired.host_uuid == host.uuid));

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
        .list_cluster_events(None, Some("WARNING"), None, 200, None)
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
            .list_cluster_events(None, None, None, 2, cursor.as_deref())
            .await
            .expect("event page");
        visited += page.items.len();
        match (page.has_more, page.next_cursor) {
            (Some(true), Some(next)) => cursor = Some(next),
            _ => break,
        }
    }
    let total_events: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM webserver_cluster_event WHERE tenant_id = 0")
            .fetch_one(&context.pool)
            .await
            .expect("count events");
    assert_eq!(
        visited, total_events as usize,
        "cursor walk must visit every event exactly once"
    );

    // Instance listing filters by health state and paginates by cursor.
    let unhealthy_page = repository
        .list_cluster_instances(None, None, None, Some("UNHEALTHY"), None, None, &[], None, None, 200, None)
        .await
        .expect("unhealthy instances");
    assert!(unhealthy_page
        .items
        .iter()
        .all(|item| item.health_state == "UNHEALTHY"));

    let mut instance_cursor = None;
    let mut instance_visited = 0_usize;
    loop {
        let page = repository
            .list_cluster_instances(
                None,
                None,
                None,
                None,
                None,
                None,
                &[],
                None,
                None,
                2,
                instance_cursor.as_deref(),
            )
            .await
            .expect("instance page");
        instance_visited += page.items.len();
        assert!(page.items.iter().all(|item| item
            .host_name
            .as_deref()
            .is_some_and(|name| !name.is_empty())));
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
            lb_strategy: Some("round_robin".to_string()),
            served_domains: Some(vec!["svc.cluster.test".to_string()]),
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
                lb_strategy: None,
                served_domains: None,
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
    assert_eq!(
        updated.offline_threshold_seconds, 60,
        "absent fields keep their value"
    );
    repository
        .delete_cluster(&created.id)
        .await
        .expect("delete cluster");
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
    assert!(
        purged >= 2,
        "both heartbeat samples are older than the watermark"
    );
}

fn hash(token: &str) -> String {
    sha256_hash(token.as_bytes())
}

fn host_write(
    tenant_id: i64,
    cluster_id: i64,
    hostname: &str,
    machine_code: &str,
) -> ClusterHostUpsert {
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
        join_mode: 0,
        tunnel_route_domain: None,
        tunnel_endpoint: None,
    }
}

/// One instance registration fixture.
///
/// `bind_port` (with the fixed `bind_host` below) is the instance's **slot** —
/// the identity that has to survive a restart — while `process_pid` is a
/// run-state observation that changes on every restart. The two are separate
/// arguments on purpose: correlating them is what made the old
/// `(tenant, host, pid)` conflict target look correct while never matching in
/// production.
#[allow(clippy::too_many_arguments)]
fn instance_write(
    tenant_id: i64,
    host_id: i64,
    cluster_id: i64,
    bind_port: i32,
    process_pid: i32,
    instance_token_hash: String,
) -> ClusterInstanceUpsert {
    ClusterInstanceUpsert {
        tenant_id,
        host_id,
        cluster_id,
        name: format!("gateway:{bind_port}"),
        role: "GATEWAY".to_string(),
        environment: "test".to_string(),
        process_pid,
        process_started_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        bind_host: Some("0.0.0.0".to_string()),
        bind_port: Some(bind_port),
        public_endpoint: Some(format!("https://node-{bind_port}.example.test")),
        build_version: Some("1.1.0".to_string()),
        instance_token_hash,
        join_mode: 0,
        tunnel_route_domain: None,
    }
}

/// Restart counter the registry persisted for one instance row.
///
/// Read straight from the table rather than through the repository: the point
/// of the assertion is what the **storage** kept, not what a mapper projected.
async fn restart_count(context: &TestContext, instance_id: i64) -> i32 {
    sqlx::query_scalar("SELECT restart_count FROM webserver_cluster_instance WHERE id = $1")
        .bind(instance_id)
        .fetch_one(&context.pool)
        .await
        .expect("restart count")
}
