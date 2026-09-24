import type { MetricsSummaryResponse } from './metrics-summary-response';

export interface PlatformMetricsSummariesRetrieveResponse {
  code: 0;
  data: unknown & { item: MetricsSummaryResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
