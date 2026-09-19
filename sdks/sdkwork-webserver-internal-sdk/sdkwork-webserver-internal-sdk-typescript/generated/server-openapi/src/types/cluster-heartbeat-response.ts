import type { ClusterPeer } from './cluster-peer';
import type { ClusterPeerMessage } from './cluster-peer-message';

export interface ClusterHeartbeatResponse {
  instanceId: string;
  status: number;
  acknowledgedAt: string;
  heartbeatIntervalSeconds: number;
  offlineThresholdSeconds: number;
  peers: ClusterPeer[];
  messages: ClusterPeerMessage[];
}
