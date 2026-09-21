export interface ClusterSyncState {
  kind: 'config' | 'applications';
  desiredRevision?: string;
  appliedRevision?: string;
  status: 'UNKNOWN' | 'IN_SYNC' | 'PENDING' | 'FAILED';
  updatedAt?: string;
}
