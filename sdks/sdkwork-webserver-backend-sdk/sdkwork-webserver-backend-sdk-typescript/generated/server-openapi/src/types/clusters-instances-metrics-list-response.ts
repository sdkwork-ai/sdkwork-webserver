import type { ClusterHeartbeatSampleResponse } from './cluster-heartbeat-sample-response';
import type { PageInfo } from './page-info';

export interface ClustersInstancesMetricsListResponse {
  code: 0;
  data: unknown & { items: ClusterHeartbeatSampleResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
