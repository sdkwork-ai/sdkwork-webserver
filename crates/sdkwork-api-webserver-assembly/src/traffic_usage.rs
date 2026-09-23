//! Traffic usage read model, supplied by the Deploy control plane.
//!
//! The edge's data plane meters every request it serves and ingests the
//! aggregated windows into the Deploy control plane's tables (see
//! `usage_metering.rs` in the standalone gateway). In the standalone deployment
//! that module is composed into this process over the same PostgreSQL pool, so
//! reading the aggregated figures means reading a sibling module's repository.
//! That is why the service declares [`TrafficUsageReadPort`] instead of
//! depending on the persistence crate, and why the adapter lives here: the API
//! assembly is the crate allowed to construct a repository and mount
//! dependencies (`RUST_CODE_SPEC` §1.1, `api-assembly` row).

use std::sync::Arc;

use async_trait::async_trait;
use sdkwork_deploy_contract as deploy;
use sdkwork_webserver_contract::{
    TrafficUsageAppTotal, TrafficUsageDailyPoint, TrafficUsageReadPort,
    TrafficUsageStatisticsResponse, TrafficUsageTenantTotal, TrafficUsageTotal,
    TrafficUsageWindow, WebServiceError, WebServiceResult,
};
use sqlx::PgPool;

use crate::DeployRepository;

/// Fact table the read model aggregates.
///
/// Named here only to answer "does this deployment serve usage facts at all";
/// the aggregation itself stays behind [`DeployRepository`].
const TRAFFIC_USAGE_FACT_TABLE: &str = "deploy_usage_event";

/// [`TrafficUsageReadPort`] over the Deploy control plane's usage facts.
pub(crate) struct DeployTrafficUsageReadPort {
    repository: DeployRepository,
}

impl DeployTrafficUsageReadPort {
    pub(crate) fn new(repository: DeployRepository) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl TrafficUsageReadPort for DeployTrafficUsageReadPort {
    async fn retrieve_traffic_usage_statistics(
        &self,
        tenant_id: Option<i64>,
        window: &TrafficUsageWindow,
    ) -> WebServiceResult<TrafficUsageStatisticsResponse> {
        self.repository
            .traffic_usage_statistics_lookup(tenant_id, &deploy_query(window))
            .await
            .map(project_statistics)
            .map_err(|error| project_error(&error))
    }
}

/// The read model to inject, or `None` when this deployment serves no facts.
///
/// Availability is established at boot rather than assumed. The facts belong to
/// a sibling module, and this assembly is also mounted by hosts that compose no
/// Deploy control plane; such a host must report the capability as **not
/// assembled** — the same HTTP `503` an absent port produces — instead of
/// letting every read fail as an internal error against a table that was never
/// there. A missing pool, a non-PostgreSQL engine, or an unresolvable fact table
/// all end in the same explicit, logged absence.
pub(crate) async fn shared_deploy_traffic_usage_reader() -> Option<Arc<dyn TrafficUsageReadPort>> {
    let Some(pool) = sdkwork_database_sqlx::process_shared_database_pool() else {
        tracing::warn!(
            "traffic usage statistics report 503: no process-shared database pool is installed"
        );
        return None;
    };
    let Some(postgres) = pool.as_postgres().cloned() else {
        tracing::warn!(
            "traffic usage statistics report 503: the process-shared pool is not PostgreSQL"
        );
        return None;
    };
    match traffic_usage_fact_table_present(&postgres).await {
        Ok(true) => Some(Arc::new(DeployTrafficUsageReadPort::new(
            DeployRepository::new_lookup(postgres),
        ))),
        Ok(false) => {
            tracing::warn!(
                table = TRAFFIC_USAGE_FACT_TABLE,
                "traffic usage statistics report 503: this deployment serves no usage facts"
            );
            None
        }
        Err(error) => {
            tracing::warn!(
                table = TRAFFIC_USAGE_FACT_TABLE,
                error = %error,
                "traffic usage statistics report 503: the usage fact table could not be resolved"
            );
            None
        }
    }
}

async fn traffic_usage_fact_table_present(pool: &PgPool) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>("SELECT to_regclass($1::text) IS NOT NULL")
        .bind(TRAFFIC_USAGE_FACT_TABLE)
        .fetch_one(pool)
        .await
}

