//! Dashboard metric summary read model.
//!
//! One reading spans four owners, and this module is the only place that knows
//! all four: the IAM subjects (`iam_user`, `iam_tenant`), the metered usage
//! facts (`deploy_usage_event`), and the Drive object catalog
//! (`dr_drive_storage_object`) belong to sibling modules, while the
//! application count is this repository's own table. The service declares
//! [`MetricsSummaryReadPort`] so it never has to depend on any of them; the
//! assembly is the crate allowed to hold a pool and mount dependencies
//! (`RUST_CODE_SPEC` §1.1, `api-assembly` row).
//!
//! **Read, never assume.** Availability is established at boot by resolving
//! every table the reading needs. A host that composes no IAM tables, or no
//! usage facts, must report the capability as **not assembled** — the same
//! `503` an absent port produces — instead of answering a row of zeros. Zero
//! users and "this deployment cannot count users" are the same picture and
//! opposite claims, so the second one never gets to be drawn as the first.
//!
//! **Two Kinds Of Absence, And They Must Not Be Confused.** The paragraph above
//! is about a source the reading *needs*. One source is different: the agents
//! catalog is not assembled by every topology profile, so its absence is a
//! normal state of a working deployment rather than an unfinished one. It is
//! resolved as an **optional** source ([`METRICS_OPTIONAL_SOURCES`]): when it is
//! missing, the whole row still answers and the metric it would have answered is
//! named in `unassembledMetrics`. Requiring it would invert the failure —
//! demanding `ai_agent` would take users, traffic, and storage down with it on
//! every deployment that does not run the agents module, which is strictly
//! worse than the figure being absent.
//!
//! **Counts, never rows.** Every figure below is an aggregate. Nothing here
//! selects a user, a tenant, an application, or an agent; the reading can say
//! how large the estate is without becoming a second way to enumerate another
//! module's subjects.
//!
//! ## Every window bound is anchored to UTC
//!
//! The three window ids name **UTC** days (see `MetricsWindowBounds`), and the
//! service resolves them from `Utc::now()`. A bound therefore cannot be handed
//! to PostgreSQL as a bare date and left to the connection to interpret: this
//! server's session `TimeZone` is `Asia/Shanghai`, so `$n::timestamptz` lands
//! eight hours before the day it names. Every bound here is written
//! `($n::date)::timestamp AT TIME ZONE 'UTC'` instead — the formulation the
//! facts owner already uses in
//! `sdkwork-deployments/.../sdkwork-intelligence-deploy-repository-sqlx/src/usage.rs`.
//!
//! This is not only a boundary preference. `webserver_application.created_at`
//! is a real `TIMESTAMPTZ` while the two IAM `created_at` columns are `TEXT`,
//! and a bare date bound against the former has **no operator at all**: the
//! comparison cannot be planned and the whole reading dies as a `500` before
//! any figure is computed.
//!
//! ## The daily series derives its day key the same way
//!
//! The per-day queries group by the **UTC** calendar day, which is the same
//! property the window bounds are anchored on, and it is derived per column
//! type for the same reason:
//!
//! - A `timestamptz` column is rendered `(created_at AT TIME ZONE 'UTC')::date
//!   ::text` — the spelling [`TRAFFIC_SINCE_SQL`] already uses to render a day
//!   back to the caller. Without the `AT TIME ZONE 'UTC'` the day would be cut
//!   in the session's timezone and a point labelled `2026-09-01` would hold
//!   part of the 2nd.
//! - The two `TEXT` columns are truncated with `substr(created_at, 1, 10)`,
//!   which is exact for the reason the lexical comparison is: the values are
//!   `YYYY-MM-DD HH:MM:SS…+00`, so the first ten characters *are* the UTC day.
//!
//! Rows are sparse. A `GROUP BY` cannot report a day it saw no rows for, so a
//! day inside the series window with no point is a real `0` that the response
//! does not spell out — the window's bounds are reported instead, and emitting
//! an explicit zero for every day would make the response's size a function of
//! the window rather than of the data.
//!
//! ## Why the aggregates are written here rather than reused
//!
//! The traffic figures go against the same fact table the traffic statistics
//! read model reads, but they are **not** routed through the Deploy control
//! plane's repository. That repository has exactly one usage entry point and it
//! answers a full drill-down: a per-day series, a per-app ranking, and a
//! per-tenant breakdown. A four-window summary needs none of them, and asking
//! for them would make one dashboard tile pay for three unbounded groupings —
//! the per-day series over the lifetime window in particular. One conditional
//! aggregate over the indexed `(tenant_id, period_start DESC)` order is the
//! whole query.
//!
//! ## Three deliberate predicate assumptions
//!
//! All three were checked against the live development database rather than
//! guessed, and all three are recorded here because a silent undercount is the
//! failure mode:
//!
//! - **Users** are counted as `status = 'active' AND is_deleted = 0`. This is
//!   the same liveness rule the standalone gateway's operator-password reset
//!   already applies to `iam_user`, so the two features cannot disagree about
//!   which users exist.
//! - **Tenants** are counted as `status = 'active'`. `iam_tenant` carries no
//!   soft-delete column at all, so the only liveness notion there is `status`,
//!   and `'active'` is the only value this repository has ever observed written
//!   to it. Should a second value appear, this is a one-line change — and it is
//!   the *only* place a tenant count is computed.
//!
//! ## The storage predicate
//!
//! Occupancy is `lifecycle_status = 'active'`, and the two widest storage
//! figures are Drive's own quota figures rather than a parallel computation of
//! them. Drive's `summarize_tenant_quota` — the port method its pre-upload check
//! calls (`sdkwork-drive/crates/sdkwork-drive-workspace-service/src/application/
//! quota_enforcement.rs`) and its quota display reads (`.../application/
//! quota_service.rs`) — is implemented in `.../src/infrastructure/sql/
//! quota_store.rs` as:
//!
//! ```sql
//! SELECT CAST(COALESCE(SUM(content_length), 0) AS BIGINT) AS total_bytes,
//!        CAST(COUNT(1) AS BIGINT) AS object_count
//! FROM dr_drive_storage_object
//! WHERE tenant_id=$1 AND lifecycle_status='active'
//! ```
//!
//! `lifetime_bytes` and `lifetime_objects` below are that same statement's two
//! projections over the same predicate, so the figure this dashboard reports
//! and the figure Drive refuses an upload against cannot disagree. Three
//! consequences are deliberate rather than tolerated:
//!
//! - **Only the two widest figures are unfiltered.** The narrow windows add a
//!   `FILTER`, and the widest pair is left exactly as Drive writes it — which is
//!   what makes the equality a property of the statement rather than of the
//!   data happening to line up. A `FILTER` added to the lifetime projections
//!   would silently end that.
//! - **The same predicate bounds all four windows.** Drive soft-deletes an
//!   object (`lifecycle_status = 'deleted'`) and a maintenance sweep later
//!   removes the row outright, so a query that counted deleted rows for the
//!   narrow windows would have its figures silently shrink the first time the
//!   sweep ran — an unrelated maintenance action changing a reported number.
//!   Filtering on the status in every window makes the sweep a no-op here.
//! - **An object in the recycle bin still counts, because it still counts for
//!   Drive.** A trashed node's object is deliberately left `active` — "restoring
//!   this node must still find its content" (`sdkwork-drive-install-worker/src/
//!   maintenance/quota_recalculation.rs`) — so both this figure and the quota
//!   include it. That is the correct behaviour for an occupancy reading even
//!   though it surprises: the space really is still held.
//! - **`lifetime` is occupancy, not an arrival total.** Because an object stops
//!   being counted in every window once it is deleted, `lifetime` is what is
//!   held *now* rather than everything that ever arrived, and a narrow window is
//!   the part of that holding added inside it. The two are not additive, so the
//!   surface must not label the widest window "累计".
//!
//! ## Why every timestamp comparison is written out
//!
//! The windows are resolved against the **UTC** clock
//! (`resolve_metrics_windows`), but the three date columns this reading touches
//! do not share a type, and neither of the two obvious spellings is right for
//! all of them:
//!
//! - `iam_user.created_at` and `iam_tenant.created_at` are **`text`** holding
//!   UTC instants (`2026-09-23 14:47:20.234514+00`). Comparing them to a
//!   `YYYY-MM-DD` parameter is a string comparison, which is already exact and
//!   already UTC — so those two queries cast nothing.
//! - `webserver_application.created_at`, `deploy_usage_event.period_start`,
//!   `dr_drive_storage_object.created_at`, and `ai_agent.created_at` are
//!   **`timestamp with time zone`**. sqlx declares every `&str` bind as `text`
//!   in its `Parse`, so a bare `created_at >= $1` is not a comparison
//!   PostgreSQL can plan at all: it fails with
//!   `operator does not exist: timestamp with time zone >= text` before a
//!   single row is read. Both sides therefore have to be spelled —
//!   `($1::date)::timestamp AT TIME ZONE 'UTC'`.
//!
//! The `AT TIME ZONE 'UTC'` is the load-bearing half. `($1::date)::timestamptz`
//! also type-checks, but it anchors the day at midnight in the **session's**
//! timezone — `Asia/Shanghai` on the development database — so a window labelled
//! as a UTC day would be cut eight hours late and quietly include part of
//! another day. Both halves were verified against the live database rather than
//! assumed: `pg_prepared_statements` reports `{text,…}` for every shape here,
//! and the UTC form resolves `2026-09-01` to midnight UTC (`08:00:00+08` on this
//! host) where the session-anchored form resolves it to `00:00:00+08`.
//! - **The two `TEXT` `created_at` columns are compared lexically against a
//!   bare `YYYY-MM-DD`**, which is only equivalent to a date window while every
//!   value is rendered in UTC with a fixed shape. Live check: every row of
//!   `iam_user` and `iam_tenant` matches
//!   `^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}(\.\d+)?\+00$`, a single distinct
//!   suffix. The owning module compares the same column the same way in its own
//!   keyset pagination, so this is its convention rather than a shortcut taken
//!   here. A local-time offset would break the lexical order, and that is the
//!   reason the applications figure — whose column is a real `TIMESTAMPTZ` —
//!   does not share the spelling.

