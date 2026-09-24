//! The dashboard's cardinal metric row, read as one summary.
//!
//! Three groups of figures, four reporting windows each:
//!
//! - **entity counts** — how many users, tenants, applications, and agents
//!   exist. The window names an *arrival* window, so `today` / `last_7_days` /
//!   `current_month` are the entities created inside it and `lifetime` is the
//!   live population. That is the reading a scale metric is asked for: how big
//!   is the estate, and how fast is it growing.
//! - **traffic** — the metered volumes the same windows accumulated. Here
//!   `lifetime` is not a population but everything the metering facts cover,
//!   which is why [`MetricsSummaryResponse::traffic_since`] reports the day the
//!   facts start on: a reader must be able to say *since when*.
//! - **storage** — the disk the tenants' Drive objects occupy. It reads like
//!   the entity group rather than the traffic group: a narrow window is the
//!   part of that occupancy *added* inside it and `lifetime` is the occupancy
//!   standing now. What makes it its own group rather than a third entity is
//!   that its figures are **held** rather than counted — an object that is
//!   deleted stops being part of every window, including the one it was born
//!   in, so `lifetime` is not the sum of what ever arrived and must never be
//!   labelled "累计".
//!
//! ## A source that a deployment may not assemble
//!
//! Three of the four owners are composed in every topology profile this
//! repository ships (the IAM subjects and the metered facts) or travel with the
//! edge as a declared sidecar (Drive). The agents catalog does **not**: no
//! profile declares `SDKWORK_AGENTS_APP_ROOT`, and the shared database may
//! therefore hold no `ai_agent` table at all.
//!
//! Requiring it would be worse than omitting it: the boot probe is
//! all-or-nothing, so a missing `ai_agent` would take the *whole* row down —
//! users, traffic, and storage included — on every deployment that does not run
//! the agents module. So a metric whose source this deployment does not
//! assemble is **named** in [`MetricsSummaryResponse::unassembled_metrics`]
//! instead of being answered as `0`. "This deployment cannot count agents" and
//! "there are no agents" are opposite claims about the system, and only one of
//! them is a figure; the surface draws them differently for the same reason the
//! absent reading is a `503` rather than a row of zeros.
//!
//! **Counts are aggregates, not records.** Nothing here returns a user, a
//! tenant, an application, or an agent — only how many there are. That is what
//! keeps the reading inside the usage/statistics capability instead of becoming
//! a second, quieter way to enumerate another module's subjects.
//!
//! **Reach is an operation, not a parameter** (same rule as the traffic
//! readings): the own-tenant operation answers the caller's tenant, the platform
//! operation answers every tenant and is restricted to the operator tenant. The
//! platform reading additionally reports `tenants`, which the own-tenant
//! reading omits outright rather than reporting at `1` — a tenant count that is
//! structurally always one is not a metric, and shipping it invites a surface to
//! draw it.
//!
//! ## The daily series, and why it takes a window the cards do not
//!
//! The same reading also reports a **per-day** shape ([`MetricsSeries`]): the
//! estate's arrivals, one point per day, so the trend chart can switch between
//! the metered volumes and how fast the estate is growing.
//!
//! That series takes a window — [`MetricsSummaryQuery::date_from`] and
//! `date_to` — and the four **card** windows deliberately do not. The two are
//! different questions and are cut against different calendars:
//!
//! - The cards answer "how big is the estate, and how much arrived today / this
//!   week / this month / ever". Those periods are properties of the product, so
//!   the server resolves them from its own clock and a caller cannot retarget
//!   them; see `MetricsWindowRequest`.
//! - The series answers "what does the shape of the last *n* days look like",
//!   and it is cut against **the caller's** days. That is not a preference: the
//!   plot it feeds also draws the traffic reading's own per-day series
//!   ([`crate::TrafficUsageStatisticsQuery`]), whose x domain is *that*
//!   reading's resolved window, so the two have to be the same days.
//!
//! The series therefore accepts exactly the traffic reading's window shape —
//! same two parameter names, same default, same bound — because **one plot has
//! one x domain**: if the two readings resolved different windows, the entity
//! series would be plotted against a domain its days were not cut against, and
//! the axis labels would be a claim about the data that the data does not
//! support. A caller that draws both must pass **one pair of bounds to both
//! operations** rather than omitting them, because each operation resolves an
//! omitted bound from the clock on its own and two defaults are two windows.
//! The response states the window it used
//! ([`MetricsSummaryResponse::series_window`]) for the same reason the traffic
//! reading does: a surface must state the basis it drew, not just name it.
//!
//! Two consequences are deliberate:
//!
//! - **Summing a series reaches the window's own total.** When the requested
//!   window coincides with one of the four card windows, the series must sum to
//!   that card's figure — both are cut against server-resolved UTC days, so
//!   they are two spellings of one count rather than two counts that happen to
//!   agree.
//! - **Only the entity metrics get a series here.** The metered volumes already
//!   have their own per-day shape in the traffic reading, and putting a second
//!   one here would give one chart two sources for one line. Storage has none
//!   on purpose: a day's occupancy is not reconstructable, because a deleted
//!   object leaves the catalog and its absence is indistinguishable from never
//!   having existed — a history this reading would have to invent.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::problem::WebServiceResult;

