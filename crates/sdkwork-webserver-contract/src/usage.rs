//! Aggregated traffic usage statistics for the Web Server management surface.
//!
//! The traffic facts are recorded by this edge's data plane and ingested into
//! the Deploy control plane (see `usages` in the standalone gateway). The read
//! side is therefore served through [`TrafficUsageReadPort`]: this crate owns
//! the wire shape and the scope rule, while the host supplies an adapter over
//! the repository that owns the facts. Keeping the port here is what lets the
//! service stay free of a dependency on another module's persistence crate.
//!
//! **Two scopes, one endpoint.** The Web Server console and the operations
//! surface render the same metrics at different reach: the console answers "my
//! own traffic", the operations surface answers "every tenant this edge
//! serves". Which one a caller gets is decided server-side — never by a query
//! parameter — because a scope switch a client can flip is not a scope rule.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::problem::WebServiceResult;

/// One usage dimension's aggregate over the requested window. This is a
/// **string on the wire** (API_SPEC §13.6): a quantity is an `int64` and
/// JavaScript loses precision above 2^53.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficUsageTotal {
    /// Usage dimension (`traffic.requests`, `traffic.ingress_bytes`,
    /// `traffic.egress_bytes`). Left open on purpose: the contract reports the
    /// dimensions the facts carry rather than rejecting an unrecognized one,
    /// so a new metered dimension reaches the surface instead of a 500.
    pub dimension: String,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub quantity: i64,
    /// Unit of `quantity` (`REQUEST`, `BYTE`).
    pub unit: String,
}

/// One day of one usage dimension, for the trend series.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficUsageDailyPoint {
    /// Calendar day in UTC (`YYYY-MM-DD`).
    #[serde(rename = "usageDate")]
    pub usage_date: String,
    pub dimension: String,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub quantity: i64,
}

/// One app's aggregate of one usage dimension over the window.
///
/// A row with neither `appUuid` nor `appSlug` is the **unattributed bucket**:
/// traffic served for a hostname the edge could not resolve to an app. It is
/// reported rather than dropped so the per-app rows keep summing back to the
/// corresponding total; a surface must render it as its own row rather than
/// hiding it, or the breakdown appears to lose traffic.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficUsageAppTotal {
    #[serde(rename = "appUuid", default, skip_serializing_if = "Option::is_none")]
    pub app_uuid: Option<String>,
    #[serde(rename = "appSlug", default, skip_serializing_if = "Option::is_none")]
    pub app_slug: Option<String>,
    pub dimension: String,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub quantity: i64,
    pub unit: String,
}

/// One tenant's aggregate of one usage dimension over the window.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficUsageTenantTotal {
    #[serde(rename = "tenantId", with = "sdkwork_utils_rust::serde_int64")]
    pub tenant_id: i64,
    pub dimension: String,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub quantity: i64,
    pub unit: String,
}

/// Aggregate traffic usage over a closed date window.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficUsageStatisticsResponse {
    /// Inclusive UTC day the window starts on (`YYYY-MM-DD`).
    #[serde(rename = "dateFrom")]
    pub date_from: String,
    /// **Exclusive** UTC day the window ends on (`YYYY-MM-DD`).
    #[serde(rename = "dateTo")]
    pub date_to: String,
    /// Whether the figures cover every tenant rather than the caller's own.
    ///
    /// Present so a surface cannot render a platform-wide number as if it were
    /// the caller's own (or the reverse): the two readings are indistinguishable
    /// from the numbers alone, and the difference is a privacy boundary.
    #[serde(rename = "platformScope")]
    pub platform_scope: bool,
    pub totals: Vec<TrafficUsageTotal>,
    pub daily: Vec<TrafficUsageDailyPoint>,
    pub apps: Vec<TrafficUsageAppTotal>,
    /// Per-tenant breakdown. Empty for a tenant-scoped read, where the answer
    /// would be the caller's own totals repeated once per dimension.
    pub tenants: Vec<TrafficUsageTenantTotal>,
}

/// Filters for a traffic usage statistics read.
///
/// Wire vocabulary is `lower_snake_case` on query strings (PAGINATION_SPEC §3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrafficUsageStatisticsQuery {
    /// Inclusive UTC day (`YYYY-MM-DD`). Defaults to the last 30 days.
    #[serde(rename = "date_from", default, skip_serializing_if = "Option::is_none")]
    pub date_from: Option<String>,
    /// Exclusive UTC day (`YYYY-MM-DD`). Defaults to tomorrow (UTC), so a read
    /// always covers today's partial day.
    #[serde(rename = "date_to", default, skip_serializing_if = "Option::is_none")]
    pub date_to: Option<String>,
    /// Restrict to one dimension; absent means every dimension.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimension: Option<String>,
    /// Size of the per-app breakdown. Absent means the default bound.
    #[serde(
        rename = "top_apps",
        default,
        deserialize_with = "sdkwork_utils_rust::http_api::deserialize_option_query_i32"
    )]
    pub top_apps: Option<i32>,
}

/// Read model port for aggregated traffic usage.
///
/// Implemented by the host over the repository that owns the usage facts; the
/// runtime dependency is inverted here so the service does not have to know
/// which module persists them.
#[async_trait]
pub trait TrafficUsageReadPort: Send + Sync {
    /// `tenant_id: None` reads every tenant. The caller (not the client)
    /// decides which one applies.
    async fn retrieve_traffic_usage_statistics(
        &self,
        tenant_id: Option<i64>,
        query: &TrafficUsageStatisticsQuery,
    ) -> WebServiceResult<TrafficUsageStatisticsResponse>;
}