use std::sync::Arc;

use async_trait::async_trait;
use sdkwork_webserver_contract::{
    MetricsMetricTotals, MetricsSeries, MetricsSeriesPoint, MetricsSeriesWindow,
    MetricsSummaryReadPort, MetricsSummaryReadings, MetricsWindowRequest, MetricsWindowValue,
    WebServiceError, WebServiceResult, METRICS_ENTITY_AGENTS, METRICS_ENTITY_APPLICATIONS,
    METRICS_ENTITY_TENANTS, METRICS_ENTITY_USERS, METRICS_STORAGE_OBJECT_COUNT,
    METRICS_STORAGE_USED_BYTES, METRICS_UNIT_BYTE, METRICS_UNIT_COUNT, METRICS_WINDOWS,
    METRICS_WINDOW_CURRENT_MONTH, METRICS_WINDOW_LAST_7_DAYS, METRICS_WINDOW_LIFETIME,
    METRICS_WINDOW_TODAY,
};
use sqlx::{PgPool, Row};

/// Every table the reading **requires**.
///
/// Resolved together at boot so the capability has one answer: partially
/// assembled is not a state this read model can express, and reporting half a
/// dashboard as if it were whole is the failure this list exists to prevent.
///
/// `dr_drive_storage_object` is on the list on the same terms as the IAM and
/// usage tables. Drive is a declared sidecar of every topology profile
/// (`SDKWORK_DRIVE_APP_ROOT` in `etc/topology/*.env`), so a deployment that
/// resolves this reading has the table; one that does not must answer `503`
/// rather than draw an occupancy of zero for a storage plane it cannot see.
const METRICS_SUMMARY_TABLES: [&str; 5] = [
    "iam_user",
    "iam_tenant",
    "webserver_application",
    "deploy_usage_event",
    "dr_drive_storage_object",
];

/// A source the reading can do **without**, and the metric it answers.
///
/// The difference from the list above is not how important the figures are but
/// what is knowable when the source is absent:
///
/// - **The agents catalog is not assembled by every topology.** No profile
///   declares `SDKWORK_AGENTS_APP_ROOT` (`etc/topology/*.env`), and production
///   is a single shared database with the IAM and Drive modules installed, so
///   `ai_agent` may legitimately hold no table at all. The boot probe is
///   all-or-nothing, so listing it as required would take the *whole* row down
///   — users, traffic, and storage included — on every deployment that does not
///   run the agents module. The metric is named in `unassembledMetrics`
///   instead, which draws the one thing that is true: this deployment cannot
///   count agents.
/// - **Everything else is composed together.** A deployment missing one of the
///   five above is not a smaller dashboard but an unfinished one, and it says
///   so once, loudly, as a `503`.
///
/// The pair travels with the table so the two cannot drift: the probe resolves
/// tables, the response names metrics, and this is the only place that knows
/// which table answers which metric.
struct OptionalMetricSource {
    table: &'static str,
    metric: &'static str,
}

const METRICS_OPTIONAL_SOURCES: [OptionalMetricSource; 1] = [OptionalMetricSource {
    table: "ai_agent",
    metric: METRICS_ENTITY_AGENTS,
}];

/// The usage fact table, named only for the documentation above.
///
/// A constant rather than a literal because it is also the field this module
/// logs when the probe finds a required table missing, so the log says which
/// fact table the reading went without.
const TRAFFIC_USAGE_FACT_TABLE: &str = "deploy_usage_event";

