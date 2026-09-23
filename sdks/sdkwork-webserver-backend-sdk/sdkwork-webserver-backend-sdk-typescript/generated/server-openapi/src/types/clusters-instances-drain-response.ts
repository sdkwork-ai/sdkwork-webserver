import type { ClusterInstanceResponse } from './cluster-instance-response';

export interface ClustersInstancesDrainResponse {
  code: 0;
  data: unknown & { item: ClusterInstanceResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
