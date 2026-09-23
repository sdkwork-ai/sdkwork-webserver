import type { ClusterSyncManifest } from './cluster-sync-manifest';

export interface ClustersSyncResponse {
  code: 0;
  data: unknown & { item: ClusterSyncManifest; };
  /** Server-owned request correlation id. */
  traceId: string;
}
