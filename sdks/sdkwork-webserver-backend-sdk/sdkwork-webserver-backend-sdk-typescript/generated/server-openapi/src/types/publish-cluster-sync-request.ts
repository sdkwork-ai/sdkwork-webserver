export interface PublishClusterSyncRequest {
  /** Desired-state track to publish. */
  kind: 'config' | 'applications';
  /** Track-specific desired-state payload; opaque to the transport. */
  payload: Record<string, unknown>;
}
