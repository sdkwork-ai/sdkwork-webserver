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
  TrafficUsageAppTotal,
  TrafficUsageDailyPoint,
  TrafficUsageStatisticsResponse,
  TrafficUsageTenantTotal,
  TrafficUsageTotal,
} from "@sdkwork/webserver-pc-admin-core";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  buildTrendSeries,
  dimensionLabelKey,
  failureMessage,
  formatQuantity,
  isEmptyReading,
  isUnavailableReading,
  utcDay,
} from "./data-statistics-model.ts";
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
 * ## Three states, not two
 *
 * A reading that could not be produced (the edge answers `503` when no usage
 * read model is assembled, or when a dependency is down) renders as an
 * unavailable *capability*; a reading that answered and holds no facts renders
 * as an empty window; anything else renders the figures. Collapsing the first
 * into the second would turn "this deployment has nothing to measure with" into
 * "this edge served no traffic", which is a different claim about the system
 * than the response actually made.
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
  const totals = reading?.totals ?? [];
  const unitByDimension = useMemo(() => {
    const units = new Map<string, string>();
    for (const total of totals) {
      units.set(total.dimension, total.unit);
    }
    return units;
  }, [totals]);
  const dimensionText = useCallback(
    (dimension: string): string => {
      const key = dimensionLabelKey(dimension) as WebserverMessageKey;
      const translated = t(key);
      // `translateWebserver` hands back the key itself when the catalog has no
      // entry, which is how a metered dimension this file has never heard of
      // still reaches the operator under its contract name.
      return translated === key ? dimension : translated;
    },
    [t],
  );
  const series = useMemo(() => buildTrendSeries(reading?.daily ?? []), [reading]);
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

  const dailyColumns = useMemo<DataTableColumn<TrafficUsageDailyPoint>[]>(
    () => [
      { id: "usageDate", header: t("dataStatistics.day"), cell: (row) => row.usageDate },
      { id: "dimension", header: t("dataStatistics.dimension"), cell: (row) => dimensionText(row.dimension) },
      {
        align: "right",
        id: "quantity",
        header: t("dataStatistics.quantity"),
        cell: (row) => formatQuantity(row.quantity, unitByDimension.get(row.dimension) ?? "", locale),
      },
    ],
    [dimensionText, locale, t, unitByDimension],
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
      { id: "dimension", header: t("dataStatistics.dimension"), cell: (row) => dimensionText(row.dimension) },
      {
        align: "right",
        id: "quantity",
        header: t("dataStatistics.quantity"),
        cell: (row) => formatQuantity(row.quantity, row.unit, locale),
      },
    ],
    [dimensionText, locale, t],
  );
  const tenantColumns = useMemo<DataTableColumn<TrafficUsageTenantTotal>[]>(
    () => [
      { id: "tenantId", header: t("dataStatistics.tenant"), cell: (row) => row.tenantId },
      { id: "dimension", header: t("dataStatistics.dimension"), cell: (row) => dimensionText(row.dimension) },
      {
        align: "right",
        id: "quantity",
        header: t("dataStatistics.quantity"),
        cell: (row) => formatQuantity(row.quantity, row.unit, locale),
      },
    ],
    [dimensionText, locale, t],
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

      {reading && isEmptyReading(reading) ? (
        <div className="statistics-state" data-tone="empty" role="status">
          <strong>{t("dataStatistics.empty.title")}</strong>
          <p>{t("dataStatistics.empty.body")}</p>
        </div>
      ) : null}

      {reading && !isEmptyReading(reading) ? (
        <>
          <div className="statistics-grid">
            {totals.map((total) => (
              <article className="statistics-card" key={total.dimension}>
                <span className="statistics-value">{formatQuantity(total.quantity, total.unit, locale)}</span>
                <span className="statistics-label">{dimensionText(total.dimension)}</span>
              </article>
            ))}
          </div>

          {series.length > 0 ? (
            <section className="statistics-block">
              {/* The trend, not the table: same days, different question. The
                  ledger view renders the same key under a detail heading. */}
              <h3>{t("dataStatistics.trend")}</h3>
              {series.map((line) => (
                <div className="statistics-series" key={line.dimension}>
                  <div className="statistics-series-head">
                    <span className="statistics-series-name">{dimensionText(line.dimension)}</span>
                    <span className="statistics-series-peak">{peakText(line.dimension, line.peak)}</span>
                  </div>
                  <div
                    aria-label={`${dimensionText(line.dimension)} — ${t("dataStatistics.seriesIndex")}`}
                    className="statistics-series-track"
                    role="img"
                  >
                    {line.points.map(([day, index]) => (
                      <span
                        className="statistics-series-bar"
                        key={day}
                        style={{ height: `${index}%` }}
                        title={`${day} · ${index}`}
                      />
                    ))}
                  </div>
                </div>
              ))}
              {/* The series are indexed, not measured, so the axis is stated
                  rather than drawn: an unlabelled pair of bars is exactly how
                  requests and bytes come to look comparable. */}
              <p className="statistics-note">{t("dataStatistics.indexNote")}</p>
            </section>
          ) : null}

          {reading.apps.length > 0 ? (
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
          ) : null}

          {/* The cards above are the totals, so this block is the reading's own
              breakdown: the per-tenant view on the operations mount (empty for a
              tenant-scoped read, where it would be the caller's own totals
              repeated once per dimension) and the daily facts behind the series. */}
          {view === "traffic-usage" ? (
            <section className="statistics-block">
              <h3>{t("dataStatistics.tenants")}</h3>
              {reading.tenants.length > 0 ? (
                <DataTable<TrafficUsageTenantTotal>
                  columns={tenantColumns}
                  density="compact"
                  emptyState={<span>{t("dataStatistics.noBreakdown")}</span>}
                  getRowId={(row) => `${row.tenantId}:${row.dimension}`}
                  rows={[...reading.tenants]}
                  stickyHeader
                />
              ) : (
                <p className="statistics-note">{t("dataStatistics.noBreakdown")}</p>
              )}
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
