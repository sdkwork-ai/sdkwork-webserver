import type { ClusterResponse } from './cluster-response';

export interface ClustersCreateResponse201 {
  code: 0;
  data: unknown & { item: ClusterResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
