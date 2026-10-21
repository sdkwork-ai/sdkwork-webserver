//! Shared bootstrap for the startup reconciliation passes.
//!
//! `served_domains` (tenant-level hostname inventory) and
//! `application_registry` (imported-application surface inventory) are two
//! views of the same authority — the effective served configuration of the
//! edge — and both run once at startup before the listener binds. They share
//! one bootstrap: the process-shared database pool, the platform-operator
//! tenant id, and the snowflake id generator that allocates row ids. Lifting
//! that bootstrap here keeps the two passes identical in their fail-open
//! posture (log and continue, never fail the boot over bookkeeping rows) and
//! prevents their preconditions from drifting apart.

use sdkwork_database_id::{NodeAllocatorConfig, SnowflakeIdGenerator, SnowflakeNodeAllocator};
use sdkwork_database_sqlx::DatabasePool;
use sdkwork_webserver_core::web_platform_operator_tenant_id;
use sqlx::PgPool;

/// `metadata.source` of every row the startup reconciliation passes own.
/// Retirement in both passes is scoped to exactly this value, so a row an
/// operator or an application created is structurally out of reach.
pub(crate) const SERVED_CONFIG_METADATA_SOURCE: &str = "served-config";

/// Whether an opt-out environment value disables its reconciliation pass.
///
/// Split out from the environment read so the accepted spellings are testable
/// without mutating a process-global.
pub fn reconcile_disabled_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "false" | "off" | "no" | "disabled"
    )
}

/// Whether the named opt-out environment variable enables its pass (on by
/// default: an edge that serves configuration but writes no inventory is the
/// defect these passes exist to remove).
pub fn reconcile_enabled_for(env_key: &str) -> bool {
    std::env::var(env_key)
        .map(|value| !reconcile_disabled_value(&value))
        .unwrap_or(true)
}

/// Everything a reconciliation pass needs to write its inventory.
#[cfg(feature = "management")]
pub(crate) struct StartupReconcileContext {
    pub pool: PgPool,
    pub id_generator: SnowflakeIdGenerator,
    pub tenant_id: i64,
}

/// Resolve the shared startup reconciliation context, or `None` when a
/// precondition is missing.
///
/// Every miss is a logged warning naming `operation`, never an error: an edge
/// whose inventory cannot be reconciled must still serve, and refusing to boot
/// over a bookkeeping row would turn a missing inventory into an outage.
#[cfg(feature = "management")]
pub(crate) async fn startup_reconcile_context(operation: &str) -> Option<StartupReconcileContext> {
    let Some(database_pool) = sdkwork_database_sqlx::process_shared_database_pool() else {
        tracing::warn!("the shared database pool is unavailable; {operation} was not reconciled");
        return None;
    };
    let pool = match &database_pool {
        DatabasePool::Postgres(pool, _) => pool.clone(),
    };
    let tenant_id = match web_platform_operator_tenant_id().parse::<i64>() {
        Ok(tenant_id) => tenant_id,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "the platform operator tenant id is not numeric; {operation} was not reconciled"
            );
            return None;
        }
    };
    // An explicit non-zero node id wins; otherwise share the same registry
    // lease allocator the management runtime uses, so reconciliation does not
    // generate row ids on a node that collides with the main process.
    let configured_node_id = match std::env::var("SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID") {
        Ok(value) => match value.trim().parse::<u16>() {
            Ok(0) => None,
            Ok(node_id) => Some(node_id),
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID is not a node id; {operation} was not reconciled"
                );
                return None;
            }
        },
        Err(_) => None,
    };
    let id_generator = match configured_node_id {
        Some(node_id) => match SnowflakeIdGenerator::new(node_id) {
            Ok(generator) => generator,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "invalid snowflake node id; {operation} was not reconciled"
                );
                return None;
            }
        },
        None => {
            let service_name = "sdkwork-webserver";
            match SnowflakeNodeAllocator::allocate_process_generator(
                &database_pool,
                &NodeAllocatorConfig::from_service_name(service_name),
            )
            .await
            {
                Ok((generator, _lease)) => generator,
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        "snowflake node id is not configured and the node registry allocation failed; {operation} was not reconciled"
                    );
                    return None;
                }
            }
        }
    };
    Some(StartupReconcileContext {
        pool,
        id_generator,
        tenant_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reconcile_switch_accepts_the_usual_spellings_of_off() {
        for value in ["0", "false", "FALSE", " off ", "no", "disabled", ""] {
            assert!(reconcile_disabled_value(value), "{value:?} should disable");
        }
        for value in ["1", "true", "on", "yes", "enabled"] {
            assert!(!reconcile_disabled_value(value), "{value:?} should enable");
        }
    }
}
