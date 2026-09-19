import type { ClusterHostResponse } from './cluster-host-response';

export interface ClustersHostsUpdateResponse {
  code: 0;
  data: unknown & { item: ClusterHostResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
