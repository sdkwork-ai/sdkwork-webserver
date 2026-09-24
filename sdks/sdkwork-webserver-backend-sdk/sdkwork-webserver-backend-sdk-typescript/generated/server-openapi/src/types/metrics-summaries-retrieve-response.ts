import type { MetricsSummaryResponse } from './metrics-summary-response';

export interface MetricsSummariesRetrieveResponse {
  code: 0;
  data: unknown & { item: MetricsSummaryResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
