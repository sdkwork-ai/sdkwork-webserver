import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  createWebserverAdminSdkClient,
  type MetricsSummaryResponse,
  type WebserverAdminSdkClient,
} from "@sdkwork/webserver-pc-admin-core";

/**
 * Dashboard metric summary client.
 *
 * Delegates to the generated backend SDK (`metricsSummary` namespace, consumed
 * through the admin-core facade), which speaks the Web Server's cardinal-metric
 * contract:
 *
 *   GET {base}/backend/v3/api/metrics_summaries            -> the caller's own tenant
 *   GET {base}/backend/v3/api/platform_metrics_summaries   -> every tenant this edge serves
 *
 * Same two-reach shape as the traffic readings, and for the same reason: which
 * tenants answer is derived server-side from the authenticated context, so
 * `reach` selects an *operation* rather than filling in a parameter. A console
 * mount that asked for the platform reach does not receive a wider reading, it
 * receives `40301` — because a count carries no identifiers, a single operation
 * whose admin page silently counted one tenant's estate as the platform's would
 * be undetectable from the response alone.
 *
 * **The card windows take no parameters.** They are resolved server-side from
 * the clock, so there is no client-supplied bound that could make the
 * response's stated basis disagree with the figures cut against it.
 *
 * The `dateFrom` / `dateTo` bounds do travel, and they reach the **series**
 * only — the per-day trend. That window is the caller's rather than the
 * product's because the plot it feeds also draws the traffic reading's own
 * per-day series, and one plot has one x domain: the entity lines have to be cut
 * against the same days the metered ones are, or the axis would be a claim the
 * data does not support. They are the same two parameter names the traffic
 * reading takes, for the same reason — and this client is handed **one** pair by
 * the surface, so the two readings are asked for one window rather than each
 * resolving an omitted bound from its own clock.
 */
export type MetricsSummaryReach = "own-tenant" | "every-tenant";

/**
 * The window the per-day series is read over.
 *
 * Deliberately its own type rather than the traffic reading's query: the two
 * clients are handed the same two days by the surface, and a shared type would
 * invite one of them to grow a field the other cannot answer.
 */
export interface MetricsSeriesQuery {
  /** Inclusive UTC calendar day (`YYYY-MM-DD`). Defaults server-side to 30 days back. */
  dateFrom?: string;
  /** Exclusive UTC calendar day (`YYYY-MM-DD`). Defaults server-side to tomorrow. */
  dateTo?: string;
}

export class MetricsSummaryClient {
  private readonly client: WebserverAdminSdkClient;

  constructor(baseUrl: string, tokenManager: AuthTokenManager) {
    this.client = createWebserverAdminSdkClient(baseUrl, tokenManager);
  }

  read(
    reach: MetricsSummaryReach,
    query: MetricsSeriesQuery = {},
    options?: { signal?: AbortSignal },
  ): Promise<MetricsSummaryResponse> {
    // The signal is threaded through rather than dropped: both readings on the
    // dashboard are re-fired whenever the operator changes the window, and a
    // metric summary that a superseded request still resolves afterwards would
    // land on top of the newer one.
    const params = {
      ...(query.dateFrom ? { dateFrom: query.dateFrom } : {}),
      ...(query.dateTo ? { dateTo: query.dateTo } : {}),
    };
    const requestOptions = options?.signal ? { signal: options.signal } : undefined;
    return reach === "every-tenant"
      ? this.client.metricsSummary.platformMetricsSummaries.retrieve(params, requestOptions)
      : this.client.metricsSummary.retrieve(params, requestOptions);
  }
}

export function createMetricsSummaryClient(
  backendApiBaseUrl: string,
  tokenManager: AuthTokenManager,
): MetricsSummaryClient {
  return new MetricsSummaryClient(backendApiBaseUrl, tokenManager);
}
