import type { ClusterOverviewResponse } from './cluster-overview-response';

export interface ClustersOverviewRetrieveResponse {
  code: 0;
  data: unknown & { item: ClusterOverviewResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
