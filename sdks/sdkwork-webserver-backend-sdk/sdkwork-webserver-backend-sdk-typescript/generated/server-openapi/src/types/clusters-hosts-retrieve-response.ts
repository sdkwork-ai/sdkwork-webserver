import type { ClusterHostResponse } from './cluster-host-response';

export interface ClustersHostsRetrieveResponse {
  code: 0;
  data: unknown & { item: ClusterHostResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
