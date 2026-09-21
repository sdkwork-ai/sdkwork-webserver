export interface ClusterSyncManifest {
  clusterId: string;
  kind: 'config' | 'applications';
  revision: string;
  sha256: string;
  /** Track-specific desired-state payload (opaque to the transport). */
  payload: Record<string, unknown>;
  createdAt: string;
}
