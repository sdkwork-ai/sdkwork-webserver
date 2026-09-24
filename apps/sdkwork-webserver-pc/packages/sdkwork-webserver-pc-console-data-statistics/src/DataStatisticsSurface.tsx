import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { DataTable, type DataTableColumn } from "@sdkwork/ui-pc-react";
import { LockKeyhole } from "lucide-react";
import {
  hasWebserverPermission,
  translateWebserver,
  type WebserverLocale,
  type WebserverMessageKey,
} from "@sdkwork/webserver-pc-commons";
import type {
  MetricsSummaryResponse,
  TrafficUsageAppTotal,
  TrafficUsageDailyPoint,
  TrafficUsageStatisticsResponse,
  TrafficUsageTenantTotal,
} from "@sdkwork/webserver-pc-admin-core";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  arrivalSign,
  buildChartPlot,
  buildChartSeries,
  buildMetricsCards,
  buildSummaryCards,
  buildWindowDays,
  CHART_KINDS,
  entitySeriesFigures,
  failureMessage,
  formatQuantity,
  isEmptyReading,
  isArrivalGroup,
  isUnavailableReading,
  METRICS_ENTITY_AGENTS,
  METRICS_ENTITY_METRICS,
  METRICS_LIFETIME_WINDOW,
  METRICS_PLATFORM_ENTITY_METRICS,
  METRICS_STORAGE_METRICS,
  METRICS_UNIT_COUNT,
  metricLabelKey,
  metricsWindowBasis,
  metricsWindowLabelKey,
  resolveChartDimension,
  seriesLabelKey,
  TRAFFIC_DIMENSIONS,
  utcDay,
  type ChartKind,
  type MetricsCard,
  type MetricsCardValue,
  type MetricsGroup,
} from "./data-statistics-model.ts";
import { createMetricsSummaryClient } from "./metrics-summary-client.ts";
import {
  createTrafficUsageClient,
  type TrafficUsageReadingQuery,
  type TrafficUsageReach,
} from "./traffic-usage-client.ts";

/**
 * The data-statistics pages: the Dashboard overview and the Traffic Statistics
 * reading, over the Web Server's own aggregated traffic contract.
 *
 * ## Two pages, one implementation, two reaches
 *
 * `view` decides how much of the reading is on screen; `reach` decides which
 * operation answers it, and only the four exported symbols below bind a reach —
 * it is not a prop the host may set:
 *
 * - `DashboardSurface` / `TrafficStatisticsSurface` read the caller's own
 *   tenant and are what the app-console module registers.
 * - `DashboardPlatformSurface` / `TrafficStatisticsPlatformSurface` read every
 *   tenant this edge serves and are re-exported under the admin names by
 *   `@sdkwork/webserver-pc-admin-*`.
 *
 * A console mount that reached for the platform symbols anyway would not get a
 * wider reading: the platform operation is restricted to the operator tenant
 * and answers `40301`, so the boundary fails closed rather than silently
 * relabelling one tenant's traffic as the platform total.
 *
 * ## Three states, not two — and only two of them replace the frame
 *
 * A reading that could not be produced (the edge answers `503` when no usage
 * read model is assembled, or when a dependency is down) renders as an
 * unavailable *capability*; a reading that answered and holds no facts renders
 * as an empty window; anything else renders the figures. Collapsing the first
 * into the second would turn "this deployment has nothing to measure with" into
 * "this edge served no traffic", which is a different claim about the system
 * than the response actually made.
 *
 * The empty window is an *annotation on a drawn page*, not a replacement for
 * it. Every answered reading — populated or all-zero — renders the same frame:
 * a card per dimension in the metering vocabulary (a dimension the window
 * carried no row for summed no facts, so its card is a real `0`), the trend
 * block, and the breakdown tables. An operator opening a quiet deployment
 * therefore sees the dashboard they are looking at, with zeros in it, rather
 * than a heading over a sentence; and the two states that cannot be drawn —
 * `unavailable` and `error` — are the only ones that take the space instead.
 * The zeros are derived from the response (its window, its dimensions) and
 * never invented: with a dimension filter applied, the frame narrows to the
 * dimension that was asked for instead of reporting the others at `0`.
 */

const TRAFFIC_READ_PERMISSION = "web.traffic.read";

/** The window the contract documents as the default, sent explicitly so the
 *  form and the response's own window agree instead of the form showing
 *  blanks until the first read comes back. */
const DEFAULT_WINDOW_DAYS = 30;
const DEFAULT_TOP_APPS = 10;
/** The overview is a glance, so it reads a shorter ranking than the ledger. */
const DASHBOARD_TOP_APPS = 5;

type DataStatisticsView = "dashboard" | "traffic-usage";

export interface DataStatisticsSurfaceProps {
  backendApiBaseUrl: string;
  locale: WebserverLocale;
  permissionScope?: readonly string[];
  resource: "dashboard" | "traffic-usage";
  tokenManager: AuthTokenManager;
}

interface DataStatisticsPageProps extends DataStatisticsSurfaceProps {
  reach: TrafficUsageReach;
  view: DataStatisticsView;
}

interface WindowDraft {
  dateFrom: string;
  dateTo: string;
  dimension: string;
  topApps: string;
}

interface LoadedReading {
  reading: TrafficUsageStatisticsResponse;
}

interface UnavailableReading {
  detail: string;
  kind: "unavailable";
}

interface FailedReading {
  detail: string;
  kind: "error";
}

export function DashboardSurface(props: DataStatisticsSurfaceProps) {
  return <DataStatisticsPage {...props} reach="own-tenant" view="dashboard" />;
}

export function TrafficStatisticsSurface(props: DataStatisticsSurfaceProps) {
  return <DataStatisticsPage {...props} reach="own-tenant" view="traffic-usage" />;
}

