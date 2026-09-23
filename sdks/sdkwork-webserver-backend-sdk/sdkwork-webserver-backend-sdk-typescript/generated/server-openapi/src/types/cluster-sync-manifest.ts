export interface ClusterSyncManifest {
  clusterId: string;
  kind: 'config' | 'applications';
  revision: string;
  /** Canonical payload digest the node verifies before applying. */
  sha256: string;
  /** Track-specific desired-state payload; opaque to the transport. */
  payload: Record<string, unknown>;
  createdAt: string;
}