/// Widens a resolved window onto the facts owner's own query type.
///
/// The window is already concrete (the service resolves the defaults and the
/// span bound), so this is a representation change only: the facts owner must
/// never be asked to interpret "no window given".
fn deploy_query(window: &TrafficUsageWindow) -> deploy::TrafficUsageStatisticsQuery {
    deploy::TrafficUsageStatisticsQuery {
        date_from: window.date_from.clone(),
        date_to: window.date_to.clone(),
        dimension: window.dimension.clone(),
        top_apps: i64::from(window.top_apps),
    }
}

/// One reading, projected field for field onto this service's response.
///
/// Two types rather than one shared type on purpose: the port inverts the
/// dependency, so the wire shape stays owned here while the facts owner keeps
/// its own view. Every reported figure is carried over, including the
/// unattributed app row, so a breakdown always sums back to its total.
fn project_statistics(source: deploy::TrafficUsageStatistics) -> TrafficUsageStatisticsResponse {
    TrafficUsageStatisticsResponse {
        date_from: source.date_from,
        date_to: source.date_to,
        platform_scope: source.platform_scope,
        totals: source.totals.into_iter().map(project_total).collect(),
        daily: source.daily.into_iter().map(project_daily_point).collect(),
        apps: source.apps.into_iter().map(project_app_total).collect(),
        tenants: source.tenants.into_iter().map(project_tenant_total).collect(),
    }
}

fn project_total(source: deploy::TrafficUsageTotal) -> TrafficUsageTotal {
    TrafficUsageTotal {
        dimension: source.dimension,
        quantity: source.quantity,
        unit: source.unit,
    }
}

fn project_daily_point(source: deploy::TrafficUsageDailyPoint) -> TrafficUsageDailyPoint {
    TrafficUsageDailyPoint {
        usage_date: source.usage_date,
        dimension: source.dimension,
        quantity: source.quantity,
    }
}

fn project_app_total(source: deploy::TrafficUsageAppTotal) -> TrafficUsageAppTotal {
    TrafficUsageAppTotal {
        app_uuid: source.app_uuid,
        app_slug: source.app_slug,
        dimension: source.dimension,
        quantity: source.quantity,
        unit: source.unit,
    }
}

fn project_tenant_total(source: deploy::TrafficUsageTenantTotal) -> TrafficUsageTenantTotal {
    TrafficUsageTenantTotal {
        tenant_id: source.tenant_id,
        dimension: source.dimension,
        quantity: source.quantity,
        unit: source.unit,
    }
}