/// Four window figures per metric, keyed by the contract's window ids.
type WindowFigures = [(&'static str, i64); 4];

/// [`MetricsSummaryReadPort`] over the process-shared database.
pub(crate) struct SharedDatabaseMetricsSummaryReadPort {
    pool: PgPool,
    /// Metric ids whose **optional** source was absent when this reader was
    /// resolved, in contract vocabulary.
    ///
    /// Carried on the reader rather than recomputed per request: it is a
    /// property of what this deployment assembled, established once at boot, and
    /// re-probing it per request would both cost a round trip and let one
    /// response disagree with the next about what the deployment contains.
    unassembled: Vec<String>,
}

impl SharedDatabaseMetricsSummaryReadPort {
    async fn entity_metric(
        &self,
        metric: &'static str,
        sql: &'static str,
        tenant: TenantFilter,
        windows: &MetricsWindowRequest,
    ) -> WebServiceResult<MetricsMetricTotals> {
        let query = sqlx::query(sql)
            // Bind order matches the `$n` numbering of every entity query: the
            // three lower bounds ascending, then the tenant filter.
            .bind(&windows.current_month_from)
            .bind(&windows.last_seven_days_from)
            .bind(&windows.as_of);
        let query = match tenant {
            TenantFilter::Text(tenant) => query.bind(tenant),
            TenantFilter::Id(tenant) => query.bind(tenant),
        };
        let row = query
            .fetch_one(&self.pool)
            .await
            .map_err(|error| read_error(metric, error))?;
        Ok(MetricsMetricTotals {
            metric: metric.to_owned(),
            unit: METRICS_UNIT_COUNT.to_owned(),
            values: window_values(
                [
                    (
                        METRICS_WINDOW_TODAY,
                        quantity_column(&row, "today_quantity", metric)?,
                    ),
                    (
                        METRICS_WINDOW_LAST_7_DAYS,
                        quantity_column(&row, "last_seven_days_quantity", metric)?,
                    ),
                    (
                        METRICS_WINDOW_CURRENT_MONTH,
                        quantity_column(&row, "current_month_quantity", metric)?,
                    ),
                    (
                        METRICS_WINDOW_LIFETIME,
                        quantity_column(&row, "lifetime_quantity", metric)?,
                    ),
                ],
                METRICS_UNIT_COUNT,
            ),
        })
    }

    /// The per-day arrivals of one entity metric over the series window.
    ///
    /// Separate statements rather than one `UNION` over the three catalogs:
    /// they key their tenant with two different types and derive their day key
    /// two different ways, so a union would have to cast every column into the
    /// common shape — putting a cast on the *indexed* side of each comparison
    /// and turning three scoped counts into three sequential scans.
    ///
    /// A day with no rows is simply absent from the result; see the module docs
    /// for why the response states the window instead of padding the series.
    async fn entity_series(
        &self,
        metric: &'static str,
        sql: &'static str,
        tenant: TenantFilter,
        window: &MetricsSeriesWindow,
    ) -> WebServiceResult<MetricsSeries> {
        let query = sqlx::query(sql)
            // `$1` inclusive, `$2` exclusive, `$3` the tenant filter — the same
            // shape every series query shares, so the three cannot drift into
            // numbering their placeholders differently.
            .bind(&window.date_from)
            .bind(&window.date_to);
        let query = match tenant {
            TenantFilter::Text(tenant) => query.bind(tenant),
            TenantFilter::Id(tenant) => query.bind(tenant),
        };
        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|error| read_error(metric, error))?;
        let mut points = Vec::with_capacity(rows.len());
        for row in &rows {
            points.push(MetricsSeriesPoint {
                // `day`, not `date`: the column is an expression over
                // `created_at`, and naming the alias after the contract field it
                // fills would read as if the table already had that column.
                date: row
                    .try_get("day")
                    .map_err(|error| read_error(&format!("{metric}.day"), error))?,
                quantity: row
                    .try_get("quantity")
                    .map_err(|error| read_error(&format!("{metric}.quantity"), error))?,
            });
        }
        Ok(MetricsSeries {
            metric: metric.to_owned(),
            unit: METRICS_UNIT_COUNT.to_owned(),
            points,
        })
    }

    /// The occupied-storage metrics: one row per dimension, two dimensions.
    ///
    /// A single statement for both, because they are two projections of one
    /// scan — the object count is not a second read of a second table, and
    /// paying for a second round trip would also let the clock move between the
    /// bytes and the objects holding them.
    ///
    /// An empty result is impossible here (`SUM` over no rows is `NULL`, which
    /// the query coalesces), so the two metrics are always reported: a tenant
    /// holding nothing is a real `0` in both, which the card has to be able to
    /// draw on a deployment whose storage plane is empty.
    async fn storage_metrics(
        &self,
        tenant: TenantFilter,
        windows: &MetricsWindowRequest,
    ) -> WebServiceResult<Vec<MetricsMetricTotals>> {
        let query = sqlx::query(STORAGE_WINDOW_TOTALS_SQL)
            // The same bind order as the entity queries — the three lower
            // bounds ascending, then the tenant filter — so the two code paths
            // cannot drift into numbering their placeholders differently.
            .bind(&windows.current_month_from)
            .bind(&windows.last_seven_days_from)
            .bind(&windows.as_of);
        let query = match tenant {
            TenantFilter::Text(tenant) => query.bind(tenant),
            TenantFilter::Id(tenant) => query.bind(tenant),
        };
        let row = query
            .fetch_one(&self.pool)
            .await
            .map_err(|error| read_error("storage", error))?;
        Ok(vec![
            MetricsMetricTotals {
                metric: METRICS_STORAGE_USED_BYTES.to_owned(),
                unit: METRICS_UNIT_BYTE.to_owned(),
                values: window_values(
                    [
                        (
                            METRICS_WINDOW_TODAY,
                            quantity_column(&row, "today_bytes", METRICS_STORAGE_USED_BYTES)?,
                        ),
                        (
                            METRICS_WINDOW_LAST_7_DAYS,
                            quantity_column(
                                &row,
                                "last_seven_days_bytes",
                                METRICS_STORAGE_USED_BYTES,
                            )?,
                        ),
                        (
                            METRICS_WINDOW_CURRENT_MONTH,
                            quantity_column(
                                &row,
                                "current_month_bytes",
                                METRICS_STORAGE_USED_BYTES,
                            )?,
                        ),
                        (
                            METRICS_WINDOW_LIFETIME,
                            quantity_column(&row, "lifetime_bytes", METRICS_STORAGE_USED_BYTES)?,
                        ),
                    ],
                    METRICS_UNIT_BYTE,
                ),
            },
            MetricsMetricTotals {
                metric: METRICS_STORAGE_OBJECT_COUNT.to_owned(),
                unit: METRICS_UNIT_COUNT.to_owned(),
                values: window_values(
                    [
                        (
                            METRICS_WINDOW_TODAY,
                            quantity_column(&row, "today_objects", METRICS_STORAGE_OBJECT_COUNT)?,
                        ),
                        (
                            METRICS_WINDOW_LAST_7_DAYS,
                            quantity_column(
                                &row,
                                "last_seven_days_objects",
                                METRICS_STORAGE_OBJECT_COUNT,
                            )?,
                        ),
                        (
                            METRICS_WINDOW_CURRENT_MONTH,
                            quantity_column(
                                &row,
                                "current_month_objects",
                                METRICS_STORAGE_OBJECT_COUNT,
                            )?,
                        ),
                        (
                            METRICS_WINDOW_LIFETIME,
                            quantity_column(&row, "lifetime_objects", METRICS_STORAGE_OBJECT_COUNT)?,
                        ),
                    ],
                    METRICS_UNIT_COUNT,
                ),
            },
        ])
    }

    /// The metered volumes, one row per dimension the facts carry.
    ///
    /// An empty result means the facts carry no dimension at all — which the
    /// boot probe has already ruled out as "the table is missing", so it reads
    /// as a real "nothing metered yet" and the surface draws its vocabulary at
    /// zero.
    async fn traffic_metrics(
        &self,
        tenant: Option<i64>,
        windows: &MetricsWindowRequest,
    ) -> WebServiceResult<Vec<MetricsMetricTotals>> {
        let rows = sqlx::query(TRAFFIC_WINDOW_TOTALS_SQL)
            .bind(&windows.as_of)
            .bind(&windows.last_seven_days_from)
            .bind(&windows.current_month_from)
            // Exclusive upper bound shared by all four windows, so "lifetime"
            // is measured as of the same instant as the other three rather than
            // including rows dated in the future.
            .bind(&windows.ends_before)
            .bind(tenant)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| read_error("traffic", error))?;
        rows.iter()
            .map(|row| {
                let metric: String = row
                    .try_get("dimension")
                    .map_err(|error| read_error("traffic.dimension", error))?;
                let unit: String = row
                    .try_get("unit")
                    .map_err(|error| read_error("traffic.unit", error))?;
                Ok(MetricsMetricTotals {
                    values: window_values(
                        [
                            (
                                METRICS_WINDOW_TODAY,
                                quantity_column(row, "today_quantity", &metric)?,
                            ),
                            (
                                METRICS_WINDOW_LAST_7_DAYS,
                                quantity_column(row, "last_seven_days_quantity", &metric)?,
                            ),
                            (
                                METRICS_WINDOW_CURRENT_MONTH,
                                quantity_column(row, "current_month_quantity", &metric)?,
                            ),
                            (
                                METRICS_WINDOW_LIFETIME,
                                quantity_column(row, "lifetime_quantity", &metric)?,
                            ),
                        ],
                        &unit,
                    ),
                    metric,
                    unit,
                })
            })
            .collect()
    }

    /// The first UTC day the metered facts cover, or `None` when there are none.
    ///
    /// This is the basis of the lifetime traffic figures. Without it, "累计"
    /// would read as the life of the product rather than the life of the fact
    /// table, which for a metering plane with retention is a different and much
    /// older claim.
    async fn traffic_since(
        &self,
        tenant: Option<i64>,
        ends_before: &str,
    ) -> WebServiceResult<Option<String>> {
        sqlx::query_scalar::<_, Option<String>>(TRAFFIC_SINCE_SQL)
            .bind(ends_before)
            .bind(tenant)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| read_error("traffic.since", error))
    }
}

#[async_trait]
impl MetricsSummaryReadPort for SharedDatabaseMetricsSummaryReadPort {
    async fn retrieve_metrics_summary(
        &self,
        tenant_id: Option<i64>,
        windows: &MetricsWindowRequest,
        series_window: &MetricsSeriesWindow,
    ) -> WebServiceResult<MetricsSummaryReadings> {
        // The IAM tables key their tenant with a `TEXT` column and this
        // repository's own tables with a `BIGINT` one, so the same scope has to
        // be spelled two ways. Casting one to the other inside the query would
        // put a cast on the *indexed* side of the comparison; spelling both out
        // here keeps each comparison between like types.
        let iam_tenant = TenantFilter::Text(tenant_id.map(|id| id.to_string()));
        let own_tenant = TenantFilter::Id(tenant_id);

        let mut entities = vec![
            self.entity_metric(
                METRICS_ENTITY_USERS,
                ENTITY_USERS_SQL,
                iam_tenant.clone(),
                windows,
            )
            .await?,
            self.entity_metric(
                METRICS_ENTITY_APPLICATIONS,
                ENTITY_APPLICATIONS_SQL,
                own_tenant.clone(),
                windows,
            )
            .await?,
        ];
        let mut series = vec![
            self.entity_series(
                METRICS_ENTITY_USERS,
                ENTITY_USERS_DAILY_SQL,
                iam_tenant.clone(),
                series_window,
            )
            .await?,
            self.entity_series(
                METRICS_ENTITY_APPLICATIONS,
                ENTITY_APPLICATIONS_DAILY_SQL,
                own_tenant.clone(),
                series_window,
            )
            .await?,
        ];
        // The agents catalog is an **optional** source: when this deployment
        // does not assemble it, the metric is named in `unassembled_metrics`
        // and nothing is queried — asking a table the boot probe already
        // reported absent would turn a known state into a runtime error.
        if self.assembles(METRICS_ENTITY_AGENTS) {
            entities.push(
                self.entity_metric(
                    METRICS_ENTITY_AGENTS,
                    ENTITY_AGENTS_SQL,
                    own_tenant.clone(),
                    windows,
                )
                .await?,
            );
            series.push(
                self.entity_series(
                    METRICS_ENTITY_AGENTS,
                    ENTITY_AGENTS_DAILY_SQL,
                    own_tenant,
                    series_window,
                )
                .await?,
            );
        }
        // Tenants are read only for the platform reach. A tenant-scoped read
        // would answer a structural `1`, and the service drops it anyway — so
        // not paying for the query keeps the own-tenant dashboard one statement
        // cheaper without changing what it can report. A tenant has no series
        // either: the point of the trend is this estate's own growth, and a
        // platform-wide tenant series on a tenant-scoped chart would be a
        // cross-tenant figure smuggled into a scoped reading.
        if tenant_id.is_none() {
            entities.push(
                self.entity_metric(
                    METRICS_ENTITY_TENANTS,
                    ENTITY_TENANTS_SQL,
                    TenantFilter::Text(None),
                    windows,
                )
                .await?,
            );
        }
        // Contract order rather than assembly order: the response is read by a
        // surface that tells the metrics apart by position and label, and an
        // order that depended on which reach asked would make the same
        // dashboard rearrange itself between the console and the operations
        // page.
        entities.sort_by_key(|metric| entity_order(&metric.metric));
        series.sort_by_key(|entry| entity_order(&entry.metric));

        let traffic = self.traffic_metrics(tenant_id, windows).await?;
        let traffic_since = if traffic.is_empty() {
            None
        } else {
            self.traffic_since(tenant_id, &windows.ends_before).await?
        };
        // The Drive object catalog keys its tenant with `VARCHAR`, like the IAM
        // tables, so the scope resolved for those columns is the one this read
        // needs — spelled once rather than converted a second time.
        let storage = self.storage_metrics(iam_tenant, windows).await?;
        Ok(MetricsSummaryReadings {
            entities,
            traffic,
            storage,
            series,
            unassembled_metrics: self.unassembled.clone(),
            traffic_since,
        })
    }
}

