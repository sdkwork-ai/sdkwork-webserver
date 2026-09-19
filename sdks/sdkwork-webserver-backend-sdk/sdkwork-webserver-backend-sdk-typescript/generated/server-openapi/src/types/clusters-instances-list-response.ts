import type { ClusterInstanceResponse } from './cluster-instance-response';
import type { PageInfo } from './page-info';

export interface ClustersInstancesListResponse {
  code: 0;
  data: unknown & { items: ClusterInstanceResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