/// Maps a facts-owner failure onto this service's error model.
///
/// The upstream error is logged with its full `Debug` detail at this boundary —
/// the only place that knows the cause — and then flattened, because
/// [`WebServiceError`] is deliberately cause-free so internal text cannot reach
/// a client through a downstream `source()` walk (`SECURITY_SPEC`).
///
/// Only the classes the contract defines as client-safe keep their class;
/// anything else is reported as internal, since a failure this service cannot
/// interpret must not reach an operator as a problem with their reading.
fn project_error(error: &deploy::DeployServiceError) -> WebServiceError {
    use deploy::DeployServiceErrorKind as Kind;
    match error.kind() {
        Kind::Forbidden => WebServiceError::Forbidden,
        Kind::Validation => WebServiceError::validation(error.to_string()),
        Kind::DatabaseUnavailable => WebServiceError::DatabaseUnavailable,
        Kind::NotFound | Kind::Conflict | Kind::QuotaExceeded | Kind::Internal => {
            tracing::error!(
                upstream = ?error,
                "traffic usage statistics read failed in the Deploy control plane"
            );
            WebServiceError::Internal(error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading() -> deploy::TrafficUsageStatistics {
        deploy::TrafficUsageStatistics {
            date_from: "2026-09-01".to_owned(),
            date_to: "2026-10-01".to_owned(),
            platform_scope: true,
            totals: vec![deploy::TrafficUsageTotal {
                dimension: "traffic.requests".to_owned(),
                quantity: 41,
                unit: "REQUEST".to_owned(),
            }],
            daily: vec![deploy::TrafficUsageDailyPoint {
                usage_date: "2026-09-01".to_owned(),
                dimension: "traffic.requests".to_owned(),
                quantity: 7,
            }],
            apps: vec![deploy::TrafficUsageAppTotal {
                app_uuid: Some("app-1".to_owned()),
                app_slug: Some("storefront".to_owned()),
                dimension: "traffic.requests".to_owned(),
                quantity: 5,
                unit: "REQUEST".to_owned(),
            }],
            tenants: vec![deploy::TrafficUsageTenantTotal {
                tenant_id: 42,
                dimension: "traffic.requests".to_owned(),
                quantity: 5,
                unit: "REQUEST".to_owned(),
            }],
        }
    }

    #[test]
    fn the_resolved_window_reaches_the_facts_owner_unchanged() {
        let query = deploy_query(&TrafficUsageWindow {
            date_from: "2026-09-01".to_owned(),
            date_to: "2026-10-01".to_owned(),
            dimension: Some("traffic.egress_bytes".to_owned()),
            top_apps: 12,
        });

        assert_eq!(query.date_from, "2026-09-01");
        assert_eq!(query.date_to, "2026-10-01");
        assert_eq!(query.dimension.as_deref(), Some("traffic.egress_bytes"));
        assert_eq!(query.top_apps, 12, "the window bound must not be re-shaped");
    }

    #[test]
    fn every_reported_figure_survives_the_projection() {
        let response = project_statistics(reading());

        assert!(response.platform_scope);
        assert_eq!(response.date_from, "2026-09-01");
        assert_eq!(response.date_to, "2026-10-01");
        assert_eq!(response.totals.len(), 1);
        assert_eq!(response.totals[0].dimension, "traffic.requests");
        assert_eq!(response.totals[0].quantity, 41);
        assert_eq!(response.totals[0].unit, "REQUEST");
        assert_eq!(response.daily[0].usage_date, "2026-09-01");
        assert_eq!(response.daily[0].quantity, 7);
        assert_eq!(response.apps[0].app_uuid.as_deref(), Some("app-1"));
        assert_eq!(response.apps[0].app_slug.as_deref(), Some("storefront"));
        assert_eq!(response.apps[0].quantity, 5);
        assert_eq!(response.tenants[0].tenant_id, 42);
    }

    #[test]
    fn the_unattributed_app_row_keeps_its_place_without_identifiers() {
        let response = project_statistics(deploy::TrafficUsageStatistics {
            apps: vec![deploy::TrafficUsageAppTotal {
                dimension: "traffic.requests".to_owned(),
                quantity: 3,
                unit: "REQUEST".to_owned(),
                ..Default::default()
            }],
            ..Default::default()
        });

        assert_eq!(response.apps.len(), 1, "dropping it would break the sum");
        assert_eq!(response.apps[0].app_uuid, None);
        assert_eq!(response.apps[0].app_slug, None);
        assert_eq!(response.apps[0].quantity, 3);
    }

    #[test]
    fn only_client_safe_upstream_failures_keep_their_class() {
        assert!(matches!(
            project_error(&deploy::DeployServiceError::Forbidden("nope".to_owned())),
            WebServiceError::Forbidden
        ));
        assert!(matches!(
            project_error(&deploy::DeployServiceError::Validation("bad".to_owned())),
            WebServiceError::Validation(_)
        ));
        assert!(matches!(
            project_error(&deploy::DeployServiceError::DatabaseUnavailable),
            WebServiceError::DatabaseUnavailable
        ));
        assert!(matches!(
            project_error(&deploy::DeployServiceError::Internal(
                "relation \"deploy_usage_event\" does not exist".to_owned()
            )),
            WebServiceError::Internal(_)
        ));
    }
}
