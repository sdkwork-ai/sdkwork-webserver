import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  createWebserverAdminSdkClient,
  type TrafficUsageStatisticsResponse,
  type WebserverAdminSdkClient,
} from "@sdkwork/webserver-pc-admin-core";

/**
 * Traffic usage API client.
 *
 * Delegates to the generated backend SDK (`trafficUsage` namespace, consumed
 * through the admin-core facade), which speaks the Web Server's aggregated
 * traffic contract:
 *
 *   GET {base}/backend/v3/api/traffic_usage            -> the caller's own tenant
 *   GET {base}/backend/v3/api/platform_traffic_usage   -> every tenant this edge serves
 *
 * The two operations are one read model with two reaches, and neither takes a
 * tenant from the query string: which tenants answer is derived server-side
 * from the authenticated context, so no parameter this client could send
 * retargets a read. That is also why `reach` selects an *operation* instead of
 * being folded into the query — a console mount that asked for the platform
 * reach does not receive a wider reading, it receives `40301`, because the
 * platform operation is restricted to the operator tenant. Failing loudly is
 * the point: the alternative is a single operation whose admin page silently
 * renders one tenant's traffic as the platform total.
 */
export type TrafficUsageReach = "own-tenant" | "every-tenant";

export interface TrafficUsageReadingQuery {
  /** Inclusive UTC calendar day (`YYYY-MM-DD`). Defaults server-side to 30 days back. */
  dateFrom?: string;
  /** Exclusive UTC calendar day (`YYYY-MM-DD`). Defaults server-side to tomorrow. */
  dateTo?: string;
  /** Restrict the totals and the series to one usage dimension. */
  dimension?: string;
  /** Size of the per-app breakdown, 1..100. */
  topApps?: number;
}

export class TrafficUsageClient {
  private readonly client: WebserverAdminSdkClient;

  constructor(baseUrl: string, tokenManager: AuthTokenManager) {
    this.client = createWebserverAdminSdkClient(baseUrl, tokenManager);
  }

  read(
    reach: TrafficUsageReach,
    query: TrafficUsageReadingQuery,
  ): Promise<TrafficUsageStatisticsResponse> {
    const params = {
      ...(query.dateFrom ? { dateFrom: query.dateFrom } : {}),
      ...(query.dateTo ? { dateTo: query.dateTo } : {}),
      ...(query.dimension ? { dimension: query.dimension } : {}),
      ...(query.topApps === undefined ? {} : { topApps: query.topApps }),
    };
    return reach === "every-tenant"
      ? this.client.trafficUsage.platformTrafficUsages.retrieve(params)
      : this.client.trafficUsage.retrieve(params);
  }
}

export function createTrafficUsageClient(
  backendApiBaseUrl: string,
  tokenManager: AuthTokenManager,
): TrafficUsageClient {
  return new TrafficUsageClient(backendApiBaseUrl, tokenManager);
}