/// Window id: the current UTC day.
pub const METRICS_WINDOW_TODAY: &str = "today";
/// Window id: the current UTC day and the six before it.
pub const METRICS_WINDOW_LAST_7_DAYS: &str = "last_7_days";
/// Window id: the current UTC calendar month to date.
pub const METRICS_WINDOW_CURRENT_MONTH: &str = "current_month";
/// Window id: everything the figures cover.
pub const METRICS_WINDOW_LIFETIME: &str = "lifetime";

/// The windows every metric reports, in the order a card reads them:
/// narrowest first, the all-time figure last.
pub const METRICS_WINDOWS: [&str; 4] = [
    METRICS_WINDOW_TODAY,
    METRICS_WINDOW_LAST_7_DAYS,
    METRICS_WINDOW_CURRENT_MONTH,
    METRICS_WINDOW_LIFETIME,
];

/// Metric id: registered users.
pub const METRICS_ENTITY_USERS: &str = "users";
/// Metric id: tenants. Reported only by the platform reading.
pub const METRICS_ENTITY_TENANTS: &str = "tenants";
/// Metric id: applications.
pub const METRICS_ENTITY_APPLICATIONS: &str = "applications";
/// Metric id: agents, from the agents module's own `ai_agent` catalog.
///
/// Unlike the other three entity metrics this one's source is **not** assembled
/// by every deployment; see the module docs. A response that could not read it
/// names it in [`MetricsSummaryResponse::unassembled_metrics`] rather than
/// reporting `0`.
pub const METRICS_ENTITY_AGENTS: &str = "agents";

/// Unit of every entity metric: a count of rows, not a metered quantity.
pub const METRICS_UNIT_COUNT: &str = "COUNT";

/// Unit of the storage volume metric: whole bytes.
///
/// The same unit the metering plane publishes for its two byte dimensions, so
/// a surface formats both through one code path.
pub const METRICS_UNIT_BYTE: &str = "BYTE";

/// Metric id: bytes the tenants' Drive objects hold right now.
pub const METRICS_STORAGE_USED_BYTES: &str = "storage.used_bytes";

/// Metric id: the Drive objects holding them.
pub const METRICS_STORAGE_OBJECT_COUNT: &str = "storage.object_count";

/// One window's figure for one metric.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsWindowValue {
    /// One of [`METRICS_WINDOWS`]. Left as a string rather than an enum so a
    /// window added to the vocabulary reaches a surface that does not know it
    /// yet as its own row, instead of failing the whole response to deserialize.
    pub window: String,
    /// The figure. A **string on the wire** (API_SPEC §13.6): an `int64`
    /// quantity and JavaScript loses precision above 2^53.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub quantity: i64,
    /// Unit of `quantity` (`COUNT`, `REQUEST`, `BYTE`).
    pub unit: String,
}

/// One metric across every window.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsMetricTotals {
    /// See `METRICS_ENTITY_*` for the entity metrics; the traffic metrics carry
    /// the metering vocabulary (`traffic.requests`, `traffic.ingress_bytes`,
    /// `traffic.egress_bytes`) and are left open for the same reason the
    /// traffic readings leave theirs open.
    pub metric: String,
    pub unit: String,
    /// One entry per window in [`METRICS_WINDOWS`]. Reported in full rather
    /// than sparsely: a window that summed nothing is a real `0`, and a missing
    /// entry would make a surface guess between "zero" and "not measured".
    pub values: Vec<MetricsWindowValue>,
}

/// The concrete day bounds behind the window ids.
///
/// Reported so a surface can state the basis it is drawing instead of only
/// naming it: "近 7 日" is a label, `2026-09-18 → 2026-09-25` is a claim an
/// operator can check.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsWindowBounds {
    /// One of [`METRICS_WINDOWS`].
    pub window: String,
    /// Inclusive UTC day (`YYYY-MM-DD`). `None` for the lifetime window, whose
    /// lower bound is "the beginning of the figures" and is reported by
    /// [`MetricsSummaryResponse::traffic_since`] instead of invented here.
    #[serde(rename = "dateFrom", default, skip_serializing_if = "Option::is_none")]
    pub date_from: Option<String>,
    /// Exclusive UTC day (`YYYY-MM-DD`).
    #[serde(rename = "dateTo")]
    pub date_to: String,
}