impl SharedDatabaseMetricsSummaryReadPort {
    /// Whether this deployment's boot probe found the source behind a metric.
    ///
    /// Every metric without an optional source is assembled by construction —
    /// the probe returned `None` otherwise, so no reader exists to be asked —
    /// which is why the check is a membership test rather than a lookup that
    /// could fail.
    fn assembles(&self, metric: &str) -> bool {
        !self.unassembled.iter().any(|missing| missing == metric)
    }
}

/// The contract's order for the entity metrics.
///
/// One function rather than two `sort_by_key` closures: the cards and the series
/// are drawn over the same vocabulary, and two orderings would let a dashboard
/// list users before tenants in its cards and after them in its chart.
fn entity_order(metric: &str) -> u8 {
    match metric {
        METRICS_ENTITY_USERS => 0,
        METRICS_ENTITY_TENANTS => 1,
        METRICS_ENTITY_APPLICATIONS => 2,
        METRICS_ENTITY_AGENTS => 3,
        _ => 4,
    }
}

/// How a query names the tenant it is scoped to.
///
/// Two variants rather than one string that every query casts: the IAM tables
/// key a tenant with `TEXT` and this repository's own tables with `BIGINT`, and
/// casting the *indexed* column to match the parameter would turn each scoped
/// count into a sequential scan.
///
/// `Clone` because more than one query now shares a scope: the Drive object
/// catalog keys its tenant the same way the IAM tables do, so the filter
/// resolved for those columns is reused rather than converted twice.
#[derive(Clone)]
enum TenantFilter {
    /// The IAM tables: `tenant_id` / `id` are `TEXT`.
    Text(Option<String>),
    /// This repository's tables: `tenant_id` is `BIGINT`.
    Id(Option<i64>),
}

/// One entity table's four figures in one statement.
///
/// A conditional aggregate rather than four `COUNT(*)` round trips: the four
/// windows are four predicates over the same rows, and reading them separately
/// both quadruples the round trips and lets the clock move between them.
///
/// `$4` is the tenant filter, `NULL` meaning every tenant. The predicate is
/// written as `$4 IS NULL OR …` so one statement serves both reaches, which is
/// what keeps the platform reading from being a second, divergent query.
const ENTITY_USERS_SQL: &str = "\
SELECT
    COUNT(*)::bigint AS lifetime_quantity,
    COUNT(*) FILTER (WHERE created_at >= $1)::bigint AS current_month_quantity,
    COUNT(*) FILTER (WHERE created_at >= $2)::bigint AS last_seven_days_quantity,
    COUNT(*) FILTER (WHERE created_at >= $3)::bigint AS today_quantity
FROM iam_user
WHERE status = 'active' AND is_deleted = 0
  AND ($4::text IS NULL OR tenant_id = $4::text)";

/// `iam_tenant` has no soft-delete column, so `status` is the only liveness
/// notion it has. It is filtered on `id` rather than a `tenant_id` column —
/// a tenant's own identifier *is* the tenant.
const ENTITY_TENANTS_SQL: &str = "\
SELECT
    COUNT(*)::bigint AS lifetime_quantity,
    COUNT(*) FILTER (WHERE created_at >= $1)::bigint AS current_month_quantity,
    COUNT(*) FILTER (WHERE created_at >= $2)::bigint AS last_seven_days_quantity,
    COUNT(*) FILTER (WHERE created_at >= $3)::bigint AS today_quantity
FROM iam_tenant
WHERE status = 'active'
  AND ($4::text IS NULL OR id = $4::text)";

/// Applications are counted regardless of `status`: draft, paused, and
/// archived are all applications that exist, and a scale metric that silently
/// dropped the archived ones would shrink whenever an operator tidied up.
/// Only a deleted row stops being an application.
///
/// **The window bounds are spelled differently here than in the two IAM
/// queries**, and both halves of the difference are load-bearing:
///
/// - `webserver_application.created_at` is `TIMESTAMPTZ` while `iam_user` and
///   `iam_tenant` keep theirs as `TEXT`. Binding the bare date string against
///   a `TIMESTAMPTZ` does not merely shift the boundary — PostgreSQL has no
///   `timestamptz >= text` operator, so the comparison cannot even be planned
///   and the whole reading fails as a `500`.
/// - `($n::date)::timestamp AT TIME ZONE 'UTC'` rather than `$n::timestamptz`:
///   a bare date cast to `timestamptz` is read in the **session** `TimeZone`,
///   which on this server is `Asia/Shanghai`. That would put the applications
///   boundary eight hours later than the users boundary computed beside it in
///   the same response, so two tiles of one dashboard would disagree about
///   where "today" starts. The formulation is the one the facts owner already
///   uses (`sdkwork-deployments`,
///   `sdkwork-intelligence-deploy-repository-sqlx/src/usage.rs`).
const ENTITY_APPLICATIONS_SQL: &str = "\
SELECT
    COUNT(*)::bigint AS lifetime_quantity,
    COUNT(*) FILTER (WHERE created_at >= ($1::date)::timestamp AT TIME ZONE 'UTC')::bigint
        AS current_month_quantity,
    COUNT(*) FILTER (WHERE created_at >= ($2::date)::timestamp AT TIME ZONE 'UTC')::bigint
        AS last_seven_days_quantity,
    COUNT(*) FILTER (WHERE created_at >= ($3::date)::timestamp AT TIME ZONE 'UTC')::bigint
        AS today_quantity
FROM webserver_application
WHERE deleted_at IS NULL
  AND ($4::bigint IS NULL OR tenant_id = $4::bigint)";

/// The metered volumes across the four windows, one row per dimension.
///
/// `lifetime` is unbounded below on purpose: what it covers is a property of
/// the fact table rather than of the calendar, and the response reports the day
/// it starts on instead of this query inventing one.
///
/// Every bound is anchored to UTC through `($n::date)::timestamp AT TIME ZONE
/// 'UTC'`, matching the contract's "Inclusive **UTC** day" wording and the day
/// this response states in `asOf`. A bare `$n::timestamptz` would instead be
/// read in the session `TimeZone` (`Asia/Shanghai` on this server), and the
/// traffic tiles would then report a different "today" from the entity tiles
/// above them *and* from the committed traffic statistics read model
/// (`sdkwork-deployments/.../usage.rs:373`), which anchors the same way. Two
/// surfaces naming the same window must not disagree about where it starts.
const TRAFFIC_WINDOW_TOTALS_SQL: &str = "\
SELECT
    dimension,
    unit,
    COALESCE(SUM(quantity), 0)::bigint AS lifetime_quantity,
    COALESCE(SUM(quantity) FILTER (WHERE period_start >= ($3::date)::timestamp AT TIME ZONE 'UTC'), 0)::bigint
        AS current_month_quantity,
    COALESCE(SUM(quantity) FILTER (WHERE period_start >= ($2::date)::timestamp AT TIME ZONE 'UTC'), 0)::bigint
        AS last_seven_days_quantity,
    COALESCE(SUM(quantity) FILTER (WHERE period_start >= ($1::date)::timestamp AT TIME ZONE 'UTC'), 0)::bigint
        AS today_quantity
