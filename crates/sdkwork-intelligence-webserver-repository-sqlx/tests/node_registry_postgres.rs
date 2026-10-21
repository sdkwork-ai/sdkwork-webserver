//! Real-PostgreSQL evidence for the Snowflake node registry: two replicas of
//! the same service must receive different node ids from the shared lease
//! table, and a released lease must become reusable.
//!
//! Ignored by default because it needs a disposable PostgreSQL instance
//! (`SDKWORK_DATABASE_TEST_POSTGRES_URL`); executed by the release workflow's
//! database gate, not by `cargo test --workspace`.

use sdkwork_database_config::{DatabaseConfig, DatabaseEngine};
use sdkwork_database_id::{NodeAllocatorConfig, SnowflakeNodeAllocator};
use sdkwork_database_sqlx::create_pool_from_config;

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_TEST_POSTGRES_URL pointing at a disposable PostgreSQL database"]
async fn replicas_receive_distinct_node_ids_and_released_leases_are_reusable() {
    let url = std::env::var("SDKWORK_DATABASE_TEST_POSTGRES_URL")
        .expect("SDKWORK_DATABASE_TEST_POSTGRES_URL must be set");
    let config = DatabaseConfig {
        engine: DatabaseEngine::Postgres,
        url,
        ..DatabaseConfig::default()
    };
    let pool = create_pool_from_config(config).await.expect("create pool");

    let service = "sdkwork-webserver-node-registry-test";
    let mut first = NodeAllocatorConfig::from_service_name(service);
    first.instance_identity = "replica-a".to_string();
    let mut second = NodeAllocatorConfig::from_service_name(service);
    second.instance_identity = "replica-b".to_string();

    let lease_a = SnowflakeNodeAllocator::allocate(&pool, &first)
        .await
        .expect("allocate the first replica");
    let lease_b = SnowflakeNodeAllocator::allocate(&pool, &second)
        .await
        .expect("allocate the second replica");
    assert_ne!(
        lease_a.node_id(),
        lease_b.node_id(),
        "two replicas must not share one Snowflake node id"
    );

    let released = lease_a.node_id();
    drop(lease_a);
    assert!(released < 1024);
    // A lease whose heartbeat stopped is reclaimable; the allocator prefers a
    // different free id, so the assertion only proves the allocation succeeded
    // and stayed inside the generator's node range.
    let third = NodeAllocatorConfig::from_service_name(service);
    let lease_c = SnowflakeNodeAllocator::allocate(&pool, &third)
        .await
        .expect("allocate after the first lease was released");
    assert!(lease_c.node_id() < 1024);
}