/// One dashboard reading: the entity counts and the metered volumes, each
/// across every window.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSummaryResponse {
    /// The UTC day the windows were resolved against (`YYYY-MM-DD`).
    ///
    /// Reported because the windows are relative to *a* clock and the caller's
    /// is not necessarily it: a surface that labels "今天" from its own date
    /// while the figures were cut against the server's would name a window the
    /// numbers do not cover.
    #[serde(rename = "asOf")]
    pub as_of: String,
    /// Whether the figures cover every tenant rather than the caller's own.
    ///
    /// Same reason as the traffic readings: an own-tenant count and a
    /// platform-wide count are indistinguishable from the numbers alone, and
    /// the difference is a privacy boundary.
    #[serde(rename = "platformScope")]
    pub platform_scope: bool,
    /// Earliest UTC day the metered traffic facts cover (`YYYY-MM-DD`), or
    /// `None` when this deployment has none. This is the basis of the lifetime
    /// traffic figures; without it, "累计流量" would imply the life of the
    /// product rather than the life of the fact table.
    #[serde(rename = "trafficSince", default, skip_serializing_if = "Option::is_none")]
    pub traffic_since: Option<String>,
    pub windows: Vec<MetricsWindowBounds>,
    /// The entity metrics. The platform reading reports users, tenants, and
    /// applications; the own-tenant reading reports users and applications,
    /// because a tenant counting itself is always one.
    pub entities: Vec<MetricsMetricTotals>,
    /// The metered traffic metrics, one row per dimension the facts carry.
    pub traffic: Vec<MetricsMetricTotals>,
    /// The occupied-storage metrics ([`METRICS_STORAGE_USED_BYTES`],
    /// [`METRICS_STORAGE_OBJECT_COUNT`]).
    ///
    /// Reported by **both** reaches, unlike the tenant count: how much space a
    /// tenant holds is a real figure for the tenant itself, and the own-tenant
    /// reading is what lets a tenant see its own consumption without asking for
    /// a platform-wide answer it is not authorized to have.
    pub storage: Vec<MetricsMetricTotals>,
    /// The per-day entity arrivals over [`Self::series_window`].
    ///
    /// Separate from the cards above rather than a fifth window on them: the
    /// cards' windows are the server's, this one's is the caller's, and folding
    /// them together would give one row two meanings for "the window".
    pub series: Vec<MetricsSeries>,
    /// The window [`Self::series`] was cut against, as the server resolved it.
    ///
    /// Reported because the caller's request is a pair of *optional* bounds: a
    /// surface that labelled the series from its own guess at the default would
    /// name a period the points were not cut against. The cards' bounds are
    /// reported separately in [`Self::windows`], and the two are deliberately
    /// not merged — they answer different questions and may not coincide.
    #[serde(rename = "seriesWindow")]
    pub series_window: MetricsSeriesWindow,
    /// Metric ids this deployment **could not read**, in contract vocabulary.
    ///
    /// Not metrics that came back empty: a metric answered `0` is reported as
    /// the figure `0`. This is the other case — the source is not assembled at
    /// all, so no figure exists and none was invented. A surface must draw
    /// these as a missing capability rather than as a zero, which is the same
    /// distinction the absent reading (`503`) draws at the level of the whole
    /// row; see the module docs.
    #[serde(rename = "unassembledMetrics")]
    pub unassembled_metrics: Vec<String>,
}

/// Filters for a dashboard metric summary read.
///
/// Wire vocabulary is `lower_snake_case` on query strings (PAGINATION_SPEC §3),
/// and the two names are the traffic reading's own, because the two readings may
/// be drawn on one chart: one plot has one x domain, so both must accept the
/// same window under the same spelling.
///
/// **These bounds apply to [`MetricsSummaryResponse::series`] only.** The four
/// card windows are resolved server-side from the clock and are unaffected by
/// anything here; see the module docs for why the two are kept apart.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricsSummaryQuery {
    /// Inclusive UTC day (`YYYY-MM-DD`). Defaults to the trailing 30 days.
    #[serde(rename = "date_from", default, skip_serializing_if = "Option::is_none")]
    pub date_from: Option<String>,
    /// Exclusive UTC day (`YYYY-MM-DD`). Defaults to tomorrow (UTC), so a read
    /// always covers today's partial day.
    #[serde(rename = "date_to", default, skip_serializing_if = "Option::is_none")]
    pub date_to: Option<String>,
}

/// A **resolved** series window handed to [`MetricsSummaryReadPort`].
///
/// Same reasoning as [`MetricsWindowRequest`]: by the time the read model is
/// asked, the caller's optional bounds have exactly one meaning, so the port is
/// never the place where "no window given" gets interpreted — an
/// interpretation that would silently differ between implementations while all
/// of them look like a working chart.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSeriesWindow {
    /// Inclusive UTC day (`YYYY-MM-DD`).
    #[serde(rename = "dateFrom")]
    pub date_from: String,
    /// **Exclusive** UTC day (`YYYY-MM-DD`).
    #[serde(rename = "dateTo")]
    pub date_to: String,
}

