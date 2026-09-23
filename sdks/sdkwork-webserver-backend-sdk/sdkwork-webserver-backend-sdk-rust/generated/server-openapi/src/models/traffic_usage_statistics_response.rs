use serde::{Deserialize, Serialize};

use crate::models::{TrafficUsageAppTotal, TrafficUsageDailyPoint, TrafficUsageTenantTotal, TrafficUsageTotal};

/// Aggregated traffic usage over a **half-open** date window (`dateFrom <= day < dateTo`). Every view is derived from the same append-only traffic facts, so the totals, the daily series, and the per-app breakdown are consistent with each other by construction rather than by a reconciliation job having run most recently.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TrafficUsageStatisticsResponse {
    #[serde(rename = "dateFrom")]
    pub date_from: String,

    #[serde(rename = "dateTo")]
    pub date_to: String,

    /// Whether the figures cover every tenant rather than the caller's own. Reported so a surface cannot render a platform-wide number as if it were the caller's own, or the reverse.
    #[serde(rename = "platformScope")]
    pub platform_scope: bool,

    pub totals: Vec<TrafficUsageTotal>,

    pub daily: Vec<TrafficUsageDailyPoint>,

    pub apps: Vec<TrafficUsageAppTotal>,

    /// Per-tenant breakdown. Empty for a tenant-scoped read, where the answer would be the caller's own totals repeated once per dimension.
    pub tenants: Vec<TrafficUsageTenantTotal>,
}
