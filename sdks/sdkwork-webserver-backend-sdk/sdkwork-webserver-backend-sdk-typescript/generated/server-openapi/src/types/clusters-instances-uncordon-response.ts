import type { ClusterInstanceResponse } from './cluster-instance-response';

export interface ClustersInstancesUncordonResponse {
  code: 0;
  data: unknown & { item: ClusterInstanceResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