/// One entity metric's per-day arrivals.
///
/// Rows are sparse — a day the metric gained nothing may carry no point at all,
/// because the read model groups by day and a `GROUP BY` cannot report a day it
/// never saw. [`MetricsSummaryResponse::series_window`] is the authority on
/// which days the chart covers, and a day inside it with no point is a real
/// `0`: the alternative, emitting an explicit zero for every day, would make
/// the response's size a function of the window rather than of the data.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSeries {
    /// One of the `METRICS_ENTITY_*` ids. Left open as a string for the same
    /// reason [`MetricsMetricTotals::metric`] is.
    pub metric: String,
    /// Unit of every point (`COUNT`).
    pub unit: String,
    /// Ascending by day. Ordered here rather than left to the surface: a chart
    /// drawn from a map's iteration order is a chart whose shape is a property
    /// of the serialization.
    pub points: Vec<MetricsSeriesPoint>,
}

/// One day of one entity metric's arrivals.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSeriesPoint {
    /// Calendar day in UTC (`YYYY-MM-DD`).
    pub date: String,
    /// The figure. A **string on the wire** (API_SPEC §13.6), for the same
    /// reason every other quantity here is.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub quantity: i64,
}

/// A **resolved** set of window bounds handed to [`MetricsSummaryReadPort`].
///
/// Same reasoning as [`crate::TrafficUsageWindow`]: by the time the read model
/// is asked, "what does this window mean" has exactly one answer, so the port
/// is never the place where the clock and the calendar get interpreted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsWindowRequest {
    /// The UTC day the windows were resolved against (`YYYY-MM-DD`). Also the
    /// inclusive lower bound of the `today` window.
    pub as_of: String,
    /// Inclusive lower bound of the `last_7_days` window.
    #[serde(rename = "lastSevenDaysFrom")]
    pub last_seven_days_from: String,
    /// Inclusive lower bound of the `current_month` window.
    #[serde(rename = "currentMonthFrom")]
    pub current_month_from: String,
    /// **Exclusive** upper bound shared by the three bounded windows.
    #[serde(rename = "endsBefore")]
    pub ends_before: String,
}

/// What the facts owner can answer: the figures, with no wire shape of their own.
///
/// Deliberately not [`MetricsSummaryResponse`]: `platformScope` and the
/// presence of the tenant metric are decided by the *authorization* the service
/// resolved, not by the query that counted the rows, so the layer that made
/// that decision is the layer that states it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSummaryReadings {
    /// The entity metrics, including tenants when the read covered every
    /// tenant. The service strips tenants from a tenant-scoped reading.
    pub entities: Vec<MetricsMetricTotals>,
    pub traffic: Vec<MetricsMetricTotals>,
    /// The occupied-storage metrics. Rows rather than a fixed pair, so the
    /// vocabulary can grow with the storage plane without this contract
    /// version having to name the next dimension.
    pub storage: Vec<MetricsMetricTotals>,
    /// The per-day entity arrivals over the requested series window.
    pub series: Vec<MetricsSeries>,
    /// Metric ids the read model could not read because their source is not
    /// assembled in this deployment. Decided by the layer that resolved the
    /// sources, which is why it travels with the figures rather than being
    /// inferred from them: `agents` absent from [`Self::entities`] means one
    /// of two opposite things, and only the probe knows which.
    pub unassembled_metrics: Vec<String>,
    #[serde(rename = "trafficSince", default, skip_serializing_if = "Option::is_none")]
    pub traffic_since: Option<String>,
}

/// Read model port for the dashboard metric summary.
///
/// Implemented by the host. Members of this reading live outside this
/// service's own database — the IAM subjects, the metered facts, and the agents
/// catalog — so the service declares the port instead of depending on any of
/// those modules' persistence, exactly as it does for the traffic readings.
#[async_trait]
pub trait MetricsSummaryReadPort: Send + Sync {
    /// `tenant_id: None` reads every tenant. The caller (not the client)
    /// decides which one applies.
    ///
    /// Two windows, because the reading reports two different periods: the
    /// cards' four (`windows`) and the series' one (`series_window`).
    async fn retrieve_metrics_summary(
        &self,
        tenant_id: Option<i64>,
        windows: &MetricsWindowRequest,
        series_window: &MetricsSeriesWindow,
    ) -> WebServiceResult<MetricsSummaryReadings>;
}

/// Span of the `last_7_days` window, in days **including** its first day.
pub const METRICS_LAST_SEVEN_DAYS_SPAN: i64 = 7;
