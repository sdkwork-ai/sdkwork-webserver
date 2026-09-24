//! Aggregated traffic usage readings (backend-admin surface).
//!
//! Two operations over one read model, and the difference between them is
//! **reach**, so reach is a property of the operation:
//!
//! - `GET /backend/v3/api/traffic_usage` answers the caller's own tenant — the
//!   reading the tenant console renders.
//! - `GET /backend/v3/api/platform_traffic_usage` answers every tenant this
//!   edge serves — the reading the operations surface renders.
//!
//! Deliberately not one operation that widens for the operator tenant. With a
//! single operation the console of an operator-tenant user would read every
//! tenant's traffic while looking exactly like their own, and no field of the
//! response would reveal the difference. Two operations also mean a misrouted
//! admin page fails loudly (403) instead of quietly returning a single
//! tenant's numbers as the platform total.

use axum::{
    extract::{Query, State},
    response::Response,
    Extension,
};
use sdkwork_routes_webserver_common::{ok_resource, WebApiError};
use sdkwork_webserver_contract::{TrafficUsageStatisticsQuery, WebBackendRequestContext};

use crate::auth::{require_backend_context, require_platform_operator};
use crate::routes::BackendState;

const TRAFFIC_USAGE_SURFACE: &str = "the platform-wide traffic usage reading";

/// `GET /backend/v3/api/traffic_usage` — the caller's own tenant.
///
/// No tenant reaches this handler from the query string: the scope comes from
/// the authenticated context, so a `?tenant_id=` cannot retarget the read.
pub(crate) async fn retrieve_tenant_traffic_usage(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<TrafficUsageStatisticsQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(
        state
            .api
            .retrieve_traffic_usage_statistics(&context, &query)
            .await,
    )
}

/// `GET /backend/v3/api/platform_traffic_usage` — every tenant.
///
/// The operator-tenant predicate runs here as well as inside the service so the
/// rejection carries the canonical `40301` permission code instead of a bare
/// `Forbidden`; belonging to the operator tenant remains necessary but not
/// sufficient, because the framework still checks the operation's `web.*`
/// permission before this handler is reached.
pub(crate) async fn retrieve_platform_traffic_usage(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<TrafficUsageStatisticsQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    require_platform_operator(&context, TRAFFIC_USAGE_SURFACE)?;
    ok_resource(
        state
            .api
            .retrieve_platform_traffic_usage_statistics(&context, &query)
            .await,
    )
}
