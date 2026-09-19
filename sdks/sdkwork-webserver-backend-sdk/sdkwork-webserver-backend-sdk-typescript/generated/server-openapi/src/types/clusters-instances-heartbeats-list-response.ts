import type { ClusterHeartbeatSampleResponse } from './cluster-heartbeat-sample-response';
import type { PageInfo } from './page-info';

export interface ClustersInstancesHeartbeatsListResponse {
  code: 0;
  data: unknown & { items: ClusterHeartbeatSampleResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
