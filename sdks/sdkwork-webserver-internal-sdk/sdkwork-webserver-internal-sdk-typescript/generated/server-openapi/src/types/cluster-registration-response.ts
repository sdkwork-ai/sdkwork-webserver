import type { ClusterHostRef } from './cluster-host-ref';
import type { ClusterInstanceRef } from './cluster-instance-ref';
import type { ClusterPeer } from './cluster-peer';
import type { ClusterRef } from './cluster-ref';

export interface ClusterRegistrationResponse {
  cluster: ClusterRef;
  host: ClusterHostRef;
  instance: ClusterInstanceRef;
  /** Secret heartbeat token for this instance (winst_ prefix); store securely. */
  instanceToken: string;
  heartbeatIntervalSeconds: number;
  offlineThresholdSeconds: number;
  peers: ClusterPeer[];
}
