import type { TrafficUsageStatisticsResponse } from './traffic-usage-statistics-response';

export interface PlatformTrafficUsagesRetrieveResponse {
  code: 0;
  data: unknown & { item: TrafficUsageStatisticsResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
