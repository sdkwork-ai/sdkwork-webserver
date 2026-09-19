import type { ClusterHostResponse } from './cluster-host-response';
import type { PageInfo } from './page-info';

export interface ClustersHostsListResponse {
  code: 0;
  data: unknown & { items: ClusterHostResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
