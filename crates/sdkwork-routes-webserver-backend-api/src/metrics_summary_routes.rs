//! Dashboard metric summary readings (backend-admin surface).
//!
//! Two operations over one read model, and the difference between them is
//! **reach**, exactly as for the traffic readings:
//!
//! - `GET /backend/v3/api/metrics_summaries` answers the caller's own tenant —
//!   the reading the tenant console renders.
//! - `GET /backend/v3/api/platform_metrics_summaries` answers every tenant this
//!   edge serves — the reading the operations surface renders, and the only one
//!   that reports a tenant count.
//!
//! Deliberately not one operation that widens for the operator tenant. With a
//! single operation the console of an operator-tenant user would count every
//! tenant in the installation while looking exactly like its own estate, and
//! nothing in the response would reveal the difference — a count carries no
//! identifiers that could give it away. Two operations also mean a misrouted
//! admin page fails loudly (403) instead of quietly returning one tenant's
//! figures as the platform's.
//!
//! Neither operation takes a **scoping** parameter. The four card windows are
//! resolved server-side from the clock, so there is no client-supplied bound for
//! a scope or a calendar to hide behind.
//!
//! Both do accept `date_from` / `date_to`, and the reason is worth stating
//! precisely because it looks like a contradiction: those bounds reach the
//! **series** only — the per-day trend — and never the cards. The series is the
//! one part of this reading that is cut against the caller's days rather than
//! the product's, because the plot it feeds also draws the traffic reading's
//! own per-day series and that plot has one x domain; the two readings must
//! resolve the same window or the lines would be plotted against a domain their
//! days were not cut against. See `MetricsSummaryQuery` in the contract.

use axum::{
    extract::{Query, State},
    response::Response,
    Extension,
};
use sdkwork_routes_webserver_common::{ok_resource, WebApiError};
use sdkwork_webserver_contract::{MetricsSummaryQuery, WebBackendRequestContext};

use crate::auth::{require_backend_context, require_platform_operator};
use crate::routes::BackendState;

const METRICS_SUMMARY_SURFACE: &str = "the platform-wide dashboard metric summary reading";

/// `GET /backend/v3/api/metrics_summaries` — the caller's own tenant.
///
/// No tenant reaches this handler from the query string: the scope comes from
/// the authenticated context, so a `?tenant_id=` cannot retarget the read.
pub(crate) async fn retrieve_metrics_summary(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<MetricsSummaryQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    ok_resource(state.api.retrieve_metrics_summary(&context, &query).await)
}

/// `GET /backend/v3/api/platform_metrics_summaries` — every tenant.
///
/// The operator-tenant predicate runs here as well as inside the service so the
/// rejection carries the canonical `40301` permission code instead of a bare
/// `Forbidden`; belonging to the operator tenant remains necessary but not
/// sufficient, because the framework still checks the operation's `web.*`
/// permission before this handler is reached.
pub(crate) async fn retrieve_platform_metrics_summary(
    State(state): State<BackendState>,
    context: Option<Extension<WebBackendRequestContext>>,
    Query(query): Query<MetricsSummaryQuery>,
) -> Result<Response, WebApiError> {
    let context = require_backend_context(context)?;
    require_platform_operator(&context, METRICS_SUMMARY_SURFACE)?;
    ok_resource(
        state
            .api
            .retrieve_platform_metrics_summary(&context, &query)
            .await,
    )
}
