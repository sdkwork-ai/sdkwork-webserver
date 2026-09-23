import type { ClusterProbeRunResponse } from './cluster-probe-run-response';

export interface ClustersInstancesProbeResponse {
  code: 0;
  data: unknown & { item: ClusterProbeRunResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
