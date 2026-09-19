import type { ClusterEventResponse } from './cluster-event-response';
import type { PageInfo } from './page-info';

export interface ClustersEventsListResponse {
  code: 0;
  data: unknown & { items: ClusterEventResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