/**
 * The operations reading of the same two pages: every tenant this edge serves.
 *
 * Exported from the console capability because the page, the formatting, and the
 * i18n are one implementation; what the admin packages add is the admin menu
 * entries and this binding.
 */
export function DashboardPlatformSurface(props: DataStatisticsSurfaceProps) {
  return <DataStatisticsPage {...props} reach="every-tenant" view="dashboard" />;
}

export function TrafficStatisticsPlatformSurface(props: DataStatisticsSurfaceProps) {
  return <DataStatisticsPage {...props} reach="every-tenant" view="traffic-usage" />;
}

function DataStatisticsPage({
  backendApiBaseUrl,
  locale,
  permissionScope = [],
  reach,
  resource,
  tokenManager,
  view,
}: DataStatisticsPageProps) {
  const client = useMemo(
    () => createTrafficUsageClient(backendApiBaseUrl, tokenManager),
    [backendApiBaseUrl, tokenManager],
  );
  const t = useCallback(
    (key: WebserverMessageKey, values?: Record<string, string | number>) =>
      translateWebserver(locale, key, values),
    [locale],
  );
  const authorized = hasWebserverPermission(permissionScope, TRAFFIC_READ_PERMISSION);
  const defaultDraft = useMemo<WindowDraft>(
    () => ({
      dateFrom: utcDay(-(DEFAULT_WINDOW_DAYS - 1)),
      dateTo: utcDay(1),
      dimension: "",
      topApps: String(view === "dashboard" ? DASHBOARD_TOP_APPS : DEFAULT_TOP_APPS),
    }),
    [view],
  );
  const [draft, setDraft] = useState<WindowDraft>(defaultDraft);
  const [applied, setApplied] = useState<TrafficUsageReadingQuery>(() => queryOf(defaultDraft));
  const [loaded, setLoaded] = useState<LoadedReading | null>(null);
  const [failure, setFailure] = useState<UnavailableReading | FailedReading | null>(null);
  const [loading, setLoading] = useState(false);
  const [inputError, setInputError] = useState<"topApps" | null>(null);
  const inFlight = useRef<AbortController | null>(null);

  useEffect(() => {
    if (!authorized) {
      return;
    }
    let cancelled = false;
    const controller = new AbortController();
    inFlight.current?.abort();
    inFlight.current = controller;
    setLoading(true);
    void client
      .read(reach, applied)
      .then((reading) => {
        if (cancelled) return;
        setLoaded({ reading });
        setFailure(null);
      })
      .catch((reason: unknown) => {
        if (cancelled) return;
        // A 503 is the edge reporting the answer it could not give, not a
        // failure of this request, so it becomes its own state instead of
        // joining the generic error path.
        setLoaded(null);
        setFailure(
          isUnavailableReading(reason)
            ? { detail: failureMessage(reason), kind: "unavailable" }
            : { detail: failureMessage(reason), kind: "error" },
        );
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [applied, authorized, client, reach]);

  const reading = loaded?.reading ?? null;
  // "The window answered with nothing" is a fact about the reading, read once:
  // it selects the note below and licenses the zero baseline the trend block
  // draws when the response carries no daily rows at all.
  const empty = reading ? isEmptyReading(reading) : false;
  /**
   * The dimensions the frame always shows a card and a series for.
   *
   * The whole metering vocabulary normally: the cards *are* the reading's
   * dimensions, and one the window carried no row for summed no facts, so its
   * card is a real `0` rather than a missing one. A dimension filter collapses
   * it to the dimension the operator asked for — the others were excluded from
   * the read rather than measured at zero, and a `0` card for them would
   * report a figure the response never made.
   */
  const overviewDimensions = useMemo(
    () => (applied.dimension ? [applied.dimension] : TRAFFIC_DIMENSIONS.map((known) => known.dimension)),
    [applied.dimension],
  );
  const cards = useMemo(
    () => buildSummaryCards(reading?.totals ?? [], overviewDimensions),
    [overviewDimensions, reading],
  );
  const unitByDimension = useMemo(() => {
    const units = new Map<string, string>();
    for (const card of cards) {
      units.set(card.dimension, card.unit);
    }
    return units;
  }, [cards]);
  /**
   * A series id's display name, from whichever vocabulary owns it.
   *
   * The trend chart draws **two** readings on one plot, so its tabs and its
   * legend hold metered dimensions and entity metrics side by side; the two are
   * labelled from different catalogs, which is why this resolves through
   * `seriesLabelKey` rather than straight to the dimension catalog. Everything
   * it labels is "a thing that can be plotted", which is also what the traffic
   * tables' dimension column holds — so one function, not two that would drift
   * apart on the next vocabulary.
   *
   * `translateWebserver` hands back the key itself when the catalog has no
   * entry, which is how an id this file has never heard of still reaches the
   * operator under its contract name.
   */
  const seriesText = useCallback(
    (dimension: string): string => {
      const key = seriesLabelKey(dimension) as WebserverMessageKey;
      const translated = t(key);
      return translated === key ? dimension : translated;
    },
    [t],
  );
  /**
   * The plot's x domain: the traffic response's own **resolved** window.
   *
   * Read off the response rather than re-derived from `applied` here, because
   * the response is the server's own answer about the days it cut the figures
   * against — the same answer the metric reading reports back as
   * `seriesWindow`, since both operations resolve the pair with one rule.
   *
   * That other spelling is not the source: the frame below is drawn only when
   * *this* reading answered, so when there is a plot there is also a response
   * to take its days from. Using one of the two keeps the axis tied to the
   * response the plot is drawn over, rather than to a second copy of the same
   * window that could only ever be equal or wrong.
   */
  const windowDays = useMemo(
    () => (reading ? buildWindowDays(reading.dateFrom, reading.dateTo) : []),
    [reading],
  );
  /** What one day's mark says when an operator rests on it. */
  const pointText = useCallback(
    (unit: string, day: string, value: number): string =>
      t("dataStatistics.chart.point", {
        day,
        value: formatQuantity(String(value), unit, locale),
      }),
    [locale, t],
  );

  const peakText = useCallback(
    (dimension: string, peak: number): string => {
      const unit = unitByDimension.get(dimension);
      return t("dataStatistics.peak", {
        value: unit
          ? formatQuantity(String(peak), unit, locale)
          : String(peak),
      });
    },
    [locale, t, unitByDimension],
  );

  /**
   * The dashboard's cardinal metric row — a second reading over a second
   * operation.
   *
   * Deliberately not derived from the traffic reading above. That one answers
   * **one** window the operator chose; a metric card reports **four** the server
   * resolved. Cutting the four out of the traffic response would mean
   * re-deriving the server's calendar here, and two calendars is exactly how a
   * card comes to be labelled with a period its figure was not measured over.
   *
   * Only the overview mounts it. The ledger page is filtered to an arbitrary
   * window, so fixed "today / 7 days / month / total" cards sitting above a form
   * that says otherwise would contradict the form.
   */
  const metricsClient = useMemo(
    () => createMetricsSummaryClient(backendApiBaseUrl, tokenManager),
    [backendApiBaseUrl, tokenManager],
  );
  const [metrics, setMetrics] = useState<MetricsSummaryResponse | null>(null);
  const [metricsFailure, setMetricsFailure] = useState<
    UnavailableReading | FailedReading | null
  >(null);
  const [metricsReload, setMetricsReload] = useState(0);
  const metricsInFlight = useRef<AbortController | null>(null);

  useEffect(() => {
    if (!authorized || view !== "dashboard") {
      return;
    }
    let cancelled = false;
    const controller = new AbortController();
    metricsInFlight.current?.abort();
    metricsInFlight.current = controller;
    void metricsClient
      .read(reach, { dateFrom: applied.dateFrom, dateTo: applied.dateTo }, { signal: controller.signal })
      .then((summary) => {
        if (cancelled) return;
        setMetrics(summary);
        setMetricsFailure(null);
      })
      .catch((reason: unknown) => {
        if (cancelled) return;
        // A 503 here means "this deployment cannot count these" — its own state,
        // for the same reason the traffic reading gives it one: a row of zeros
        // is a claim about the installation, and an unwired read model is not
        // entitled to make it.
        setMetrics(null);
        setMetricsFailure(
          isUnavailableReading(reason)
            ? { detail: failureMessage(reason), kind: "unavailable" }
            : { detail: failureMessage(reason), kind: "error" },
        );
      });
    return () => {
      cancelled = true;
      controller.abort();
    };
    // `applied` is a dependency because the metric reading now carries the
    // series, and the series is cut against this page's window: a changed window
    // that re-read only the traffic facts would leave the entity lines on the
    // old days under axis labels naming the new ones.
  }, [applied, authorized, metricsClient, metricsReload, reach, view]);

  /**
   * Whether the response's own scope agrees with the reach this mount bound.
   *
   * Not a formality. A count carries no identifiers, so a platform-wide count
   * drawn on the console would be indistinguishable from the console's own
   * estate — which is why the row is withheld rather than drawn when the two
   * disagree, instead of being labelled from the header's scope (that one comes
   * from the traffic reading and would attach the wrong scope to these figures).
   */
  const metricsScopeAgrees = metrics
    ? metrics.platformScope === (reach === "every-tenant")
    : false;
  const metricsBasis = useMemo(() => metricsWindowBasis(metrics?.windows ?? []), [metrics]);
  const entityVocabulary = useMemo(
    () =>
      (reach === "every-tenant" ? METRICS_PLATFORM_ENTITY_METRICS : METRICS_ENTITY_METRICS).map(
        (metric) => ({ metric, unit: METRICS_UNIT_COUNT }),
      ),
    [reach],
  );
  const trafficVocabulary = useMemo(
    () => TRAFFIC_DIMENSIONS.map((known) => ({ metric: known.dimension, unit: known.unit })),
    [],
  );
  /**
   * The metrics this deployment **could not read**, as the response named them.
   *
   * Read off the response rather than inferred from an absent row: `agents`
   * missing from `entities` means one of two opposite things — "this
   * deployment cannot count agents" or "there are no agents" — and only the
   * response knows which. A surface that guessed would draw a zero it never
   * took.
   */
  const unassembledMetrics = useMemo(
    () => new Set(metrics?.unassembledMetrics ?? []),
    [metrics],
  );
  const entityCards = useMemo(
    () => buildMetricsCards(metrics?.entities ?? [], entityVocabulary, metrics?.unassembledMetrics ?? []),
    [entityVocabulary, metrics],
  );
  const trafficCards = useMemo(
    () => buildMetricsCards(metrics?.traffic ?? [], trafficVocabulary),
    [metrics, trafficVocabulary],
  );
  /**
   * The storage group's cards.
   *
   * Vocabulary-fed like the other two, so an empty storage plane renders two
   * real `0` cards rather than an absent group: the boot probe has already
   * ruled out "the read model is missing", and a tenant that has stored nothing
   * is a tenant whose storage figure is zero.
   */
  const storageCards = useMemo(
    () => buildMetricsCards(metrics?.storage ?? [], METRICS_STORAGE_METRICS),
    [metrics],
  );

  /**
   * How the chart draws, and which metric it draws.
   *
   * Both are the *operator's* choices rather than the reading's, so they live in
   * state and survive a re-read: someone who switched to egress and then widened
   * the window is still looking at egress.
   */
  const [chartKind, setChartKind] = useState<ChartKind>("bar");
  const [chartSelection, setChartSelection] = useState<string | null>(null);

  /**
   * The metric reading's series ids, in the response's own order.
   *
   * Taken from `series` rather than from the card vocabulary: the response
   * reports one entry per metric **it could read**, so a deployment that cannot
   * count agents contributes no line, and the chart's tabs say exactly what the
   * figures behind them are. A metric the response carried with no points at all
   * is still a line — flat at zero over the window — which is what makes a quiet
   * period read as a quiet period rather than as a missing chart.
   */
  const entitySeriesIds = useMemo(
    () => (metrics?.series ?? []).map((entry) => entry.metric),
    [metrics],
  );
  /**
   * Whether the **metered** breakdown is missing while the reading is not empty.
   *
   * Kept separate from "the chart has nothing to draw" now that a second reading
   * feeds the same chart: this one suppresses the metered dimensions, because a
   * zero line would contradict the non-zero traffic cards above it — but it must
   * not take the entity lines down with them, since those are backed by their
   * own facts and are correctly drawn.
   */
  const volumeBreakdownMissing = reading !== null && reading.daily.length === 0 && !empty;
  /**
   * The plot's inputs: both readings' days, one vocabulary, one domain.
   *
   * The two readings are cut against **one** pair of bounds by this page — the
   * same `applied` window that drives the traffic read is handed to the metric
   * client — so their days share one x domain and the tabs can switch between
   * them without the axis changing meaning. `windowDays` is that domain, taken
   * from the traffic response's own resolved bounds, and it is the *same*
   * window the metric response reports back as `seriesWindow` because both
   * operations resolve the pair with one server-side rule. The equality is not
   * assumed here: it is pinned where it is produced, by the assertion that both
   * reads receive the same two dates.
   *
   * A metric day outside the domain is therefore unreachable — and if it were
   * reached it would not be plotted, rather than silently widening the axis and
   * relabelling the metered days around it.
   */
  const chartInputs = useMemo(() => {
    const entityFigures = entitySeriesFigures(metrics?.series ?? []);
    const daily = volumeBreakdownMissing
      ? entityFigures
      : [...(reading?.daily ?? []), ...entityFigures];
    const vocabulary = volumeBreakdownMissing
      ? entitySeriesIds
      : [...overviewDimensions, ...entitySeriesIds];
    // The entity metrics are counts, so their unit comes from that vocabulary
    // rather than from the traffic cards' map, which has never heard of them and
    // would format a count as a bare unlabelled number.
    const units = new Map(unitByDimension);
    for (const metric of entitySeriesIds) {
      units.set(metric, METRICS_UNIT_COUNT);
    }
    return { daily, vocabulary, units };
  }, [entitySeriesIds, metrics, overviewDimensions, reading, unitByDimension, volumeBreakdownMissing]);

  /**
   * The chart's series — one per series id, each on its own real axis.
   *
   * From both readings' daily facts. A window that answered with none is drawn
   * over its own window at `0` instead — the read models group by day, so no
   * rows means every day summed to nothing — but **only** for a source that is
   * not contradicting its own totals: see `volumeBreakdownMissing`.
   */
  const chartSeries = useMemo(() => {
    if (!reading && chartInputs.daily.length === 0) {
      return [];
    }
    if (chartInputs.daily.length === 0 && !empty) {
      return [];
    }
    return buildChartSeries(
      chartInputs.daily,
      windowDays,
      chartInputs.vocabulary,
      chartInputs.units,
    );
  }, [chartInputs, empty, reading, windowDays]);

  // Resolved on every render rather than kept in state: a re-read can drop the
  // dimension the operator picked — a filter narrows the vocabulary to one, a
  // wider window can carry a set that no longer holds it — and the chart has to
  // land on something that is actually there.
  const chartDimension = resolveChartDimension(chartSeries, chartSelection);
  const chartLine = chartSeries.find((line) => line.dimension === chartDimension) ?? null;
  const chartPlot = useMemo(() => buildChartPlot(chartLine), [chartLine]);
  // An axis is *built* bottom-up and *read* top-down. The ticks carry their own
  // positions, so this order is only about how the labels come out of the
  // accessibility tree — a screen reader should hear the top of the scale first.
  const chartTicks = [...(chartLine?.ticks ?? [])].reverse();
  /** The axis labels, formatted once so the gutter can be sized from them. */
  const chartTickLabels = chartTicks.map((tick) =>
    formatQuantity(String(tick.value), chartLine?.unit ?? "", locale),
  );
  /**
   * A metric's display name.
   *
   * Entity metrics have their own catalog entries; a metered dimension falls
   * back to the dimension vocabulary, and anything neither knows reaches the
   * operator under its contract id — the same treatment the dimension labels
   * get, so a newly metered dimension arrives as its own card rather than as an
   * unlabelled one.
   */
  const metricText = useCallback(
    (metric: string): string => {
      const key = metricLabelKey(metric) as WebserverMessageKey;
      const translated = t(key);
      return translated === key ? seriesText(metric) : translated;
    },
    [seriesText, t],
  );
  const metricsBasisText = (window: string): string | undefined => {
    const bounds = metricsBasis.get(window);
    if (!bounds) {
      return undefined;
    }
    return bounds.from
      ? t("dataStatistics.window.basis", { from: bounds.from, to: bounds.to })
      : t("dataStatistics.window.basisOpen", { to: bounds.to });
  };
  const metricsWindowText = (group: MetricsGroup, window: string): string =>
    t(metricsWindowLabelKey(group, window) as WebserverMessageKey);
  const metricsCellText = (group: MetricsGroup, value: MetricsCardValue): string => {
    const formatted = formatQuantity(value.quantity, value.unit, locale);
    // The arrival groups' narrow windows are deltas, so they are signed — by
    // `arrivalSign`, which owns the two cases that must stay unsigned.
    return isArrivalGroup(group) ? `${arrivalSign(value.quantity)}${formatted}` : formatted;
  };
  /**
   * One group of metric cards.
   *
   * A plain function called inline rather than a component: it closes over the
   * render context, and as a component it would be a fresh type on every render,
   * remounting the whole grid each time the surface re-rendered.
   */
  const renderMetricsGroup = (
    group: MetricsGroup,
    heading: string,
    hint: string,
    cards: readonly MetricsCard[],
  ) => (
    <section className="statistics-metrics-group" key={group}>
      <div className="statistics-metrics-legend">
        <h3>{heading}</h3>
        <span>{hint}</span>
      </div>
      <div className="statistics-metrics-grid">
        {cards.map((card) => (
          <article
            className={
              card.unassembled
                ? "statistics-metric-card statistics-metric-card--unassembled"
                : "statistics-metric-card"
            }
            key={card.metric}
          >
            <div className="statistics-metric-head">
              <span className="statistics-metric-name">{metricText(card.metric)}</span>
              {/* The badge names the headline's window, so it is drawn only when
                  there is a headline: a card left with its sub-row alone must
                  not caption the absence with the widest window's name. The name
                  comes from the group, because storage's widest window is a
                  standing holding rather than a running total.

                  A card whose source this deployment does not assemble has no
                  headline either, and gets this badge instead — the slot is the
                  card's top-right corner in both cases, so the exception is
                  visible where the other cards state their window rather than
                  looking like a card that simply lost its figures. */}
              {card.unassembled ? (
                <span className="statistics-metric-badge statistics-metric-badge--unassembled">
                  {t("dataStatistics.metricCard.unassembled")}
                </span>
              ) : card.headline ? (
                <span className="statistics-metric-badge">
                  {metricsWindowText(group, METRICS_LIFETIME_WINDOW)}
                </span>
              ) : null}
            </div>
            {card.unassembled ? (
              // The card keeps its frame and its name, and says what is missing
              // instead of drawing four zeroes: "this deployment cannot count
              // agents" and "there are no agents" are opposite claims, and only
              // the reading knows which one holds.
              <p className="statistics-metric-unassembled">
                {t("dataStatistics.metricCard.unassembledBody")}
              </p>
            ) : (
              <>
                {card.headline ? (
                  <span className="statistics-metric-headline">
                    {formatQuantity(card.headline.quantity, card.headline.unit, locale)}
                  </span>
                ) : null}
                <div className="statistics-metric-breakdown">
                  {card.narrower.map((value) => (
                    <div
                      className="statistics-metric-cell"
                      key={value.window}
                      title={metricsBasisText(value.window)}
                    >
                      <span>{metricsWindowText(group, value.window)}</span>
                      <strong>{metricsCellText(group, value)}</strong>
                    </div>
                  ))}
                </div>
              </>
            )}
          </article>
        ))}
      </div>
    </section>
  );

  const dailyColumns = useMemo<DataTableColumn<TrafficUsageDailyPoint>[]>(
    () => [
      { id: "usageDate", header: t("dataStatistics.day"), cell: (row) => row.usageDate },
      { id: "dimension", header: t("dataStatistics.dimension"), cell: (row) => seriesText(row.dimension) },
      {
        align: "right",
        id: "quantity",
        header: t("dataStatistics.quantity"),
        cell: (row) => formatQuantity(row.quantity, unitByDimension.get(row.dimension) ?? "", locale),
      },
    ],
    [seriesText, locale, t, unitByDimension],
  );
  const appColumns = useMemo<DataTableColumn<TrafficUsageAppTotal>[]>(
    () => [
      {
        id: "app",
        header: t("dataStatistics.app"),
        cell: (row) => row.appSlug ?? row.appUuid ?? (
          <span className="statistics-unattributed">{t("dataStatistics.unattributed")}</span>
        ),
      },
      { id: "dimension", header: t("dataStatistics.dimension"), cell: (row) => seriesText(row.dimension) },
      {
        align: "right",
        id: "quantity",
        header: t("dataStatistics.quantity"),
        cell: (row) => formatQuantity(row.quantity, row.unit, locale),
      },
    ],
    [seriesText, locale, t],
  );
  const tenantColumns = useMemo<DataTableColumn<TrafficUsageTenantTotal>[]>(
    () => [
      { id: "tenantId", header: t("dataStatistics.tenant"), cell: (row) => row.tenantId },
      { id: "dimension", header: t("dataStatistics.dimension"), cell: (row) => seriesText(row.dimension) },
      {
        align: "right",
        id: "quantity",
        header: t("dataStatistics.quantity"),
        cell: (row) => formatQuantity(row.quantity, row.unit, locale),
      },
    ],
    [seriesText, locale, t],
  );

  function applyDraft(): void {
    const topApps = draft.topApps.trim();
    if (topApps !== "") {
      const parsed = Number(topApps);
      if (!Number.isInteger(parsed) || parsed < 1) {
        setInputError("topApps");
        return;
      }
    }
    setInputError(null);
    setApplied(queryOf(draft));
  }

  function resetDraft(): void {
    setDraft(defaultDraft);
    setInputError(null);
    setApplied(queryOf(defaultDraft));
  }

  const windowLabel = reading
    ? t("dataStatistics.window", { from: reading.dateFrom, to: reading.dateTo })
    : null;

  /**
   * Which scope the toolbar names while a reading is on screen.
   *
   * An answered reading wins, because the scope is a fact the response reports
   * rather than one this page chose. With no reading there is still a true
   * answer: the reach is bound by which symbol was mounted, and the platform
   * operation cannot answer a narrower slice (it refuses a tenant-bound context
   * outright) — so naming the reach is a statement about what this page reads.
   * Defaulting to the tenant scope instead would let the operations page tell an
   * operator they are looking at their own tenant while it is asking for every
   * tenant, which is the one thing this mount must never say.
   */
  const scopeLabel = (reading ? reading.platformScope : reach === "every-tenant")
    ? t("dataStatistics.scope.everyTenant")
    : t("dataStatistics.scope.ownTenant");

  // A custom renderer replaces the registry page, and with it the permission
  // gate that page applies. The console surface lists every entry for any
  // authenticated user, so without this the page would fire a request the edge
  // answers 40301 and render it as a load failure — the same copy the registry
  // page shows, rendered where the operator can see it before anything is sent.
  if (!authorized) {
    return (
      <section className="statistics-surface" data-resource={resource}>
        <div className="resource-access-state" role="status">
          <LockKeyhole aria-hidden="true" size={22} />
          <div>
            <strong>{t("access.resource.title")}</strong>
            <p>{t("access.resource.description")}</p>
          </div>
        </div>
      </section>
    );
  }

  return (
    <section className="statistics-surface" data-resource={resource}>
      <header className="statistics-header">
        <h2>{t(resource === "dashboard" ? "resource.dashboard.label" : "resource.traffic-usage.label")}</h2>
        <span className="statistics-meta" aria-live="polite">
          {loading
            ? t("dataStatistics.loading")
            : [scopeLabel, windowLabel]
                .filter((part): part is string => Boolean(part))
                .join(" · ")}
        </span>
      </header>

      {failure?.kind === "unavailable" ? (
        <div className="statistics-state" data-tone="unavailable" role="status">
          <strong>{t("dataStatistics.unavailable.title")}</strong>
          <p>{t("dataStatistics.unavailable.body")}</p>
          <p className="statistics-state-detail">{failure.detail}</p>
        </div>
      ) : null}

      {failure?.kind === "error" ? (
        <div className="statistics-state" data-tone="error" role="alert">
          <strong>{t("dataStatistics.error")}</strong>
          <p className="statistics-state-detail">{failure.detail}</p>
          <button className="secondary-button" onClick={() => setApplied({ ...applied })} type="button">
            {t("dataStatistics.retry")}
          </button>
        </div>
      ) : null}

      {view === "traffic-usage" && !failure ? (
        <form
          className="statistics-filters"
          onSubmit={(event) => {
            event.preventDefault();
            applyDraft();
          }}
        >
          <span className="statistics-filters-label">{t("dataStatistics.filters.title")}</span>
          <label className="statistics-field">
            <span>{t("dataStatistics.dateFrom")}</span>
            <input
              aria-label={t("dataStatistics.dateFrom")}
              onChange={(event) => setDraft((current) => ({ ...current, dateFrom: event.target.value }))}
              type="date"
              value={draft.dateFrom}
            />
          </label>
          <label className="statistics-field">
            <span>{t("dataStatistics.dateTo")}</span>
            <input
              aria-label={t("dataStatistics.dateTo")}
              onChange={(event) => setDraft((current) => ({ ...current, dateTo: event.target.value }))}
              type="date"
              value={draft.dateTo}
            />
          </label>
          <label className="statistics-field">
            <span>{t("dataStatistics.dimensionFilter")}</span>
            <input
              aria-label={t("dataStatistics.dimensionFilter")}
              onChange={(event) => setDraft((current) => ({ ...current, dimension: event.target.value }))}
              placeholder={t("dataStatistics.allDimensions")}
              value={draft.dimension}
            />
          </label>
          <label className="statistics-field">
            <span>{t("dataStatistics.topApps")}</span>
            <input
              aria-invalid={inputError === "topApps" ? true : undefined}
              aria-label={t("dataStatistics.topApps")}
              min={1}
              onChange={(event) => setDraft((current) => ({ ...current, topApps: event.target.value }))}
              type="number"
              value={draft.topApps}
            />
          </label>
          <div className="statistics-filters-actions">
            <button className="command-button" type="submit">{t("dataStatistics.apply")}</button>
            <button className="secondary-button" onClick={resetDraft} type="button">
              {t("dataStatistics.reset")}
            </button>
          </div>
          {inputError === "topApps" ? (
            <span className="statistics-filters-error" role="alert">
              {t("dataStatistics.topAppsInvalid")}
            </span>
          ) : null}
        </form>
      ) : null}

      {view === "dashboard" ? (
        <>
          {metricsFailure?.kind === "unavailable" ? (
            <div className="statistics-state" data-tone="unavailable" role="status">
              <strong>{t("dataStatistics.metrics.unavailable")}</strong>
              <p>{t("dataStatistics.metrics.unavailableBody")}</p>
              <p className="statistics-state-detail">{metricsFailure.detail}</p>
            </div>
          ) : null}

          {metricsFailure?.kind === "error" ? (
            <div className="statistics-state" data-tone="error" role="alert">
              <strong>{t("dataStatistics.metrics.error")}</strong>
              <p className="statistics-state-detail">{metricsFailure.detail}</p>
              <button
                className="secondary-button"
                onClick={() => setMetricsReload((current) => current + 1)}
                type="button"
              >
                {t("dataStatistics.retry")}
              </button>
            </div>
          ) : null}

          {/* A reading whose own scope disagrees with this mount is not drawn.
              Its copy is its own, because "this deployment cannot count" would
              be the wrong explanation for a reading that arrived and said its
              figures are about a different estate. */}
          {metrics && !metricsScopeAgrees ? (
            <div className="statistics-state" data-tone="unavailable" role="status">
              <strong>{t("dataStatistics.metrics.scopeMismatch")}</strong>
              <p>{t("dataStatistics.metrics.scopeMismatchBody")}</p>
            </div>
          ) : null}

          {metrics && metricsScopeAgrees ? (
            <div className="statistics-metrics">
              {renderMetricsGroup(
                "arrival",
                t("dataStatistics.metrics.entities"),
                t("dataStatistics.metrics.entitiesHint"),
                entityCards,
              )}
              {renderMetricsGroup(
                "volume",
                t("dataStatistics.metrics.traffic"),
                [
                  t("dataStatistics.metrics.trafficHint"),
                  // The lifetime traffic figure is only readable with its basis:
                  // "total" over a metering plane with retention is not the life
                  // of the product, and the server says which day it starts on.
                  metrics.trafficSince
                    ? t("dataStatistics.metrics.trafficSince", { date: metrics.trafficSince })
                    : null,
                ]
                  .filter((part): part is string => Boolean(part))
                  .join(" · "),
                trafficCards,
              )}
              {/* Storage is a third group rather than a third entity, because
                  its widest window is a holding rather than a running total. It
                  reports for both reaches — a tenant's own consumption is a real
                  figure for that tenant — so it renders wherever the row does. */}
              {renderMetricsGroup(
                "storage",
                t("dataStatistics.metrics.storage"),
                t("dataStatistics.metrics.storageHint"),
                storageCards,
              )}
            </div>
          ) : null}

          {/* Neither a reading nor a failure yet is the request in flight. Stated
              rather than left as a gap, so the blocks below do not shift down
              when the row arrives. */}
          {!metrics && !metricsFailure ? (
            <p className="statistics-note">{t("dataStatistics.metrics.loading")}</p>
          ) : null}
        </>
      ) : null}

      {/* The empty window is a note on a drawn page, not a replacement for it:
          the frame below renders for every reading that answered, so a quiet
          deployment shows the dashboard with zeros in it rather than a heading
          over a sentence. Only `unavailable` and `error` take the space, and
          both of those have no figures to draw at all. */}
      {empty ? (
        <div className="statistics-state" data-tone="empty" role="status">
          <strong>{t("dataStatistics.empty.title")}</strong>
          <p>{t("dataStatistics.empty.body")}</p>
        </div>
      ) : null}

      {reading ? (
        <>
          {/* The ledger's cards mirror the window the form above asked for. The
              overview's cards are the server-resolved metric row, because a
              dashboard is read at fixed periods — "this month" has to mean the
              same thing on every visit. Both are "a card per dimension", but
              only one of them can be right about *which* window, so each page
              draws its own rather than sharing a row that means two things. */}
          {view === "traffic-usage" ? (
            <div className="statistics-grid">
              {cards.map((card) => (
                <article className="statistics-card" key={card.dimension}>
                  <span className="statistics-value">
                    {formatQuantity(card.quantity, card.unit, locale)}
                  </span>
                  <span className="statistics-label">{seriesText(card.dimension)}</span>
                </article>
              ))}
            </div>
          ) : null}

          <section className="statistics-block">
            {/* The trend, not the table: same days, different question. The
                ledger view renders the same key under a detail heading. */}
            <h3>{t("dataStatistics.trend")}</h3>
            {/* The metered breakdown can be missing while the chart still has
                the estate's own lines to draw. Saying so *above* the plot is the
                difference between "this chart shows what was published" and a
                chart the reader assumes is complete — the traffic cards above
                carry non-zero figures, and their absence from the tabs would
                otherwise be unexplained. */}
            {volumeBreakdownMissing && chartLine !== null ? (
              <p className="statistics-note">{t("dataStatistics.chart.noVolumeBreakdown")}</p>
            ) : null}
            {chartLine !== null ? (
              <>
                {/* Two switches, both the operator's: which metric, and whether
                    its shape reads better as columns or as a line. They are a
                    tablist and a pressed-button group rather than selects,
                    because both are one keystroke away from the thing they
                    change and neither opens a surface over the chart. */}
                <div className="statistics-chart-controls">
                  <div aria-label={t("dataStatistics.chart.series")} className="statistics-chart-tabs" role="tablist">
                    {chartSeries.map((line) => (
                      <button
                        aria-selected={line.dimension === chartDimension}
                        className="statistics-chart-tab"
                        key={line.dimension}
                        onClick={() => setChartSelection(line.dimension)}
                        role="tab"
                        type="button"
                      >
                        {seriesText(line.dimension)}
                      </button>
                    ))}
                  </div>
                  <div aria-label={t("dataStatistics.chart.kind")} className="statistics-chart-kinds" role="group">
                    {CHART_KINDS.map((kind) => (
                      <button
                        aria-pressed={kind === chartKind}
                        className="statistics-chart-kind"
                        key={kind}
                        onClick={() => setChartKind(kind)}
                        type="button"
                      >
                        {t(kind === "bar" ? "dataStatistics.chart.bar" : "dataStatistics.chart.line")}
                      </button>
                    ))}
                  </div>
                </div>
                <div className="statistics-chart">
                  {/* The ticks are the axis, and they are the metric's own
                      figures — so they are text, not decoration, and are not
                      hidden from assistive tech. */}
                  <div className="statistics-chart-axis">
                    {/* An in-flow copy of the widest label, so the gutter is the
                        label's own width rather than a guess. An absolutely
                        positioned label cannot size its parent, and a fixed
                        gutter clips exactly the figures that matter most — the
                        nine-digit ones. The figures are set in tabular figures,
                        so the longest string is also the widest: comparing
                        lengths is exact here rather than an approximation. */}
                    <span aria-hidden="true" className="statistics-chart-tick-sizer">
                      {chartTickLabels.reduce(
                        (widest, label) => (label.length > widest.length ? label : widest),
                        "",
                      )}
                    </span>
                    {chartTicks.map((tick, index) => (
                      <span
                        className="statistics-chart-tick"
                        key={tick.value}
                        style={{ bottom: `${tick.fraction * 100}%` }}
                      >
                        {chartTickLabels[index]}
                      </span>
                    ))}
                  </div>
                  <div
                    aria-label={`${seriesText(chartLine.dimension)} — ${t(
                      chartKind === "bar" ? "dataStatistics.chart.bar" : "dataStatistics.chart.line",
                    )}`}
                    className="statistics-chart-plot"
                    role="img"
                  >
                    {chartTicks.map((tick) => (
                      <span
                        className="statistics-chart-grid"
                        key={tick.value}
                        style={{ bottom: `${tick.fraction * 100}%` }}
                      />
                    ))}
                    {chartKind === "bar"
                      ? chartPlot.bars.map((bar) => (
                          <span
                            className="statistics-chart-bar"
                            key={bar.day}
                            style={{
                              height: `${bar.y}%`,
                              left: `${bar.x}%`,
                              width: `${bar.width}%`,
                            }}
                            title={pointText(chartLine.unit, bar.day, bar.value)}
                          />
                        ))
                      : null}
                    {/* The line is one SVG per run, in a 0–100 viewBox stretched
                        to the box. Only the *positions* may be stretched; the
                        stroke is held at its own width by
                        `vector-effect: non-scaling-stroke`, and the day markers
                        are HTML spans sized in pixels for the same reason. */}
                    {chartKind === "line"
                      ? chartPlot.segments.map((segment) => (
                          <span className="statistics-chart-segment" key={segment[0]?.day}>
                            {segment.length > 1 ? (
                              <svg
                                aria-hidden="true"
                                className="statistics-chart-line"
                                preserveAspectRatio="none"
                                viewBox="0 0 100 100"
                              >
                                <polyline
                                  points={segment
                                    .map((mark) => `${mark.x},${100 - mark.y}`)
                                    .join(" ")}
                                  vectorEffect="non-scaling-stroke"
                                />
                              </svg>
                            ) : null}
                            {segment.map((mark) => (
                              <span
                                className="statistics-chart-dot"
                                key={mark.day}
                                style={{ bottom: `${mark.y}%`, left: `${mark.x}%` }}
                                title={pointText(chartLine.unit, mark.day, mark.value)}
                              />
                            ))}
                          </span>
                        ))
                      : null}
                  </div>
                  {/* A row of the chart's own grid, in the plot's own column, so
                      the readout lines up with the axis it describes. */}
                  <div className="statistics-chart-meta">
                    <span className="statistics-chart-peak">
                      {peakText(chartLine.dimension, chartLine.peak)}
                    </span>
                    {/* The x axis's own extent — deliberately not the *window*,
                        whose upper bound is exclusive and therefore a day past
                        the last column drawn. Naming it through the window
                        template would state a range one day wider than the
                        screen shows. */}
                    <span className="statistics-chart-range">
                      {windowDays.length > 0
                        ? t("dataStatistics.chart.span", {
                            days: String(windowDays.length),
                            from: windowDays[0] ?? "",
                            to: windowDays[windowDays.length - 1] ?? "",
                          })
                        : null}
                    </span>
                  </div>
                </div>
                {/* One metric at a time is a *choice*, and the block says why:
                    a shared axis between requests and bytes would make their
                    relative height a property of the units. */}
                <p className="statistics-note">{t("dataStatistics.chart.basis")}</p>
              </>
            ) : (
              // No daily rows and the reading is not empty: the block keeps its
              // heading and says why it holds no lines, instead of leaving a
              // title with nothing under it.
              <p className="statistics-note">{t("dataStatistics.noBreakdown")}</p>
            )}
          </section>

          <section className="statistics-block">
            <h3>{t("dataStatistics.apps")}</h3>
            <DataTable<TrafficUsageAppTotal>
              columns={appColumns}
              density="compact"
              emptyState={<span>{t("dataStatistics.noBreakdown")}</span>}
              getRowId={(row) => `${row.appUuid ?? row.appSlug ?? "unattributed"}:${row.dimension}`}
              rows={[...reading.apps]}
              stickyHeader
            />
          </section>

          {/* The cards above are the totals, so this block is the reading's own
              breakdown: the per-tenant view on the operations mount (empty for a
              tenant-scoped read, where it would be the caller's own totals
              repeated once per dimension) and the daily facts behind the series.
              Both tables render whether or not they have rows — an empty table
              is a table with an empty state, and its absence is what made a
              quiet window read as a broken page. */}
          {view === "traffic-usage" ? (
            <section className="statistics-block">
              <h3>{t("dataStatistics.tenants")}</h3>
              <DataTable<TrafficUsageTenantTotal>
                columns={tenantColumns}
                density="compact"
                emptyState={<span>{t("dataStatistics.noBreakdown")}</span>}
                getRowId={(row) => `${row.tenantId}:${row.dimension}`}
                rows={[...reading.tenants]}
                stickyHeader
              />
            </section>
          ) : null}

          {view === "traffic-usage" ? (
            <section className="statistics-block">
              <h3>{t("dataStatistics.daily")}</h3>
              <DataTable<TrafficUsageDailyPoint>
                columns={dailyColumns}
                density="compact"
                emptyState={<span>{t("dataStatistics.noBreakdown")}</span>}
                getRowId={(row) => `${row.usageDate}:${row.dimension}`}
                rows={[...reading.daily]}
                stickyHeader
              />
            </section>
          ) : null}
        </>
      ) : null}
    </section>
  );
}

/** Turns the form's strings into the wire query, omitting what was left blank so
 *  the contract's own defaults apply to the parts the operator did not set. */
function queryOf(draft: WindowDraft): TrafficUsageReadingQuery {
  const topApps = draft.topApps.trim();
  const parsed = topApps === "" ? undefined : Number(topApps);
  return {
    ...(draft.dateFrom ? { dateFrom: draft.dateFrom } : {}),
    ...(draft.dateTo ? { dateTo: draft.dateTo } : {}),
    ...(draft.dimension.trim() ? { dimension: draft.dimension.trim() } : {}),
    ...(parsed === undefined || !Number.isInteger(parsed) ? {} : { topApps: parsed }),
  };
}
