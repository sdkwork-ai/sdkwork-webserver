export interface ClusterSyncAckRequest {
  kind: 'config' | 'applications';
  revision: string;
  status: 'UNKNOWN' | 'IN_SYNC' | 'PENDING' | 'FAILED';
  detail?: string;
}