FROM deploy_usage_event
WHERE period_start < ($4::date)::timestamp AT TIME ZONE 'UTC'
  AND ($5::bigint IS NULL OR tenant_id = $5::bigint)
GROUP BY dimension, unit
ORDER BY dimension, unit";

/// First UTC day the facts cover, under the same tenant filter and the same
/// exclusive upper bound as the totals above — otherwise the stated basis and
/// the figure sitting on it could be about different rows. The bound is
/// UTC-anchored for the reason spelled out on [`TRAFFIC_WINDOW_TOTALS_SQL`],
/// and because the value this query *returns* is rendered in UTC by the
/// `AT TIME ZONE 'UTC'` on the aggregate: a bound and a figure that name
/// different time zones cannot be checked against each other.
const TRAFFIC_SINCE_SQL: &str = "\
SELECT (MIN(period_start) AT TIME ZONE 'UTC')::date::text
FROM deploy_usage_event
WHERE period_start < ($1::date)::timestamp AT TIME ZONE 'UTC'
  AND ($2::bigint IS NULL OR tenant_id = $2::bigint)";

/// The occupied storage, as two dimensions over one scan.
///
/// Written against Drive's own occupancy predicate (`lifecycle_status =
/// 'active'`) so this dashboard and Drive's quota enforcement cannot disagree
/// about what a tenant is holding; see the module docs for why that predicate
/// bounds **every** window, narrow ones included, and why `lifetime` is
/// therefore occupancy rather than an arrival total.
///
/// Eight columns rather than two grouped rows: both dimensions are projections
/// of the same rows, so one statement keeps them on one snapshot and one
/// predicate. The bounds are anchored on the same terms as the queries above,
/// and the tenant filter is `text` because `dr_drive_storage_object.tenant_id`
/// is `VARCHAR` — the same type the IAM queries compare against.
///
/// `SUM` over no rows is `NULL`, so every figure is coalesced: a tenant holding
/// nothing is a real `0` rather than a missing column.
const STORAGE_WINDOW_TOTALS_SQL: &str = "\
SELECT
    COALESCE(SUM(content_length), 0)::bigint AS lifetime_bytes,
    COALESCE(SUM(content_length) FILTER (WHERE created_at >= ($1::date)::timestamp AT TIME ZONE 'UTC'), 0)::bigint
        AS current_month_bytes,
    COALESCE(SUM(content_length) FILTER (WHERE created_at >= ($2::date)::timestamp AT TIME ZONE 'UTC'), 0)::bigint
        AS last_seven_days_bytes,
    COALESCE(SUM(content_length) FILTER (WHERE created_at >= ($3::date)::timestamp AT TIME ZONE 'UTC'), 0)::bigint
        AS today_bytes,
    COUNT(*)::bigint AS lifetime_objects,
    COUNT(*) FILTER (WHERE created_at >= ($1::date)::timestamp AT TIME ZONE 'UTC')::bigint
        AS current_month_objects,
    COUNT(*) FILTER (WHERE created_at >= ($2::date)::timestamp AT TIME ZONE 'UTC')::bigint
        AS last_seven_days_objects,
    COUNT(*) FILTER (WHERE created_at >= ($3::date)::timestamp AT TIME ZONE 'UTC')::bigint
        AS today_objects
FROM dr_drive_storage_object
WHERE lifecycle_status = 'active'
  AND ($4::text IS NULL OR tenant_id = $4::text)";

/// The agents catalog, counted by its own liveness rule.
///
/// `deleted_at IS NULL` is the **agents module's own** predicate, not a choice
/// made here: `sdkwork-intelligence-agents-service`'s list and count queries
/// both spell it that way (`persistence/sql.rs`), and `status` is a *business*
/// state (`Draft`, `Active`, `Disabled`, `Archived`, `Deleted`) rather than a
/// liveness one — an archived agent still exists, and a dashboard that dropped
/// it would shrink whenever an operator tidied up. That is the same rule the
/// applications figure follows for the same reason.
///
/// The tenant filter is `bigint` because `ai_agent.tenant_id` is `BIGINT`, like
/// this repository's own tables and unlike the IAM catalogs, and the bounds are
/// anchored on the same terms as [`ENTITY_APPLICATIONS_SQL`] because
/// `created_at` is a `TIMESTAMPTZ` there too.
const ENTITY_AGENTS_SQL: &str = "\
SELECT
    COUNT(*)::bigint AS lifetime_quantity,
    COUNT(*) FILTER (WHERE created_at >= ($1::date)::timestamp AT TIME ZONE 'UTC')::bigint
        AS current_month_quantity,
    COUNT(*) FILTER (WHERE created_at >= ($2::date)::timestamp AT TIME ZONE 'UTC')::bigint
        AS last_seven_days_quantity,
    COUNT(*) FILTER (WHERE created_at >= ($3::date)::timestamp AT TIME ZONE 'UTC')::bigint
        AS today_quantity
FROM ai_agent
WHERE deleted_at IS NULL
  AND ($4::bigint IS NULL OR tenant_id = $4::bigint)";

/// The per-day variant of [`ENTITY_USERS_SQL`].
///
/// Same predicate, same lexical bound, same tenant spelling; the only
/// difference is that it groups by the UTC day instead of summing four windows,
/// because the series window is the caller's rather than one of the four.
///
/// `$1` is inclusive and `$2` exclusive, mirroring the contract's own wording.
/// The day key is `substr(created_at, 1, 10)` — the same truncation the lexical
/// comparison relies on, and exact for the same reason: the column holds
/// `YYYY-MM-DD HH:MM:SS…+00`, so its first ten characters *are* the UTC day.
const ENTITY_USERS_DAILY_SQL: &str = "\
SELECT substr(created_at, 1, 10) AS day, COUNT(*)::bigint AS quantity
FROM iam_user
WHERE status = 'active' AND is_deleted = 0
  AND created_at >= $1 AND created_at < $2
  AND ($3::text IS NULL OR tenant_id = $3::text)
GROUP BY day
ORDER BY day";

/// The per-day variant of [`ENTITY_APPLICATIONS_SQL`], deriving its day key the
/// way [`TRAFFIC_SINCE_SQL`] renders one.
///
/// `AT TIME ZONE 'UTC'` on the value, not on the bound: without it the day would
/// be cut in the session's timezone (`Asia/Shanghai` here), so a point labelled
/// `2026-09-01` would hold eight hours of the 1st **and** — read the other way —
/// the first eight hours of the 2nd would land in the 2nd's point while the
/// window that bounds them is a UTC one. The bound and the group key have to
/// name the same day.
const ENTITY_APPLICATIONS_DAILY_SQL: &str = "\
SELECT (created_at AT TIME ZONE 'UTC')::date::text AS day, COUNT(*)::bigint AS quantity
FROM webserver_application
WHERE deleted_at IS NULL
  AND created_at >= ($1::date)::timestamp AT TIME ZONE 'UTC'
  AND created_at < ($2::date)::timestamp AT TIME ZONE 'UTC'
  AND ($3::bigint IS NULL OR tenant_id = $3::bigint)
GROUP BY day
ORDER BY day";

/// The per-day variant of [`ENTITY_AGENTS_SQL`], on the same terms as
/// [`ENTITY_APPLICATIONS_DAILY_SQL`].
const ENTITY_AGENTS_DAILY_SQL: &str = "\
SELECT (created_at AT TIME ZONE 'UTC')::date::text AS day, COUNT(*)::bigint AS quantity
FROM ai_agent
WHERE deleted_at IS NULL
  AND created_at >= ($1::date)::timestamp AT TIME ZONE 'UTC'
  AND created_at < ($2::date)::timestamp AT TIME ZONE 'UTC'
  AND ($3::bigint IS NULL OR tenant_id = $3::bigint)
GROUP BY day
ORDER BY day";

/// Reads one `bigint` column, naming the metric in the failure so a schema
/// drift is diagnosable from the log alone.
fn quantity_column(
    row: &sqlx::postgres::PgRow,
    column: &str,
    metric: &str,
) -> WebServiceResult<i64> {
    row.try_get(column)
        .map_err(|error| read_error(&format!("{metric}.{column}"), error))
}

/// The four figures in the contract's window order.
///
/// Ordered by [`METRICS_WINDOWS`] rather than by the caller's literal order, for
/// the same reason the server orders the window *bounds* that way: a window
/// added to the vocabulary must not end up described in one order and drawn in
/// another.
fn window_values(figures: WindowFigures, unit: &str) -> Vec<MetricsWindowValue> {
    METRICS_WINDOWS
        .iter()
        .filter_map(|window| {
            figures
                .iter()
                .find(|(id, _)| id == window)
                .map(|(_, quantity)| MetricsWindowValue {
                    window: (*window).to_owned(),
                    quantity: *quantity,
                    unit: unit.to_owned(),
                })
        })
        .collect()
}

