import type { EnqueueClusterPeerMessagesResponse } from './enqueue-cluster-peer-messages-response';

export interface ClustersMessagesCreateResponse201 {
  code: 0;
  data: unknown & { item: EnqueueClusterPeerMessagesResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
