import { useWebserverAdminSdk } from "@sdkwork/webserver-pc-admin-core";
import type {
  ClusterEventResponse,
  ClusterOverviewResponse,
} from "@sdkwork/webserver-pc-admin-core";
import type { WebserverLocale } from "@sdkwork/webserver-pc-core";
import { translateWebserver } from "@sdkwork/webserver-pc-commons";
import { DataTable, type DataTableColumn } from "@sdkwork/ui-pc-react";
import { useEffect, useMemo, useRef, useState } from "react";

/**
 * Distributed cluster overview: platform-level health cards plus the most
 * recent lifecycle events, auto-refreshed on a bounded interval so operators
 * can watch instance liveness without leaving the page (PRD-FR-028).
 *
 * The page renders inside `WebserverAdminSdkProvider`, so the SDK client is
 * injected through the admin-core hook; no client is constructed here.
 */

const REFRESH_INTERVAL_MS = 10_000;
const RECENT_EVENT_COUNT = 8;

const SEVERITY_KEYS = {
  INFO: "resource.cluster-overview.severity.info",
  WARNING: "resource.cluster-overview.severity.warning",
  ERROR: "resource.cluster-overview.severity.error",
} as const;

export interface ClusterOverviewSurfaceProps {
  locale: WebserverLocale;
  resource: "cluster-overview";
}

interface OverviewSnapshot {
  overview: ClusterOverviewResponse;
  events: ClusterEventResponse[];
}

export function ClusterOverviewSurface({ locale, resource }: ClusterOverviewSurfaceProps) {
  const client = useWebserverAdminSdk();
  const [snapshot, setSnapshot] = useState<OverviewSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const inFlight = useRef<AbortController | null>(null);

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      inFlight.current?.abort();
      const controller = new AbortController();
      inFlight.current = controller;
      setRefreshing(true);
      try {
        const [overview, events] = await Promise.all([
          client.cluster.overview.retrieve({ signal: controller.signal }),
          client.cluster.events.list({ pageSize: RECENT_EVENT_COUNT }, { signal: controller.signal }),
        ]);
        if (!cancelled) {
          setSnapshot({ overview, events: events.items });
          setError(null);
        }
      } catch (cause) {
        if (cancelled || controller.signal.aborted) return;
        setError(cause instanceof Error ? cause.message : String(cause));
      } finally {
        if (!cancelled) setRefreshing(false);
      }
    };
    void load();
    const timer = window.setInterval(() => void load(), REFRESH_INTERVAL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
      inFlight.current?.abort();
    };
  }, [client]);

  const t = (key: Parameters<typeof translateWebserver>[1]) => translateWebserver(locale, key);

  if (error && !snapshot) {
    return (
      <section className="data-surface" data-resource={resource}>
        <p className="bootstrap-state" role="alert">{t("resource.cluster-overview.loadFailed")}: {error}</p>
      </section>
    );
  }

  if (!snapshot) {
    return (
      <section className="data-surface" data-resource={resource}>
        <p className="bootstrap-state" role="status">{t("resource.cluster-overview.loading")}</p>
      </section>
    );
  }

  const { overview, events } = snapshot;
  // Int64 wire fields arrive as decimal strings (API_SPEC §13.6); they are
  // display values here, and only converted for the tone comparison.
  const cards: readonly { label: string; value: string; tone: "ok" | "warn" | "neutral" }[] = [
    { label: t("resource.cluster-overview.onlineHosts"), value: overview.onlineHosts, tone: "ok" },
    { label: t("resource.cluster-overview.totalHosts"), value: overview.totalHosts, tone: "neutral" },
    { label: t("resource.cluster-overview.onlineInstances"), value: overview.onlineInstances, tone: "ok" },
    { label: t("resource.cluster-overview.totalInstances"), value: overview.totalInstances, tone: "neutral" },
    { label: t("resource.cluster-overview.unhealthyInstances"), value: overview.unhealthyInstances, tone: positive(overview.unhealthyInstances) ? "warn" : "ok" },
    { label: t("resource.cluster-overview.pendingPeerMessages"), value: overview.pendingPeerMessages, tone: positive(overview.pendingPeerMessages) ? "warn" : "ok" },
  ];
  // 最近事件表：与既有四列一一对应。事件条数固定在 RECENT_EVENT_COUNT，不做分页。
  const eventColumns: DataTableColumn<ClusterEventResponse>[] = [
    { id: "occurredAt", header: t("resource.cluster-overview.eventTime"), cell: (event) => formatInstant(event.occurredAt, locale) },
    {
      id: "severity",
      header: t("resource.cluster-overview.eventSeverity"),
      cell: (event) => (
        <span className={`status-badge cluster-severity-${event.severity.toLowerCase()}`}>
          {t(SEVERITY_KEYS[event.severity] ?? "resource.cluster-overview.severity.info")}
        </span>
      ),
    },
    { id: "eventType", header: t("resource.cluster-overview.eventType"), cell: (event) => <code>{event.eventType}</code> },
    { id: "message", header: t("resource.cluster-overview.eventMessage"), cell: (event) => event.message },
  ];

  return (
    <section className="data-surface" data-resource={resource}>
      <header className="resource-toolbar">
        <h2>{t("resource.cluster-overview.label")}</h2>
        <span className="toolbar-meta" aria-live="polite">
          {refreshing ? t("resource.cluster-overview.refreshing") : `${t("resource.cluster-overview.updatedAt")}: ${formatInstant(overview.generatedAt, locale)}`}
        </span>
      </header>
      <div className="cluster-overview-grid">
        {cards.map((card) => (
          <article key={card.label} className="cluster-overview-card" data-tone={card.tone}>
            <span className="cluster-overview-value">{card.value}</span>
            <span className="cluster-overview-label">{card.label}</span>
          </article>
        ))}
      </div>
      <h3>{t("resource.cluster-overview.recentEvents")}</h3>
      <DataTable<ClusterEventResponse>
        columns={eventColumns}
        density="compact"
        emptyState={<span>{t("resource.cluster-overview.noEvents")}</span>}
        getRowId={(event) => event.id}
        rows={events}
        stickyHeader
      />
      {error ? <p className="bootstrap-state" role="alert">{t("resource.cluster-overview.refreshFailed")}: {error}</p> : null}
    </section>
  );
}

function positive(decimalString: string): boolean {
  return /^\d+$/.test(decimalString) && Number(decimalString) > 0;
}

function formatInstant(instant: string, locale: WebserverLocale): string {
  const parsed = new Date(instant);
  if (Number.isNaN(parsed.getTime())) return instant;
  return parsed.toLocaleString(locale === "zh-CN" ? "zh-CN" : "en-US", { hour12: false });
}