/// Maps a read failure onto the service's error model.
///
/// Only the connection-level classes are reported as `DatabaseUnavailable`; a
/// malformed aggregate or a column that moved is `Internal`, because `503`
/// tells an operator that a capability is missing or the database is down, and
/// neither is true. The detail is logged here — the only layer holding it — and
/// never crosses into the response (`SECURITY_SPEC`).
fn read_error(operation: &str, error: sqlx::Error) -> WebServiceError {
    tracing::error!(
        operation,
        error = %error,
        "dashboard metrics summary read failed"
    );
    match error {
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_) => {
            WebServiceError::DatabaseUnavailable
        }
        other => WebServiceError::Internal(other.to_string()),
    }
}

/// The read model to inject, or `None` when this deployment cannot answer it.
///
/// Established at boot rather than assumed, for the reason the traffic read
/// model does the same: a host that composes no IAM tables must report the
/// capability as absent, not count zero users and draw it. A missing pool, a
/// non-PostgreSQL engine, or an unresolvable **required** table all end in the
/// same explicit, logged absence.
///
/// An unresolvable **optional** table is not one of those endings: the reader is
/// built anyway and the metric that table would have answered travels with it in
/// `unassembled`. See [`METRICS_OPTIONAL_SOURCES`] for why absence there is a
/// normal state rather than an unfinished one.
pub(crate) async fn shared_metrics_summary_reader() -> Option<Arc<dyn MetricsSummaryReadPort>> {
    let Some(pool) = sdkwork_database_sqlx::process_shared_database_pool() else {
        tracing::warn!(
            "dashboard metrics summary reports 503: no process-shared database pool is installed"
        );
        return None;
    };
    let Some(postgres) = pool.as_postgres().cloned() else {
        tracing::warn!(
            "dashboard metrics summary reports 503: the process-shared pool is not PostgreSQL"
        );
        return None;
    };
    let missing = match missing_summary_tables(&postgres).await {
        Ok(missing) => missing,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "dashboard metrics summary reports 503: a dependency table could not be resolved"
            );
            return None;
        }
    };
    // Required tables first, so a deployment missing one reports the `503` it
    // would have reported before this split existed — the optional source is an
    // addition to the probe, not a relaxation of it.
    let missing_required = missing_required_tables(&missing);
    if !missing_required.is_empty() {
        tracing::warn!(
            missing = ?missing_required,
            fact_table = TRAFFIC_USAGE_FACT_TABLE,
            "dashboard metrics summary reports 503: this deployment does not assemble every table the reading needs"
        );
        return None;
    }
    let unassembled = unassembled_metrics(&missing);
    if !unassembled.is_empty() {
        // Logged at a level below `warn`: on a deployment without the agents
        // module this is the expected state on every boot, and a warning that
        // is always present is a warning nobody reads.
        tracing::info!(
            unassembled = ?unassembled,
            agents_catalog = "ai_agent",
            "dashboard metrics summary reports these metrics as unassembled"
        );
    }
    Some(Arc::new(SharedDatabaseMetricsSummaryReadPort {
        pool: postgres,
        unassembled,
    }))
}

