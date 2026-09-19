import type { ClusterResponse } from './cluster-response';
import type { PageInfo } from './page-info';

export interface ClustersListResponse {
  code: 0;
  data: unknown & { items: ClusterResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