/// The **required** tables the probe could not resolve.
///
/// A free function rather than an inline `filter` because the `503`-or-not
/// decision is the one a mutation has to be able to flip: with the filter
/// buried in the boot path, the only way to test it would be to stand up a
/// database missing a table.
fn missing_required_tables(missing: &[&'static str]) -> Vec<&'static str> {
    missing
        .iter()
        .copied()
        .filter(|table| METRICS_SUMMARY_TABLES.contains(table))
        .collect()
}

/// The metrics whose **optional** source the probe could not resolve.
///
/// Matched through the table/metric pair rather than by index, so reordering the
/// optional list cannot silently name the wrong metric — the failure mode being
/// a response that says "agents cannot be counted" while reporting the
/// application figure as absent.
fn unassembled_metrics(missing: &[&'static str]) -> Vec<String> {
    METRICS_OPTIONAL_SOURCES
        .iter()
        .filter(|source| missing.contains(&source.table))
        .map(|source| source.metric.to_owned())
        .collect()
}

/// The tables the probe could not resolve, required and optional together.
///
/// One probe rather than one per list: the two sets are asked the same question
/// and answering it twice would let a table be present for one and absent for
/// the other.
async fn missing_summary_tables(pool: &PgPool) -> Result<Vec<&'static str>, sqlx::Error> {
    let mut missing = Vec::new();
    for table in METRICS_SUMMARY_TABLES
        .iter()
        .copied()
        .chain(METRICS_OPTIONAL_SOURCES.iter().map(|source| source.table))
    {
        let present = sqlx::query_scalar::<_, bool>("SELECT to_regclass($1::text) IS NOT NULL")
            .bind(table)
            .fetch_one(pool)
            .await?;
        if !present {
            missing.push(table);
        }
    }
    Ok(missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> MetricsWindowRequest {
        MetricsWindowRequest {
            as_of: "2026-09-24".to_owned(),
            last_seven_days_from: "2026-09-18".to_owned(),
            current_month_from: "2026-09-01".to_owned(),
            ends_before: "2026-09-25".to_owned(),
        }
    }

    #[test]
    fn every_window_is_reported_in_the_contract_order() {
        let values = window_values(
            [
                (METRICS_WINDOW_LIFETIME, 40),
                (METRICS_WINDOW_TODAY, 1),
                (METRICS_WINDOW_CURRENT_MONTH, 30),
                (METRICS_WINDOW_LAST_7_DAYS, 10),
            ],
            METRICS_UNIT_COUNT,
        );

        let order: Vec<&str> = values.iter().map(|value| value.window.as_str()).collect();
        assert_eq!(order, METRICS_WINDOWS, "the caller's literal order must not decide the response");
        assert_eq!(
            values.iter().map(|value| value.quantity).collect::<Vec<i64>>(),
            vec![1, 10, 30, 40],
            "each window must keep its own figure through the reordering"
        );
        assert!(values.iter().all(|value| value.unit == METRICS_UNIT_COUNT));
    }

    #[test]
    fn a_figure_for_a_window_outside_the_vocabulary_is_omitted_rather_than_invented() {
        // Reachable only if the vocabulary grows past the local array. Pinned
        // because both tempting alternatives are worse than omission:
        // defaulting the missing entry to `0` reports "none" about a window
        // nobody measured, and appending the unknown id would put a row on the
        // surface that no label exists for.
        let values = window_values(
            [
                (METRICS_WINDOW_TODAY, 7),
                ("last_30_days", 99),
                (METRICS_WINDOW_CURRENT_MONTH, 70),
                (METRICS_WINDOW_LIFETIME, 700),
            ],
            "REQUEST",
        );

        assert_eq!(values.len(), 3);
        assert!(
            !values.iter().any(|value| value.window == "last_30_days"),
            "a window absent from the vocabulary must not reach the response"
        );
        assert_eq!(values[0].quantity, 7, "the surviving windows keep their own figures");
    }

    /// The placeholder each `created_at >=` comparison refers to, in textual
    /// order.
    ///
    /// A scan rather than three literal `contains` checks because the two
    /// column types need different spellings of the same bound: a `TEXT`
    /// column compares against `$1` directly, and a `TIMESTAMPTZ` one wraps it
    /// in the UTC anchor. Pinning the literal text would make a correct anchor
    /// read as numbering drift, and that is precisely the failure this test is
    /// not about.
    fn window_placeholders(sql: &str) -> Vec<String> {
        sql.match_indices("created_at >= ")
            .map(|(index, marker)| {
                let tail = &sql[index + marker.len()..];
                let dollar = tail
                    .find('$')
                    .expect("every window comparison must bind a parameter");
                let digits: String = tail[dollar + 1..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                format!("${digits}")
            })
            .collect()
    }

    #[test]
    fn the_entity_queries_share_one_parameter_shape() {
        // The four statements are bound by one code path, so a query that
        // numbered its placeholders differently would still compile and would
        // silently compare a month bound against `created_at` as if it were the
        // day bound. Pinned here so the shape is a checked fact.
        for sql in [
            ENTITY_USERS_SQL,
            ENTITY_TENANTS_SQL,
            ENTITY_APPLICATIONS_SQL,
            ENTITY_AGENTS_SQL,
        ] {
            assert_eq!(
                window_placeholders(sql),
                vec!["$1", "$2", "$3"],
                "each `created_at >=` comparison must bind the month, seven-day, then day \
                 bound in that order: {sql}"
            );
            assert!(sql.contains("$4"), "$4 must be the tenant filter: {sql}");
            assert!(!sql.contains("$5"), "no entity query may take a fifth bind: {sql}");
        }
    }

    #[test]
    fn the_timestamptz_entity_count_anchors_its_windows_to_utc() {
        // Both halves of this are load-bearing, and the first one is the whole
        // reason this test exists:
        //
        // * Binding the bare date against a `TIMESTAMPTZ` column has no
        //   operator, so PostgreSQL cannot plan the comparison and the reading
        //   fails as a `500` — not a shifted count, a dead endpoint.
        // * `($n::date)::timestamp AT TIME ZONE 'UTC'` rather than
        //   `$n::timestamptz`, because the latter is read in the *session*
        //   `TimeZone`. That would move this figure's boundary off the one the
        //   `TEXT`-column figures beside it use, in the same response.
        //
        // The list is every query whose column is a `TIMESTAMPTZ`; adding a
        // fifth table without adding it here is the drift this pins against.
        // The windows and the series are counted separately because a series
        // query carries a third anchor — the one on its day *key*.
        for sql in [ENTITY_APPLICATIONS_SQL, ENTITY_AGENTS_SQL] {
            assert_eq!(
                sql.matches("created_at >= ($").count(),
                3,
                "all three windows must cast their bound before comparing it to a timestamptz \
                 column: {sql}"
            );
            assert_eq!(
                sql.matches("AT TIME ZONE 'UTC'").count(),
                3,
                "and all three must anchor that cast to UTC rather than to the session \
                 TimeZone: {sql}"
            );
            assert!(
                !sql.contains("::timestamptz"),
                "a bare date cast to timestamptz is resolved in the session TimeZone: {sql}"
            );
        }
        for sql in [ENTITY_APPLICATIONS_DAILY_SQL, ENTITY_AGENTS_DAILY_SQL] {
            assert_eq!(sql.matches("created_at >= ($").count(), 1, "one inclusive bound: {sql}");
            assert_eq!(sql.matches("created_at < ($").count(), 1, "one exclusive bound: {sql}");
            assert_eq!(
                sql.matches("AT TIME ZONE 'UTC'").count(),
                3,
                "two bounds and the day key — the key needs the anchor for the same reason \
                 the bounds do: {sql}"
            );
            assert!(!sql.contains("::timestamptz"), "{sql}");
        }
        // The two `TEXT` columns keep the comparison their storage format and
        // their owning module's own pagination both use — lexical against a
        // bare `YYYY-MM-DD`, with no anchor anywhere.
        for sql in [ENTITY_USERS_SQL, ENTITY_TENANTS_SQL, ENTITY_USERS_DAILY_SQL] {
            assert!(
                sql.contains("created_at >= $"),
                "a TEXT `created_at` compares lexically against the bare bound: {sql}"
            );
            assert!(!sql.contains("AT TIME ZONE"), "and needs no time zone anchor: {sql}");
        }
    }

    #[test]
    fn the_daily_series_derives_one_utc_day_key_per_column_type() {
        // The day key has to be the *UTC* day for the same reason the bounds do,
        // and it has to agree with them: a window ending at midnight UTC whose
        // points were grouped in the session timezone would label its last point
        // with a day the window does not cover.
        for sql in [ENTITY_APPLICATIONS_DAILY_SQL, ENTITY_AGENTS_DAILY_SQL] {
            assert!(
                sql.contains("(created_at AT TIME ZONE 'UTC')::date::text AS day"),
                "a timestamptz day key must be rendered in UTC and as `YYYY-MM-DD`: {sql}"
            );
        }
        // A `TEXT` column needs no anchor, and the truncation is the same
        // operation the lexical comparison relies on: the first ten characters
        // of `YYYY-MM-DD HH:MM:SS…+00` are the UTC day.
        assert!(ENTITY_USERS_DAILY_SQL.contains("substr(created_at, 1, 10) AS day"));
        assert!(!ENTITY_USERS_DAILY_SQL.contains("AT TIME ZONE"));
    }

    /// The distinct `$n` placeholders a statement binds, in order of first
    /// appearance.
    ///
    /// Distinct rather than every occurrence, because one parameter is
    /// deliberately used twice (`($3::bigint IS NULL OR tenant_id = $3::bigint)`
    /// so a single statement serves both reaches). Distinct rather than a set,
    /// because *which* bound is `$1` is the property at stake: a query that
    /// numbered its bounds differently would still compile and silently compare
    /// a month bound as if it were the day bound.
    ///
    /// Spelling-insensitive on purpose: the two column types need different
    /// text for the same bound (`$1` for a `TEXT` column, `($1::date)::timestamp
    /// AT TIME ZONE 'UTC'` for a `TIMESTAMPTZ` one), so pinning the literal text
    /// would make a correct anchor read as numbering drift — which is the one
    /// failure this is not about.
    fn bound_placeholders(sql: &str) -> Vec<String> {
        let mut ordered: Vec<String> = Vec::new();
        let bytes = sql.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'$' {
                let digits: String = sql[index + 1..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                if !digits.is_empty() {
                    let placeholder = format!("${digits}");
                    if !ordered.contains(&placeholder) {
                        ordered.push(placeholder);
                    }
                }
                index += 1 + digits.len();
            } else {
                index += 1;
            }
        }
        ordered
    }

    #[test]
    fn the_series_queries_share_one_parameter_shape() {
        // `$1` inclusive, `$2` exclusive, `$3` the tenant filter — and nothing
        // else. A query that numbered them differently would still compile and
        // would silently read an arbitrary window, which on a trend chart is
        // indistinguishable from real data.
        for sql in [
            ENTITY_USERS_DAILY_SQL,
            ENTITY_APPLICATIONS_DAILY_SQL,
            ENTITY_AGENTS_DAILY_SQL,
        ] {
            assert_eq!(
                bound_placeholders(sql),
                vec!["$1", "$2", "$3"],
                "a series binds the window's two bounds then the tenant filter: {sql}"
            );
            assert!(sql.contains("created_at >= $") || sql.contains("created_at >= ($"));
            assert!(
                sql.contains("created_at < $") || sql.contains("created_at < ($"),
                "the upper bound is exclusive: {sql}"
            );
            assert!(
                sql.contains("GROUP BY day") && sql.contains("ORDER BY day"),
                "a series is grouped and ordered by day, not left to the scan's order: {sql}"
            );
        }
    }

    #[test]
    fn only_an_optional_source_ever_names_a_metric() {
        // The load-bearing negative of the whole optional-source split: a
        // missing *required* table must not be reported as an unassembled
        // metric. Naming a metric the row still answers — or naming nothing at
        // all while the row is drawn from tables that are not there — is the
        // failure this pins against.
        assert!(
            unassembled_metrics(&["iam_user", "deploy_usage_event"]).is_empty(),
            "a missing required table is a 503, not an unassembled metric"
        );
        assert_eq!(
            unassembled_metrics(&["ai_agent"]),
            vec![METRICS_ENTITY_AGENTS.to_owned()],
            "the agents metric is named when its own table is the one missing"
        );
        assert!(
            unassembled_metrics(&[]).is_empty(),
            "a deployment that assembles everything names nothing"
        );
        // And the split itself: whatever is optional must not also be required,
        // or the 503 branch would win and the metric would never be named.
        for source in METRICS_OPTIONAL_SOURCES {
            assert!(
                !METRICS_SUMMARY_TABLES.contains(&source.table),
                "an optional source listed as required would 503 the whole row"
            );
        }
        assert_eq!(
            missing_required_tables(&["ai_agent"]),
            Vec::<&str>::new(),
            "and an optional table missing must not refuse the reading"
        );
        assert_eq!(
            missing_required_tables(&["ai_agent", "iam_user"]),
            vec!["iam_user"],
            "while a required one still does"
        );
    }

    #[test]
    fn the_entity_order_is_one_order_for_the_cards_and_the_series() {
        // Both use this function, so a metric cannot sit in one place in the
        // cards and another in the chart's tabs.
        assert_eq!(entity_order(METRICS_ENTITY_USERS), 0);
        assert_eq!(entity_order(METRICS_ENTITY_TENANTS), 1);
        assert_eq!(entity_order(METRICS_ENTITY_APPLICATIONS), 2);
        assert_eq!(entity_order(METRICS_ENTITY_AGENTS), 3);
        // The agents metric sorts after the three that every deployment
        // assembles, so its arrival or absence cannot reorder the rest.
        assert!(entity_order(METRICS_ENTITY_AGENTS) > entity_order(METRICS_ENTITY_APPLICATIONS));
        // An unknown metric sorts last rather than colliding with a known one.
        assert!(entity_order("traffic.requests") > entity_order(METRICS_ENTITY_AGENTS));
    }

    #[test]
    fn the_traffic_totals_bound_every_window_on_the_same_instant() {
        // `lifetime` is the tempting one to leave unbounded; doing so would let
        // a future-dated fact make the widest window disagree with all three
        // narrower ones, and the dashboard would read as if the clock moved.
        assert!(TRAFFIC_WINDOW_TOTALS_SQL.contains("period_start < ($4::date)::timestamp AT TIME ZONE 'UTC'"));
        assert!(!TRAFFIC_WINDOW_TOTALS_SQL.contains("period_start >= ($4::date)"));
        assert!(TRAFFIC_SINCE_SQL.contains("period_start < ($1::date)::timestamp AT TIME ZONE 'UTC'"));
        assert_eq!(
            TRAFFIC_WINDOW_TOTALS_SQL.matches("FILTER").count(),
            3,
            "the lifetime figure must be the unfiltered sum and the other three filtered"
        );
        assert!(TRAFFIC_WINDOW_TOTALS_SQL.contains("COALESCE(SUM(quantity), 0)::bigint"));
    }

    #[test]
    fn every_traffic_bound_is_anchored_to_utc_rather_than_the_session_time_zone() {
        // Measured, not argued: with two facts on 2026-09-23 18:00Z and 20:00Z
        // and the window the service resolves for "today", the bare
        // `$1::timestamptz` spelling counted both of them into the UTC day
        // 2026-09-24 (`150`) while the committed traffic statistics read model
        // counted neither (`0`) — the session here is `Asia/Shanghai`, so a
        // bare date cast lands eight hours early. Same window, same rows, two
        // answers; the anchor is what removes the second one.
        for sql in [TRAFFIC_WINDOW_TOTALS_SQL, TRAFFIC_SINCE_SQL] {
            assert!(
                !sql.contains("::timestamptz"),
                "a bare date cast to timestamptz is resolved in the session TimeZone: {sql}"
            );
        }
        assert_eq!(
            TRAFFIC_WINDOW_TOTALS_SQL.matches("AT TIME ZONE 'UTC'").count(),
            4,
            "all four window bounds must carry the anchor: {TRAFFIC_WINDOW_TOTALS_SQL}"
        );
        assert_eq!(
            TRAFFIC_SINCE_SQL.matches("AT TIME ZONE 'UTC'").count(),
            2,
            "the bound and the rendered first day must both be UTC: {TRAFFIC_SINCE_SQL}"
        );
    }

    #[test]
    fn the_storage_totals_read_one_occupancy_predicate() {
        // The figure this dashboard reports and the figure Drive refuses an
        // upload against must be the same figure, so the predicate is Drive's
        // own (`summarize_tenant_quota`). It is applied **once**, in the
        // statement's only `WHERE`: a second copy inside a window filter is how
        // one dimension comes to count rows the other does not.
        assert_eq!(
            STORAGE_WINDOW_TOTALS_SQL.matches("lifecycle_status = 'active'").count(),
            1,
            "the occupancy predicate must appear exactly once: {STORAGE_WINDOW_TOTALS_SQL}"
        );
        assert!(STORAGE_WINDOW_TOTALS_SQL.contains("FROM dr_drive_storage_object"));
        // Nothing here reads a soft-deleted object, which is what makes the
        // maintenance sweep — the job that finally deletes those rows — unable
        // to change any figure in this group.
        assert!(
            !STORAGE_WINDOW_TOTALS_SQL.contains("'deleted'"),
            "counting soft-deleted rows would let the sweep move a reported figure"
        );
    }

    #[test]
    fn every_storage_bound_is_anchored_to_utc_like_the_queries_beside_it() {
        // The storage columns are `timestamp with time zone`, so the same two
        // halves that apply to `ENTITY_APPLICATIONS_SQL` apply here: a bare date
        // has no operator against them, and a bare date cast to `timestamptz` is
        // resolved in the session `TimeZone`.
        assert!(
            !STORAGE_WINDOW_TOTALS_SQL.contains("::timestamptz"),
            "a bare date cast to timestamptz is resolved in the session TimeZone"
        );
        assert_eq!(
            STORAGE_WINDOW_TOTALS_SQL.matches("AT TIME ZONE 'UTC'").count(),
            6,
            "all six window comparisons must carry the anchor: {STORAGE_WINDOW_TOTALS_SQL}"
        );
        for bound in ["$1", "$2", "$3"] {
            assert_eq!(
                STORAGE_WINDOW_TOTALS_SQL
                    .matches(&format!("({bound}::date)::timestamp AT TIME ZONE 'UTC'"))
                    .count(),
                2,
                "{bound} must bound the month/week/day window of both dimensions"
            );
        }
        // Both dimensions walk the same three bounds in the same order, which is
        // what keeps the bytes and the objects holding them on one snapshot.
        assert_eq!(
            window_placeholders(STORAGE_WINDOW_TOTALS_SQL),
            vec!["$1", "$2", "$3", "$1", "$2", "$3"],
            "the bytes and the object count must use one bound order"
        );
        assert_eq!(
            STORAGE_WINDOW_TOTALS_SQL.matches("FILTER").count(),
            6,
            "six of the eight projections are windowed and two are standing totals"
        );
        // The two standing totals must be Drive's own aggregate, unfiltered.
        // This is the assertion that makes "the dashboard and the quota cannot
        // disagree" a property of the statement: Drive's `summarize_tenant_quota`
        // selects exactly `COALESCE(SUM(content_length), 0)` and a `COUNT` with
        // no window filter, over the same predicate. A `FILTER` added to either
        // of these two lines would leave every other assertion here green and
        // end the equality, so it is pinned literally.
        assert!(
            STORAGE_WINDOW_TOTALS_SQL
                .contains("COALESCE(SUM(content_length), 0)::bigint AS lifetime_bytes"),
            "the widest byte figure must be the unfiltered aggregate Drive computes"
        );
        assert!(
            STORAGE_WINDOW_TOTALS_SQL.contains("COUNT(*)::bigint AS lifetime_objects"),
            "the widest object figure must be the unfiltered count Drive computes"
        );
        // `SUM` over no rows is `NULL`, so an empty storage plane must still
        // answer `0` rather than a column that cannot be read. Counting the
        // aggregate rather than the coalesced substring: three of the four sums
        // carry a `FILTER` between `SUM(content_length)` and the `, 0)`, so the
        // whole coalesced form appears only on the unfiltered line.
        assert_eq!(
            STORAGE_WINDOW_TOTALS_SQL.matches("SUM(content_length)").count(),
            4,
            "there are four byte figures and each is a sum of `content_length`"
        );
        assert_eq!(
            STORAGE_WINDOW_TOTALS_SQL.matches("COALESCE(").count(),
            4,
            "and every one of them is coalesced, with no other coalesce beside them"
        );
        assert!(!STORAGE_WINDOW_TOTALS_SQL.contains("COALESCE(COUNT"));
    }

    #[test]
    fn a_missing_table_is_named_rather_than_counted_as_zero() {
        // The list is what the boot probe walks; dropping an entry would make
        // the capability claim to be assembled while one of its figures has no
        // source, and the zeros it produced would look like real ones.
        //
        // Asserted against literals rather than against the constants the list
        // is *about*, so the assertion can still fail: a list built from those
        // constants would make this test a tautology.
        assert_eq!(
            METRICS_SUMMARY_TABLES,
            [
                "iam_user",
                "iam_tenant",
                "webserver_application",
                TRAFFIC_USAGE_FACT_TABLE,
                "dr_drive_storage_object",
            ]
        );
        // The load-bearing negative: `ai_agent` must **not** be required. The
        // probe is all-or-nothing, so listing it here would take the whole row
        // — users, traffic, and storage included — down as a `503` on every
        // deployment that does not run the agents module, which is strictly
        // worse than the agent figure being absent.
        assert!(!METRICS_SUMMARY_TABLES.contains(&"ai_agent"));
        assert_eq!(
            METRICS_OPTIONAL_SOURCES.len(),
            1,
            "the optional list is where such a source belongs instead"
        );
        assert_eq!(METRICS_OPTIONAL_SOURCES[0].table, "ai_agent");
        assert_eq!(METRICS_OPTIONAL_SOURCES[0].metric, METRICS_ENTITY_AGENTS);
    }

    #[test]
    fn a_tenant_scoped_read_carries_the_same_bounds_it_was_given() {
        // Guards the contract between the service's resolution and this reader:
        // the reader must not re-derive a bound from the clock, or the response
        // and the query behind it would be able to name different days.
        let windows = request();
        assert_eq!(windows.as_of, "2026-09-24");
        assert_eq!(windows.last_seven_days_from, "2026-09-18");
        assert_eq!(windows.current_month_from, "2026-09-01");
        assert_eq!(windows.ends_before, "2026-09-25");
    }
}
